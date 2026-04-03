# Component: graph_expand_security_comment
# File: crates/unimatrix-engine/src/graph_expand.rs

## Purpose

Add a two-line `// SECURITY:` comment immediately before the `pub fn graph_expand(` signature,
making the caller quarantine obligation visible at every IDE call site. This is a documentation-only
change. No logic, no behavior, no BFS traversal, no return type, and no module-level doc block
are modified.

---

## Context

`graph_expand.rs` already carries the full quarantine obligation in:
- The module-level doc block (lines 12-18): "## Caller Quarantine Obligation (FR-06)"
- The function-level `///` doc comment (lines 42-67): behavioral contract including the BFS rules

The crt-042 security review identified Finding 1: the module-header quarantine obligation is not
visible when hovering over `graph_expand` in an IDE because the module-level doc block is not
attached to the function signature. The `// SECURITY:` comment fixes this gap by placing the
obligation at the exact line an IDE navigates to.

The `// SECURITY:` prefix is already established in this codebase at
`graph_enrichment_tick.rs:155` for SQL injection prevention. Using the same prefix makes security
obligations consistently grep-able across the codebase. (ADR-003)

---

## Change: Comment Text to Insert

Insert exactly these two lines immediately before `pub fn graph_expand(` (currently line 68 of
`graph_expand.rs`):

```rust
// SECURITY: caller MUST apply SecurityGateway::is_quarantined() before inserting
// returned IDs into result sets. graph_expand performs NO quarantine filtering.
pub fn graph_expand(
```

The comment sits between the closing `///` doc comment line and the `pub fn` keyword line.

---

## Exact Insertion Point

Current state (lines 66-73 of graph_expand.rs):

```rust
/// - `can_expand_further`: a node at `current_depth == depth` is added to result but
///   does NOT enqueue its neighbors, enforcing the depth limit.
pub fn graph_expand(
    graph: &TypedRelationGraph,
    seed_ids: &[u64],
    depth: usize,
    max_candidates: usize,
) -> HashSet<u64> {
```

After change (lines 66-75):

```rust
/// - `can_expand_further`: a node at `current_depth == depth` is added to result but
///   does NOT enqueue its neighbors, enforcing the depth limit.
// SECURITY: caller MUST apply SecurityGateway::is_quarantined() before inserting
// returned IDs into result sets. graph_expand performs NO quarantine filtering.
pub fn graph_expand(
    graph: &TypedRelationGraph,
    seed_ids: &[u64],
    depth: usize,
    max_candidates: usize,
) -> HashSet<u64> {
```

Total change: +2 lines. No lines removed, no lines modified.

---

## What Does NOT Change

- BFS traversal logic
- `pub fn graph_expand(` function signature (parameters, return type)
- Module-level doc block (lines 1-33)
- Function-level `///` doc comment (lines 42-67)
- `edges_of_type(Direction::Outgoing)` traversal implementation
- Any other file in `unimatrix-engine`
- `graph_expand_tests.rs` (if it exists)
- Anything in `unimatrix-store` or `unimatrix-server`

---

## Error Handling

Not applicable. Documentation-only change. No runtime behavior is modified. Zero risk of
behavioral regression (ADR-003). The only failure mode is a typo in the comment text — the
implementation agent should copy the exact two lines from this pseudocode file.

---

## Key Test Scenarios (for tester agent)

1. **Static presence check** — Verify `grep '// SECURITY:' crates/unimatrix-engine/src/graph_expand.rs`
   returns at least one match. The match must be on the line immediately preceding `pub fn graph_expand(`.
   (AC-08, R-08)

2. **Exact text check** — The comment text matches:
   ```
   // SECURITY: caller MUST apply SecurityGateway::is_quarantined() before inserting
   // returned IDs into result sets. graph_expand performs NO quarantine filtering.
   ```
   (AC-08, FR-S-01)

3. **No logic regression** — `cargo test --workspace` passes. `cargo clippy --workspace` passes.
   (AC-11, NFR-02). The comment uses `//` not `///` — it must not affect rustdoc output for the
   function.

---

## Constraints Traced

| Constraint | How Satisfied |
|-----------|--------------|
| C-07 | No logic change to `graph_expand`; comment is documentation only |
| FR-S-01 | Exact comment text matches GH#495 specification |
| FR-S-02 | No logic change permitted; confirmed by zero modification to function body |
| ADR-003  | Inline `// SECURITY:` at signature; not `#[doc]` attribute or unit test |
