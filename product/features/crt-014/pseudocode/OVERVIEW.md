# crt-014 Pseudocode Overview — Topology-Aware Supersession

## Components Involved

| Component | File | Change Type |
|-----------|------|-------------|
| graph.rs | `crates/unimatrix-engine/src/graph.rs` | NEW |
| lib.rs | `crates/unimatrix-engine/src/lib.rs` | MODIFY (one line) |
| Cargo.toml | `crates/unimatrix-engine/Cargo.toml` | MODIFY (two deps) |
| confidence.rs | `crates/unimatrix-engine/src/confidence.rs` | MODIFY (removals only) |
| search.rs | `crates/unimatrix-server/src/services/search.rs` | MODIFY (import + Steps 6a/6b) |

---

## Data Flow

```
Store::query(QueryFilter::default())
  → Vec<EntryRecord>  [all entries, any status]
      ↓
build_supersession_graph(&all_entries)
  → Ok(SupersessionGraph) | Err(GraphError::CycleDetected)
      ↓ Ok path
graph_penalty(entry.id, &graph, &all_entries)          [Step 6a — per penalized entry]
  → f64 in (0.0, 1.0)  → inserted into penalty_map

find_terminal_active(entry.id, &graph, &all_entries)   [Step 6b — per superseded entry]
  → Option<u64>  → injected into results_with_scores
      ↓ CycleDetected path
FALLBACK_PENALTY                                        [Step 6a — all penalized entries]
entry.superseded_by                                     [Step 6b — single-hop fallback]
```

The `all_entries` slice is loaded once before Step 6a via `Store::query(QueryFilter::default())`. It is passed to both `graph_penalty` and `find_terminal_active` — these functions do linear entry lookup by id. The slice is read-only throughout the search step.

---

## Shared Types Introduced

All new types live in `unimatrix-engine/src/graph.rs`:

```
pub enum GraphError { CycleDetected }

pub struct SupersessionGraph {
    pub(crate) inner: StableGraph<u64, ()>,        // petgraph, directed, stable
    pub(crate) node_index: HashMap<u64, NodeIndex>, // O(1) entry id → graph node
}

pub const ORPHAN_PENALTY: f64 = 0.75
pub const CLEAN_REPLACEMENT_PENALTY: f64 = 0.40
pub const HOP_DECAY_FACTOR: f64 = 0.60
pub const PARTIAL_SUPERSESSION_PENALTY: f64 = 0.60
pub const DEAD_END_PENALTY: f64 = 0.65
pub const FALLBACK_PENALTY: f64 = 0.70
pub const MAX_TRAVERSAL_DEPTH: usize = 10
```

Types removed from `unimatrix-engine/src/confidence.rs`:
```
pub const DEPRECATED_PENALTY: f64 = 0.7   // REMOVED
pub const SUPERSEDED_PENALTY: f64 = 0.5   // REMOVED
```

---

## Cargo.toml / lib.rs Changes

### unimatrix-engine/Cargo.toml

Add under `[dependencies]`:
```toml
petgraph = { version = "0.8", default-features = false, features = ["stable_graph"] }
thiserror = { workspace = true }
```

Note: `thiserror` is not currently present in this Cargo.toml. Check whether `thiserror` is a workspace dependency in the root `Cargo.toml` before using `{ workspace = true }`. If it is not a workspace dep, use `thiserror = "1"` instead.

### unimatrix-engine/src/lib.rs

Add one line after the existing `pub mod confidence;` line:
```
pub mod graph;
```

---

## Sequencing Constraints

1. `graph.rs` must be created first — `search.rs` imports from it.
2. `lib.rs` must be updated before `search.rs` imports compile.
3. `Cargo.toml` must be updated before `graph.rs` compiles (petgraph + thiserror).
4. `confidence.rs` constant removals and `graph.rs` behavioral ordering tests must land in the same commit (R-05 — no coverage gap window).
5. `search.rs` test updates (remove `DEPRECATED_PENALTY`/`SUPERSEDED_PENALTY` references) must accompany the import change.

---

## Integration Surface Reference

Defined in ARCHITECTURE.md Integration Surface table. All function signatures, constants, and type names in the pseudocode files trace directly to that table — no invented names.
