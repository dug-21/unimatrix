## ADR-003: inverse Mode AND Semantics for missing_edge_types

### Context

`inverse` mode returns entries of a given category that have no incoming edges of
specified types. When `missing_edge_types` contains more than one type, two semantics
are possible:

- **AND semantics**: return entries missing ALL specified types (narrower result).
  Example: `missing_edge_types=["Cites", "Supports"]` returns entries that have neither
  a `Cites` nor a `Supports` incoming edge.
- **OR semantics**: return entries missing ANY specified type (wider result).
  Example: same call returns entries that lack at least one of the two types — which
  is most entries (an entry with a `Cites` but no `Supports` qualifies).

The primary use case is gap detection: "sources that are genuinely uncited and
unsupported" — a narrow, actionable set. OR semantics would return nearly every entry
(most entries lack most edge types) and provide no useful signal.

SQL implementation maps directly to semantics: AND semantics = N LEFT JOINs each
null-checked and ANDed in the WHERE clause; OR semantics = a single LEFT JOIN with an
OR condition or multiple passes. The LEFT JOIN pattern naturally produces AND semantics.

Callers wanting OR behavior can issue two separate `inverse` queries and union the
results client-side with one `context_graph` call per type.

The `idx_graph_edges_target_type (target_id, relation_type)` composite index (schema v27)
makes each LEFT JOIN a single composite range scan — AND semantics adds no per-type
performance overhead beyond the additional JOIN clause.

### Decision

AND semantics: `inverse` mode returns entries missing ALL types specified in
`missing_edge_types`. Each type in the list contributes one LEFT JOIN in the antijoin
SQL; all null checks are ANDed in the WHERE clause.

The tool description must state AND semantics explicitly with an example, consistent
with SR-06 recommendation: "If multiple types are listed in missing_edge_types, only
entries missing ALL listed types are returned. To find entries missing ANY one type,
issue one inverse query per type."

### Consequences

Easier: AND semantics is more useful for gap detection (narrower, more actionable
results). SQL mapping is direct and predictable. OR behavior is composable via multiple
calls.

Harder: Callers expecting OR semantics get unexpectedly narrow results if the
distinction is not clear. Tool description AND semantics example is mandatory.
