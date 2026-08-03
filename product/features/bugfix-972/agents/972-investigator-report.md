# Agent Report: 972-investigator

Diagnosis posted: https://github.com/dug-21/unimatrix/issues/972#issuecomment-5160201646

## Outcome
Root cause identified with high confidence: `VectorIndex::load` (`crates/unimatrix-vector/src/persistence.rs:202`) rebuilds the IdMap from the DB `vector_map` table with no validation against the graph actually loaded from disk. After a DB-only copy, `contains()` (`index.rs:293`) returns true for every entry (heal Sub-case B, `services/status.rs:1119`, matches zero) and `stale_count()` (`index.rs:334`) saturates to 0 (compaction never triggers) — both repair paths structurally blind to DB-vs-index divergence.

Proposed fix (single insertion point, `VectorIndex::load` between `load_hnsw` and `from_parts`): enumerate loaded-graph origin_ids (all layers — IterPoint, GH#286 trap), filter `vector_map` mappings to graph-present data_ids, warn with recorded/actual/dropped counts. Truthful IdMap makes the existing capped heal fire unmodified; retained mappings keep valid points searchable during repopulation. No new healing logic. Cannot reintroduce the #5718 deadlock (removes the shared-source-of-truth lie both legs depended on).

Missing test: load a dump whose graph under-counts `vector_map` → assert `contains()` false for graph-absent entries; server-level tick-heals-and-search round-trip (#2676 pattern).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — #5718 (same incident, mechanism + DELETE cure), #5272 (boot fallback / reuse heal), #3761 (heal invariants), #1712 (all-layers iteration trap), #2676 (test pattern); context_get full text on #5718, #5272, #3761.
- Stored: nothing novel to store — root-cause mechanism already captured in lesson #5718; fix-side insight deferred to post-implementation retro.
