## ADR-006: Edge Directionality — v1 Matches Bootstrap (One Direction Only)

### Context

The bootstrap migration (v12→v13) wrote one `GRAPH_EDGES` row per co_access pair:
- `source_id = entry_id_a` (lower numeric ID of the pair)
- `target_id = entry_id_b` (higher numeric ID)

`co_access` rows are keyed such that `entry_id_a < entry_id_b` (canonical ordering).

PPR traverses `Direction::Outgoing` from seed entries. With one-directional edges, a
query seeding the lower-ID entry can reach the higher-ID entry via the graph, but not
the reverse. For symmetric co-access signal (two entries accessed together), half the
traversal paths are structurally absent.

The correct fix is to write both `(entry_id_a, entry_id_b)` and `(entry_id_b,
entry_id_a)` edges. However, writing bidirectional edges in crt-034 would:
1. Diverge from the bootstrap's existing edge set, producing PPR asymmetry between
   bootstrapped pairs (one direction) and newly promoted pairs (two directions).
2. Require the follow-up to audit which existing CoAccess edges lack their reverse and
   back-fill them — a larger, riskier operation.

SCOPE.md §Known Limitation documents this explicitly and mandates v1 consistency.

SR-04 (SCOPE-RISK-ASSESSMENT.md) asks that the v1 contract be documented as an ADR so
the follow-up can safely add reverse edges without collision.

### Decision

v1 promotion writes edges with:
- `source_id = entry_id_a` (as stored in `co_access`, which guarantees `entry_id_a < entry_id_b`)
- `target_id = entry_id_b`
- `relation_type = 'CoAccess'`
- `source = EDGE_SOURCE_CO_ACCESS` (`"co_access"`)
- `created_by = 'tick'`
- `bootstrap_only = 0`

This exactly mirrors the bootstrap SQL shape, ensuring `UNIQUE(source_id, target_id,
relation_type)` never conflicts between bootstrap-era and tick-era edges.

**Follow-up contract**: A future issue adding reverse edges must:
1. Write `(entry_id_b, entry_id_a, 'CoAccess')` — this is distinct from the existing
   `(entry_id_a, entry_id_b, 'CoAccess')` under the UNIQUE constraint.
2. Handle back-fill of bootstrapped pairs (which also have only one direction).
3. Reference this ADR to confirm the forward-edge layout was intentional.

### Consequences

- v1 is consistent with bootstrap: both have one-directional CoAccess edges.
- PPR asymmetry for co-accessed pairs is a known, documented limitation, not a bug.
- The follow-up can safely write reverse edges without collision risk: the unique
  constraint treats `(a, b, type)` and `(b, a, type)` as distinct rows.
- `EDGE_SOURCE_CO_ACCESS` and the `created_by = 'tick'` field allow a future audit
  query to identify tick-promoted edges vs bootstrap-promoted edges without ambiguity.
