# Risk Coverage Report: infra-004

> Stage 3c — Test Execution. Test/CI-only feature (no `crates/` change). DoD:
> **a cross-tenant leak cannot ship a release.** Dominant failure class:
> silently-vacuous enforcement (blocking yet never RED, never GREEN).
> All pre-merge logic proven by EXECUTING the shipped bytes through the off-Docker
> stub seam + static YAML/`needs:`-graph assertions. Cold-model determinism is now
> proven GREEN by the AC-11 dispatch run (28298217877); only the `:v<ver>` tag-push
> leg remains as a budgeted post-merge tag round (see CI Operational Evidence).

## Test Results

### Shell logic / static suites (the pre-merge contract — load-bearing)

| Suite | Component(s) | Expected | Passed | Failed | Exit |
|-------|-------------|----------|--------|--------|------|
| `release-gate-isolation-logic-test.sh` | C-WB warmup barrier (+ #859 two-filter marker safety) | 44 | 44 | 0 | 0 |
| `release-gate-tristate-logic-test.sh` | C-TS `run_smoke_gate_tristate` truth table | 19 | 19 | 0 | 0 |
| `release-gate-logic-test.sh` | C-TS sibling no-regression (`run_smoke_gate`) | 15 | 15 | 0 | 0 |
| `release-gate-isolation-lane-static-test.sh` | C-LN / C-FLIP YAML + `needs:`-graph | 13 | 13 | 0 | 0 |
| **Total** | | **91** | **91** | **0** | |

All counts match the expected totals exactly. The isolation-logic suite grew across
the #859 fold-in arc: **39 → 43** (four adversarial **(c) nonce-PII-safety** cases,
commit `511ba824`), then **43 → 44** (one **symmetric feature-id canary** case in the
complete marker+warmup fix, commit `543e8d08`); the other three suites are unchanged.
Each suite sources the REAL shipped bytes (`multi-tenant-isolation-smoke.sh`,
`release-gate-lib.sh`) or parses the shipped `.github/workflows/release.yml`; RC
fidelity proven by execution, never by reading YAML (#4873/#5192/#5345 class).

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

### Unit tests (cargo) — one test-only scanner anchor (#859 fold-in)

- Command: `cargo test -p unimatrix-server --lib test_scan_isolation_gate_golden_markers_pass`
- Total: 1
- Passed: 1
- Failed: 0
- The anchor feeds the SHARED golden derived-marker set through the REAL production
  `ContentScanner::global().scan()` and asserts `Ok`. It pins the **PII half** of the
  marker contract — the only check that cannot drift from the scanner source of truth
  (`scanning.rs:300-309` PhoneNumber/SSN), pairing with the bash-side charset-reduced
  ERE canary and the off-Docker (c) battery (which share the same golden list,
  cross-verified by `test_c_golden_markers_match_rust_anchor`).
- The **feature-id half** of the contract (`looks_like_feature_id`, `uds/listener.rs`)
  is **module-private** and so cannot be exercised by a cross-crate test; it is pinned
  **off-Docker** instead by the bash `assert_marker_feature_id_shaped` self-check plus
  the fixture golden oracle that mirrors the filter's rule (≥1 all-digit hyphen-segment
  AND ≥1 alpha segment). Together the Rust scanner anchor (PII) + the bash feature-id
  oracle pin both server filters the marker must satisfy.

**AC-15 amendment:** "no `crates/` **production** change; one **test-only** scanner
anchor added." The sole `crates/` delta in commit `511ba824` is
`test_scan_isolation_gate_golden_markers_pass` inside `#[cfg(test)] mod tests` in
`crates/unimatrix-server/src/infra/scanning.rs` — no production code, no scanner
pattern change. `cargo build --release -p unimatrix-server` was also run to produce
the binary the mandatory smoke gate exercises (cached, exit 0).

### Warmup readiness signal change (#859 fix `543e8d08`)

The warmup barrier probe switched from an **observe-durability** round trip to the
**MCP `context_store` → read-back round trip** (`multi-tenant-isolation-smoke.sh`
`warmup_barrier`, lines ~434-477; `WARMUP_DEADLINE_SECS=180` unchanged). Rationale
(per `agents/infra-004-warmup-timing-investigator-report.md`): the observe path has
**zero embedding dependency** (fire-and-forget SQL insert, 204 before durable) — so
observe-durability never proved "embedding model warm," the barrier's stated purpose.
The MCP `context_store` write is the only served path that exercises the embedding
model, so it is the correct readiness signal and warms the exact surface the
load-bearing matrix MCP writes use. The change keeps the barrier load-bearing for
R-01 and makes a healthy cold run deterministically GREEN.

**`WARMUP_DEADLINE_SECS=180` justified by measurement** (not guessed): cold model
load measured **~3-4s**, cold MCP round-trip ready **~5s** from boot; #767's
retry/backoff floor for a throttled CI HF link is **~70s** (10+20+40); 180s is
~2.5x that floor and ~36x measured cold-ready — comfortable margin, env-overridable.

## Coverage Summary (15 Phase-2 risks + 1 additional #859-traceable class)

| Risk ID | Description (abbrev) | Pri | Test(s) | Result | Coverage |
|---------|---------------------|-----|---------|--------|----------|
| **R-01** | Ceremonial warmup barrier / false-pass | **Crit** | `test_warmup_present_requires_durable_read_roundtrip` (read-fail→INFRA), `test_warmup_result_is_consumed` (WTB consumed in a gating CASE), `test_warmup_uses_write_then_barrier_not_store_size`, `test_warmup_present_proceeds_to_matrix`; cold-path zero-flap proven by AC-11 (run 28298217877, GREEN, zero INFRA) | **PASS** (pre-merge + cold-path) | Full |
| **R-05** | Swallowed-exit-code false-green | **Crit** | `test_tristate_rc_survives_capture` (1→1,2→2,3→3), `test_tristate_no_pipe_static_return_not_exit`, `test_tristate_captures_stderr[_fail]`, `test_tristate_only_exit2_nonblocking` + full truth table | PASS | Full |
| R-03 | #767 bound under-covers readiness | High | `test_assert_routes_live_precedes_barrier` (routes<warmup<matrix), `test_warmup_bound_default_documented` (180 = #767 derivation); cold headroom proven by AC-11 (GREEN well within 180s; measured cold-ready ~5s) | **PASS** (pre-merge + empirical headroom) | Full |
| R-06 | Anchored run-marker invariant break | High | `test_tristate_marker_anchored_substring`, `test_tristate_marker_whole_line_anywhere_is_green`, `test_tristate_marker_byte_identical` | PASS | Full |
| R-08 | Fail-closed / blast-radius inversion | High | `test_tristate_only_exit2_nonblocking`, full tri-state truth table, `test_lane_in_manifest_needs` (`needs:`-graph) | PASS | Full |
| R-09 | Pull-404 / wrong-tag → visible-INFRA | High | `test_tristate_infra_exit2_nonblocking_visible`, `test_tristate_infra_exit2_canonical_marker_pinned`, `test_lane_no_ref_strip`, `test_lane_calls_resolve_image` | PASS | Full pre-merge |
| R-10 | Never-green-on-a-tag (tag-push unproven pre-merge) | High | AC-11 (run 28298217877) proves the dispatch path GREEN; budgeted post-merge tag round (C-10) for the `:v<ver>` tag-push leg | PASS (dispatch); tag-push = budgeted post-merge | By design (post-merge tag round) |
| R-13 | AC-11 cold-model proof ceremonial (warm cache) | High | AC-11 run was a genuine cold first-boot (not warm cache / not `:783-smoke`), job GREEN | **PASS** | Full (cold path proven) |
| R-02 | Warmup-marker collision → false RED | Med | `test_warmup_marker_non_substring_asserted`, `test_warmup_row_inert_to_negatives` | PASS | Full |
| R-07 | Sibling-lane regression (shared lib) | Med | `release-gate-logic-test.sh` (15/15 byte-identical), `test_run_smoke_gate_sibling_unchanged_exit4`; `git diff` adds new fn only | PASS | Full |
| R-14 | Verification harness false-green (`set -e` re-enable) | Med | Suites print final summary line (`N passed, 0 failed`) as completeness witness; intentionally-RED rows run without aborting (90/90 + 24 smoke + 1 Rust anchor executed) | PASS | Full |
| R-04 | Cold HF download variance | Med | `test_warmup_timeout_is_infra_not_pass` (timeout→INFRA exit 2, never RED/GREEN, diagnostic logged) | PASS (timeout→INFRA); residual accepted | Full (residual documented) |
| R-11 | Stale-image proof (main drift) | Med | AC-11 dispatch built from `feature/infra-004` tip (rebased on `main`), SR-06 | **PASS** | Full |
| R-12 | Dispatch-from-branch GHCR write strands Step 3 | Med | AC-11 dispatch ran successfully from the feature branch (`:latest-amd64` pulled), so the GHCR-write-from-branch capability is confirmed | **PASS** | Full |
| R-15 | Chronic-INFRA = human-vigilance only | Med | `test_tristate_infra_exit2_canonical_marker_pinned` (stable greppable marker); human ACCEPT-or-escalate of the VARIANCE (OQ-3) gates N3 `proven` | PASS (marker stability); human gate PENDING | Marker proven; residual is a human decision |
| **R-MPII (#859)** | **Two-filter marker contract** — the marker must satisfy TWO conflicting server filters at once: (1) the production `ContentScanner` PII patterns on the MCP `context_store` **write** leg (rejects phone/SSN-shaped digit runs → -32006 → INFRA), AND (2) `looks_like_feature_id` (`uds/listener.rs`) on the **observe** persistence leg (requires ≥1 all-digit hyphen-segment AND ≥1 alpha segment, else `topic_signal` is silently dropped to NULL → read-back never finds it → INFRA). The first root cause (numeric epoch nonce) tripped (1); the #859 PII-safe nonce then broke (2) — observe persistence + the matrix observe positive controls (C5). | High (incident) | **PII half:** `test_scan_isolation_gate_golden_markers_pass` (Rust — real `ContentScanner` accepts the golden set), `assert_marker_pii_safe` charset-reduced ERE canary, `test_c_nonce_battery_shape_safe`, `test_c_default_path_self_check_passes`, `test_c_canary_trips_on_regression` (canary has teeth). **Feature-id half:** `assert_marker_feature_id_shaped` self-check (teeth), `test_c_feature_id_check_trips_on_non_feature_id` (symmetric canary, #859 fix `543e8d08`). **Cross-pin:** `test_c_golden_markers_match_rust_anchor` (bash↔Rust shared golden list) | PASS | **Full pre-merge.** New marker `infra003-<leg>-<ab>-1-<b36>x<b36>` — the fixed all-digit `1` token (`MARKER_FID_TOKEN`) makes it feature-id-valid AND too short to form a phone/SSN shape, so BOTH filters pass **by construction**. Converts a probabilistic CI flake into a **deterministic pre-merge guarantee**; also repairs the matrix observe positive controls (C5), not just warmup |

Critical Gate 3c blockers R-01 and R-05 are both PASS on the pre-merge contract and
remain **unregressed** after the full #859 fold-in (isolation-logic 44/44 incl. the
prior R-01 cases; tristate 19/19 R-05 unchanged). The warmup probe now exercises the
embedding model via the MCP `context_store` round trip (the only embed-dependent
served write), strengthening R-01's load-bearing claim.
R-01's cold-path leg (zero warmup-attributable INFRA flap on a real cold model) is
now **proven** by the AC-11 dispatch run 28298217877 (GREEN, zero INFRA) — on top of
its pre-merge load-bearing (non-ceremonial) construction proven by the stub seam.

## Gaps

No risk is uncovered. The cold-model operational legs (R-01 cold-path, R-03 headroom,
R-13, R-11, R-12, and AC-04/AC-11) are now **GATHERED and GREEN** via the AC-11
dispatch run 28298217877 (see CI Operational Evidence below). The only remaining
post-merge item is R-10's `:v<ver>` tag-push leg — the budgeted one post-merge tag
round (C-10), non-blocking-on-INFRA and therefore safe; not a pre-merge gap.
R-15's chronic-INFRA residual is a human ACCEPT-or-escalate decision (VARIANCE /
OQ-3) that gates the N3 `proven` claim (AC-14) — not a tester assertion. The
additional **R-MPII (#859)** two-filter marker-contract class (PII
content-scan AND `looks_like_feature_id`) is fully covered pre-merge (Rust scanner
anchor + bash feature-id oracle + off-Docker (c) battery + two in-gate self-checks
with teeth) and made deterministic by construction — no residual gap.

## CI Operational Evidence (cold-model dispatch — now GATHERED, GREEN)

The CI-only legs that required a real `workflow_dispatch` cold-model run are now
satisfied by run **[28298217877](https://github.com/dug-21/unimatrix/actions/runs/28298217877)**
on `feature/infra-004`:

| AC / Risk | Status | Evidence from the dispatch run |
|-----------|--------|-------------------------------|
| **AC-11** cold-model dispatch GREEN | **PASS** | Job `multi-tenant-isolation-amd64` = success; `[infra003-smoke] ALL GATES PASSED — bidirectional 2x2 isolation holds on both surfaces`; real cold first-boot path; marker `infra003-warmup-1-22axthb07f` (both-filter-safe) |
| **AC-04** deterministic GREEN on cold container | **PASS** | Proven-by-AC-11: cold first-boot, warmup via MCP round trip, **zero INFRA flap** |
| R-01 cold-path zero-flap | **PASS** | Warmup via MCP `context_store` round trip PRESENT; zero warmup-attributable INFRA |
| R-03 cold headroom | **PASS** | Cold run GREEN well within `WARMUP_DEADLINE_SECS=180` (measured cold-ready ~5s) |
| R-13 cold-model not ceremonial | **PASS** | Genuine cold first-boot (not warm cache / not `:783-smoke`) |
| R-11 stale-image proof | **PASS** | Dispatch built from `feature/infra-004` tip (rebased on `main`) |

Remaining post-merge (NOT a pre-merge gap, budgeted C-10):

| AC / Risk | Why post-merge | Handling |
|-----------|----------------|----------|
| (R-10) `:v<ver>-amd64` tag-push resolution | First runs on a real tag only post-merge | Budget one post-merge tag round (C-10); tag-path INFRA degrades non-blocking (safe); the only blocking first-tag path (a harness step) already exercised by AC-11 |

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_assert_routes_live_precedes_barrier` (routes<warmup<matrix ordering), `test_warmup_bound_default_documented` (default 180 = #767 `READY_TIMEOUT_SECS` derivation, now corroborated by measurement: cold-ready ~5s, #767 ~70s floor); barrier sits between `assert_routes_live` and `run_isolation_matrix`. Warmup probe is the **MCP `context_store` round trip** (the embed-dependent served write — confirms model loaded), per #859 fix `543e8d08` |
| AC-02 | PASS | `test_warmup_uses_write_then_barrier_not_store_size` — reuses the existing `write_then_barrier` MCP round-trip idiom (now keyed to the `context_store` leg), no new readiness mechanism |
| AC-03 | PASS | `test_warmup_timeout_is_infra_not_pass` — timeout → INFRA (exit 2), not RED, not proceed, diagnostic logged |
| AC-04 | **PASS** | Proven by the AC-11 cold-model dispatch run (below): deterministic GREEN on a cold first-boot container, warmup via the MCP `context_store` round trip, **zero INFRA flap**. Pre-merge construction also covered by `test_warmup_present_requires_durable_read_roundtrip` |
| AC-05 | PASS | `test_post_barrier_{green,red,infra}_still_drives` — full verdict truth table still drives off-Docker post-barrier |
| AC-06 | PASS | `test_lane_job_exists`, `test_lane_needs_build_container_x64`, `test_workflow_triggers_tags_and_dispatch`, `test_lane_no_if_guard` |
| AC-07 | PASS | `test_lane_calls_resolve_image`, `test_lane_exports_image`, `test_lane_no_docker_build`, `test_lane_no_ref_strip` (no `${GITHUB_REF_NAME#v}`), `test_lane_invokes_tristate`, `test_lane_not_plain_run_smoke_gate` |
| AC-08 | PASS | tri-state truth table through the real sourced lib: 0+marker/0-no-marker/1/2/3/other; `test_tristate_rc_survives_capture`, `test_tristate_no_pipe_static_return_not_exit`, `test_tristate_only_exit2_nonblocking` (CRITICAL R-05) |
| AC-09 | PASS | `test_tristate_marker_anchored_substring` (substring NOT credited), `test_tristate_marker_whole_line_anywhere_is_green`, `test_tristate_marker_byte_identical` (runtime line) |
| AC-10 | PASS | `test_lane_provisions_node`, `test_lane_provisions_sqlite3` (self-contained step) |
| AC-11 | **PASS** | Cold-model fresh-build GREEN via `workflow_dispatch` on `feature/infra-004`. Run [28298217877](https://github.com/dug-21/unimatrix/actions/runs/28298217877); job `multi-tenant-isolation-amd64` = success; `[infra003-smoke] ALL GATES PASSED — bidirectional 2x2 isolation holds on both surfaces`; warmup via MCP `context_store` round trip PRESENT; observe GREEN + mcp GREEN; **zero INFRA / zero `::warning::`**; marker `infra003-warmup-1-22axthb07f` (both-filter-safe) |
| AC-12 | PASS | `test_lane_in_manifest_needs` (`needs:`-graph: lane ∈ `create-container-manifest.needs`) + forced-RED tri-state cell returns 1 → gates the edge (CRITICAL R-05/R-08) |
| AC-13 | PASS | `test_tristate_infra_exit2_nonblocking_visible` + `test_tristate_infra_exit2_canonical_marker_pinned` — INFRA returns success (non-blocking) with `::warning::` + pinned canonical marker `[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN` |
| AC-14 | PENDING (delivery + human gate) | N3 (#5161) `status: proven` set by delivery post-merge via `context_correct`, gated on the human VARIANCE decision (OQ-3 / R-15). Not a tester assertion |
| AC-15 | PASS (amended) | **Amended:** "no `crates/` **production** change; one **test-only** scanner anchor added." Production diff (gate scripts + fixtures) = `multi-tenant-isolation-smoke.sh`, `isolation-probe-lib.sh`, `release-gate-lib.sh`, `.github/workflows/release.yml`, + the `release-gate-isolation-logic-test.sh` suite & `fixtures/isolation-nonce-logic-cases.sh`; the sole `crates/` delta (commit `511ba824`) is `test_scan_isolation_gate_golden_markers_pass` inside `#[cfg(test)] mod tests` in `scanning.rs` — no production/scanner change. Smoke-script diff is warmup-barrier + #859 marker-contract (two-filter nonce/token derivation + MCP warmup probe) scoped only |

**Final tally (AC-01..AC-15): 14 PASS, 1 deferred to a human gate.**
- Pre-merge PASS (12): AC-01, AC-02, AC-03, AC-05, AC-06, AC-07, AC-08, AC-09,
  AC-10, AC-12, AC-13, AC-15.
- Operationally PASS via the AC-11 cold-model dispatch run (2): **AC-11** (cold-model
  fresh-build GREEN, run 28298217877) and **AC-04** (deterministic cold GREEN,
  proven-by-AC-11).
- Deferred to the human's post-delivery call (1): **AC-14** (N3 #5161 `proven`) — now
  with this GREEN evidence available; still gated on the human's chronic-INFRA
  VARIANCE decision (OQ-3 / R-15).

Remaining post-merge item (not an AC gap): the blocking `needs:` edge's **tag-push**
leg (`:v<ver>-amd64` resolution, R-10) first executes on a real tag — the budgeted
one post-merge tag round (C-10). It degrades non-blocking on INFRA (safe), and the
only blocking first-tag path (a harness step) was already exercised by AC-11.

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
  as #5192/#5345/#5267/#4974/#5354; this feature instantiates them. The #859
  marker two-filter contract (MCP PII content-scan vs observe `looks_like_feature_id`,
  which pull in opposite directions on digit runs) is already partially captured as
  lesson #5355 (and the concrete defect lives on GH #859 per the "bugs are GH issues,
  not lessons" rule) — no new entry; the fixing agent may extend #5355 with the
  two-filter generalization. The cumulative shell stub-seam execution pattern
  (now 44+19+15+13 = 91) is the established `release-gate-*-logic-test.sh` convention,
  not a new technique.
- #859 fold-in note: this regeneration documents the **complete marker+warmup fix**
  state (commit `543e8d08`, Gate 3b iter3). Counts re-confirmed by foreground re-run
  (isolation-logic 44/44; Rust anchor 1/1); the other suites (19/15/13) and smoke
  24/24 unchanged from the prior runs.
