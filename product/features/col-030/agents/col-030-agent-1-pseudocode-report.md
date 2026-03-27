# Agent Report: col-030-agent-1-pseudocode

## Task

Produce per-component pseudocode for:
1. `suppress_contradicts` in `graph_suppression.rs`
2. Step 10b insertion in `search.rs`

## Files Produced

- `product/features/col-030/pseudocode/OVERVIEW.md`
- `product/features/col-030/pseudocode/graph_suppression.md`
- `product/features/col-030/pseudocode/search_step10b.md`

## Components Covered

| Component | File |
|-----------|------|
| `suppress_contradicts` (new module) | `graph_suppression.md` |
| Step 10b insertion | `search_step10b.md` |
| `graph.rs` wiring (2-line edit) | Described in `graph_suppression.md` |

## Source Documents Read

- `IMPLEMENTATION-BRIEF.md` — primary delivery contract
- `architecture/ARCHITECTURE.md` — integration surface, component map
- `specification/SPECIFICATION.md` — FR-01 through FR-15, AC-01 through AC-12
- `RISK-TEST-STRATEGY.md` — R-01 through R-13
- All 5 ADR files (ADR-001 through ADR-005)
- `crates/unimatrix-engine/src/graph.rs` lines 1-300 — verified exact types, field visibility,
  `edges_of_type` signature, `TypedRelationGraph` struct
- `crates/unimatrix-server/src/services/search.rs` lines 594-934 — verified exact variable names,
  binding mutability (`let` vs `let mut`), Step 10/11 insertion point, existing import line

## Open Question / Gap Flagged

**Chain suppression algorithm choice (test case 4 in ARCHITECTURE.md)**

ARCHITECTURE.md documents test case 4 as: "rank-0 contradicts rank-2, rank-2 contradicts rank-3
→ `[true, true, false, false]`" (both rank-2 and rank-3 suppressed).

Two possible algorithm interpretations exist:
- Option A: Suppressed nodes do NOT propagate. Outer loop skips `keep_mask[i] == false` entries.
  Result: `[true, true, false, true]` — only rank-2 suppressed.
- Option B: Suppressed nodes DO propagate. Outer loop processes all entries regardless of
  suppression status. Result: `[true, true, false, false]` — matches ARCHITECTURE.md test case 4.

The pseudocode in `graph_suppression.md` implements option B to match the documented test
expectation. This choice is flagged explicitly so the implementation agent does not silently
choose option A (which would compile and run correctly but fail test case 6 in the test suite).

If option A is intended (simpler semantics, only surviving entries suppress others), the
ARCHITECTURE.md test case 4 expected value must be updated to `[true, true, false, true]`.

## Key Design Decisions Made

### Return type: `(Vec<bool>, Vec<Option<u64>>)`

`suppress_contradicts` returns a tuple of two parallel Vecs rather than `Vec<bool>` alone.
The second Vec carries the contradicting entry ID for each suppressed slot (`None` for kept
entries). This satisfies FR-09 / NFR-05 (debug log requires both IDs) without requiring the
caller to re-derive the contradicting ID by re-querying the graph.

The spawn prompt noted "decide: return it, or let caller derive it." Returning it is cleaner —
the suppression function already has the ID in hand during inner-loop processing, and deriving
it at the call site would require a second graph traversal or a map lookup.

### Step 10b scoping: if-expression form

The spawn prompt showed `let final_scores = new_fs;` as a shadow inside an `if` block. On
reading the actual search.rs code, this shadow would not be visible at Step 11 (it would go
out of scope at the closing brace of the `if` block). The pseudocode resolves this by using
an if-expression form:

```
let final_scores = if !use_fallback { ... new_fs } else { final_scores };
```

This satisfies R-03 (no `let mut` at line 893), places the rebinding at the correct scope for
Step 11 visibility, and moves the original `final_scores` binding into the `else` branch
(valid because `final_scores` is not used again after this point until Step 11 uses the new binding).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #3627, #3630, #3626, #3628
  (all col-030 ADRs), #3616 (Step 10b insertion pattern), #3624 (mandatory positive integration
  test pattern). All directly applicable. No gaps from briefing.
- Queried: `context_search` for graph traversal patterns — entry #3616 confirmed Step 10b
  pattern and parallel Vec invariant.
- Queried: `context_search` for col-030 ADRs — confirmed all 5 ADRs present in Unimatrix.
- Deviations from established patterns: none. All interface names, types, and import paths
  traced directly from architecture documents and source code.
