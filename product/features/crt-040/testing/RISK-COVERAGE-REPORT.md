# Risk Coverage Report: crt-040 — Cosine Supports Edge Detection

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Category data unavailable from `candidate_pairs` — AC-03 filter requires HashMap | `test_path_c_qualifying_pair_writes_supports_edge` (TC-01), `test_path_c_missing_entry_id_no_panic_no_edge` (TC-05), `test_path_c_disallowed_category_no_edge` (TC-04) | PASS | Full |
| R-02 | `write_nli_edge` delegation retags existing NLI edges | `test_write_nli_edge_still_writes_nli_source` (TC-02), `test_write_graph_edge_writes_cosine_supports_source` (TC-01), `test_write_graph_edge_and_write_nli_edge_distinct_sources` (TC-03) | PASS | Full |
| R-03 | `impl Default` and serde backing function diverge on `supports_cosine_threshold` | `test_inference_config_default_supports_cosine_threshold` (TC-01), `test_inference_config_serde_empty_toml_supports_cosine_threshold` (TC-02), `test_default_supports_cosine_threshold_backing_fn` (TC-03) | PASS | Full |
| R-04 | `nli_post_store_k` removal masks test regression | `test_inference_config_toml_with_nli_post_store_k_succeeds` (TC-11), AC-17 grep gate | PASS | Full |
| R-05 | `inferred_edge_count` silently understates inference activity | `test_inferred_edge_count_unchanged_after_path_c_write` (TC-15) | PASS | Full |
| R-06 | Path C observability log must emit even when `candidate_pairs` is empty | `test_path_c_observability_log_fires_with_empty_candidates` (TC-12), `test_path_c_observability_log_counts_correct` (TC-13) | PASS | Full |
| R-07 | Path B + Path C collision treated as error | `test_path_c_budget_counter_not_incremented_on_unique_conflict` (TC-08), `test_write_graph_edge_duplicate_returns_false_no_panic` (TC-04) | PASS | Full |
| R-08 | Intra-tick Phase 4 normalization inconsistency | `test_path_c_reversed_pair_no_duplicate_edge` (TC-17) | PASS | Full |
| R-09 | NaN/Inf cosine guard missing | `test_path_c_nan_cosine_no_edge` (TC-09), `test_path_c_infinity_cosine_no_edge` (TC-10), `test_path_c_nan_guard_order_threshold_not_evaluated` (TC-11) | PASS | Full |
| R-10 | `all_active` linear scan for category resolution | `test_path_c_qualifying_pair_writes_supports_edge`, code review confirms `category_map: HashMap<u64, &str>` built once before loop | PASS | Full |
| R-11 | `nli_detection_tick.rs` file size extraction | Code review: `run_cosine_supports_path` extracted as private helper (NFR-07 compliant) | PASS | Full |
| R-12 | Eval gate MRR regression | AC-14 deferred — eval harness requires live corpus with ticks; not blocking merge per IMPLEMENTATION-BRIEF (marked pending) | Partial | Partial |
| R-13 | Config merge function not updated for `supports_cosine_threshold` | `test_inference_config_merge_supports_cosine_threshold_override` (TC-10) | PASS | Full |

## Test Results

### Unit Tests

- Total: 4285 (workspace)
- Passed: 4285
- Failed: 0
- New tests added by this agent (7 missing TCs from Gate 3b): all pass

#### New Unit Tests Added (Gate 3b Gap Fill)

| File | Test | TC | Risk/AC |
|------|------|----|---------|
| `crates/unimatrix-store/src/read.rs` | `test_edge_source_cosine_supports_crate_root_accessible` | TC-02 store-constant | AC-08 re-export |
| `crates/unimatrix-server/src/services/nli_detection_tick.rs` | `test_path_c_exact_threshold_boundary_qualifies` | TC-03 path-c-loop | AC-02 boundary (>= not >) |
| `crates/unimatrix-server/src/services/nli_detection_tick.rs` | `test_path_c_infinity_cosine_no_edge` | TC-10 path-c-loop | R-09 |
| `crates/unimatrix-server/src/services/nli_detection_tick.rs` | `test_path_c_nan_guard_order_threshold_not_evaluated` | TC-11 path-c-loop | R-09 guard placement |
| `crates/unimatrix-server/src/services/nli_detection_tick.rs` | `test_path_c_observability_log_counts_correct` | TC-13 path-c-loop | R-06, AC-19 |
| `crates/unimatrix-server/src/services/nli_detection_tick.rs` | `test_inferred_edge_count_unchanged_after_path_c_write` | TC-15 path-c-loop | AC-15, R-05, NFR-06 |
| `crates/unimatrix-server/src/services/nli_detection_tick.rs` | `test_path_c_reversed_pair_no_duplicate_edge` | TC-17 path-c-loop | R-08 |

### Integration Tests

#### Smoke Gate (`pytest -m smoke`)
- Total: 22
- Passed: 22
- Failed: 0
- Result: PASS (mandatory gate satisfied)

#### Lifecycle Suite (`suites/test_lifecycle.py`)
- Total collected: 44
- Passed: 41
- xfailed: 2 (pre-existing: `test_auto_quarantine_after_consecutive_bad_ticks`, `test_dead_knowledge_entries_deprecated_by_tick`)
- xpassed: 1 (pre-existing: `test_search_multihop_injects_terminal_active` — xfail marker present but test passes; pre-existing state, not caused by crt-040)
- Failed: 0

#### Tools Suite (`suites/test_tools.py`)
- Total collected: 100
- Passed: 98
- xfailed: 2 (pre-existing: `test_deprecated_visible_in_search_with_lower_confidence`, `test_retrospective_baseline_present`)
- Failed: 0

#### New Integration Tests Added
Added to `product/test/infra-001/suites/test_lifecycle.py`:

| Test | Covers | Status |
|------|--------|--------|
| `test_context_status_supports_edge_count_increases_after_tick` | AC-05, NFR-05, R-05 | XFAIL (no embedding model in CI) |
| `test_inferred_edge_count_unchanged_by_cosine_supports` | AC-15, R-05, NFR-06 | XFAIL (no embedding model in CI) |

Both tests are marked `@pytest.mark.xfail` because the test environment has no ONNX embedding model. Without embeddings, the tick cannot compute cosine similarity, `candidate_pairs` remains empty, and Path C writes zero edges. The test structure and assertions are correct — remove the xfail markers when an embedding model is available in CI.

## Pre-existing Integration Test Issues (Not Caused by crt-040)

| Test | Suite | Status | Notes |
|------|-------|--------|-------|
| `test_auto_quarantine_after_consecutive_bad_ticks` | lifecycle | XFAIL (pre-existing) | Requires `UNIMATRIX_TICK_INTERVAL_SECONDS` env var to drive ticks |
| `test_dead_knowledge_entries_deprecated_by_tick` | lifecycle | XFAIL (pre-existing) | Background tick timing |
| `test_search_multihop_injects_terminal_active` | lifecycle | XPASS (pre-existing) | Pre-existing xfail marker; test passes unexpectedly |
| `test_deprecated_visible_in_search_with_lower_confidence` | tools | XFAIL (pre-existing) | Background scoring timing |
| `test_retrospective_baseline_present` | tools | XFAIL (pre-existing, GH#305) | Baseline comparison null with synthetic features |

No GH Issues filed — all failures are pre-existing xfail markers not caused by crt-040.

## Gaps

**R-12 (Eval Gate)**: AC-14 (MRR >= 0.2875) was not executed. The eval harness at
`product/research/ass-039/harness/run_eval.py` requires a live server with at least one
background tick completed on a production corpus with valid embeddings. This environment
has no ONNX model. The eval gate is marked as deferred per the IMPLEMENTATION-BRIEF,
which states: "Eval gate is mandatory before PR merge." This gap is acceptable for the
Stage 3c test execution phase; it should be run manually before merge.

**No other gaps** — every risk from RISK-TEST-STRATEGY.md that is unit-testable has
explicit test coverage. R-10, R-11 are code-review risks (covered by inspection). R-12
is the eval gate (deferred).

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_path_c_qualifying_pair_writes_supports_edge` — Supports edge written with source='cosine_supports' |
| AC-02 | PASS | `test_path_c_below_threshold_no_edge` (0.64 rejected) + `test_path_c_exact_threshold_boundary_qualifies` (0.65 accepted) |
| AC-03 | PASS | `test_path_c_disallowed_category_no_edge` — decision/decision at 0.80 produces no edge |
| AC-04 | PASS | `test_path_c_existing_pair_skipped` — pre-filter prevents duplicate; row count = 1 |
| AC-05 | PASS | `test_path_c_runs_unconditionally_nli_disabled` — Path C writes edge with nli_enabled=false |
| AC-06 | PASS | All existing Path A tests pass unchanged (unit test suite: 4285 passed, 0 failed) |
| AC-07 | PASS | All existing Path B tests pass unchanged (unit test suite: 4285 passed, 0 failed) |
| AC-08 | PASS | `test_edge_source_cosine_supports_value` (value), `test_edge_source_cosine_supports_crate_root_accessible` (re-export) |
| AC-09 | PASS | `test_inference_config_validate_*` suite — 0.0 and 1.0 rejected, 0.65/0.001/0.999 accepted |
| AC-10 | PASS | `test_inference_config_default_supports_cosine_threshold` — InferenceConfig::default() returns 0.65 |
| AC-11 | PASS | Code inspection: line 841 uses `EDGE_SOURCE_COSINE_SUPPORTS`, not string literal; `test_write_graph_edge_writes_cosine_supports_source` asserts source='cosine_supports' |
| AC-12 | PASS | `test_path_c_budget_cap_50_from_60_qualifying` — 60 qualifying pairs → exactly 50 edges |
| AC-13 | PASS | Code inspection: `run_cosine_supports_path` called after Path A log (line 523), before Path B entry gate (line 552); ordering confirmed in Gate 3b |
| AC-14 | PENDING | Eval harness not run (no ONNX model in environment); mandatory before PR merge |
| AC-15 | PASS | `test_inferred_edge_count_unchanged_after_path_c_write` — inferred_edge_count unchanged after cosine_supports write |
| AC-16 | PASS | `test_inference_config_default_supports_cosine_threshold` (impl Default), `test_inference_config_serde_empty_toml_supports_cosine_threshold` (serde), `test_default_supports_cosine_threshold_backing_fn` (backing fn) — all three independent assertions |
| AC-17 | PASS | `grep -n "nli_post_store_k" config.rs` returns 4 lines, all inside TC-11 test body; zero production code references |
| AC-18 | PASS | `test_inference_config_toml_with_nli_post_store_k_succeeds` — TOML with removed field deserializes without error |
| AC-19 | PASS | `test_path_c_observability_log_fires_with_empty_candidates` (zero counts, no panic), `test_path_c_observability_log_counts_correct` (non-zero counts proxy) |

## Log Field Name Collision Check (R-06, TC-14)

- Path A structured log fields (from code): `informs_edges_written`, `informs_candidates_after_cap`
- Path C structured log fields (from code): `cosine_supports_candidates`, `cosine_supports_edges_written`
- Result: no collision; field names are distinct and non-overlapping.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced lessons #3935 (Gate 3b WARN, missing tests), #2758 (grep test names before accepting PASS), #3806 (handler integration tests), #3579 (missing test modules). These directly guided the 7-test gap-fill strategy.
- Stored: nothing novel to store — the pattern of "fill Gate 3b WARN TCs with exact boundary/guard/backward-compat tests" is already captured in the lessons surfaced by context_briefing. The specific TC-15 approach (write_graph_edge with source='nli' to establish baseline, then run Path C, compare inferred_edge_count) is a natural extension of existing NLI test patterns already in the codebase.
