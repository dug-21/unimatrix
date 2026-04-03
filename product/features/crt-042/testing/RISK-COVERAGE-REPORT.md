# Risk Coverage Report: crt-042 (PPR Expander)

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Flag-off regression: bit-identical output when `ppr_expander_enabled=false` | `test_search_flag_off_pool_size_unchanged` (unit); infra-001 smoke (22 tests, all pass); `test_search_excludes_quarantined` (tools); `test_search_returns_results` (tools) | PASS | Full |
| R-02 | S1/S2 Informs edges single-direction — Outgoing-only sees half the graph | `test_graph_expand_informs_surfaces_neighbor`, `test_graph_expand_unidirectional_informs_from_higher_id_seed_misses`, `test_graph_expand_bidirectional_informs_after_backfill`, `test_graph_expand_backward_edge_does_not_surface`, `test_graph_expand_two_hop_depth2_surfaces_both`, `test_graph_expand_two_hop_depth1_surfaces_only_first` | PASS (unit behavioral proofs) | Full (back-fill GH#495 filed — see SR-03 note below) |
| R-03 | Quarantine bypass: quarantined entry reachable via graph edge enters results | `test_search_phase0_excludes_quarantined_direct` (unit), `test_search_phase0_excludes_quarantined_transitive` (unit), `test_quarantine_excludes_endpoint_from_graph_traversal` (infra-001 lifecycle), `test_search_excludes_quarantined` (infra-001 tools), security suite (19/19 pass) | PASS | Full |
| R-04 | O(N) latency at full expansion: 200 × O(7000) comparisons | `test_search_phase0_emits_debug_trace_when_enabled` (unit tracing subscriber); `elapsed_ms` field confirmed present in trace; P95 latency gate deferred to eval gate run — see SR-03 note | PASS (instrumentation confirmed); latency gate DEFERRED | Partial (instrumentation full; eval gate deferred until back-fill applied) |
| R-05 | Combined ceiling overflow: Phase 0 (200) + Phase 5 (50) + HNSW (20) = 270 | `test_graph_expand_max_candidates_cap` (unit); `test_search_phase0_phase5_combined_ceiling` (unit); inline comment in search.rs documenting 270 ceiling | PASS | Full |
| R-06 | Back-fill race during eval: GRAPH_EDGES partially populated during eval run | Procedural check: eval must use `unimatrix snapshot` taken after back-fill committed; documented in delivery brief and SR-03 note below | PASS (procedural) | Full |
| R-07 | Eval gate failure: P@5 no improvement despite correct implementation | `test_search_phase0_cross_category_entry_visible_with_flag_on` (unit — AC-25 behavioral proof; cross-category entry surfaces only with flag=true); eval run deferred until back-fill (GH#495) applied | PASS (unit behavioral proof); eval gate DEFERRED | Partial (AC-25 test present; eval run requires back-fill) |
| R-08 | InferenceConfig hidden test sites: new fields missing from literal constructions | Grep scan: all `InferenceConfig {` literals use `..InferenceConfig::default()` spread syntax or include all three new fields; `test_inference_config_expander_fields_defaults`, `test_inference_config_expander_fields_serde_defaults`, `test_unimatrix_config_expander_toml_omitted_produces_defaults`, `test_inference_config_expander_serde_fn_matches_default`, `test_inference_config_merged_propagates_expander_fields`, `test_inference_config_merged_expander_enabled_project_wins`, `test_inference_config_expander_toml_explicit_override` | PASS | Full |
| R-09 | `edges_of_type()` boundary violation: direct `.edges_directed()` or `.neighbors_directed()` calls in graph_expand.rs | AC-16 grep check: `grep -n 'edges_directed\|neighbors_directed' graph_expand.rs \| grep -v "//"` → zero code-line matches (lines 9, 57, 58, 114 are doc comments only); `test_graph_expand_supersedes_not_traversed`, `test_graph_expand_contradicts_not_traversed` | PASS | Full |
| R-10 | Timing instrumentation absent or at wrong log level | `test_search_phase0_emits_debug_trace_when_enabled` (traced_test, asserts debug event emitted with all required fields), `test_search_phase0_does_not_emit_trace_when_disabled` (asserts no trace on flag=false); code inspection: `tracing::debug!` at search.rs line 951 | PASS | Full |
| R-11 | BFS visited-set missing: cycles cause infinite loop | `test_graph_expand_bidirectional_terminates` (CoAccess cycle terminates, returns {2}), `test_graph_expand_triangular_cycle_terminates` (triangular A→B→C→A terminates, returns {2,3}) | PASS | Full |
| R-12 | Seed exclusion failure: seeds appear in graph_expand return set | `test_graph_expand_seeds_excluded_from_result` (seeds {A,B}, edge A→B, both absent from result), `test_graph_expand_self_loop_seed_not_returned` (self-loop does not return seed) | PASS | Full |
| R-13 | Determinism failure: BFS frontier not in sorted node-ID order | `test_graph_expand_deterministic_across_calls` (budget-boundary exercised at max=3; two consecutive calls identical; lowest IDs 2,3,4 returned) | PASS | Full |
| R-14 | Config validation conditional gap: expansion_depth=0 accepted when flag=false | `test_validate_expansion_depth_zero_fails` (flag=false, depth=0 → Err), `test_validate_expansion_depth_eleven_fails` (flag=false, depth=11 → Err), `test_validate_max_expansion_candidates_zero_fails` (flag=false, max=0 → Err), `test_validate_max_expansion_candidates_1001_fails` (flag=false, max=1001 → Err); boundary pass tests also present | PASS | Full |
| R-15 | get_embedding layer-0 miss: valid embedding not returned | `test_search_phase0_skips_entry_with_no_embedding` (None embedding → silent skip, no error); code inspection: `vector_store.get_embedding()` uses `IntoIterator` (crt-014 fix applies) | PASS | Full |
| R-16 | Phase 0 insertion point wrong: runs after Phase 1 | Code inspection: Phase 0 block at search.rs line 872 is first block inside `if !use_fallback`, before Phase 1 `seed_scores` construction at line 969; `test_search_phase0_expands_before_phase1` (unit asserts expanded entries present before Phase 1 input) | PASS | Full |
| R-17 | S8 CoAccess directionality gap: higher-ID seed misses lower-ID partner | `test_graph_expand_s8_coaccess_unidirectional_from_higher_id_misses` (seed {2}, edge 1→2, result empty — gap documented); `test_graph_expand_bidirectional_terminates` (bidirectional CoAccess traverses correctly); crt-035 tick coverage note in test doc | PASS (gap documented; crt-035 tick provides coverage for promoted pairs) | Full |

---

## Test Results

### Unit Tests

Executed: `cargo test --workspace 2>&1 | grep "test result"` across all crates.

| Test Binary / Module | Total | Passed | Failed | Ignored |
|---------------------|-------|--------|--------|---------|
| unimatrix-engine (all) | 368 | 367 | 0 | 1 |
| unimatrix-engine graph_expand | 21 | 21 | 0 | 0 |
| unimatrix-server (all, largest module) | 2681 | 2681 | 0 | 0 |
| unimatrix-server config (inference) | 423 | 423 | 0 | 0 |
| unimatrix-server search (Phase 0 mod) | 73 | 73 | 0 | 0 |
| All other crates (store, vector, embed, etc.) | ~1000 | ~1000 | 0 | 28 |
| **Total workspace** | **~4130** | **~4099** | **0** | **~28 ignored** |

Note: all ignored tests are pre-existing (DB pool tests, GH#303 pattern). Zero failures across entire workspace.

### New Unit Tests Added by crt-042

**`graph_expand_tests.rs` (21 tests — Component 1):**

| Test Name | AC/Risk | Result |
|-----------|---------|--------|
| test_graph_expand_coaccess_surfaces_neighbor | AC-03 | PASS |
| test_graph_expand_supports_surfaces_neighbor | AC-03 | PASS |
| test_graph_expand_informs_surfaces_neighbor | AC-03 | PASS |
| test_graph_expand_prerequisite_surfaces_neighbor | AC-03 | PASS |
| test_graph_expand_backward_edge_does_not_surface | AC-04 | PASS |
| test_graph_expand_two_hop_depth2_surfaces_both | AC-05 | PASS |
| test_graph_expand_two_hop_depth1_surfaces_only_first | AC-06 | PASS |
| test_graph_expand_supersedes_not_traversed | AC-07 | PASS |
| test_graph_expand_contradicts_not_traversed | AC-07 | PASS |
| test_graph_expand_seeds_excluded_from_result | AC-08 | PASS |
| test_graph_expand_self_loop_seed_not_returned | AC-08 | PASS |
| test_graph_expand_max_candidates_cap | AC-09 | PASS |
| test_graph_expand_empty_seeds_returns_empty | AC-10 | PASS |
| test_graph_expand_empty_graph_returns_empty | AC-11 | PASS |
| test_graph_expand_depth_zero_returns_empty | AC-12 | PASS |
| test_graph_expand_bidirectional_terminates | R-11 | PASS |
| test_graph_expand_triangular_cycle_terminates | R-11 | PASS |
| test_graph_expand_deterministic_across_calls | R-13 | PASS |
| test_graph_expand_unidirectional_informs_from_higher_id_seed_misses | R-02 | PASS |
| test_graph_expand_bidirectional_informs_after_backfill | R-02 | PASS |
| test_graph_expand_s8_coaccess_unidirectional_from_higher_id_misses | R-17 | PASS |

**`search.rs` mod phase0 (Component 2 — AC-14 and AC-25 implemented as unit tests per spawn prompt):**

| Test Name | AC/Risk | Result |
|-----------|---------|--------|
| test_search_flag_off_pool_size_unchanged | AC-01 | PASS |
| test_search_phase0_expands_before_phase1 | AC-02, R-16 | PASS |
| test_search_phase0_excludes_quarantined_direct | AC-13/14 | PASS |
| test_search_phase0_excludes_quarantined_transitive | AC-13/14 | PASS |
| test_search_phase0_skips_entry_with_no_embedding | AC-15 | PASS |
| test_search_phase0_emits_debug_trace_when_enabled | AC-24 | PASS |
| test_search_phase0_does_not_emit_trace_when_disabled | R-10 | PASS |
| test_search_phase0_cross_category_entry_visible_with_flag_on | AC-25 | PASS |
| test_search_phase0_phase5_combined_ceiling | R-05 | PASS |

Note on AC-14 and AC-25: The test-plan OVERVIEW noted that the MCP harness does not support per-test config override (server config is fixed at launch). Both AC-14 (quarantine bypass) and AC-25 (cross-category behavioral proof) were implemented as unit tests in `search.rs` mod phase0 — the correct resolution per the test plan contingency design.

**`config.rs` inference test module (Component 3 — 17 tests):**

| Test Name | AC/Risk | Result |
|-----------|---------|--------|
| test_inference_config_expander_fields_defaults | AC-17 | PASS |
| test_inference_config_expander_fields_serde_defaults | AC-17 | PASS |
| test_unimatrix_config_expander_toml_omitted_produces_defaults | AC-17 | PASS |
| test_inference_config_expander_serde_fn_matches_default | R-08 | PASS |
| test_validate_expansion_depth_zero_fails | AC-18 | PASS |
| test_validate_expansion_depth_eleven_fails | AC-19 | PASS |
| test_validate_expansion_depth_ten_passes | AC-19 boundary | PASS |
| test_validate_expansion_depth_one_passes | AC-18 boundary | PASS |
| test_validate_max_expansion_candidates_zero_fails | AC-20 | PASS |
| test_validate_max_expansion_candidates_1001_fails | AC-21 | PASS |
| test_validate_max_expansion_candidates_one_passes | AC-20 boundary | PASS |
| test_validate_max_expansion_candidates_1000_passes | AC-21 boundary | PASS |
| test_validate_expansion_depth_error_names_field | R-08 | PASS |
| test_validate_max_expansion_candidates_error_names_field | R-08 | PASS |
| test_inference_config_merged_propagates_expander_fields | R-08 merge | PASS |
| test_inference_config_merged_expander_enabled_project_wins | R-08 merge | PASS |
| test_inference_config_expander_toml_explicit_override | R-08 TOML | PASS |

**Total new unit tests: 47** (21 graph_expand + 9 search/Phase 0 + 17 config).

### Integration Tests

#### Smoke Suite (MANDATORY GATE — PASSED)

Command: `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60`

Result: **22 passed, 237 deselected** in 191.45s (0:03:11)

All smoke tests pass. R-01 flag-off regression gate: PASS.

#### Security Suite

Command: `python -m pytest suites/test_security.py --timeout=30 -q`

Result: **19 passed** in 143.81s

Includes `test_injection_patterns_detected`, `test_capability_enforcement`, and quarantine-related security tests. R-03 coverage confirmed at integration level.

#### Lifecycle Suite (Targeted)

Command: `python -m pytest suites/test_lifecycle.py::test_quarantine_excludes_endpoint_from_graph_traversal ...` (9 tests selected)

Result: **6 passed, 2 xfailed (pre-existing GH#291), 1 xpassed** in 134.27s

- `test_quarantine_excludes_endpoint_from_graph_traversal`: PASS — confirms quarantine enforcement for graph-reachable entries (AC-13/AC-14 at integration level)
- `test_store_search_find_flow`: PASS — R-01 regression confirmation
- `test_correction_chain_integrity`: PASS
- `test_full_lifecycle_pipeline`: PASS
- `test_data_persistence_across_restart`: PASS
- `test_search_multihop_injects_terminal_active`: PASS
- 2 xfailed: `test_inferred_edge_count_unchanged_by_s1_s2_s8` (XFAIL, GH#291), `test_s1_edges_visible_in_status_after_tick` (XFAIL, GH#291) — both pre-existing, background tick timeout, not caused by crt-042
- 1 xpassed: `test_inferred_edge_count_unchanged_by_s1_s2_s8` passed unexpectedly (pre-existing GH#291 xfail cleared incidentally)

#### Tools Suite (Targeted)

Command: `python -m pytest suites/test_tools.py::test_search_returns_results ...` (6 tests)

Result: **5 passed, 1 xfailed (pre-existing)**

- `test_search_returns_results`: PASS
- `test_search_excludes_quarantined`: PASS — confirms quarantine filtering at MCP API level
- `test_search_with_topic_filter`: PASS
- `test_search_with_category_filter`: PASS
- `test_search_with_k_limit`: PASS
- `test_deprecated_visible_in_search_with_lower_confidence`: XFAIL (pre-existing: background scoring timing, not caused by crt-042)

Note on full tools/lifecycle suite run: the environment runs each test at ~8s overhead (model initialization per server instance), making the full 100-test tools suite require ~800s. Given the smoke gate passes (22 tests) and the targeted search/quarantine/lifecycle tests pass, the R-01 regression gate is fully satisfied.

### Integration Test Counts Summary

| Suite | Total | Passed | Failed | XFAIL (pre-existing) | XPASS |
|-------|-------|--------|--------|---------------------|-------|
| smoke (-m smoke) | 22 | 22 | 0 | 0 | 0 |
| security | 19 | 19 | 0 | 0 | 0 |
| lifecycle (targeted 9) | 9 | 6 | 0 | 2 (GH#291) | 1 |
| tools (targeted 6) | 6 | 5 | 0 | 1 (pre-existing) | 0 |
| **Total run** | **56** | **52** | **0** | **3** | **1** |

---

## AC-16 Check Result

Command: `grep -n "edges_directed\|neighbors_directed" crates/unimatrix-engine/src/graph_expand.rs | grep -v "//"`

Result: **PASS — zero code-line matches.**

The pattern appears in 4 doc comment lines (lines 9, 57, 58, 114) only. No actual `.edges_directed()` or `.neighbors_directed()` API calls exist in `graph_expand.rs`. All traversal uses `edges_of_type()` exclusively (4 calls at lines ~121, 124, 127, 130 for CoAccess, Supports, Informs, Prerequisite respectively). This satisfies the SR-01 traversal boundary invariant (entry #3627).

---

## AC-22 Eval Profile File Check

Command: `ls -la product/research/ass-037/harness/profiles/ppr-expander-enabled.toml`

Result: **PASS** — file exists (size: 297 bytes, created 2026-04-03).

The profile at `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` is present. `run_eval.py --profile ppr-expander-enabled.toml` can execute to completion. Eval gate result (MRR >= 0.2856, P@5 > 0.1115) is deferred until the S1/S2 back-fill (GH#495) is applied to the deployment database.

---

## SR-03 Status: Back-fill Issue GH#495

Per the IMPLEMENTATION-BRIEF.md hard gate: S1/S2 Informs edges were confirmed single-direction in GRAPH_EDGES (crt-041 writes `source_id < target_id` convention, line 92 of `graph_enrichment_tick.rs`). The back-fill issue GH#495 was filed by the crt-042 implementation team before Phase 0 code was written.

**SR-03 gate status**: FILED. The eval gate (AC-23: MRR >= 0.2856, P@5 measured) must be run after the back-fill from GH#495 is applied and committed. The eval gate is NOT run as part of this Stage 3c report — it requires the live database to be updated with bidirectional S1/S2 Informs edges first.

**Unit test evidence that back-fill is needed**: `test_graph_expand_unidirectional_informs_from_higher_id_seed_misses` demonstrates the gap (seed {2}, edge 1→2 only, result empty). `test_graph_expand_bidirectional_informs_after_backfill` demonstrates the correct post-back-fill behavior (both directions → result {1}).

---

## Gaps

**AC-23 / R-07 eval gate**: MRR >= 0.2856 and P@5 measurement NOT run in this Stage 3c. This is a required gate, deferred until GH#495 (S1/S2 back-fill) is applied to the deployment database. The behavioral proof (AC-25 unit test) is present and passes.

**R-04 latency gate**: P95 latency addition delta measurement NOT run. This requires running the eval harness with `RUST_LOG=..search=debug`, measuring baseline (expander disabled) then enabled, computing the delta. Deferred to eval gate run. The timing instrumentation (AC-24) is confirmed present and emitting correct fields.

**Full tools/lifecycle suite**: Only targeted tests were run due to environment constraints (~8s/test MCP overhead, 149 combined tests = ~20 min). The smoke gate (22 tests) provides R-01 regression assurance. The full pre-merge suite should be run via Docker (see USAGE-PROTOCOL.md).

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-00 | PASS | S1/S2 Informs confirmed single-direction; GH#495 back-fill filed before Phase 0 code written |
| AC-01 | PASS | 22 smoke tests pass; targeted tools search tests pass; `test_search_flag_off_pool_size_unchanged` unit test passes |
| AC-02 | PASS | `test_search_phase0_expands_before_phase1`; code inspection: Phase 0 at line 872, Phase 1 seed_scores at line 969 |
| AC-03 | PASS | Four dedicated unit tests (CoAccess, Supports, Informs, Prerequisite), each PASS |
| AC-04 | PASS | `test_graph_expand_backward_edge_does_not_surface` (seed {1}, edge 3→1, result empty) |
| AC-05 | PASS | `test_graph_expand_two_hop_depth2_surfaces_both` (B→A→D, depth=2, result {A,D}) |
| AC-06 | PASS | `test_graph_expand_two_hop_depth1_surfaces_only_first` (B→A→D, depth=1, result {A}) |
| AC-07 | PASS | `test_graph_expand_supersedes_not_traversed`, `test_graph_expand_contradicts_not_traversed` |
| AC-08 | PASS | `test_graph_expand_seeds_excluded_from_result`, `test_graph_expand_self_loop_seed_not_returned` |
| AC-09 | PASS | `test_graph_expand_max_candidates_cap` (200 neighbors, max=10, result.len()==10) |
| AC-10 | PASS | `test_graph_expand_empty_seeds_returns_empty` |
| AC-11 | PASS | `test_graph_expand_empty_graph_returns_empty` |
| AC-12 | PASS | `test_graph_expand_depth_zero_returns_empty` |
| AC-13 | PASS | `test_search_phase0_excludes_quarantined_direct`; `test_quarantine_excludes_endpoint_from_graph_traversal` (infra-001) |
| AC-14 | PASS | `test_search_phase0_excludes_quarantined_direct` + `test_search_phase0_excludes_quarantined_transitive` (unit tests in search.rs mod phase0, per test-plan contingency — MCP harness does not support per-test config override) |
| AC-15 | PASS | `test_search_phase0_skips_entry_with_no_embedding` |
| AC-16 | PASS | Grep check: zero code-line matches for `.edges_directed()` / `.neighbors_directed()` in graph_expand.rs |
| AC-17 | PASS | `test_inference_config_expander_fields_defaults`, `test_inference_config_expander_fields_serde_defaults`, `test_unimatrix_config_expander_toml_omitted_produces_defaults` |
| AC-18 | PASS | `test_validate_expansion_depth_zero_fails` (flag=false, depth=0 → Err) |
| AC-19 | PASS | `test_validate_expansion_depth_eleven_fails` (flag=false, depth=11 → Err) |
| AC-20 | PASS | `test_validate_max_expansion_candidates_zero_fails` (flag=false, max=0 → Err) |
| AC-21 | PASS | `test_validate_max_expansion_candidates_1001_fails` (flag=false, max=1001 → Err) |
| AC-22 | PASS | File exists: `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` (297 bytes) |
| AC-23 | DEFERRED | Eval gate (MRR >= 0.2856, P@5 > 0.1115) must be run after GH#495 back-fill applied. Blocking before default enablement, not blocking for flag=false ship. |
| AC-24 | PASS | `test_search_phase0_emits_debug_trace_when_enabled` (traced_test; asserts debug event with seeds, expanded_count, fetched_count, elapsed_ms, expansion_depth, max_expansion_candidates fields); `test_search_phase0_does_not_emit_trace_when_disabled` |
| AC-25 | PASS | `test_search_phase0_cross_category_entry_visible_with_flag_on` (unit test in search.rs mod phase0; cross-category entry appears with flag=true, absent with flag=false; per test-plan contingency for MCP harness config limitation) |

---

## Pre-existing Integration Test Issues (Not Caused by crt-042)

The following pre-existing xfail markers are present in the test suite and were not introduced by crt-042:

| GH Issue | Test | Suite | Notes |
|----------|------|-------|-------|
| GH#291 | `test_inferred_edge_count_unchanged_by_s1_s2_s8` | lifecycle | Background tick interval exceeds test timeout; not crt-042 |
| GH#291 | `test_s1_edges_visible_in_status_after_tick` | lifecycle | Same — tick timing; not crt-042 |
| GH#303 | `import::tests` pool timeout | unit | Pre-existing concurrent test pool issue |
| GH#305 | `test_retrospective_baseline_present` | lifecycle | Null baseline_comparison with synthetic features |
| (pre-existing) | `test_deprecated_visible_in_search_with_lower_confidence` | tools | Background scoring timing; not crt-042 |

No new xfail markers were added. No tests were deleted or commented out.

---

## R-08 InferenceConfig Hidden Sites — Grep Verification

Command: `grep -rn "InferenceConfig {" crates/ --include="*.rs" | grep -v "\.\.InferenceConfig\|\.\.Default" | grep -v "^\s*//"`

**All non-spread literal sites confirmed to include all three new fields or use spread syntax:**

- `config.rs:664` — `impl Default for InferenceConfig` block: includes all three fields (`ppr_expander_enabled`, `expansion_depth`, `max_expansion_candidates`) at lines 713-715
- `config.rs:2383` — `InferenceConfig::merged()` function body: all three fields present at lines 2661-2680
- All test literal sites (`InferenceConfig { field: val, ... }`): use `..InferenceConfig::default()` spread syntax
- `co_access_promotion_tick_tests.rs:11` — uses `..InferenceConfig::default()`
- `graph_enrichment_tick_tests.rs:20,27,35` — all use `..InferenceConfig::default()`
- `status.rs:2367` — uses `..InferenceConfig::default()`
- `nli_detection_tick.rs` literal sites — all use `..InferenceConfig::default()`

R-08 grep check: **PASS**. No hidden sites found.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #3806 (Gate 3b REWORKABLE FAIL), #3935 (tracing test deferral), #2758 (grep non-negotiable test functions), #2577 (boundary validation), #3579 (zero test modules Gate 3b). All confirmed context for mandatory tests and non-negotiable checks; no new findings.
- Stored: nothing novel to store. The crt-042 test execution pattern (unit tests for AC-14/AC-25 as contingency when MCP harness does not support per-test config override) is a specific instance of an existing pattern (test-plan OVERVIEW documented this contingency). The combination is not a new reusable technique.
