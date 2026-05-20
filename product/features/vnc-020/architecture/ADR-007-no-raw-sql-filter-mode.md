## ADR-007: No Raw SQL in filter Mode — Typed Parameters Only

### Context

ASS-057 Track B Section 3 proposed a `where_clause: Option<String>` field in
`GraphParams` to allow callers to specify arbitrary SQL WHERE clauses for filter mode.
This would allow flexible property filtering without enumerating every possible filter
combination in the API.

The problem: `context_graph` is an MCP tool. Its callers are AI agents — including
the same agent that writes the `where_clause` value. A free-form SQL string accepted
over MCP is a SQL injection surface. The server executes it via `sqlx::query` against
the production SQLite database without parameterization.

Concrete attack surface:
- An agent generating a `where_clause` from user input or retrieved content could
  inadvertently (or adversarially) include `; DROP TABLE entries; --` or `UNION SELECT ...`.
- Unlike a human-facing web application, there is no intermediary sanitization layer.
- MCP tool parameters are strings that pass through JSON deserialization with no
  content-based escaping.

The alternative is to enumerate the property filter dimensions as typed parameters.
ASS-057 Track B identifies the concrete filter use cases: age threshold (Q10 stale Goal),
confidence bounds, and topic prefix/exact match. These map to three well-defined columns
in `entries`: `created_at`, `confidence`, and `topic`.

Parameterized SQL for these is 3 conditional WHERE clause fragments, each using a
`?`-bound value. No free-form string is accepted.

### Decision

Reject the `where_clause: String` proposal. Filter mode expresses all property filters
via typed `GraphParams` fields: `min_age_days: Option<u32>`, `min_confidence: Option<f64>`,
`max_confidence: Option<f64>`. All values are bound as SQL parameters via sqlx.

The correlated subquery for edge-count filtering uses `edge_types` (already in
`GraphParams`) bound as parameters, and `min_edge_count`/`max_edge_count` bound as
integer parameters.

No user-supplied SQL string is accepted at any point in the filter mode execution path.

If a future use case requires a filter dimension not covered by the current typed params,
the correct response is to add a new typed `Option<T>` field to `GraphParams` via the
backward-compatible extension pattern (ADR-002, ADR-003 vnc-018), not to introduce a
free-form SQL escape hatch.

### Consequences

Easier: No SQL injection surface via MCP. All filter dimensions are type-checked at
deserialization. Query plans are predictable (optimizer can plan for each fixed SQL
shape).

Harder: Each new filter dimension requires a `GraphParams` field addition and a new
`validate_no_unsupported_params` rejection clause. The current set (age, confidence)
covers the documented use cases (Q10, Q11) but not arbitrary future queries. Callers
needing other property filters must use `context_lookup` (which handles tags, topic,
status) or issue SQL directly via the store layer if they have server access.
