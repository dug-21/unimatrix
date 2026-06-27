# Risk Coverage Report: infra-004

> Stage 3c — Test Execution. Test/CI-only feature (no `crates/` change). DoD:
> **a cross-tenant leak cannot ship a release.** Dominant failure class:
> silently-vacuous enforcement (blocking yet never RED, never GREEN).
> All pre-merge logic proven by EXECUTING the shipped bytes through the off-Docker
> stub seam + static YAML/`needs:`-graph assertions. Cold-model determinism and
> `:v<ver>` tag resolution are CI-only operational evidence (see Carve-Out).

## Test Results

### Shell logic / static suites (the pre-merge contract — load-bearing)

| Suite | Component(s) | Expected | Passed | Failed | Exit |
|-------|-------------|----------|--------|--------|------|
| `release-gate-isolation-logic-test.sh` | C-WB warmup barrier | 39 | 39 | 0 | 0 |
| `release-gate-tristate-logic-test.sh` | C-TS `run_smoke_gate_tristate` truth table | 19 | 19 | 0 | 0 |
| `release-gate-logic-test.sh` | C-TS sibling no-regression (`run_smoke_gate`) | 15 | 15 | 0 | 0 |
| `release-gate-isolation-lane-static-test.sh` | C-LN / C-FLIP YAML + `needs:`-graph | 13 | 13 | 0 | 0 |
| **Total** | | **86** | **86** | **0** | |

All counts match the planned expected totals exactly. Each suite sources the REAL
shipped bytes (`multi-tenant-isolation-smoke.sh`, `release-gate-lib.sh`) or parses
the shipped `.github/workflows/release.yml`; RC fidelity proven by execution, never
by reading YAML (#4873/#5192/#5345 class).

### Integration smoke (mandatory no-regression gate)

- Command: `python -m pytest suites/ -m smoke --timeout=60` (binary
  `target/release/unimatrix`, ORT `/usr/local/lib/libonnxruntime.so`)
- Total: 24 selected (601 deselected)
- Passed: 24
- Failed: 0
- xfail / skip introduced: 0
- Pure no-regression check — this feature changes no MCP surface; the three-file
  shell/YAML change did not perturb the server binary or harness. No new pytest
  tests planned or needed (per OVERVIEW §4: pure CI/shell logic, no MCP-visible
  effect → shell stub-seam suffices).

### Unit tests (cargo)

Not applicable — no `crates/` change (C-1 / AC-15). `cargo build --release -p
unimatrix-server` was run only to produce the binary the mandatory smoke gate
exercises (cached, exit 0). No Rust source changed, so `cargo test --workspace`
is not a gate for this feature.

## Coverage Summary (all 15 risks)

| Risk ID | Description (abbrev) | Pri | Test(s) | Result | Coverage |
|---------|---------------------|-----|---------|--------|----------|
| **R-01** | Ceremonial warmup barrier / false-pass | **Crit** | `test_warmup_present_requires_durable_read_roundtrip` (read-fail→INFRA), `test_warmup_result_is_consumed` (WTB consumed in a gating CASE), `test_warmup_uses_write_then_barrier_not_store_size`, `test_warmup_present_proceeds_to_matrix`; cold-path zero-flap = AC-11 (CI) | PASS (pre-merge); cold-path PENDING-operational | Full pre-merge; cold leg via AC-11 |
| **R-05** | Swallowed-exit-code false-green | **Crit** | `test_tristate_rc_survives_capture` (1→1,2→2,3→3), `test_tristate_no_pipe_static_return_not_exit`, `test_tristate_captures_stderr[_fail]`, `test_tristate_only_exit2_nonblocking` + full truth table | PASS | Full |
| R-03 | #767 bound under-covers readiness | High | `test_assert_routes_live_precedes_barrier` (routes<warmup<matrix), `test_warmup_bound_default_documented` (180 = #767 derivation); cold headroom = AC-11 (CI) | PASS (pre-merge); headroom PENDING-operational | Full pre-merge; empirical headroom via AC-11 |
| R-06 | Anchored run-marker invariant break | High | `test_tristate_marker_anchored_substring`, `test_tristate_marker_whole_line_anywhere_is_green`, `test_tristate_marker_byte_identical` | PASS | Full |
| R-08 | Fail-closed / blast-radius inversion | High | `test_tristate_only_exit2_nonblocking`, full tri-state truth table, `test_lane_in_manifest_needs` (`needs:`-graph) | PASS | Full |
| R-09 | Pull-404 / wrong-tag → visible-INFRA | High | `test_tristate_infra_exit2_nonblocking_visible`, `test_tristate_infra_exit2_canonical_marker_pinned`, `test_lane_no_ref_strip`, `test_lane_calls_resolve_image` | PASS | Full pre-merge |
| R-10 | Never-green-on-a-tag (tag-push unproven pre-merge) | High | AC-11 scoped to dispatch only; budgeted post-merge tag round (C-10) | PENDING-operational | By design (CI/post-merge) |
| R-13 | AC-11 cold-model proof ceremonial (warm cache) | High | AC-11 log must show real first-boot HF download, not warm cache / `:783-smoke` | PENDING-operational | CI-only |
| R-02 | Warmup-marker collision → false RED | Med | `test_warmup_marker_non_substring_asserted`, `test_warmup_row_inert_to_negatives` | PASS | Full |
| R-07 | Sibling-lane regression (shared lib) | Med | `release-gate-logic-test.sh` (15/15 byte-identical), `test_run_smoke_gate_sibling_unchanged_exit4`; `git diff` adds new fn only | PASS | Full |
| R-14 | Verification harness false-green (`set -e` re-enable) | Med | Suites print final summary line (`N passed, 0 failed`) as completeness witness; intentionally-RED rows run without aborting (86/86 + 24 smoke executed) | PASS | Full |
| R-04 | Cold HF download variance | Med | `test_warmup_timeout_is_infra_not_pass` (timeout→INFRA exit 2, never RED/GREEN, diagnostic logged) | PASS (timeout→INFRA); residual accepted | Full (residual documented) |
| R-11 | Stale-image proof (main drift) | Med | branch-point == `main` HEAD recorded at AC-11 run time (SR-06) | PENDING-operational | CI-only |
| R-12 | Dispatch-from-branch GHCR write strands Step 3 | Med | verify `:latest-amd64` push from branch early; two-step fallback specified | PENDING-operational | CI-only |
| R-15 | Chronic-INFRA = human-vigilance only | Med | `test_tristate_infra_exit2_canonical_marker_pinned` (stable greppable marker); human ACCEPT-or-escalate of the VARIANCE (OQ-3) gates N3 `proven` | PASS (marker stability); human gate PENDING | Marker proven; residual is a human decision |

Critical Gate 3c blockers R-01 and R-05 are both PASS on the pre-merge contract.
R-01's cold-path leg (zero warmup-attributable INFRA flap on a real cold model) is
provable only by the AC-11 dispatch run (carve-out below) — its pre-merge,
load-bearing (non-ceremonial) construction is fully proven here.

## Gaps

No pre-merge risk is uncovered. The five PENDING-operational items (R-10, R-11,
R-13, plus the cold-path legs of R-01/R-03/AC-04/AC-11, and R-12) are CI-only by
design (require a real `workflow_dispatch` cold-model run + GHCR push) and are
carved out below — they are operational evidence the leader gathers before merge,
not pre-merge unit gaps. R-15's chronic-INFRA residual is a human ACCEPT-or-escalate
decision (VARIANCE / OQ-3) that gates the N3 `proven` claim (AC-14) — not a tester
assertion.

## CI-Only Carve-Out (NOT unit-testable here — operational evidence)

Per OVERVIEW §5 and brief C-10/C-11. These require a real CI dispatch run and are
recorded PENDING-operational for the leader to gather before merge:

| AC / Risk | Why CI-only | Required operational evidence |
|-----------|-------------|-------------------------------|
| **AC-11** cold-model dispatch GREEN | Needs fresh cold build + GHCR pull + real HF download on a runner | Dispatch run URL + log: GREEN verdict; real first-boot HF download lines (not warm cache / not `:783-smoke`, R-13); branch-point == `main` HEAD recorded (R-11); zero warmup-attributable INFRA flap (R-01) |
| **AC-04** deterministic GREEN on cold container | "Proven by AC-11" — same dispatch run | Derived from AC-11 evidence; COVERED-BY-AC-11 |
| (R-10) `:v<ver>-amd64` tag-push resolution | First runs on a real tag only post-merge | Budget one post-merge tag round (C-10); tag-path INFRA degrades to non-blocking (safe); the only blocking first-tag path (a harness step) is already exercised by AC-11 |
| (R-12) dispatch-from-branch GHCR write | Token/branch push capability | Confirm `:latest-amd64` push from `feature/infra-004` before Step 3; two-step fallback ready |

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_assert_routes_live_precedes_barrier` (routes<warmup<matrix ordering), `test_warmup_bound_default_documented` (default 180 = #767 `READY_TIMEOUT_SECS` derivation); barrier sits between `assert_routes_live` and `run_isolation_matrix` in the diff |
| AC-02 | PASS | `test_warmup_uses_write_then_barrier_not_store_size` — reuses `write_then_barrier`, no new readiness mechanism |
| AC-03 | PASS | `test_warmup_timeout_is_infra_not_pass` — timeout → INFRA (exit 2), not RED, not proceed, diagnostic logged |
| AC-04 | PENDING-operational | COVERED-BY-AC-11 (cold-model dispatch run). Pre-merge: PRESENT proven load-bearing (real durable read round-trip) by `test_warmup_present_requires_durable_read_roundtrip` |
| AC-05 | PASS | `test_post_barrier_{green,red,infra}_still_drives` — full verdict truth table still drives off-Docker post-barrier |
| AC-06 | PASS | `test_lane_job_exists`, `test_lane_needs_build_container_x64`, `test_workflow_triggers_tags_and_dispatch`, `test_lane_no_if_guard` |
| AC-07 | PASS | `test_lane_calls_resolve_image`, `test_lane_exports_image`, `test_lane_no_docker_build`, `test_lane_no_ref_strip` (no `${GITHUB_REF_NAME#v}`), `test_lane_invokes_tristate`, `test_lane_not_plain_run_smoke_gate` |
| AC-08 | PASS | tri-state truth table through the real sourced lib: 0+marker/0-no-marker/1/2/3/other; `test_tristate_rc_survives_capture`, `test_tristate_no_pipe_static_return_not_exit`, `test_tristate_only_exit2_nonblocking` (CRITICAL R-05) |
| AC-09 | PASS | `test_tristate_marker_anchored_substring` (substring NOT credited), `test_tristate_marker_whole_line_anywhere_is_green`, `test_tristate_marker_byte_identical` (runtime line) |
| AC-10 | PASS | `test_lane_provisions_node`, `test_lane_provisions_sqlite3` (self-contained step) |
| AC-11 | PENDING-operational | CI dispatch run required — see Carve-Out. Cannot be unit-tested here |
| AC-12 | PASS | `test_lane_in_manifest_needs` (`needs:`-graph: lane ∈ `create-container-manifest.needs`) + forced-RED tri-state cell returns 1 → gates the edge (CRITICAL R-05/R-08) |
| AC-13 | PASS | `test_tristate_infra_exit2_nonblocking_visible` + `test_tristate_infra_exit2_canonical_marker_pinned` — INFRA returns success (non-blocking) with `::warning::` + pinned canonical marker `[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN` |
| AC-14 | PENDING (delivery + human gate) | N3 (#5161) `status: proven` set by delivery post-merge via `context_correct`, gated on the human VARIANCE decision (OQ-3 / R-15). Not a tester assertion |
| AC-15 | PASS | `git diff --name-only main...HEAD`: 0 `crates/` paths; production diff = exactly `multi-tenant-isolation-smoke.sh`, `release-gate-lib.sh`, `.github/workflows/release.yml` (+ feature docs); smoke-script diff is warmup-barrier-scoped only |

Pre-merge PASS: AC-01, AC-02, AC-03, AC-05, AC-06, AC-07, AC-08, AC-09, AC-10,
AC-12, AC-13, AC-15 (12). CI-only operational: AC-04, AC-11. Delivery + human
gate: AC-14.

## GH Issues Filed

None. No integration test failed; no pre-existing/unrelated failure surfaced; no
`xfail` markers added.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #5192 (sourceable shell-gate
  verify-by-name + capture invariants), #5350 (ADR-002 tri-state additive fn),
  #5349 (ADR-001 warmup barrier), #5354/#5335 (infra-003 isolation gate-logic
  collision pattern), #840 (USAGE-PROTOCOL). All applied directly to suite execution
  and risk mapping.
- Stored: nothing novel — the patterns exercised (release-gate false-green capture,
  ceremonial seam, never-green-on-tag, runtime-marker anchor) are already captured
  as #5192/#5345/#5267/#4974/#5354; this feature instantiates them. The four-suite
  cumulative shell stub-seam execution pattern (39+19+15+13) is the established
  `release-gate-*-logic-test.sh` convention, not a new technique.
