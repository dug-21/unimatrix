# Risk Coverage Report: crt-021 (W1-1 Typed Relationship Graph)

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `graph_penalty` behavioral regression on TypedRelationGraph | 30 existing `graph.rs` unit tests (all penalty/traversal scenarios: ORPHAN, DEAD_END, PARTIAL_SUPERSESSION, CLEAN_REPLACEMENT, hop-decay, cycles) | PASS | Full |
| R-02 | `edges_of_type` filter boundary bypassed by direct `.edges_directed()` call | `test_graph_penalty_identical_with_mixed_edge_types`, `test_find_terminal_active_ignores_non_supersedes_edges`, `test_edges_of_type_filters_correctly`, `test_edges_of_type_empty_graph_returns_empty_iterator`; code inspection confirms zero direct `.edges_directed()` calls in `graph_penalty`, `find_terminal_active`, `dfs_active_reachable`, `bfs_chain_depth` | PASS | Full |
| R-03 | `bootstrap_only=1` edges reach `graph_penalty` | `test_build_typed_graph_excludes_bootstrap_only_edges`, `test_build_typed_graph_includes_confirmed_excludes_bootstrap`, `test_graph_penalty_with_bootstrap_only_supersedes_returns_no_chain_penalty` | PASS | Full |
| R-04 | Tick sequencing violated — compaction races with TypedGraphState rebuild | `test_background_tick_compacts_orphaned_graph_edges`, `test_background_tick_compaction_removes_multiple_orphaned_edges`, `test_background_tick_compaction_handles_empty_graph_edges`, `test_background_tick_compaction_uses_write_pool_not_analytics_queue`, `test_background_tick_compaction_completes_within_budget` | PASS | Full |
| R-05 | Cold-start regression — `TypedGraphState::new()` does not set `use_fallback=true` | `test_typed_graph_state_handle_swap_in_tick_pattern` (asserts `use_fallback=true` on new handle), `test_typed_graph_state_handle_cycle_sets_fallback_without_swap`, `test_effectiveness_search_ordering_after_cold_start` (infra-001) | PASS | Full |
| R-06 | CoAccess weight normalization NULL on empty `co_access` table (High likelihood, kills migration) | `test_v12_to_v13_empty_co_access_succeeds`, `test_v12_to_v13_empty_entries_and_co_access`, `test_v12_to_v13_co_access_all_below_threshold` | PASS | Full |
| R-07 | `weight: f32` NaN/Inf propagation into `GRAPH_EDGES` | `test_weight_guard_rejects_nan`, `test_weight_guard_rejects_positive_infinity`, `test_weight_guard_rejects_negative_infinity`, `test_weight_guard_accepts_zero`, `test_weight_guard_accepts_half`, `test_weight_guard_accepts_one`, `test_weight_guard_accepts_f32_max`, `test_analytics_graph_edge_drain_rejects_nan_weight`, `test_relation_edge_weight_validation_rejects_nan`, `test_relation_edge_weight_validation_rejects_inf`, `test_relation_edge_weight_validation_rejects_neg_inf`, `test_relation_edge_weight_validation_passes_valid` | PASS | Full |
| R-08 | v12→v13 migration not idempotent (duplicate edges on double run) | `test_v12_to_v13_idempotent_double_run` | PASS | Full |
| R-09 | `sqlx-data.json` stale after schema change | `SQLX_OFFLINE=true cargo build --workspace` succeeds | PASS | Full |
| R-10 | `RelationType` string deserialization silent failure | `test_relation_type_from_str_unknown_returns_none`, `test_build_typed_graph_skips_unknown_relation_type` | PASS | Full |
| R-11 | Orphaned-edge compaction cost regression (unbounded DELETE) | `test_background_tick_compaction_completes_within_budget` | PASS | Full |
| R-12 | Supersedes edge source divergence — GRAPH_EDGES vs `entries.supersedes` | `test_supersedes_edges_from_entries_not_graph_edges_table`, `test_supersedes_edge_not_doubled_by_graph_edges_row` | PASS | Full |
| R-13 | `AnalyticsWrite::GraphEdge` shed silently drops bootstrap edges during migration | Code inspection: zero `AnalyticsWrite` references in `migration.rs`; `inspect_migration_no_analytics_write_calls` (documents boundary) | PASS | Full |
| R-14 | `TypedGraphState` rename incomplete — residual `SupersessionState` / `SupersessionStateHandle` symbols | `cargo build --workspace` passes; grep over all crates shows zero non-comment occurrences of `SupersessionState` / `SupersessionStateHandle` | PASS | Full |
| R-15 | CoAccess weight formula — flat 1.0 instead of normalized formula | `test_v12_to_v13_co_access_threshold_and_weights` (asserts weight(count=5)=1.0, weight(count=3)=0.6, strict ordering, all weights in (0, 1]) | PASS | Full |

---

## Test Results

### Unit Tests

| Crate | Binary | Total | Passed | Failed |
|-------|--------|-------|--------|--------|
| unimatrix-engine | lib | 291 | 291 | 0 |
| unimatrix-engine | pipeline_calibration | 14 | 14 | 0 |
| unimatrix-engine | pipeline_regression | 3 | 3 | 0 |
| unimatrix-engine | pipeline_retrieval | 6 | 6 | 0 |
| unimatrix-engine | test_scenarios_unit | 7 | 7 | 0 |
| unimatrix-store | lib | 129 | 129 | 0 |
| unimatrix-server | lib | 1462 | 1462 | 0 |
| unimatrix-server | integration (16 tests) | 16 | 16 | 0 |
| unimatrix-server | integration (16 tests) | 16 | 16 | 0 |
| unimatrix-server | integration (7 tests) | 7 | 7 | 0 |

**Unit test subtotal: 1951 passed, 0 failed**

Graph-specific unit tests in `unimatrix-engine` (55 total):
- 30 pre-existing graph.rs tests: all pass on `TypedRelationGraph` without modification
- 25 new crt-021 tests: all pass

#### Pre-existing config.rs doctest failure (not a regression)
- `crates/unimatrix-server/src/infra/config.rs - infra::config (line 21)` — FAILED
- **Triage**: Pre-existing from dsn-001, unrelated to crt-021. Not fixed in this feature. No xfail marker needed for doctests; it is an excluded test binary.

### Integration Tests (migration_v12_to_v13)

| Test | Result | Covers |
|------|--------|--------|
| `test_current_schema_version_is_13` | PASS | AC-18 |
| `test_v12_to_v13_supersedes_bootstrap` | PASS | AC-05, AC-06, AC-18, R-01 |
| `test_v12_to_v13_empty_co_access_succeeds` | PASS | R-06, AC-07 |
| `test_v12_to_v13_co_access_threshold_and_weights` | PASS | AC-07, R-15 |
| `test_v12_to_v13_co_access_all_below_threshold` | PASS | R-06 (counts<threshold) |
| `test_v12_to_v13_no_contradicts_bootstrapped` | PASS | AC-08 |
| `test_v12_to_v13_idempotent_double_run` | PASS | R-08, AC-05 |
| `test_v13_bootstrap_only_promotion_delete_insert` | PASS | AC-21 |
| `test_v12_to_v13_empty_entries_and_co_access` | PASS | R-06 (edge case) |
| `test_v12_to_v13_supersedes_edge_direction` | PASS | AC-06 (VARIANCE 1) |
| `test_v12_to_v13_supersedes_bootstrap_only_zero` | PASS | AC-06 |
| `inspect_migration_no_analytics_write_calls` | PASS | R-13 |

**Migration integration test subtotal: 12 passed, 0 failed**

### Integration Tests (infra-001)

| Suite | Total | Passed | Failed | XFailed |
|-------|-------|--------|--------|---------|
| smoke | 20 | 20 | 0 | 0 |
| tools | 72 | 72 | 0 | 1 (GH#305, pre-existing) |
| lifecycle | 26 | 26 | 0 | 1 (pre-existing, TICK_INTERVAL env) |
| confidence | 13 | 13 | 0 | 0 |

**infra-001 subtotal: 131 passed, 0 failed, 2 xfailed (both pre-existing)**

#### Pre-existing xfail details
- `test_retrospective_baseline_present`: `@pytest.mark.xfail(reason="Pre-existing: GH#305 — baseline_comparison null when synthetic features lack delivery counter registration")` — unrelated to crt-021.
- `test_auto_quarantine_after_consecutive_bad_ticks`: xfail with reason about UNIMATRIX_TICK_INTERVAL_SECONDS env var — unrelated to crt-021.

---

## Overall Test Counts

| Category | Total | Passed | Failed |
|----------|-------|--------|--------|
| Unit (all crates) | 1951 | 1951 | 0 |
| Migration integration | 12 | 12 | 0 |
| infra-001 smoke | 20 | 20 | 0 |
| infra-001 tools+lifecycle+confidence | 111 | 111 | 0 (+ 2 pre-existing xfail) |
| **Total** | **2094** | **2094** | **0** |

---

## Gaps

None. All 15 risks from RISK-TEST-STRATEGY.md have explicit test coverage mapped above.

**R-09 (sqlx-data.json)**: Verified by `SQLX_OFFLINE=true cargo build --workspace` succeeding. This is a compile-time gate, not a runtime test; coverage is confirmed.

**R-13 (AnalyticsWrite shed)**: Verified by code inspection plus `inspect_migration_no_analytics_write_calls` test. The risk is structural (migration uses direct SQL, not analytics queue) and the code inspection confirms it. No analytics queue drain test was needed because the migration path never enqueues.

**R-14 (rename completeness)**: Verified by compiler + grep. Zero non-comment occurrences of `SupersessionState`/`SupersessionStateHandle` across all crates.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `cargo test -p unimatrix-engine` — 291 passed; `grep -r "SupersessionGraph" crates/unimatrix-engine/src/` returns zero matches outside comments |
| AC-02 | PASS | `test_relation_type_roundtrip_all_variants` (all five variants), `test_relation_type_prerequisite_roundtrips` |
| AC-03 | PASS | `test_relation_edge_weight_validation_rejects_nan/inf/neg_inf`, `test_relation_edge_weight_validation_passes_valid` |
| AC-04 | PASS | `test_graph_edges_table_created_on_fresh_db`, `test_graph_edges_columns_and_types`, `test_graph_edges_indexes_exist`, `test_graph_edges_metadata_default_null`, `test_graph_edges_unique_constraint_prevents_duplicate` |
| AC-05 | PASS | `test_v12_to_v13_supersedes_bootstrap` (schema_version=13, graph_edges exists, Supersedes rows present) |
| AC-06 | PASS | `test_v12_to_v13_supersedes_bootstrap` (bootstrap_only=0, source='entries.supersedes', direction source_id=old, target_id=new); `test_v12_to_v13_supersedes_edge_direction`; `test_v12_to_v13_supersedes_bootstrap_only_zero` |
| AC-07 | PASS | `test_v12_to_v13_co_access_threshold_and_weights` (threshold=3, weight(5)=1.0, weight(3)=0.6, bootstrap_only=0); `test_v12_to_v13_empty_co_access_succeeds` (empty co_access succeeds, zero rows) |
| AC-08 | PASS | `test_v12_to_v13_no_contradicts_bootstrapped` (zero Contradicts rows); schema column existence confirmed by `test_graph_edges_columns_and_types` |
| AC-09 | PASS | `test_analytics_write_graph_edge_variant_name` (variant_name()="GraphEdge"); `test_analytics_graph_edge_drain_inserts_row`, `test_analytics_graph_edge_bootstrap_only_field_persisted`, `test_analytics_graph_edge_metadata_column_is_null` |
| AC-10 | PASS | All 30 pre-existing graph.rs tests pass unchanged on `TypedRelationGraph`; `test_graph_penalty_identical_with_mixed_edge_types` |
| AC-11 | PASS | `test_graph_penalty_identical_with_mixed_edge_types` (Supersedes + Contradicts on same node produces same penalty as Supersedes-only graph) |
| AC-12 | PASS | `test_build_typed_graph_excludes_bootstrap_only_edges` (all bootstrap_only=true → zero edges); `test_build_typed_graph_includes_confirmed_excludes_bootstrap` (one confirmed + one bootstrap → only confirmed in graph) |
| AC-13 | PASS | `test_background_tick_compacts_orphaned_graph_edges` (seeds GRAPH_EDGES, triggers tick, asserts graph contains expected edges) |
| AC-14 | PASS | `test_background_tick_compacts_orphaned_graph_edges`, `test_background_tick_compaction_removes_multiple_orphaned_edges` (orphaned rows deleted before rebuild) |
| AC-15 | PASS | `test_typed_graph_state_handle_swap_in_tick_pattern` (new_handle() → use_fallback=true); `test_effectiveness_search_ordering_after_cold_start` (infra-001 lifecycle) |
| AC-16 | PENDING | Manual verification required: `context_lookup` for entry #2417 (new ADR) and entry #1604 (deprecated ADR). Not executable in automated tests. |
| AC-17 | PASS | `test_weight_guard_rejects_nan/positive_infinity/negative_infinity` (all rejected); `test_analytics_graph_edge_drain_rejects_nan_weight` (NaN not written to graph_edges) |
| AC-18 | PASS | `test_current_schema_version_is_13`, `test_v12_to_v13_supersedes_bootstrap` (schema_version=13 after migration) |
| AC-19 | PASS | `SQLX_OFFLINE=true cargo build --workspace` succeeds; sqlx-data.json regenerated and committed |
| AC-20 | PASS | `test_relation_type_prerequisite_roundtrips`; grep confirms no INSERT or write-path references to `Prerequisite` beyond enum definition and round-trip test |
| AC-21 | PASS | `test_v13_bootstrap_only_promotion_delete_insert` (bootstrap_only=1 → DELETE → INSERT bootstrap_only=0 → assert 0; idempotent INSERT OR IGNORE → one row) |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: "procedure") for testing procedures — found #487 (how to run workspace tests without hanging), #750 (pipeline validation tests), #2326 (fire-and-forget async pattern test strategy). Not directly applicable to crt-021 execution; existing patterns were sufficient.
- Stored: nothing novel to store. The migration integration test pattern (`create_v12_database` fixture + `SqlxStore::open` trigger + direct SQL assertions) is feature-specific to the v12→v13 schema shape. The weight guard unit test pattern (pass NaN/Inf/valid to a validation guard) is a standard Rust testing convention. Neither is cross-feature novel enough to warrant a Unimatrix entry.
