# Agent Report: col-026-agent-6-knowledge-reuse-extension

**Component**: FeatureKnowledgeReuse extension + batch metadata lookup (Component 3)
**Feature**: col-026
**Agent ID**: col-026-agent-6-knowledge-reuse-extension

---

## Files Modified

- `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` — primary implementation
- `crates/unimatrix-observe/src/lib.rs` — export new types (EntryRef, PhaseStats, GateResult, ToolDistribution)

Note: `tools.rs` changes (call site update: `compute_knowledge_reuse_for_sessions` signature, `batch_entry_meta_lookup`, `build_batch_meta_query`, `total_stored` query) were already committed by the wave-1 phase-stats agent. Confirmed present in commit `e432afe`.

---

## What Was Implemented

### 1. `EntryMeta` struct in `knowledge_reuse.rs`

New `pub struct EntryMeta { title, feature_cycle, category }`. Made `pub` (not `pub(crate)`) because it appears in the generic bound of the `pub fn compute_knowledge_reuse` signature — `pub(crate)` triggers a `private_bounds` warning on a public function.

### 2. Extended `compute_knowledge_reuse` signature

Added second generic parameter `G: Fn(&[u64]) -> HashMap<u64, EntryMeta>` as `entry_meta_lookup`. Also added `current_feature_cycle: &str` parameter for cross/intra split classification.

### 3. New field population inside `compute_knowledge_reuse`

- Step 7a: calls `entry_meta_lookup` exactly once with full ID slice (skipped when empty per ADR-003)
- Step 7b: cross/intra split — entries absent from meta_map excluded from both buckets (R-04)
- Step 7c: `total_served = delivery_count` (same value, distinct semantic name)
- Step 7d: `top_cross_feature_entries` — filtered cross-feature candidates, sorted descending by serve_count, truncated at 5, deterministic tie-breaking by id

### 4. `unimatrix-observe/src/lib.rs` exports

Added `EntryRef`, `PhaseStats`, `GateResult`, `ToolDistribution` to the crate's public re-exports. These types existed in `types.rs` (from Wave 1 agent) but were not exported, causing compile errors.

### 5. All existing tests migrated

Added `current_feature_cycle: "test-cycle"` and `empty_meta_lookup()` to all pre-existing test calls. No existing test assertions changed.

### 6. New tests (18 tests)

- `test_total_served_distinct_ids` — same ID in multiple logs counted once
- `test_cross_feature_vs_intra_cycle_split` — 4 entries, 2 cross + 2 intra
- `test_entry_meta_lookup_called_once` — AtomicUsize counter, asserts = 1
- `test_entry_meta_lookup_skipped_on_empty` — panic closure, asserts not called
- `test_top_cross_feature_entries_top_5` — 7 cross-feature, only top 5 returned, sorted descending
- `test_knowledge_reuse_partial_meta_lookup` — 5 served, 3 in meta_map, IDs 40/50 excluded
- `test_knowledge_reuse_all_meta_missing` — empty map, all new fields = 0
- `test_knowledge_reuse_all_cross_feature` — intra_cycle_reuse = 0
- `test_knowledge_reuse_all_intra_cycle` — cross_feature_reuse = 0, top entries empty
- `test_top_cross_feature_entries_fewer_than_three` — 2 entries, len = 2
- `test_top_cross_feature_entries_empty_when_none` — no cross-feature, empty vec
- `test_total_served_equals_delivery_count` — assert equal
- `test_knowledge_reuse_serde_backward_compat` — old JSON deserializes with defaults

---

## Tests

**48 passed / 0 failed** (knowledge_reuse filter)

Pre-existing failures (not introduced by this agent, confirmed via git stash):
- `test_phase_stats_no_inline_multiply` — Wave 1 scan window catches `ts: i * 1000` in test code
- `test_phase_stats_rework_detection` — Wave 1 component
- `test_subagent_start_goal_present_routes_to_index_briefing` — pre-existing
- `test_subagent_start_goal_wins_over_nonempty_prompt_snippet` — pre-existing

---

## Issues / Blockers

None. All constraints satisfied:

- ADR-003: `entry_meta_lookup` called exactly once, skipped when empty
- R-04: missing entries excluded from both buckets without panic
- `total_stored` set by caller in `tools.rs` (after calling `compute_knowledge_reuse_for_sessions`)
- `#[serde(default)]` on all new fields ensures backward compatibility
- No `.unwrap()` in non-test code
- No `todo!()` / `unimplemented!()`

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — found pattern #883 (chunked batch scan) and #3029 (write_pool_server/try_get). Applied both: chunking at 100 IDs, `write_pool_server()` for IN-clause query.
- Stored: entry #3428 "pub fn with pub(crate) type in generic bound triggers private_bounds warning — make type pub" via `/uni-store-pattern` — non-obvious: the compiler allows it (compiles and works) but emits a warning that clippy -D warnings would fail on. The fix is making the type `pub`, not fighting the closure bound.
