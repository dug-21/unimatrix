# Pseudocode: `graph_read_neighbors.rs` Changes

## Purpose

`graph_read_neighbors.rs` requires one functional change for vnc-019: the function
`follow_to_current` must change from private (`async fn`) to `pub(super)`. No other
change is made to this file.

`all_non_supersedes_types` is already `pub(super)` from vnc-018 delivery and requires
no change.

---

## Current State (post-vnc-018, on `feature/vnc-018`)

```
// Private — not accessible to sibling modules:
async fn follow_to_current(store: &Store, id: u64) -> Option<u64>

// Already pub(super) — no change needed:
pub(super) fn all_non_supersedes_types() -> Vec<RelationType>
pub(super) async fn handle_neighbors(...) -> Result<NeighborsResponse, ErrorData>
```

Source confirmed in ARCHITECTURE.md §3:
> `follow_to_current`: `async fn follow_to_current(store: &Store, id: u64) -> Option<u64>` —
> private (no pub). Must be changed to `pub(super)` in vnc-019 delivery.

---

## Required Change

Change the visibility of `follow_to_current` from private to `pub(super)`:

```
// BEFORE (vnc-018):
async fn follow_to_current(store: &Store, id: u64) -> Option<u64> {

// AFTER (vnc-019):
pub(super) async fn follow_to_current(store: &Store, id: u64) -> Option<u64> {
```

One keyword addition. Function body and doc comment: unchanged.

---

## Why `pub(super)` and Not `pub(crate)` or `pub`

Both `graph_read_neighbors.rs` and `graph_read_subgraph.rs` are declared as
`#[path]`-submodules of `graph_read.rs`. The module hierarchy is:

```
graph_read (mod)            ← parent module
  graph_read_supersession   ← sibling submodule
  graph_read_neighbors      ← sibling submodule (defines follow_to_current)
  graph_read_subgraph       ← sibling submodule (consumes follow_to_current)
```

`pub(super)` makes `follow_to_current` visible to the parent module (`graph_read`)
and all its submodules. This is the minimal necessary visibility — `pub(crate)` or
`pub` would expose it beyond the `graph_read` module tree unnecessarily.

This matches the visibility used for `handle_neighbors` and `all_non_supersedes_types`,
maintaining consistency within the file.

---

## Why No Private Copy Is Permitted

ARCHITECTURE.md §3 explicitly states:
> "A private copy of `follow_to_current` is NOT acceptable — if the 50-hop guard or
> `Store::get` signature changes, a copy drifts silently."

The 50-hop guard, `Store::get` call, poison-recovery pattern, and `Status::Active`
check must be the same in both the neighbors and subgraph callers. A copy creates a
maintenance hazard that will not be caught at compile time.

---

## No Other Changes

- `all_non_supersedes_types`: already `pub(super)`, no change.
- `handle_neighbors`, `neighbors_sql`, `neighbors_bfs`: no changes.
- The existing BFS in `neighbors_bfs` is not modified. Subgraph BFS is a new
  implementation in `graph_read_subgraph.rs` that imports from this module.
- Tests in `graph_read_neighbors_tests.rs`: no new tests required for the visibility
  change itself. The compilation success of `graph_read_subgraph.rs` is the functional
  verification (R-10).

---

## Delivery Order Note

This change must land as the **first action** in vnc-019 delivery. Without it,
`graph_read_subgraph.rs` will fail to compile when it imports `follow_to_current`
from the parent module tree.

ARCHITECTURE.md §3:
> "The delivery agent must add `pub(super)` to `follow_to_current` in
> `graph_read_neighbors.rs` as the first action in vnc-019 delivery (compilation will
> fail without it)."

---

## Key Test Scenarios

No behavioral tests are required for the visibility change itself. Tests that cover the
`follow_to_current` function semantics:

1. `follow_to_current` with active entry → returns `Some(id)`. (Existing test.)
2. `follow_to_current` with deprecated entry → follows chain to terminal. (Existing test.)
3. `follow_to_current` with circular chain → 50-hop guard terminates, returns `None`.
   (R-06 coverage in `graph_read_subgraph_tests.rs`.)
4. `follow_to_current` with 50-hop chain → terminates, returns `None`.
   (R-13 coverage in `graph_read_subgraph_tests.rs`.)

These scenarios are covered in the subgraph test file, not in a new neighbors test.
The neighbors module test file (`graph_read_neighbors_tests.rs`) is not modified.

---

## Integration Risk (IR-01 from RISK-TEST-STRATEGY.md)

> `follow_to_current` must be `pub(super)` or duplicated. If left private in neighbors,
> subgraph must either use pub(super) or maintain its own copy. A stale copy that
> misses guard changes is a silent correctness bug.

Resolution: `pub(super)` is chosen. No copy permitted. Code review must verify
the chosen visibility appears in the delivered file.
