## ADR-005 nan-021: Docker Acquisition + False-Green Discriminator — Reuse nan-019's Contract Verbatim, Release-Gate Lane via workflow_dispatch/tag

### Context
AC-05 requires the fixture be a standing gate in the release-gate Docker lane via `workflow_dispatch`/tag
(D-3), mirroring nan-019 — NOT a per-PR Docker lane (promotable later). A green run must PROVABLY mean the
fixture actually ran end-to-end over the live bridge: skip-when-Docker-absent is a hard failure, and a
positive terminal run-marker is asserted (reuse the nan-019 `release-gate-lib.sh` verify-by-name
contract). No early-exit-0 or environment skip can masquerade as parity-proven.

SR-03 (High/Med) is nan-019's exact trap, doubled: (a) `docker image inspect` does NOT pull — a
cross-runner cache miss false-FAILED the gate (#5208); (b) inversely, Docker absent → an early-exit-0
could masquerade as parity-proven. Both re-create the false-green class (#4796/#4970). The standing rule:
GH `ci.yml` is JS-client-only; Rust/container validation lives in the release workflow + protocol gates —
so the home is `workflow_dispatch`/tag, not `pull_request`.

The nan-019 contract is already SHIPPED and battle-tested (#5183 ADR-003 verify-by-name; #5208 the
acquisition fix; #5258 the stub-drive pattern). Re-authoring it would re-introduce the bugs it already
fixed. The Risk Assessment is explicit: reuse the acquisition + exit-code discriminator VERBATIM, extend,
do not re-author. A release-only lane reached for the first time has no green baseline (#5266/#5267) —
budget multiple tag rounds to first-green, surfacing failures in sequence.

### Decision
**Reuse the nan-019 acquisition + verify-by-name contract verbatim; the new cycle/parity gates extend the
smoke behind its sourceable guard; release-gate lane via `workflow_dispatch`/tag.**

1. **Image acquisition — verbatim (#5208).** The IMAGE= branch uses the SHIPPED conditional:
   `docker pull "$IMAGE" || docker image inspect "$IMAGE" >/dev/null 2>&1 || { echo "could not acquire
   $IMAGE"; exit 4; }` — pull first (cross-runner), fall back to local inspect (same-runner build),
   exit 4 (DISTINCT code) only if both fail. nan-021 adds NO new acquisition path.

2. **Verify-by-name run-marker — verbatim (#5183).** Green iff `rc==0` AND the anchored terminal marker
   captured: `grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'`. The smoke prints its marker as the
   LAST statement; it cannot print on rc 1/3/4 or an early exit 0. `run_smoke_gate IMAGE SMOKE_CMD...`
   manages `set -e` so the rc survives the capture; the YAML must not swallow it with
   `if:`/`continue-on-error`. Exit-code truth table reused: `0`=passed, `3`=skipped (Docker absent →
   HARD FAIL the gate), `4`=image unacquirable, `1`=shipped-image-path broken, `*`=unexpected.

3. **New gates extend, behind the sourceable guard (#5258, AC-07).** The new cloud-cycle + parity gates
   are a single sourceable function (e.g. `cloud_cycle_gates`) defined alongside the existing helpers,
   ABOVE the `BASH_SOURCE`-guard line, with env-injectable seams (the `SMOKE_*_CMD` convention) so the
   pre-merge gate-logic test can `source` the REAL script and drive the new gates against stubs WITHOUT a
   container (Docker-bound Gates 1–7 never run in the unit test). This is how nan-021 is pre-merge-provable
   despite living in a release-only lane.

4. **Non-skip is structurally proven (Risk Rec 1, #4977).** A green run asserts NON-SKIP: run-time delta
   present, absence of skip/"skipping" log lines, AND the positive terminal marker — not just exit 0.
   The parity gate additionally asserts the cycle observations EXIST and are DERIVED (ADR-004) and the
   `MetricVector` is NON-EMPTY (ADR-003) — a believable `0` cannot green.

5. **Capture-first child stderr (#5266/#5267).** Every child's stderr → hermetic `$SANDBOX` file,
   tail-dumped on failure only (the `emit_bundle` blob stays suppressed — it carries the bearer). The
   first real tag-lane run will likely fail; capture-first makes it diagnosable in one round. Budget N
   tag rounds to first-green for this never-before-green lane.

6. **Lane placement (D-3).** Wire into the release workflow's Docker smoke job(s) via `workflow_dispatch`
   + tag, mirroring nan-019. Guard the publish/linux jobs against `workflow_dispatch` dry-runs (the
   nan-019 Finding-2 fix #5208) so a sanctioned dry-run runs only build-container-* + smoke-*. NOT
   `pull_request` (promotable later).

### Consequences
- **Easier:** zero new false-green surface — the acquisition, exit-code discrimination, and run-marker are
  the already-proven nan-019 bytes (#5183/#5208), so the documented traps cannot recur. The sourceable
  guard makes the new gates pre-merge-provable against stubs (#5258), so the release-only lane isn't the
  first place the gate logic runs. Capture-first + budgeted tag rounds make first-green tractable.
- **Harder:** the gate is coupled to the literal marker string and the exit-code truth table — a smoke
  edit that changes them fails loudly (intended). The release-only lane means live end-to-end runs only
  on a tag/dispatch, so the bridge/cert/SSE path is first truly exercised post-merge — budget the
  round-trip (#5267). The new gates must keep the `SMOKE_*_CMD` seams injectable (including any new store
  sampler) or the delta assertion is not stub-drivable (#5258 gotcha 2).

Related: D-3, SR-01, SR-03; AC-05, AC-07. Reuses #5183 (ADR-003 verify-by-name), #5208 (pull-or-inspect-
or-exit-4 + dispatch-guard), #5258 (stub-drive sourceable-gate + hermeticity negative control),
#5266/#5267 (capture-first, never-green-first-run), #4977/#5180 (self-skip → hard-fail). Pairs with
ADR-002 (the readiness gates + capture this lane runs) and ADR-003/ADR-004 (the non-empty/derived
assertions that make non-skip provable).
