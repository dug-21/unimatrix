## ADR-001: EDGE_SOURCE_NLI Named Constant in unimatrix-store

### Context

The string `"nli"` is the value written to `graph_edges.source` for NLI-inferred
edges (confirmed in `nli_detection.rs`). At the time of col-029, this string appears
as a bare literal in at least six locations across the codebase:

- `nli_detection.rs` lines 545, 599 (INSERT statements)
- `nli_detection.rs` lines 1054, 1147, 1172, 1265 (SELECT filters in tests and
  circuit-breaker queries)

The new `compute_graph_cohesion_metrics()` function will add a seventh use site
(the `inferred_edge_count` filter `WHERE source = 'nli'`).

SR-01 in the scope risk assessment flagged this as High risk: if GH #412 and col-029
independently write or query the same string, a future rename introduces a silent
mismatch — both queries continue to compile but measure different sets.

A named constant eliminates the coupling. The constant lives in `unimatrix-store`
because that crate owns the `graph_edges` schema and all SQL that touches it.
`unimatrix-server` (which hosts `nli_detection.rs`) already depends on
`unimatrix-store`, so importing from there adds no new dependency edge.

### Decision

Introduce `pub const EDGE_SOURCE_NLI: &str = "nli"` in
`crates/unimatrix-store/src/read.rs`, placed near the `GraphEdgeRow` and
`ContradictEdgeRow` type definitions (lines ~1379). Re-export it from
`crates/unimatrix-store/src/lib.rs` alongside `GraphEdgeRow` and `ContradictEdgeRow`.

The `compute_graph_cohesion_metrics()` SQL uses this constant via string interpolation
in the query (or direct comparison). The existing bare `"nli"` literals in
`nli_detection.rs` are candidates for migration in a follow-up; col-029 does not
require rewriting them as a prerequisite, but the implementer should open a task
against GH #412 to complete the migration.

### Consequences

Easier:
- A single definition point prevents divergence between the query writer and the query
  filter string
- New features that need to filter by NLI source have an obvious import path
- The constant is testable by reference, not by string comparison

Harder:
- The `nli_detection.rs` migration to use the constant (replacing bare literals) is
  deferred work; until complete, the constant and literals coexist. This is acceptable
  because the constant is the authoritative value — the literals are candidates for
  cleanup, not definitions.
