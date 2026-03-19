# server-state — Pseudocode

**File renamed**: `crates/unimatrix-server/src/services/supersession.rs`
  → `crates/unimatrix-server/src/services/typed_graph.rs`

**Dependent files updated** (call-site rename ~20 sites):
- `crates/unimatrix-server/src/services/mod.rs`
- `crates/unimatrix-server/src/background.rs`
- `crates/unimatrix-server/src/main.rs`
- `crates/unimatrix-server/src/services/search.rs`
- `crates/unimatrix-server/src/server.rs`

---

## Purpose

Rename `SupersessionState`/`SupersessionStateHandle` to `TypedGraphState`/
`TypedGraphStateHandle`. Upgrade the state struct to hold a pre-built `TypedRelationGraph`
(not raw `Vec<GraphEdgeRow>` — FR-22, VARIANCE 2 governs). Update `rebuild` to query both
`all_entries` and `GRAPH_EDGES`, call `build_typed_relation_graph`, and store the result.

---

## TypedGraphState struct

```
/// In-memory tick-rebuild cache of the typed relation graph and entry snapshot.
///
/// Pre-built: the background tick calls TypedGraphState::rebuild() to construct the
/// TypedRelationGraph from GRAPH_EDGES and stores the result here. The search hot
/// path reads the pre-built graph under a short read lock — it never rebuilds per query.
/// (FR-22, SPECIFICATION governs over ARCHITECTURE §3a/3b discrepancy — VARIANCE 2)
///
/// Cold-start: empty all_entries, empty typed_graph, use_fallback=true until first tick.
///
/// All RwLock acquisitions use .unwrap_or_else(|e| e.into_inner()) for poison recovery
/// (consistent with EffectivenessState and CategoryAllowlist conventions).
pub struct TypedGraphState {
    /// Pre-built typed relation graph. Never rebuilt per search query.
    /// Empty TypedRelationGraph on cold-start.
    pub typed_graph:  TypedRelationGraph,

    /// Snapshot of all entries at last rebuild time.
    /// Used by graph_penalty / find_terminal_active (called outside the lock on a clone).
    pub all_entries:  Vec<EntryRecord>,

    /// When true, search applies FALLBACK_PENALTY without graph traversal.
    /// Set on cold-start or when CycleDetected is returned by build_typed_relation_graph.
    pub use_fallback: bool,
}

pub type TypedGraphStateHandle = Arc<RwLock<TypedGraphState>>;
```

---

## TypedRelationGraph default for cold-start

The cold-start state needs an empty `TypedRelationGraph`. Add a helper:

```
FUNCTION TypedRelationGraph::empty() -> TypedRelationGraph:
    RETURN TypedRelationGraph {
        inner: StableGraph::new(),
        node_index: HashMap::new(),
    }
```

Alternatively, `new()` with no arguments suffices if the implementer prefers. The key
requirement is that `TypedGraphState::new()` can construct a valid cold-start state without
any I/O.

---

## TypedGraphState::new (cold-start constructor)

```
FUNCTION TypedGraphState::new() -> Self:
    RETURN TypedGraphState {
        typed_graph:  TypedRelationGraph::empty(),
        all_entries:  Vec::new(),
        use_fallback: true,
    }
```

---

## TypedGraphState::new_handle (handle constructor)

```
FUNCTION TypedGraphState::new_handle() -> TypedGraphStateHandle:
    RETURN Arc::new(RwLock::new(TypedGraphState::new()))
```

Called once by `ServiceLayer` constructor (in main.rs or server.rs). Result is
`Arc::clone`-d into `SearchService` and `spawn_background_tick`.

---

## TypedGraphState::rebuild

```
FUNCTION TypedGraphState::rebuild(store: &Store) -> Result<TypedGraphState, StoreError>:

    -- Step 1: Query all entries from store (existing call — unchanged)
    LET all_entries = store.query_all_entries().await?

    -- Step 2: Query all GRAPH_EDGES rows from store (new call)
    LET all_edges = store.query_graph_edges().await?

    -- Step 3: Build typed graph from entries + edges
    --         bootstrap_only=true edges are excluded structurally inside build_typed_relation_graph.
    --         Cycle detection runs on the Supersedes sub-graph.
    LET typed_graph = MATCH build_typed_relation_graph(&all_entries, &all_edges):
        Ok(graph) → graph
        Err(GraphError::CycleDetected):
            -- Cycle detected: do not return a new state; caller handles this case.
            -- The caller (background tick) sets use_fallback=true on the existing handle.
            -- rebuild() returns Err so the tick can distinguish cycle-detected from store error.
            RETURN Err(StoreError::InternalError("supersession cycle detected".to_string()))
            -- NOTE: If StoreError has no InternalError variant, add one or repurpose an
            -- existing variant. The caller checks the error type to distinguish cycle vs. I/O.
            -- An alternative: return a special marker value. The critical behavior is that
            -- the caller retains old state and sets use_fallback=true.

    RETURN Ok(TypedGraphState {
        typed_graph,
        all_entries,
        use_fallback: false,
    })
```

Caller contract (in background tick):
```
MATCH TypedGraphState::rebuild(&store).await:
    Ok(new_state):
        LET mut guard = handle.write().unwrap_or_else(|e| e.into_inner())
        *guard = new_state
        DROP guard

    Err(e) if e is CycleDetected marker:
        LET mut guard = handle.write().unwrap_or_else(|e| e.into_inner())
        guard.use_fallback = true
        DROP guard
        tracing::error!("TypedGraphState rebuild: cycle detected; search using FALLBACK_PENALTY")

    Err(e):
        tracing::error!(error = %e, "TypedGraphState rebuild failed; retaining old state")
        -- Do NOT update the handle — retain last known good state
```

---

## Default implementation

```
impl Default for TypedGraphState:
    fn default() -> Self:
        Self::new()
```

---

## Search path usage (in search.rs)

The search path reads `typed_graph` and `all_entries` under a short read lock, clones them,
releases the lock, then calls `graph_penalty` on the clones outside the lock.

```
-- In SearchService::apply_graph_penalty (or equivalent method in search.rs):

LET (typed_graph, all_entries, use_fallback) = {
    LET guard = self.typed_graph_handle
        .read()
        .unwrap_or_else(|e| e.into_inner())
    LET result = (
        guard.typed_graph.clone(),
        guard.all_entries.clone(),
        guard.use_fallback,
    )
    DROP guard      -- read lock released here, before any graph traversal
    result
}

IF use_fallback:
    RETURN FALLBACK_PENALTY

RETURN graph_penalty(node_id, &typed_graph, &all_entries)
```

INVARIANT: `build_typed_relation_graph` is NEVER called on the search hot path.
Only `graph_penalty` is called, and only on the pre-built `typed_graph` clone.
(FR-22, crt-014 lesson learned: no store I/O or graph construction on hot path)

Note: `TypedRelationGraph` must implement `Clone`. Since it contains only
`StableGraph<u64, RelationEdge>` and `HashMap<u64, NodeIndex>`, both of which are
`Clone`, deriving or implementing `Clone` is straightforward.

---

## Call-Site Rename Map (~20 sites)

| File | Old | New |
|------|-----|-----|
| `services/supersession.rs` (entire file) | — | renamed to `services/typed_graph.rs` |
| `services/mod.rs` | `pub mod supersession;` | `pub mod typed_graph;` |
| `services/mod.rs` | `pub use supersession::*;` (if present) | `pub use typed_graph::*;` |
| `background.rs` line 43 | `use crate::services::supersession::{SupersessionState, SupersessionStateHandle}` | `use crate::services::typed_graph::{TypedGraphState, TypedGraphStateHandle}` |
| `background.rs` (all occurrences) | `SupersessionState` | `TypedGraphState` |
| `background.rs` (all occurrences) | `SupersessionStateHandle` | `TypedGraphStateHandle` |
| `main.rs` | `SupersessionState::new_handle()` | `TypedGraphState::new_handle()` |
| `main.rs` | `SupersessionStateHandle` | `TypedGraphStateHandle` |
| `server.rs` (ServiceLayer struct) | `supersession_handle: SupersessionStateHandle` | `typed_graph_handle: TypedGraphStateHandle` |
| `server.rs` (constructor + field access) | `.supersession_handle` | `.typed_graph_handle` |
| `services/search.rs` | `supersession_handle` field | `typed_graph_handle` |
| `services/search.rs` | `SupersessionStateHandle` | `TypedGraphStateHandle` |
| `services/search.rs` | `build_supersession_graph` call | removed; use pre-built `typed_graph` |
| `services/search.rs` | `state.all_entries.clone()` | retained (field name unchanged) |

No type aliases permitted. The compiler enforces all 20 sites via compilation error.
Use `cargo build --workspace` to verify completeness (NF-07, R-14).

---

## Imports Required

```
-- In typed_graph.rs:
use unimatrix_engine::graph::{
    TypedRelationGraph, build_typed_relation_graph, GraphError,
};
use unimatrix_store::GraphEdgeRow;   // or re-exported from unimatrix_core
```

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `query_all_entries()` fails | `rebuild()` returns `Err`; caller retains old state |
| `query_graph_edges()` fails | `rebuild()` returns `Err`; caller retains old state |
| `CycleDetected` | `rebuild()` returns Err (or special marker); caller sets `use_fallback=true` |
| RwLock poisoned (read) | `.unwrap_or_else(|e| e.into_inner())` recovers; serve last good state |
| RwLock poisoned (write) | `.unwrap_or_else(|e| e.into_inner())` recovers; overwrite with new state |

---

## Key Test Scenarios

1. **Cold-start state** (AC-15, R-05):
   - `TypedGraphState::new()` returns `use_fallback=true`, `all_entries=[]`,
     `typed_graph` is an empty graph.

2. **new_handle readable after creation** (existing test ported):
   - `TypedGraphState::new_handle()` returns handle; read lock produces cold-start state.

3. **Poison recovery** (ported from SupersessionState tests):
   - Poison handle by panicking under write lock; read via `unwrap_or_else` succeeds.

4. **Arc::clone shares state** (ported):
   - Write `use_fallback=false` through one Arc clone; read through another; assert consistent.

5. **Search path uses pre-built graph, not rebuild** (AC-13, FR-22):
   - Seed typed_graph_handle with a pre-built graph containing a Supersedes edge.
   - Call search path `apply_graph_penalty`.
   - Assert `graph_penalty` was called on the pre-built graph (no call to `build_typed_relation_graph`).

6. **use_fallback=true yields FALLBACK_PENALTY on search path** (AC-15):
   - Set `use_fallback=true` in handle.
   - Call `apply_graph_penalty`.
   - Assert FALLBACK_PENALTY returned without traversal.

7. **Compiler enforces rename completeness** (R-14):
   - `cargo build --workspace` must succeed with zero references to `SupersessionState`
     or `SupersessionStateHandle` in non-comment source lines.
