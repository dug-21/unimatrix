# Test Plan: `graph_read_neighbors.rs`

## Scope

The vnc-019 change to `graph_read_neighbors.rs` is a visibility-only modification:

- `follow_to_current`: `async fn` → `pub(super) async fn`
- `all_non_supersedes_types`: already `pub(super)` per Architecture §3; no change required

No behavioral change is introduced. All existing tests in `graph_read_neighbors_tests.rs`
must continue to pass. No new behavioral tests are required for this component.

---

## Verification Strategy

### Primary Check: Compilation

**If `pub(super)` is missing from `follow_to_current`, `cargo build --workspace` fails.**
This is the primary and most reliable check. No test can pass if the module does not compile.

The delivery agent adds `pub(super)` as the **first action** before writing any subgraph code.

```bash
cargo build --workspace 2>&1 | tail -3
```

Expected: `Finished ...` with no errors.

### Secondary Check: Existing Tests Must Pass

All tests in `graph_read_neighbors_tests.rs` must continue to pass unchanged.
These tests exercise:
- `handle_neighbors` with various invalid edge_type configurations
- `handle_neighbors` with invalid direction
- `handle_neighbors` with depth out of range (0, 11) and boundary (1)

```bash
cargo test --workspace 2>&1 | tail -30
```

Expected: all existing tests pass; no new failures.

### Tertiary Check: Import Used in Sibling Module

The subgraph module imports `follow_to_current` and `all_non_supersedes_types`
via `super::graph_read_neighbors::*` (or equivalent path). This import compiles
only if the visibility is `pub(super)`. The BFS tests in `graph_read_subgraph_tests.rs`
that call `handle_subgraph` with non-empty graphs implicitly prove the import works —
`handle_subgraph` would fail to compile if the import were broken.

---

## Key Assertion

```
test_validate_edge_types_absent_defaults_to_all_non_supersedes
```

This test (defined in `graph_read_subgraph.md`, Section A-4) calls `handle_subgraph`
with `edge_types=None` and asserts that Supports edges are traversed. This proves:

1. `all_non_supersedes_types()` is imported correctly from `graph_read_neighbors.rs`.
2. The function returns at least the Supports type.
3. The `pub(super)` visibility is correct and the import path resolves.

---

## No New Tests Required

No additional tests are needed for `graph_read_neighbors.rs` itself. The component's
behavioral correctness is unchanged by vnc-019. The visibility change is verified
by compilation + the subgraph BFS tests that depend on the re-used functions.

---

## Assertions Checklist

- [ ] `cargo build --workspace` succeeds with no errors
- [ ] All tests in `graph_read_neighbors_tests.rs` pass (no regressions)
- [ ] `test_validate_edge_types_absent_defaults_to_all_non_supersedes` passes
      (proves `all_non_supersedes_types` import works)
- [ ] Any BFS test that calls `follow_to_current` indirectly via `resolve_supersessions=true`
      passes (proves `follow_to_current` import and `pub(super)` visibility correct)
