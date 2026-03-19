# Agent Report: crt-021-agent-7-server-state

## Component
`server-state` — `crates/unimatrix-server/src/services/typed_graph.rs`

## Status
COMPLETE

## Files Created / Modified / Deleted

**Created:**
- `crates/unimatrix-server/src/services/typed_graph.rs` — new file; `TypedGraphState`, `TypedGraphStateHandle`, tests

**Modified:**
- `crates/unimatrix-engine/src/graph.rs` — added `Clone` to `TypedRelationGraph`; added `TypedRelationGraph::empty()`
- `crates/unimatrix-server/src/services/mod.rs` — `supersession` → `typed_graph`; `SupersessionState` → `TypedGraphState` throughout; `supersession_state_handle()` → `typed_graph_handle()`
- `crates/unimatrix-server/src/services/search.rs` — `SupersessionStateHandle` → `TypedGraphStateHandle`; field `supersession_state` → `typed_graph_handle`; removed `build_supersession_graph` call on hot path; reads pre-built graph from handle under short read lock (FR-22)
- `crates/unimatrix-server/src/services/briefing.rs` — test helper updated: `SupersessionState` → `TypedGraphState`
- `crates/unimatrix-server/src/background.rs` — `SupersessionState/Handle` → `TypedGraphState/Handle`; rebuild block updated to call `TypedGraphState::rebuild` with correct cycle-vs-error branching
- `crates/unimatrix-server/src/main.rs` — `supersession_state_handle()` → `typed_graph_handle()` (both daemon and stdio paths)

**Deleted:**
- `crates/unimatrix-server/src/services/supersession.rs`

## Build / Tests

- `cargo build --workspace`: PASS (zero errors)
- `cargo test -p unimatrix-server --lib`: 1455 passed, 0 failed
- `cargo test --workspace --lib`: all crates pass (0 failures)
- Pre-existing doctest failure in `config.rs` confirmed pre-existing (verified by stash/restore)
- No TODO, unimplemented!, or HACK in non-test code

## Implementation Notes

### FR-22 Invariant Enforced
`build_typed_relation_graph` is never called on the search hot path. The search path reads the pre-built `TypedRelationGraph` from `typed_graph_handle` under a short read lock, clones it, releases the lock, then calls `graph_penalty`/`find_terminal_active` on the clone. Lock ordering R-01 preserved.

### TypedRelationGraph::Clone Added
`graph.rs` needed `#[derive(Clone)]` on `TypedRelationGraph` (required by the search path clone pattern). `StableGraph<u64, RelationEdge>` and `HashMap<u64, NodeIndex>` are both `Clone`, so this is safe and zero-copy of the field types.

### Two GraphEdgeRow Types
`unimatrix-engine::graph::GraphEdgeRow` and `unimatrix-store::GraphEdgeRow` are identical structs but distinct Rust types. `build_typed_relation_graph` expects the engine type. `TypedGraphState::rebuild()` does a field-by-field `.map()` to convert store rows to engine rows before passing to the builder. Stored as pattern entry #2451.

### Cycle Detection in Background Tick
`TypedGraphState::rebuild()` returns `Err(StoreError::InvalidInput { reason: "supersession cycle detected" })` on `GraphError::CycleDetected`. The background tick's rebuild block matches on the error message to distinguish cycle from I/O failure, setting `use_fallback=true` on the existing handle (retaining old graph) rather than discarding it.

### No Type Aliases
Per NF-07 constraint: no type aliases for the rename. All 20+ call sites updated directly so the compiler enforces completeness.

## Deviations from Pseudocode

None. All pseudocode spec sections followed exactly.

## Knowledge Stewardship
- Queried: `/uni-query-patterns` for `unimatrix-server` — found background-tick state cache pattern (#1560), confirming Arc<RwLock<T>> + sole-writer tick pattern. Applied as designed.
- Stored: entry #2451 "unimatrix-server: two GraphEdgeRow types require field-by-field mapping in typed_graph.rs" via `/uni-store-pattern` — gotcha invisible in source code that would cause E0308 at compile time.
