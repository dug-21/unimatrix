# Agent Report: crt-021-gate-3a

**Agent ID**: crt-021-gate-3a
**Role**: Gate 3a Validator (Component Design Review)
**Date**: 2026-03-19

## Task

Validate pseudocode and test plans for crt-021 (W1-1 Typed Relationship Graph) against
the approved architecture, specification, and risk-test strategy. Enforce the six
known design issues flagged in the spawn prompt.

## Gate Result

**PASS** — all checks pass with one WARN.

## Report Location

`product/features/crt-021/reports/gate-3a-report.md`

## Checks Run (11 total)

| Check | Result |
|-------|--------|
| Architecture alignment | PASS |
| Specification coverage (FR-01–FR-27, NF-01–NF-10) | PASS |
| Risk coverage (R-01–R-15) | PASS |
| Interface consistency across pseudocode files | PASS |
| Known issue 1: Cycle detection on Supersedes-only subgraph | PASS |
| Known issue 2: Supersedes edge direction | PASS |
| Known issue 3: bootstrap_only=1 structural exclusion | PASS |
| Known issue 4: TypedGraphState holds pre-built graph | PASS |
| Known issue 5: edges_of_type sole filter boundary | PASS |
| Known issue 6: R-06 CoAccess empty-table guard | PASS |
| Knowledge stewardship compliance | WARN |

## WARN: Pseudocode agent stewardship block incomplete

crt-021-agent-1-pseudocode-report.md stewardship block lists "nothing novel to store"
without a reason. The deviations section (two novel patterns: Supersedes-only cycle
detection subgraph, skip-Supersedes-in-Pass-2b) documents the rationale elsewhere but
not in the stewardship block itself. This is a WARN per gate rules (does not block).

## Key Design Issue Findings

All six known design issues resolve as PASS:

1. **Cycle detection**: engine-types.md Pass 3 builds a temporary `StableGraph<u64, ()>`
   containing only Supersedes edges before calling `is_cyclic_directed`. Full inner graph
   (with CoAccess bidirectional pairs) is NOT passed to cycle detection.

2. **Edge direction**: store-migration.md Step 2 uses `supersedes AS source_id, id AS target_id`.
   engine-types.md Pass 2a uses pred_idx → succ_idx. Test 7 asserts source_id=1, target_id=2
   for entry id=2 with supersedes=1. VARIANCE 1 correctly resolved.

3. **Structural bootstrap_only exclusion**: engine-types.md Pass 2b: `IF row.bootstrap_only: CONTINUE`
   — rows never reach add_edge(). No traversal-time check needed.

4. **Pre-built graph in TypedGraphState**: server-state.md holds `typed_graph: TypedRelationGraph`
   field. INVARIANT note confirms build_typed_relation_graph is never called on search hot path.
   VARIANCE 2 correctly resolved.

5. **edges_of_type boundary**: INVARIANT in engine-types.md prohibits direct .edges_directed()
   calls in all four traversal functions. All helpers use edges_of_type() exclusively.

6. **R-06 guard**: store-migration.md Step 3 uses `COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)`.
   On empty table, INSERT selects zero rows; COALESCE never evaluated; migration succeeds.

## Knowledge Stewardship

- Queried: /uni-query-patterns for gate-3a validation patterns before review. No directly
  applicable cross-feature gate lesson patterns found.
- Stored: nothing novel to store — validation findings are feature-specific. CoAccess
  empty-table guard pattern was already stored by test-plan agent (entry #2428).
  Supersedes-only cycle detection subgraph pattern is crt-021-specific and not yet
  a recurring cross-feature concern.
