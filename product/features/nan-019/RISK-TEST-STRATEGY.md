# Risk-Based Test Strategy: nan-019

> Standing release gate wiring `docker-http-posture-smoke.sh` into `release.yml` as a
> verify-by-name / skip-is-failure gate blocking the multi-arch manifest. Advances N5,
> guards N3. This is a YAML job-topology + shell exit-code/run-marker contract change —
> **no Rust application code.** The dominant risk is that the gate's own correctness
> cannot be proven by local Linux validation, yet a defect in it manufactures false
> confidence (the exact #4796/#4970 class this feature exists to kill).
>
> Historical grounding: pattern #5180 (verify-by-name), ADR-003 #5183, lesson #4873
> (setsid swallows RC — structurally-plausible capture that silently returns 0), lesson
> #4796 (CI-dependent ACs cannot be asserted pre-merge), ADR #329 (WAL auto-checkpoint —
> main DB file size is NOT monotone on a single small write).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | **Gate logic itself is wrong and untested.** The exit-code `case` + run-marker grep is the feature's spine, yet it is shell glue that runs only post-merge. If never unit-tested against synthetic outputs, a bug ships as a green-but-blind gate. | High | High | **Critical** |
| R-02 | **RC swallowed before it is read** — `set -e`/`pipefail`, a subshell, a `\| tee`, or a YAML `if:` consumes the smoke's non-zero exit so the job sees 0. (The #4873 `setsid`-without-`-w` class: structurally plausible, silently returns 0, provable only empirically.) | High | Med | **Critical** |
| R-03 | **Early-exit-0 with run-marker absent passes green.** Run-marker assertion is missing, mis-anchored (`grep` not `grep -qx`/anchored), or matches a substring/log echo rather than the terminal line — a future smoke change that exits 0 before `ALL GATES PASSED` slips through. | High | Med | **Critical** |
| R-04 | **AC-05 grew-signal is flaky and cannot be retried away (OQ-6).** Main `unimatrix.db` file size is NOT monotone on one small committed write under WAL: the write may land in `-wal` and not grow the main file until checkpoint (ADR #329, autocheckpoint ~1000 pages/~4MB). A naive main-file size delta false-fails a healthy write — and `\|\| retry` is forbidden. | High | Med | **Critical** |
| R-05 | **arm64 cold first-boot exceeds the smoke's 90s boot deadline** (ONNX/embedding model load #767, plausibly arch-sensitive + slower arm64 runner) — false-fails a healthy boot, masquerading as a deployability defect. Widening the deadline too far masks a true hang. | High | Med | **Critical** |
| R-06 | **ADR-004 independence regression** — a future `needs:` edit (or this one done naively) couples the container/smoke branch to the binary/npm branch, letting an arm64/Docker flake block an unrelated binary/npm release. No standing test pins the absence of that edge. | High | Med | **Critical** |
| R-07 | **Pushed-bytes contract silently degrades to a rebuild.** `IMAGE=` is unset/mistyped, the smoke falls back to its build branch, and the gate tests a rebuild — definitionally not what ships (misses the #783/#5130 class). Green, but blind. | High | Low | High |
| R-08 | **Manifest not actually gated** — `create-container-manifest.needs` omits a smoke (or keeps `build-container-*` and the smoke is unreachable/skipped), so a released artifact is un-smoked despite a green run. | High | Low | High |
| R-09 | **Tag-resolution mismatch (OCCURRED in first design draft).** The smoke's resolved push tag must be byte-identical to the metadata-action push pattern, or `docker pull` 404s on every release. The first draft resolved the version **stripped** (`${GITHUB_REF_NAME#v}` ⇒ `:1.2.3-<arch>`) while build-container pushes **un-stripped** (`pattern=v{{version}}-<arch>` ⇒ `:v1.2.3-<arch>`) — a guaranteed 404 at every tag. Corrected to un-stripped (`VERSION="${GITHUB_REF_NAME}"` ⇒ `:v<version>-<arch>`). Severity raised Med→**High** because it materialized, not hypothetical. | High | High | **High** |
| R-10 | **GHCR push not yet pullable when the smoke runs** — registry propagation lag after `--push`; the pull races the push completion and intermittently fails (no retry allowed). | Med | Low | Med |
| R-11 | **Trigger surface over/under-reach** — `workflow_dispatch` missing (no pre-tag dry-run, R-01/R-05 only surface at real tag), or `pull_request` accidentally included (CI-lane pollution + un-Docker'd lanes hard-failing PRs). | Med | Low | Med |
| R-12 | **AC-05 hardening regresses the smoke / breaks the run-marker.** The grew-assertion is added above the terminal line, or `docker exec`s into distroless instead of the `vol()` busybox sidecar, breaking the very script the gate keys on. | Med | Low | Med |
| R-13 | **Inherited latent smoke bug.** Feature assumes the #786 smoke is correct (ran 3/3 locally); the gate inherits any latent defect. arm64 is a never-before-run path for this smoke. | Med | Low | Med |
| R-14 | **Briefly-public un-smoked per-arch intermediates** consumed by an operator pulling `:<tag>-amd64` directly (not the manifest). Accepted by design (NFR-09) — listed for completeness, not mitigation. | Low | High | Low |

## Risk-to-Scenario Mapping

### R-01: Gate logic is wrong and untested
**Severity**: High · **Likelihood**: High · **Impact**: A blind gate that reports green ships — the feature's entire reason to exist is defeated, silently.

**Test Scenarios** (all local, pre-merge — the gate logic IS provable locally even though hosted execution is not):
1. Extract the exit-code `case` + run-marker grep into a shape that can be exercised with a **stub smoke** (a script that prints a fixture and `exit`s a chosen code). Drive it with synthetic inputs:
   - `exit 0` + output containing the anchored marker → **job green**.
   - `exit 1` + `fail()` output, no marker → **job red**, `exit 1` diagnostic present.
   - `exit 3` + `SKIP: Docker not available` → **job red**, "mis-provisioned / hard failure" diagnostic present.
   - `exit 0` + output **without** the marker (early-exit-0) → **job red**, "exited 0 but never printed ALL GATES PASSED" diagnostic.
   - unexpected code (`exit 2`/`139`) → **job red**, unexpected-code diagnostic.
2. Confirm each red path emits a `::error::` line keyed to the specific cause (so a post-tag failure is diagnosable without a re-run).

**Coverage Requirement**: A truth table of {0, 1, 3, early-0, unexpected} × {marker present, absent} is exercised against the actual capture-and-branch logic; only `(0, marker present)` yields green. This table is the gate's unit test and MUST exist before merge.

### R-02: RC swallowed before it is read
**Severity**: High · **Likelihood**: Med · **Impact**: A real smoke failure (`exit 1`) or skip (`exit 3`) is silently read as `0` → false-green, the #4873 class.

**Test Scenarios**:
1. **Empirically** (not by structural reading — #4873/#4876) verify the captured `RC` equals the smoke's real exit: run the stub at `exit 1` and `exit 3` through the *exact* `set +e; OUT="$(... )"; RC=$?; set -e` shape and assert `RC` is 1 and 3 respectively.
2. Adversarial variants that MUST be rejected in review: smoke invoked inside a pipe (`smoke | tee`) so `$?` reads `tee`; smoke under a job-level `set -eo pipefail` with no `set +e` guard; a YAML `if: ${{ success() }}` or `continue-on-error` that re-greens the step.
3. Confirm `2>&1` is captured so a `fail()` written to stderr still reaches the marker grep.

**Coverage Requirement**: The exit-code propagation is verified by execution, not by reading the YAML. No pipe between the smoke and `$?`; no `pipefail`-without-guard; no `continue-on-error`/`if:` that can re-green a non-zero RC.

### R-03: Early-exit-0, run-marker absent, passes green
**Severity**: High · **Likelihood**: Med · **Impact**: A future smoke edit that returns 0 before all gates run masquerades as success — the precise gap the positive run-marker exists to close.

**Test Scenarios**:
1. Stub prints partial output (e.g. up to "Gate 2 passed") then `exit 0`, no `ALL GATES PASSED` → **job red**.
2. Anchoring adversarial cases: marker appears as a substring of a longer log line, or echoed in a comment/diagnostic earlier in output, or with trailing text — confirm the grep (`grep -qx '\[783-smoke\] ALL GATES PASSED.*'` per ADR-003) matches the **terminal whole line** and not an incidental occurrence.
3. Confirm the marker string asserted by the gate is byte-identical to the smoke's emitted line (`[783-smoke] ALL GATES PASSED`), and that AC-05 did not move the marker off the true end of the script (see R-12).

**Coverage Requirement**: A partial-output-then-exit-0 fixture produces red; the marker match is line-anchored and pinned to the smoke's literal terminal line.

### R-04: AC-05 grew-signal flaky (un-retryable)
**Severity**: High · **Likelihood**: Med · **Impact**: A healthy per-slug write intermittently reports "did not grow" → false-fail with no retry escape (OQ-6) → release blocked on a phantom. Conversely a too-loose signal misses a real mis-route (the #783 symptom AC-05 exists to pin).

**Test Scenarios**:
1. **Signal-choice validation** (OQ-C / ADR-005): under the shipped DB config (WAL, autocheckpoint ~1000 pages, ADR #329), confirm the chosen "grew" measurement is reliably non-decreasing for one committed write. A single small write may sit in `-wal` and NOT enlarge the main `unimatrix.db` file until checkpoint — so a main-file-only size delta is the flaky form. Validate a WAL-inclusive signal (`du -s` over the per-slug store dir, or sum of `unimatrix.db` + `-wal` + `-shm`) is monotone across repeated runs.
2. Run the full smoke end-to-end **N times** (≥5) locally and confirm the grew-assertion passes every time — no intermittent "did not grow."
3. **Positive-control / negative-control**: confirm the assertion (a) passes when the write lands in the per-slug store, and (b) the hash-store-unchanged half would FAIL if the write were mis-routed to the hash dir (the literal #783 symptom). A grew-check that can't fail on a mis-route is theater.
4. Confirm the measurement uses the `vol()` busybox read-only sidecar, never `docker exec`.

**Coverage Requirement**: The grew-signal is WAL-robust (validated monotone over ≥5 runs), discriminating (fails on a hash-store mis-route), and sidecar-based. No flaky signal ships — because it cannot be retried away.

### R-05: arm64 cold-boot exceeds the boot deadline
**Severity**: High · **Likelihood**: Med · **Impact**: `smoke-arm64` false-fails a healthy cold boot → manifest blocked on a phantom, and (per OQ-6) no retry to paper over it. Over-widening masks a true hang.

**Test Scenarios**:
1. **Post-tag (AC-07, the only true proof)**: watch the first real `smoke-arm64` on `ubuntu-22.04-arm` to completion; record the actual cold first-boot-to-`HTTP transport active` wall time vs the 90s deadline and the margin.
2. **Pre-tag (`workflow_dispatch` dry-run)**: trigger the workflow manually to exercise the real arm64 hosted runner against `:latest-arm64` before cutting a release — the first cross-platform signal on R-05 without tagging (primary reason `workflow_dispatch` is in scope, ADR-004).
3. If the 90s budget is insufficient (NFR-07/OQ-B), widen it to a value that clears a healthy cold arm64 boot with margin **while still bounding a true hang** (a deadline, not removal) — and re-confirm a deliberately-hung boot still fails within bound.

**Coverage Requirement**: `smoke-arm64` passes on a healthy cold arm64 boot with recorded margin; the boot deadline still fails a true hang. Phrased "configured + verified locally; arm64 cold-boot margin confirmed post-tag/dispatch," never asserted pre-execution (#4796).

### R-06: ADR-004 independence regression
**Severity**: High · **Likelihood**: Med · **Impact**: An arm64/Docker flake on the smoke blocks a binary/npm release that has nothing to do with the container — the exact coupling ADR-004 (#4572) forbids.

**Test Scenarios**:
1. Trace the full `needs:` graph in `release.yml`: assert no `smoke-*` job appears in any `build-linux-*` / `package-npm` / `create-release` `needs`, and no binary/npm job appears in any `smoke-*` `needs`. `create-release` still `needs: package-npm` only.
2. **Mutation check**: confirm that forcing a smoke job to fail leaves `package-npm` and `create-release` reachable/unaffected (only the manifest is blocked) — verifiable by graph reasoning locally, observable post-tag.
3. **Regression guard for the future**: state the closed-set invariant explicitly (smoke jobs depend ONLY on container-branch jobs; the manifest is the single block point) so a later `needs:` edit that violates it is a flagged change, not a silent one.

**Coverage Requirement**: A `needs:`-graph assertion proves zero cross-branch edges and a single manifest block point; the invariant is documented for future edits.

### R-07: Pushed-bytes contract degrades to a rebuild
**Severity**: High · **Likelihood**: Low · **Impact**: The gate tests a rebuild, not the shipped artifact — definitionally misses the #783 first-run class.

**Test Scenarios**:
1. Assert each smoke job sets `IMAGE=ghcr.io/<owner>/unimatrix:<tag>-<arch>` and that `IMAGE=` set ⇒ the smoke skips its build branch (smoke lines 53–60).
2. Assert no `docker build`/`buildx build` of the production image appears in either smoke job; the smoke log shows "using prebuilt image: ghcr.io/...".
3. Confirm `docker/login-action@v3` precedes the pull so the GHCR read succeeds.

**Coverage Requirement**: Both smoke jobs run against the pushed per-arch tag via `IMAGE=`; no production build runs in a smoke job; the "using prebuilt image" log line is observed post-tag (AC-06).

### R-08: Manifest not actually gated
**Severity**: High · **Likelihood**: Low · **Impact**: A released multi-arch tag is un-smoked despite a green run.

**Test Scenarios**:
1. Assert `create-container-manifest.needs` includes BOTH `smoke-amd64` and `smoke-arm64` (FR-06).
2. Confirm neither smoke job carries `continue-on-error` / `if:` that lets the manifest proceed on a red smoke.
3. Post-tag: a deliberately-red smoke leaves the manifest step **skipped** (not run).
4. **Dispatch gating** — the manifest job carries `if: github.event_name != 'workflow_dispatch'` so a `workflow_dispatch` dry-run (which smokes `:latest-<arch>` and pushes no release manifest) does NOT run the manifest job and does NOT report a false-red manifest gate. Assert the dispatch run leaves the manifest job **skipped (green-skip)**, not failed.

**Coverage Requirement**: The manifest `needs` both smokes; a red smoke demonstrably skips the manifest (config-verified locally; behavior confirmed post-tag). On `workflow_dispatch` the manifest is gated off and skips cleanly rather than going falsely red.

### R-09: Tag-resolution mismatch (OCCURRED — now a pre-merge gate)
**Severity**: High · **Likelihood**: High · **Impact**: The smoke's resolved push tag must be byte-identical to the bytes build-container actually pushed; otherwise `docker pull` 404s on **every** release. **This defect actually occurred in the first design draft** (it is not hypothetical): the smoke resolved the version *stripped* (`${GITHUB_REF_NAME#v}` ⇒ `:1.2.3-<arch>`) while build-container's metadata-action pushes *un-stripped* (`pattern=v{{version}}-<arch>` ⇒ `:v1.2.3-<arch>`) — a guaranteed 404 that local Linux validation could not surface (no tag pushed) and that would have first shown up post-tag on a real release.

**Corrected contract**: push resolves **un-stripped** — `VERSION="${GITHUB_REF_NAME}"` ⇒ `:v<version>-<arch>` — byte-for-byte matching the `pattern=v{{version}}-<arch>` push pattern (ADR-002 #5187, ADR-004 #5188 integration surface).

**Test Scenarios** (the primary mitigation is now PRE-MERGE and static — no tag push needed):
1. **PRIMARY — bounded pre-merge static tag-parity assertion (provable before merge).** Assert the smoke's resolved push-tag *string* is **byte-identical** to the metadata-action push *pattern* (`v{{version}}-<arch>` ⇒ `:v<version>-<arch>`). Any divergence (a stray `#v` strip, a missing/extra `v`, a swapped suffix) is **RED at merge**, not discovered post-tag. This converts R-09 from a post-tag discovery into a pre-merge gate — the defect class that already bit the first draft cannot recur silently.
2. Confirm the per-arch suffix matches each runner (`smoke-amd64`→`-amd64`, `smoke-arm64`→`-arm64`) — no swapped suffix; covered by the same byte-identity assertion.
3. Dispatch trigger resolves `:latest-<arch>`; per R-08, the manifest job is gated OFF on dispatch (`if: github.event_name != 'workflow_dispatch'`) so the dispatch dry-run does not push a manifest nor falsely red the manifest gate.
4. A pull of an absent tag still surfaces a clear diagnostic (the smoke's `fail()`/non-zero → job `exit 1`), not a silent green — the post-tag backstop behind the pre-merge assertion.

**Coverage Requirement**: A **pre-merge** static assertion proves the smoke's resolved push tag (`:v<version>-<arch>`, un-stripped) is byte-identical to the metadata-action push pattern; it is RED at merge on any mismatch — no tag push required. Per-arch suffix correct; absent-tag pull fails loudly post-tag as the backstop.

### R-10: GHCR push not yet pullable
**Severity**: Med · **Likelihood**: Low · **Impact**: The smoke races push propagation and intermittently fails to pull — and no retry is allowed.

**Test Scenarios**:
1. Confirm ordering: `smoke-amd64 needs: [build-container-x64]`, `smoke-arm64 needs: [build-container-arm64]` — the push job completes before the smoke starts.
2. Post-tag: confirm the pull succeeds first-try on the real run; if a propagation race appears, it is **in-scope rework** (job ordering / pull-readiness check inside the contract), NOT `|| retry` (OQ-6).

**Coverage Requirement**: Smoke ordered strictly after its push; any pull race surfaced post-tag is fixed structurally, not retried.

### R-11: Trigger surface over/under-reach
**Severity**: Med · **Likelihood**: Low · **Impact**: No pre-tag dry-run (R-01/R-05 surface only at real tag), or PR-lane pollution with hard-failing un-Docker'd lanes.

**Test Scenarios**:
1. Assert `on:` includes `push.tags: ['v*']` AND `workflow_dispatch`, and EXCLUDES `pull_request` (FR-02, DECIDED OQ-5).
2. Confirm the smoke jobs do not run on `pull_request`.

**Coverage Requirement**: Exactly `{tag push, workflow_dispatch}` trigger the smoke; never `pull_request`.

### R-12: AC-05 hardening regresses the smoke / breaks the run-marker
**Severity**: Med · **Likelihood**: Low · **Impact**: The grew-assertion breaks the script the gate keys on, or moves/suppresses `ALL GATES PASSED`.

**Test Scenarios**:
1. After FR-05, run the smoke end-to-end and confirm it still emits the terminal `[783-smoke] ALL GATES PASSED` AS THE LAST gate line (the grew-assertion runs *before* the marker, not after it).
2. Confirm the change uses `vol()` busybox, adds one bounded assertion pair, introduces no new script, and re-implements nothing in YAML (NFR-08/C-12).

**Coverage Requirement**: Post-AC-05, the smoke still passes 3/3 locally and still emits the marker last; the change is bounded and sidecar-based.

### R-13: Inherited latent smoke bug
**Severity**: Med · **Likelihood**: Low · **Impact**: The gate inherits a latent #786 defect; arm64 is a never-run path for this smoke.

**Test Scenarios**:
1. Re-run the smoke 3/3 locally on amd64 post-AC-05 to confirm the baseline still holds.
2. Treat the first arm64 execution (dispatch dry-run, then tag) as discovery: any arm64-specific smoke assumption (path, ENV, model-load) failing is in-scope rework (AC-07).

**Coverage Requirement**: amd64 baseline re-confirmed; arm64 first-run watched as discovery, surprises treated as in-scope rework.

### R-14: Briefly-public un-smoked intermediates (accepted)
**Severity**: Low · **Likelihood**: High · **Impact**: An operator pulling a per-arch intermediate directly gets un-smoked bytes. **Accepted by design (NFR-09)** — operators pull the manifest, which is gated. No mitigation; documented so it is not re-litigated as a defect.

## Integration Risks

- **Smoke ↔ release.yml exit-code boundary (R-01/R-02/R-03).** The single most failure-prone interaction: a shell exit code crossing into a YAML job result. The #4873 lesson proves this boundary fails *silently and structurally-plausibly* — the capture must be tested by execution, never by reading.
- **Smoke ↔ GHCR pushed bytes (R-07/R-09/R-10).** Ordering (`needs:` push → smoke), tag resolution per trigger, and registry propagation all sit on this edge. A break here either tests the wrong artifact or fails opaquely. The smoke's resolved push tag MUST be byte-identical to build-container's metadata-action push pattern (un-stripped `:v<version>-<arch>`); the first draft's stripped form (`:1.2.3-<arch>`) would have 404'd every release. A bounded pre-merge static tag-parity assertion now pins this byte-identity RED at merge (R-09) — the parity break is a flagged change, not a post-tag surprise.
- **smoke-* ↔ create-container-manifest (R-08).** The gating edge. Must include both smokes and admit no `continue-on-error`/`if:` bypass.
- **container branch ↔ binary/npm branch (R-06).** The edge that MUST NOT exist. A closed-set invariant: smoke jobs depend only on container-branch jobs.
- **AC-05 assertion ↔ smoke terminal marker (R-12).** The grew-assertion must sit *before* `ALL GATES PASSED`, or it silently breaks the run-marker the gate keys on.

## Edge Cases

- Smoke output empty / truncated (runner OOM-killed) → RC non-zero or marker absent → red (R-01/R-02).
- `ALL GATES PASSED` appears mid-output as a substring or echoed diagnostic → anchored `grep -qx` must not match it (R-03).
- One-page write fully absorbed by WAL with no main-file growth before checkpoint → WAL-inclusive signal still grows (R-04).
- arm64 cold boot at exactly the deadline boundary (±a few seconds) → flaky pass/fail; deadline must have margin (R-05).
- `workflow_dispatch` on a non-default branch smokes `:latest-<arch>` of that branch's build, not a historical version (architecture OQ note) — documented, out of scope.
- Absent pushed tag (build job partially failed) → pull fails → `fail()` → red with diagnostic, not green (R-09).
- Unexpected exit code (`139` segfault, `137` OOM-kill, `124` timeout) → `*)` case → red (R-01).

## Security Risks

- **Untrusted input surface is minimal.** The smoke accepts no external/attacker-controlled input at gate time: `IMAGE=` is a repo-controlled GHCR tag; the bearer token + TLS cert are read from the *running container's own data volume* via a read-only busybox sidecar (never `docker exec` into distroless). No new secret is introduced (NFR-04) — only `GITHUB_TOKEN` with the existing `packages: write` scope for GHCR read.
- **Blast radius.** A compromised smoke runner has GHCR read (pull) via `GITHUB_TOKEN`; it does NOT gain push/write beyond what the build jobs already hold, and it runs on the container branch only — it cannot reach the binary/npm release path (ADR-004 / R-06). The busybox sidecar mounts the volume **read-only** (`:ro`), so volume inspection cannot tamper with the artifact under test.
- **Briefly-public intermediates (R-14).** The per-arch `:<tag>-<arch>` tags are public before their smokes run. This is an exposure-window, not an injection vector — operators are directed to the gated manifest. Accepted (NFR-09).
- **No path-traversal / injection / deserialization surface** is added: the change is YAML topology + a bounded size-delta assertion. The one caution: the run-marker `grep` must be anchored so output content cannot spoof the success marker (R-03) — output is from the repo's own smoke, but anchoring removes the spoofing class entirely.

## Failure Modes

| Condition | Expected gate behavior |
|-----------|------------------------|
| Docker absent (`exit 3`) | Job **red**, "smoke SKIPPED — mis-provisioned, hard failure" diagnostic; manifest blocked (NFR-02). Never green/deferred. |
| Shipped image first-run broken (`exit 1`) | Job **red**, "first-run path broken" diagnostic; manifest blocked (AC-02). |
| Early-`exit 0`, marker absent | Job **red**, "exited 0 but never printed ALL GATES PASSED" diagnostic; manifest blocked (AC-03). |
| Mis-routed write (#783 class) | AC-05 grew/hash-unchanged assertion `fail()` → `exit 1` → red; manifest blocked (guards N3). |
| Wrong/absent pushed tag | Caught **pre-merge** by the static tag-parity assertion (byte-identity to the un-stripped `:v<version>-<arch>` push pattern) → RED at merge before any tag (R-09). Post-tag backstop: pull fails → `fail()`/non-zero → red with diagnostic; manifest blocked. |
| arm64 runner / Docker flake | Job red, manifest blocked; **binary/npm release UNAFFECTED** (ADR-004); treated as in-scope rework, NOT retried (R-06/R-10, OQ-6). |
| arm64 cold-boot slow but healthy | Must PASS within a margin-bearing deadline; a false-fail here is a defect to fix by widening (bounded), not by retry (R-05). |

The gate **never** degrades to green on uncertainty: every failure mode above is loud, diagnosable, and blocks only the manifest. No `|| retry`, no `continue-on-error`, no silent skip.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (false-green via self-skip / early-exit-0) | R-01, R-02, R-03 | The gate's spine. Unit-tested truth table (R-01), empirically-verified RC propagation against the #4873 class (R-02), anchored run-marker assertion (R-03). ADR-003 contract. |
| SR-02 (pushed-bytes-not-rebuild) | R-07 | `IMAGE=` pushed per-arch tag, no production build in smoke job, "using prebuilt image" log asserted (AC-06). |
| SR-03 (ONNX build/runtime cost & arm64 first-boot) | R-05, R-10 | Cold-boot deadline margin validated post-dispatch/post-tag (R-05); reuse-pushed-bytes removes rebuild cost; pull ordered after push (R-10). |
| SR-04 (arm64 runner flake / push-ordering / Docker drift) | R-06, R-09, R-10, R-13 | No retry — flake is signal (OQ-6); strict `needs:` ordering; Docker-absent → hard fail; first arm64 run watched as discovery. Tag-parity (R-09) is now a pre-merge static assertion pinning the smoke's un-stripped `:v<version>-<arch>` byte-identical to the push pattern — RED at merge, no tag push needed. |
| SR-05 (AC-05 scope-creep / distroless inspection) | R-12 | Bounded assertion pair via `vol()` busybox sidecar, marker preserved last, no new script (NFR-08). |
| SR-06 (trigger surface over/under-reach) | R-11 | Exactly `{tag push, workflow_dispatch}`; `pull_request` excluded (DECIDED OQ-5). |
| SR-07 (arch coverage silently capped) | R-05, R-08 | Both `smoke-amd64` + `smoke-arm64` exist and both gate the manifest (NFR-06 HARD RULE); any deferral is named/tracked, never buried. |
| SR-08 (ADR-004 independence violation) | R-06 | `needs:`-graph assertion: zero cross-branch edges; single manifest block point; invariant documented for future edits. |
| SR-09 (briefly-public un-smoked intermediates) | R-14 | Accepted by design (NFR-09); documented as exposure-window, not a defect; operators pull the gated manifest. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 6 (R-01..R-06) | 17 |
| High | 3 (R-07, R-08, R-09) | 11 |
| Medium | 4 (R-10, R-11, R-12, R-13) | 8 |
| Low | 1 (R-14) | 0 (accepted, documented) |

> **Validation phasing (the gate cannot be proven by local Linux validation — #4796):**
> Critical gate-logic risks R-01/R-02/R-03 ARE fully provable locally via stub-smoke unit
> tests and MUST be before merge. R-04 (grew-signal monotonicity) is provable locally via
> repeated full-smoke runs. R-09 tag-parity is now **fully provable pre-merge** via the
> bounded static byte-identity assertion (un-stripped `:v<version>-<arch>` vs the push
> pattern) — RED at merge, no tag push needed; the pull-failure path is a post-tag backstop.
> R-05 (arm64 cold-boot), R-08 behavior, R-10 are
> "configured + verified locally; GH execution confirmed post-dispatch/post-tag" — never
> asserted as executed fact before the first real hosted run (AC-07). The `workflow_dispatch`
> dry-run is the primary pre-tag cross-platform proof.

## Knowledge Stewardship
- Queried: context_search for false-green/verify-by-name + risk patterns -- surfaced #5180 (verify-by-name pattern, this gate's spine), ADR-003 #5183, #4873 (setsid swallows RC — structurally-plausible false-green, elevated R-02), #4796 (CI-dependent ACs not assertable pre-merge), #329 (WAL auto-checkpoint — main DB size not monotone on a single write, elevated R-04).
- Stored: nothing novel to store -- the verify-by-name self-skip pattern (#5180) and the exit-code-swallow false-green trap (#4873) already capture the cross-feature patterns; nan-019's risks are feature-specific and live in this document.
