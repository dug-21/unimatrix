# Agent Report: 444-agent-1-fix

## Summary

Implemented all 5 fixes for GH #444 (enforce index-active-set invariant).

## Files Modified

- `crates/unimatrix-vector/src/index.rs` — added `VectorIndex::remove_entry()` (IdMap-only mutation, marks HNSW point stale)
- `crates/unimatrix-store/src/write_ext.rs` — added `delete_vector_mapping()` and `update_embedding_dim()` for prune/heal confirmation writes
- `crates/unimatrix-server/src/infra/config.rs` — added `InferenceConfig.heal_pass_batch_size` (default 20, serde default, config merge wired)
- `crates/unimatrix-server/src/services/status.rs` — prune pass + heal pass (Fix 1, Fix 2) in `run_maintenance()`; `unembedded_active_count` SQL count + updated `embedding_consistency_score` formula in `compute_report()` (Fix 5); added `InferenceConfig` parameter to `run_maintenance()`
- `crates/unimatrix-server/src/services/typed_graph.rs` — filter quarantined entries from `TypedGraphState::rebuild()` (Fix 4); deprecated retained for SR-01
- `crates/unimatrix-server/src/server.rs` — `restore_with_audit()` re-inserts into HNSW when `embedding_dim > 0` and not in index (Fix 3); heal pass handles `embedding_dim=0` case
- `crates/unimatrix-server/src/background.rs` — thread `inference_config` through `maintenance_tick()` to `run_maintenance()`
- `crates/unimatrix-server/src/mcp/response/status.rs` — added `unembedded_active_count: u64` field to `StatusReport`; surfaced in summary format output
- `crates/unimatrix-server/src/mcp/response/mod.rs` — added `unembedded_active_count: 0` to all test `StatusReport` initializers

## New Tests

### unimatrix-vector (index.rs)
- `test_remove_entry_not_in_contains_after_removal`
- `test_remove_entry_idempotent`
- `test_remove_entry_increments_stale_count`
- `test_remove_entry_nonexistent_is_noop`

### unimatrix-server (services/status.rs — bugfix_444_tests)
- `test_prune_pass_removes_quarantined_vector`
- `test_metric_unembedded_active_count_and_consistency_score`
- `test_inference_config_heal_pass_batch_size_default`
- `test_inference_config_heal_pass_batch_size_configurable`

### unimatrix-server (services/typed_graph.rs)
- `test_rebuild_excludes_quarantined_entries`
- `test_rebuild_retains_deprecated_entries`

## Tests: pass/fail

All 10 new tests pass. Full workspace: 0 failures.

Previous count was approximately 2325 unit + infra tests. All pass.

## Design Decisions

### Tick ordering: prune → heal → compact

Prune fires first so quarantined HNSW points are absent from both the heal set and the compaction input. Heal fires second so newly-embedded entries are included in compaction rather than requiring a second cycle.

### `embedding_consistency_score` formula change

The old formula only applied when `check_embeddings=true` (opt-in ONNX path). The new formula uses the always-available SQL count `unembedded_active_count / total_active`. When `check_embeddings=true`, the ONNX-derived count still takes precedence (more precise). When `false` but `total_active > 0`, the SQL-derived formula fires. This makes the metric self-reporting without any opt-in.

### Heal pass: VECTOR_MAP row may already exist

Per the investigator report, when `context_store` runs with embed adapter unavailable, a VECTOR_MAP row IS written (with an allocated `data_id`) but no HNSW point is inserted. The heal pass checks `get_vector_mapping()` first: if a row exists, it reuses the `data_id`; if not (unusual case), it allocates and writes a new one.

### `restore_with_audit` HNSW re-insert is best-effort

If the embed service is unavailable at restore time, the restore still succeeds (status updated). The heal pass will re-embed the entry on the next tick. This matches the "heal pass covers it" contract described in the approved fix approach.

### Missing test: heal pass sub-case A (end-to-end with real ONNX)

The heal pass sub-case A test (embed adapter functioning, actually embeds an entry with `embedding_dim=0`) requires the ONNX model to be present. The test harness in `test_support.rs` gates on model availability. This test is omitted from the unit test suite — it belongs in the integration test suite (like existing `pipeline_e2e` tests) that run against a real model. The prune/metric/config tests are all model-free.

## Issues / Blockers

None.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned ADR entries for HNSW, graph compaction, crt-005, quarantine restore, and lesson #3761 from the investigator. Applied the prune-before-heal ordering insight.
- Stored: entry #3762 "run_maintenance() tick order: prune → heal → compact (never reverse)" via `/uni-store-pattern`
