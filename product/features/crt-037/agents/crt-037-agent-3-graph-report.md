# Agent Report: crt-037-agent-3-graph

**Feature**: crt-037 — Informs Edge Type
**Component**: `RelationType::Informs` variant in `crates/unimatrix-engine/src/graph.rs`
**Agent ID**: crt-037-agent-3-graph
**Date**: 2026-03-31

---

## Summary

Implemented `RelationType::Informs` as the sixth variant in the `RelationType` enum.
Extended `as_str()` and `from_str()` with the new arm. Updated module-level doc comment
and enum doc comment to include `Informs` in the non-Supersedes examples and to document
the SR-01 penalty invariant. Added 10 unit tests covering all acceptance criteria from
the test plan.

---

## Files Modified

- `crates/unimatrix-engine/src/graph.rs`
- `crates/unimatrix-engine/src/graph_tests.rs`

---

## Changes Made

### graph.rs

1. Module doc comment (line 15): added `Informs` to the non-Supersedes edge list in the SR-01 mitigation note.
2. `RelationType` enum doc: changed "Five edge types" to "Six edge types"; added `Informs` documentation sentence.
3. `RelationType` enum body: added `Informs` variant with inline doc comment (crt-037, SR-01 reference).
4. `as_str()`: added `RelationType::Informs => "Informs"` arm.
5. `from_str()`: added `"Informs" => Some(RelationType::Informs)` arm before the wildcard.
6. No changes to `graph_penalty`, `find_terminal_active`, or any traversal function — SR-01 invariant is preserved structurally via `edges_of_type(RelationType::Supersedes)`.

### graph_tests.rs

Added 10 new tests in a clearly marked `crt-037` section:

| Test | AC Covered |
|------|------------|
| `test_relation_type_informs_from_str_returns_some` | AC-01 |
| `test_relation_type_informs_as_str_returns_string` | AC-02 |
| `test_relation_type_informs_round_trip` | AC-01 + AC-02 combined |
| `test_relation_type_from_str_case_sensitive` | Case-sensitivity contract |
| `test_existing_relation_type_variants_unchanged` | Regression (AC variant) |
| `test_build_typed_relation_graph_includes_informs_edge` | AC-03 |
| `test_build_typed_relation_graph_informs_no_warn_log` | AC-04 (structural) |
| `test_graph_penalty_with_informs_only_returns_fallback` | AC-24 (penalty) |
| `test_find_terminal_active_with_informs_only_returns_empty` | AC-24 (terminal) |
| `test_graph_penalty_informs_plus_supersedes_uses_supersedes_only` | AC-24 (mixed graph) |
| `test_informs_edge_weight_preserved` | Weight fidelity |

AC-04 uses a structural proxy: if `build_typed_relation_graph` includes the edge
(`edge_count == 1`), the `warn!`-and-skip branch was not reached. The `tracing_test`
crate is not a workspace dependency; this approach is documented in the test comment.

---

## Test Results

```
test result: ok. 339 passed; 0 failed; 1 ignored
```

All 10 new tests pass. No regressions. The full `unimatrix-engine` test suite (339 unit
tests across all test binaries) passes clean.

---

## Acceptance Criteria Verified

| AC | Status | Evidence |
|----|--------|---------|
| AC-01 | PASS | `test_relation_type_informs_from_str_returns_some` |
| AC-02 | PASS | `test_relation_type_informs_as_str_returns_string` |
| AC-03 | PASS | `test_build_typed_relation_graph_includes_informs_edge` |
| AC-04 | PASS | `test_build_typed_relation_graph_informs_no_warn_log` (structural) |
| AC-24 | PASS | penalty + terminal + mixed-graph tests |

---

## SR-01 Invariant Verification

`graph_penalty` and `find_terminal_active` were NOT modified. Both continue to filter
exclusively through `edges_of_type(RelationType::Supersedes)`. The new `Informs` variant
is structurally invisible to penalty traversal — no code change was needed to enforce this,
which confirms the `edges_of_type` filter boundary design works as intended.

Test `test_graph_penalty_informs_plus_supersedes_uses_supersedes_only` verifies the
invariant on a mixed graph: adding an Informs edge does not alter the penalty computed
from the Supersedes edge alone.

---

## Issues / Blockers

None. The changes are purely additive. The constraint list (C-06 through C-07) was met
without any penalty function modification.

Pre-existing clippy warning in `auth.rs` (collapsible-if at line 113) was already present
before this change and is outside scope.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #2429, #3650, #3731, #3740,
  #2451 covering TypedRelationGraph patterns, PPR architecture, and SR-01 filter boundary.
  Applied: confirmed edges_of_type as the sole filter boundary; no penalty function changes needed.
- Stored: entry #3944 "Adding a RelationType variant requires three coordinated updates in
  graph.rs or the R-10 guard silently drops the new edge type" via `/uni-store-pattern`
