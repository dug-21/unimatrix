# Scope Risk Assessment: nan-019

NFR-maintenance feature: wire existing `docker-http-posture-smoke.sh` (#786) into `release.yml` as a standing, verify-by-name, skip-is-failure gate that blocks the multi-arch manifest. Advances N5, guards N3. Historical grounding: #4796, #5130, #4582, #4572, #5180.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | **False-green via self-skip / early-exit-0.** Smoke `exit 3` (Docker absent) or any early `exit 0` before `ALL GATES PASSED` could pass the job green, re-creating the #4796/#4970 false-green class — the single load-bearing risk. | High | Med | Architect/spec MUST key on the three distinct exit codes (`0`/`1`/`3`) AND assert the positive run-marker (terminal `ALL GATES PASSED` line grepped from captured smoke output). Job-exit-0 alone is NOT proof. Encode both checks; do not let `set -e`/pipefail or YAML `if:` swallow the code. |
| SR-02 | **Pushed-bytes-not-rebuild.** If the smoke self-builds (or `IMAGE=` defaults to a local build) it tests a rebuild, not the shipped artifact — definitionally misses the #783/#5130 class the feature exists to close. | High | Med | Spec must require `IMAGE=ghcr.io/<owner>/unimatrix:<tag>-<arch>` after GHCR `docker login`; order each smoke `needs:` its per-arch push; assert NO duplicate full build runs. (AC-06, DECIDED OQ-2.) |
| SR-03 | **ONNX image build/runtime cost & ARM64 first-boot.** The ONNX/embedding-model image (#767) is slow to build/pull; first-boot model loading is plausibly arch-sensitive. arm64 smoke on `ubuntu-22.04-arm` adds wall-clock + pull cost to every release. | Med | Med | Reuse pushed bytes (no rebuild, SR-02 already mitigates build cost). Architect should bound the boot-wait/log-poll timeout generously enough for cold arm64 model load without masking a real hang; budget release-time increase. |
| SR-04 | **ARM64 runner flakiness / GHCR push-ordering / Docker-availability drift.** `ubuntu-22.04-arm` is the youngest runner class (flake-prone); GHCR push must complete + be pullable before smoke; hosted-runner Docker preinstall is assumed-not-guaranteed. | Med | Med | No silent retry (Goal 5 / DECIDED OQ-6) — a flaky deployability gate is signal. Smoke `needs:` the push job (ordering). Do NOT tolerate Docker-absent: `exit 3` → hard fail (SR-01). Treat first-run flake as in-scope rework, not a reason to add `|| retry`. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | **AC-05 smoke hardening scope-creep.** "Assert per-slug DB grew, hash store did not" is the only in-scope script change; risk of expanding into a smoke rewrite or new scenarios. | Low | Med | Keep AC-05 bounded: one assertion pair via the existing read-only busybox sidecar — never `docker exec` into distroless. No new scripts (Non-Goals; cumulative test infra). |
| SR-06 | **Trigger surface over/under-reach.** Gate must fire on tag `v*` push AND `workflow_dispatch`, but explicitly NOT `pull_request`. Wrong trigger = either no coverage or CI-lane pollution. | Med | Low | Spec the trigger set exactly (DECIDED OQ-5). `workflow_dispatch` is required for human pre-release dry-run. |
| SR-07 | **Arch coverage silently capped.** A future "amd64-only = N5-proven" outcome is FORBIDDEN by the HARD RULE; only acceptable fallback is amd64-now + arm64 as a NAMED, TRACKED fast-follow with N5 staying PARTIAL. | Med | Low | N5 `done_when` and the gate's promise MUST name actual arch coverage. Decided path = both arches. Any deferral is logged, never buried under a green check. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | **ADR-004 independence violation (#4572).** A naive `needs:` edge could couple the container/smoke branch to the binary/npm branch, letting ARM64/Docker flake block an unrelated binary/npm release. | High | Med | Land the block ONLY on `create-container-manifest` (`needs: [smoke-amd64, smoke-arm64]`); introduce NO edge to/from the binary/npm branch; `create-release` stays independent (OQ-1 human-preferred, architect owns final mechanics). AC-04. |
| SR-09 | **Briefly-public un-smoked per-arch intermediates.** `:<tag>-amd64`/`-arm64` are pushed (public) before smokes run; only the manifest is gated. | Low | High | Accepted (DECIDED OQ-1/OQ-2): operators pull the manifest, which is never released until both smokes pass. Spec should state this explicitly so it is not later read as a defect. |

## Assumptions

- **The #786 smoke is correct and complete as-shipped** (SCOPE Background; ran 3/3 locally). This feature *wires* it — if the smoke itself has a latent bug, the gate inherits it. AC-05 is the only sanctioned change.
- **Hosted `ubuntu-*` runners ship Docker** (SCOPE Background). The `exit 3` guard exists precisely because this is not guaranteed forever; the gate must encode intolerance (SR-01), not depend on the assumption.
- **The gate cannot be proven by local Linux validation** (SCOPE Constraints; lesson #4796). First real execution is post-merge on tag push. If this assumption is mishandled, ACs get asserted as executed fact before they ever ran.

## Design Recommendations

1. **Make the false-green guard the design's spine (SR-01).** Capture exit code explicitly, branch on `0`/`1`/`3`, AND grep the captured output for the terminal `ALL GATES PASSED` marker. Both gates required for green. This is the feature's reason to exist — getting it wrong is worse than no gate.
2. **Pin the artifact under test to pushed GHCR bytes (SR-02), order after push, no rebuild.** Self-build is rejected.
3. **Honor ADR-004 by gating only the manifest (SR-08/SR-09).** No binary/npm coupling; no `|| retry` (SR-04).
4. **Budget a post-tag CI round-trip and watch the FIRST real release run to completion on BOTH arches (SR-03/SR-04, AC-07, lesson #4796).** Treat any platform/runner surprise as in-scope rework. Phrase AC-07 as "configured + verified locally; GH execution confirmed post-tag," never asserted before execution.
5. **Keep AC-05 bounded and within the busybox-sidecar pattern (SR-05).** No smoke rewrite.
