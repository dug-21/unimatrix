# Agent Report: crt-021-agent-1-architect

## Status: Complete

## Artifacts Produced

- `/workspaces/unimatrix/product/features/crt-021/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/crt-021/architecture/ADR-001-typed-edge-weight-model.md`

## ADR Unimatrix Entry IDs

| ADR | Entry ID | Action |
|-----|----------|--------|
| ADR-001 (crt-021): Typed Edge Weight Model | #2416 | Stored (new) |
| ADR-004 (crt-014): Topology-Derived Penalty Scoring | #1604 | Deprecated (superseded by #2416) |

## Key Architectural Decisions Made

1. **`edges_of_type` filter boundary (SR-01 mitigation)**: Single method on
   `TypedRelationGraph` enforces Supersedes-only access for all penalty/traversal
   functions. No ad-hoc type checks at individual call sites.

2. **AC-08 is a dead path**: `shadow_evaluations` has no entry ID pairs (confirmed by
   entry #2404, SR-04). The v12→v13 migration creates zero Contradicts rows. All
   Contradicts edges are W1-2 runtime writes.

3. **SR-07 — Promotion path defined**: W1-2 promotes bootstrap-only edges via
   DELETE + INSERT on the direct `write_pool` path (not the analytics queue). No
   schema changes needed at W1-2. W1-1 must not ship any UPDATE path for `bootstrap_only`.

4. **SR-02 — Write routing boundary**: Bootstrap `GraphEdge` writes are shed-safe
   (idempotent via INSERT OR IGNORE + migration re-creates). W1-2 NLI-confirmed edge
   writes must use direct `write_pool` — not the analytics queue. Documented as a
   W1-2 constraint, not a W1-1 deliverable.

5. **bootstrap_only exclusion is structural**: `build_typed_relation_graph` filters
   `bootstrap_only=1` edges at graph construction, not at query time. No penalty
   function ever sees a bootstrap-only edge.

6. **`TypedGraphState` rename (not extension)**: `supersession.rs` → `typed_graph.rs`,
   `SupersessionState` → `TypedGraphState`. Compiler enforces complete rename across
   ~20 call sites. No type aliases permitted.

7. **`CO_ACCESS_BOOTSTRAP_MIN_COUNT = 3`**: Locked as per scope decision #4.
   CoAccess edges bootstrapped from `co_access` pairs with count >= 3 carry
   `bootstrap_only=0` (authoritative promoted signal).

## Open Questions for Spec Writer

1. **SR-08 (W3-1 GNN edge features)**: Does W3-1 require an NLI confidence score
   column separate from `weight`? If yes, add `metadata TEXT` (JSON) to v13 now at
   zero migration cost. Spec writer must confirm before implementation begins.

2. **Supersedes edge construction source**: The design derives Supersedes edges from
   `entries.supersedes` (authoritative) during graph construction, not from `GRAPH_EDGES`
   rows. This preserves the cycle-detection path but means `GRAPH_EDGES` Supersedes rows
   are for persistence/attribution only. Spec writer should confirm this is intended.

3. **CoAccess weight normalization edge case**: The bootstrap migration uses
   `count / MAX(count)` for CoAccess edge weights. If `co_access` is empty, this
   produces NULL. The migration must use
   `COALESCE(CAST(count AS REAL) / NULLIF((SELECT MAX(count) FROM co_access), 0), 1.0)`.
   Spec writer should confirm the formula.

4. **SR-09 — `sqlx-data.json` regeneration**: The v12→v13 migration and new GRAPH_EDGES
   queries require `cargo sqlx prepare` to regenerate the offline query cache. This must
   be an explicit AC in the spec, with CI validation.
