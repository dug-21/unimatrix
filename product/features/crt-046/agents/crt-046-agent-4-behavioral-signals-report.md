# Agent Report: crt-046-agent-4-behavioral-signals

## Task
Wave 2 — Implement `services/behavioral_signals.rs` with six `pub(crate)` functions for behavioral signal collection, edge emission, goal cluster population, and briefing blending.

## Files Modified

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/services/behavioral_signals.rs` | **Created** — 999 lines including 28 unit tests |
| `crates/unimatrix-server/src/services/mod.rs` | Added `pub(crate) mod behavioral_signals;` declaration |

## Functions Implemented

| Function | Signature | Notes |
|----------|-----------|-------|
| `collect_coaccess_entry_ids` | `(obs: &[ObservationRow]) -> (HashMap<String, Vec<(u64, i64)>>, usize)` | Filters to context_get, parses input JSON `id` field, counts parse failures |
| `build_coaccess_pairs` | `(by_session: HashMap<...>) -> (Vec<(u64,u64)>, bool)` | Self-pair exclusion (DN-3) before dedup; cap at 200 at enumeration time |
| `outcome_to_weight` | `(outcome: Option<&str>) -> f32` | "success" → 1.0; all others → 0.5 |
| `emit_behavioral_edges` | `async (store, pairs, weight) -> (usize, usize)` | Direct write_pool_server; pattern #4041 governs counters |
| `populate_goal_cluster` | `async (store, fc, emb, ids, phase, outcome) -> Result<bool>` | INSERT OR IGNORE; calls store.insert_goal_cluster |
| `blend_cluster_entries` | `(semantic, cluster_with_scores, k) -> Vec<IndexEntry>` | Pure function; score-based interleave, dedup, top-k |

Module-private helper: `write_graph_edge` — executes `INSERT OR IGNORE INTO graph_edges` on `write_pool_server()`, returns `Result<bool>` keyed on `rows_affected() > 0`.

## Tests

28/28 pass. Zero failures.

| Test Group | Count |
|------------|-------|
| `collect_coaccess_entry_ids` | 5 |
| `build_coaccess_pairs` | 7 |
| `outcome_to_weight` | 4 |
| `emit_behavioral_edges` (store required) | 5 |
| `populate_goal_cluster` (store required) | 2 |
| `blend_cluster_entries` (pure) | 5 |

Gate-blocking tests covered:
- E-02: Self-pair exclusion (`test_build_coaccess_pairs_self_pairs_excluded`)
- R-02-contract: UNIQUE conflict → `edges_enqueued == 0, pairs_skipped == 1` (`test_emit_behavioral_edges_unique_conflict_not_counted`)
- Cap enforced at enumeration time (`test_build_coaccess_pairs_cap_enforced_at_200`)

## Build Status

- `cargo build -p unimatrix-server`: 0 errors, 27 pre-existing warnings
- `cargo build --workspace`: 0 errors
- `cargo fmt -p unimatrix-server`: applied (whitespace only)
- `cargo clippy -p unimatrix-server` on behavioral_signals.rs: 0 warnings from new code

## Deviations from Pseudocode

None. Implementation follows validated pseudocode exactly.

Key decisions honoured:
- `write_graph_edge` uses `write_pool_server()` directly — NOT `enqueue_analytics` (ADR-006)
- `edges_enqueued` increments on `Ok(true)` ONLY (pattern #4041)
- Self-pair filter `(a == b)` applied BEFORE dedup in `build_coaccess_pairs` (DN-3)
- `blend_cluster_entries` is pure — no store access
- `PAIR_CAP = 200` enforced at enumeration time (early return inside loop)
- `populate_goal_cluster` is the final step — called only after entry_ids fully assembled (R-06)

## Commit

`aee662af` — `impl(behavioral-signals): co-access pairs + goal cluster + blend (#511)`

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced patterns #4108, #4124 (ADR-006), #4041 (write_graph_edge return contract), #3883 (direct write_pool_server for tick graph edges). All applied.
- Stored: nothing novel to store — the key patterns (#4041 write_graph_edge return contract, ADR-006 direct write_pool_server for behavioral edges) are already in Unimatrix and were correctly identified by the briefing. The implementation is a faithful application of those existing patterns; no new traps or gotchas were discovered during implementation.

## Issues / Blockers

None. Wave 1 (store-v22, InferenceConfig fields) was complete and compiled cleanly before this agent started.

One minor issue resolved: `crate::mcp::response::briefing` is a private module; `IndexEntry` must be imported via the public re-export at `crate::mcp::response::IndexEntry`. Fixed immediately on first build.
