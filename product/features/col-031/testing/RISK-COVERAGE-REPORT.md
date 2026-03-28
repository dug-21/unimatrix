# Risk Coverage Report: col-031 — Phase-Conditioned Frequency Table

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Silent wiring bypass: `run_single_tick` constructs services directly, bypassing `ServiceLayer` | `test_phase_freq_table_handle_is_correct_type_for_spawn` (compile-level); 7-site grep audit (all pass); `cargo build --workspace` passes | PASS | Full |
| R-02 | Vacuous AC-12 gate: `replay.rs` never forwards `current_phase` | `replay.rs` line 108 confirmed: `current_phase: record.context.phase.clone()`; AC-16 fix verified; AC-12 eval gate documented below | PASS | Full |
| R-03 | `use_fallback` guard absent or fires too late in fused scoring | `test_scoring_use_fallback_true_sets_phase_explicit_norm_zero`; `test_scoring_score_identity_cold_start`; `test_search_cold_start_phase_score_identity` (integration) | PASS | Full |
| R-04 | Wrong cold-start return for PPR — `phase_affinity_score` returns `0.0` | `test_phase_affinity_score_use_fallback_returns_one`; `test_phase_affinity_score_absent_phase_returns_one`; `test_phase_affinity_score_absent_entry_returns_one` | PASS | Full |
| R-05 | `CAST(json_each.value AS INTEGER)` omitted | `test_query_phase_freq_table_returns_correct_entry_id` (TestDb, confirms `entry_id = assigned_id`, not zero); SQL code review confirmed | PASS | Full |
| R-06 | Lock held across scoring loop | `test_scoring_lock_released_before_scoring_loop`; code review confirms guard released before loop | PASS | Full |
| R-07 | Rank normalization off-by-one (`1-rank/N` vs `1-(rank-1)/N`) | `test_phase_affinity_score_single_entry_bucket_returns_one` (N=1→1.0); `test_rebuild_normalization_three_entry_bucket_exact_scores`; `test_rebuild_normalization_last_entry_in_five_bucket` | PASS | Full |
| R-08 | `query_log_lookback_days` range not validated | `test_validate_lookback_days_zero_is_error`; `test_validate_lookback_days_3651_is_error`; `test_validate_lookback_days_boundary_values_pass` | PASS | Full |
| R-09 | Rebuild failure overwrites existing state with cold-start | `test_phase_freq_table_handle_retain_on_error` (confirms no-write on error path); code review | PASS | Full |
| R-10 | Phase vocabulary staleness on rename | `test_phase_affinity_score_unknown_phase_returns_one`; `test_phase_affinity_score_unknown_phase_returns_one` in `phase_freq_table.rs` | PASS | Full |
| R-11 | `w_phase_explicit = 0.05` causes CC@5/ICD regression | AC-12 eval gate — see below | Partial | Partial — eval scenarios not available in CI; gate documented |
| R-12 | Lock acquisition order violated by future refactor | Code comment present at lock sequence site in `background.rs` lines 577-581; `typed_graph_state` write precedes `phase_freq_table` write; grep order audit confirmed | PASS | Full |
| R-13 | `PhaseFreqRow.freq` typed as `u64` instead of `i64` | `test_query_phase_freq_table_returns_correct_entry_id` (asserts `freq == 10i64`); compile-time type assertion in `assert_phase_freq_row_field_types` | PASS | Full |
| R-14 | Test helper sites miss new constructor parameter | `cargo build --workspace` — 3840 tests pass, zero compile errors; 7-site grep audit complete | PASS | Full |

---

## Test Results

### Unit Tests

Run: `cargo test --workspace 2>&1 | grep -E "^test result"`

- **Total across all crates**: 3840 passed
- **Failed**: 0
- **Ignored**: 27 (pre-existing, unrelated to col-031)

Test result lines (summary):
```
test result: ok. 73 passed; 0 failed (unimatrix-store integration tests, includes query_log_tests.rs)
test result: ok. 2266 passed; 0 failed (unimatrix-server unit tests, includes all col-031 tests)
test result: ok. 422 passed; 0 failed (unimatrix-server eval tests)
... [all other crates: ok, 0 failures]
```

### Integration Tests (infra-001)

#### Smoke Suite (mandatory gate)

- Command: `python -m pytest suites/ -v -m smoke --timeout=60`
- **Total**: 20
- **Passed**: 20
- **Failed**: 0
- **Result**: PASS (mandatory gate satisfied)

#### Tools Suite

- Command: `python -m pytest suites/test_tools.py --timeout=60`
- **Total**: 95
- **Passed**: 93
- **xfailed**: 2 (pre-existing, unrelated to col-031)
- **Failed**: 0
- **Result**: PASS

#### Lifecycle Suite (includes 2 new col-031 tests)

- Command: `python -m pytest suites/test_lifecycle.py --timeout=60`
- **Total**: 43
- **Passed**: 40
- **xfailed**: 2 (pre-existing)
- **xpassed**: 1 (pre-existing xfail now passing — `test_search_multihop_injects_terminal_active`)
- **Failed**: 0
- **Result**: PASS
- **Note**: `test_search_multihop_injects_terminal_active` is an XPASS — it was marked xfail (expected failure) but now passes. This is pre-existing state, not caused by col-031 (multihop injection is unrelated to PhaseFreqTable). The xfail marker can be removed in a future cleanup PR.

#### Edge Cases Suite

- Command: `python -m pytest suites/test_edge_cases.py --timeout=60`
- **Total**: 24
- **Passed**: 23
- **xfailed**: 1 (pre-existing: `test_100_rapid_sequential_stores`)
- **Failed**: 0
- **Result**: PASS

### Integration Test Total

| Suite | Tests | Passed | xfailed | xpassed | Failed |
|-------|-------|--------|---------|---------|--------|
| smoke | 20 | 20 | 0 | 0 | 0 |
| tools | 95 | 93 | 2 | 0 | 0 |
| lifecycle | 43 | 40 | 2 | 1 | 0 |
| edge_cases | 24 | 23 | 1 | 0 | 0 |
| **Total** | **182** | **176** | **5** | **1** | **0** |

---

## New Integration Tests Added (col-031)

Two new tests added to `product/test/infra-001/suites/test_lifecycle.py`:

### `test_search_cold_start_phase_score_identity` (L-COL031-01)

Validates NFR-04 and AC-11: on a fresh (cold-start) server with `use_fallback=true`, context_search with a phase-active session produces results without error. The cold-start guard fires, `phase_explicit_norm=0.0` for all candidates, and scoring is identical to the no-phase path.

- Fixture: `server` (fresh DB, cold-start guaranteed)
- Result: **PASS**

### `test_search_current_phase_none_succeeds` (L-COL031-02)

Validates AC-11 Test 1: `context_search` with no phase context (no session, `current_phase=None`) succeeds normally and finds stored entries. The lock on `PhaseFreqTableHandle` is never acquired.

- Fixture: `server`
- Result: **PASS**

### Note on `test_search_phase_affinity_influences_ranking`

The planned test requiring a populated `PhaseFreqTable` after a real tick was assessed at Stage 3c. This test requires direct DB seeding of `query_log` rows and triggering a background tick synchronously — neither is available through the MCP interface. The unit-level equivalent (`test_phase_freq_table_handle_swap_on_success` in `background.rs`) covers the success-path swap. The MCP-level ranking test is deferred to a future follow-up when a tick-trigger mechanism is exposed.

---

## Gaps

### R-11 / AC-12 — Eval Regression Gate (Partial Coverage)

**Status**: Partial. The eval binary exists as `unimatrix eval` (subcommand), but no eval scenario JSONL file is available in the CI environment for this run. The AC-12 gate (MRR ≥ 0.35, CC@5 ≥ 0.2659, ICD ≥ 0.5340) cannot be declared PASS without a scenario file.

**AC-16 prerequisite is satisfied**: `replay.rs` line 108 contains `current_phase: record.context.phase.clone()`. This is the one-line fix required by AC-16 and ADR-004.

**Evidence**: Code inspection of `replay.rs` confirms non-null `current_phase` will appear in eval output for any scenario record with `context.phase` set.

**Gate 3c impact**: AC-12 should be verified by the delivery team with a snapshot database and scenario file. This report documents that the AC-16 prerequisite is complete, but AC-12 itself requires a separate eval run.

### `test_search_phase_affinity_influences_ranking` (Integration)

The ranking-influence integration test (requiring populated `PhaseFreqTable` after a real tick) is not implemented at the MCP harness level — as assessed above. The unit-level coverage (`test_phase_freq_table_handle_swap_on_success`) covers the success-path mechanism. This is a known gap documented in the test plan OVERVIEW.md.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_phase_freq_table_new_returns_cold_start`: `use_fallback==true`, `table.is_empty()` |
| AC-02 | PASS | Covered by AC-08 and AC-14; SQL form inspected in code review (CAST forms present) |
| AC-03 | PASS | `test_new_handle_wraps_cold_start_state`; all lock acquisitions use `.unwrap_or_else(|e| e.into_inner())` confirmed by grep |
| AC-04 | PASS | `test_phase_freq_table_handle_swap_on_success` (success path); `test_phase_freq_table_handle_retain_on_error` (error path retains state) |
| AC-05 | PASS | `test_service_layer_phase_freq_table_handle_returns_arc_clone`; `test_service_layer_phase_freq_table_handle_is_non_optional`; 7-site grep audit complete; `cargo build --workspace` passes |
| AC-06 | PASS | `test_scoring_use_fallback_true_sets_phase_explicit_norm_zero` (guard fires); `test_scoring_lock_released_before_scoring_loop` (lock released before loop); code review confirmed |
| AC-07 | PASS | `test_phase_affinity_score_use_fallback_returns_one`; `test_phase_affinity_score_absent_phase_returns_one`; `test_phase_affinity_score_absent_entry_returns_one` — all assert `== 1.0f32` |
| AC-08 | PASS | `test_query_phase_freq_table_returns_correct_entry_id` (TestDb): `entry_id` round-trip confirmed, `freq == 10i64` |
| AC-09 | PASS | `default_w_phase_explicit()` returns `0.05`; `InferenceConfig::default()` uses it; deserialization test present |
| AC-10 | PASS | `default_query_log_lookback_days()` returns `30u32`; default and deserialization tests pass |
| AC-11 | PASS | Three unit tests: (1) `test_scoring_current_phase_none_sets_phase_explicit_norm_zero`; (2) `test_scoring_use_fallback_true_sets_phase_explicit_norm_zero`; (3) `test_phase_affinity_score_use_fallback_returns_one` |
| AC-12 | PENDING | Eval scenario file not available in CI. AC-16 prerequisite is verified. Gate requires delivery team to run `unimatrix eval` with scenario JSONL and confirm MRR ≥ 0.35, CC@5 ≥ 0.2659, ICD ≥ 0.5340 |
| AC-13 | PASS | `test_phase_affinity_score_single_entry_bucket_returns_one` (N=1 → 1.0, not 0.0) |
| AC-14 | PASS | `test_rebuild_normalization_three_entry_bucket_exact_scores` (rank-1=1.0, rank-2≈0.666, rank-3≈0.333) |
| AC-15 | PASS | `wc -l phase_freq_table.rs` = 411 lines (≤ 500 limit) |
| AC-16 | PASS | `replay.rs` line 108: `current_phase: record.context.phase.clone()` — fix present and verified |
| AC-17 | PASS | `phase_affinity_score` doc comment names both callers (PPR #398, fused scoring) with their respective cold-start contracts |

---

## Code Review Checks (Non-Test Assertions)

| Check | Status | Location |
|-------|--------|----------|
| `CAST(json_each.value AS INTEGER)` in SQL | PASS | `query_log.rs` lines 213, 217, 221 |
| No bare `.unwrap()` on lock acquisitions | PASS | All RwLock sites use `.unwrap_or_else(|e| e.into_inner())` |
| Lock order comment at tick site | PASS | `background.rs` lines 577-581: `// LOCK ACQUISITION ORDER: 1. EffectivenessStateHandle, 2. TypedGraphStateHandle, 3. PhaseFreqTableHandle` |
| `typed_graph_state.write()` before `phase_freq_table.write()` | PASS | background.rs: `typed_graph_state` handled at line ~540, `phase_freq_table` at line ~607 |
| `PhaseFreqTableHandle` non-optional at all 7 sites | PASS | All sites compile; no `Option<PhaseFreqTableHandle>` found at any call site |
| `FusionWeights` doc-comment updated to `0.95 + 0.02 + 0.05 = 1.02` | PASS | `search.rs` doc comment confirmed |
| AC-17 doc comment on `phase_affinity_score` | PASS | `phase_freq_table.rs` lines 172-193 |
| `replay.rs` diff limited to `current_phase` field assignment only | PASS | `replay.rs` line 108 only; `extract.rs` and `output.rs` unchanged |

---

## xfail Inventory

| Test | Suite | Reason | GH Issue |
|------|-------|--------|----------|
| `test_100_rapid_sequential_stores` | edge_cases | Pre-existing pool contention under rapid load | (pre-existing) |
| `test_auto_quarantine_after_consecutive_bad_ticks` | lifecycle | Tick interval env var needed for deterministic timing | (pre-existing) |
| `test_dead_knowledge_entries_deprecated_by_tick` | lifecycle | Background tick timing in test environment | (pre-existing) |
| `test_retrospective_baseline_present` | (not run) | Pre-existing: null baseline_comparison with synthetic features | GH#305 |
| Pool timeout tests | (not run) | Pre-existing concurrent pool timeout | GH#303 |

All xfail markers were pre-existing before col-031. No new xfail markers were added by this feature.

---

## XPASS Note

`test_search_multihop_injects_terminal_active` in `test_lifecycle.py` reported as XPASS (was expected to fail, now passes). This is unrelated to col-031 — multihop injection behavior change occurred in a prior feature. The xfail marker can be removed in a separate cleanup.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found entries #229 (tester duties), #238 (test infrastructure conventions), #3526 (dual-type-copy JSON schema boundary pattern). Informative for boundary-testing approach.
- Stored: nothing novel — the `test_search_cold_start_phase_score_identity` pattern (MCP-level cold-start score identity via fresh `server` fixture) is a straightforward application of existing conventions. No new harness patterns discovered beyond what is already in #238.
