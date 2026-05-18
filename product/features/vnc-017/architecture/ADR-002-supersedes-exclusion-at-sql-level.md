## ADR-002: Supersedes Exclusion — At SQL Level, Not Loop Level

### Context

`context_correct` writes a `Supersedes` edge row to `graph_edges` as part of the
correction chain (predecessor_id → new_id). The `build_typed_relation_graph` function
derives Supersedes edges from `entries.supersedes` — the `graph_edges` Supersedes rows
are a derived, non-authoritative representation that is rebuilt on the next tick.

If the redirect loop processes a `Supersedes` row from `query_incoming_edges`, it would
call `redirect_graph_edge` to point the Supersedes edge at the new target. This would
assert a semantically incorrect claim: that C supersedes B, when in fact C only
superseded A. The `graph_edges` Supersedes row will be corrected on the next tick rebuild
regardless.

Two exclusion approaches were considered:

1. **Loop-level exclusion** — fetch all rows from `query_incoming_edges` including
   Supersedes, then skip them in the loop with an explanatory comment.
2. **SQL-level exclusion** — add `AND relation_type != 'Supersedes'` to the
   `query_incoming_edges` WHERE clause.

### Decision

Exclude `Supersedes` rows at the SQL level in `query_incoming_edges`. The WHERE clause
must include `AND relation_type != 'Supersedes'` with an explanatory comment in the SQL
string explaining why (OQ-03 resolution).

Rationale:
1. Fetching rows only to discard them is wasteful (SR-04). The `idx_graph_edges_target_id`
   index makes the filter essentially free.
2. The intent comment belongs with the filtering logic — co-locating the exclusion and
   the explanation in the SQL is more readable than a loop-level guard.
3. If `Supersedes` rows do not exist in practice (never written to `graph_edges`), the
   exclusion is dead code but harmless.

### Consequences

Easier: the redirect loop never sees Supersedes rows; no special-case branching in the
loop; intent is documented at the source of truth (the query).

Harder: any future caller of `query_incoming_edges` that wants Supersedes rows (e.g., a
repair tool) would need a separate query or a flag parameter. This is unlikely given the
non-authoritative status of graph_edges Supersedes rows, but should be noted in the
function's doc comment.
