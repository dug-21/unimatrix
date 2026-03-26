## ADR-002: Two SQL Queries for Graph Cohesion Metrics

### Context

The six cohesion metrics split into two natural groups by JOIN shape:

**Group A — pure GRAPH_EDGES aggregates** (no join to entries needed):
- `supports_edge_count`: `COUNT WHERE relation_type = 'Supports'`
- `inferred_edge_count`: `COUNT WHERE source = 'nli'`
- `total_edges` (used to derive `mean_entry_degree`)

**Group B — connectivity and cross-category** (requires JOIN to entries):
- `active_entry_count` (denominator for connectivity rate and mean degree)
- `connected_entry_count` (entries with ≥1 non-bootstrap edge)
- `cross_category_edge_count` (edges where src.category != tgt.category)

Alternatives considered:

1. **One mega-query**: A single CTE-based query joining both groups. Technically
   possible but produces a wide intermediate result set that mixes two different
   aggregation granularities (edge-level counts vs. entry-level counts). CTEs in
   SQLite are not materialised by default — the planner may re-evaluate them
   multiple times. The resulting SQL is ~40 lines and hard to unit-test in isolation.

2. **Three separate queries**: One per metric group plus a separate connected-count
   query. More queries but each is simple and directly maps to one acceptance criterion.
   Adds one more round-trip per `context_status` call.

3. **Two queries** (chosen): Query 1 covers Group A (pure GRAPH_EDGES). Query 2
   covers Group B (entry-JOIN). The connected-entry-count sub-query is embedded inside
   Query 2 as a scalar sub-query using a UNION approach, keeping the total at two
   round-trips.

The `compute_status_aggregates` precedent uses two `fetch_all` calls (one for the
main aggregates, one for the trust_source distribution). Two queries is therefore
consistent with existing patterns in `read.rs`.

### Decision

Implement `compute_graph_cohesion_metrics()` with exactly two SQL queries:

- **Query 1**: `SELECT COUNT(*), SUM(supports), SUM(nli) FROM graph_edges WHERE bootstrap_only=0`
  — single-row aggregate, no JOIN, uses existing GRAPH_EDGES indexes.
- **Query 2**: main query joining `entries` to `graph_edges` for connectivity and
  cross-category counts, with an embedded UNION scalar sub-query for
  `connected_entry_count`. The outer query drives from `entries WHERE status=0` to
  ensure the active entry count is the denominator.

`mean_entry_degree` is computed in Rust from Query 1's `total_edges` and Query 2's
`active_entry_count`: `(2.0 * total_edges as f64) / active_entry_count as f64`.

`connectivity_rate` is computed in Rust from `connected_entry_count / active_entry_count`.

`isolated_entry_count` is computed in Rust as `active_entry_count - connected_entry_count`.

All division-by-zero cases (active_entry_count = 0) return `0.0`.

### Consequences

Easier:
- Each query tests one concern: pure edge aggregates vs. entry-joined topology
- Simpler SQL is easier to explain in code comments and review
- Consistent with the two-query pattern already established in `compute_status_aggregates`

Harder:
- Two round-trips to SQLite instead of one; in practice negligible because
  `context_status` is not on a hot path and both queries are O(edges) scans with
  index support
- The UNION sub-query for connected-entry-count adds complexity to Query 2; the
  implementer may instead compute this in Rust by collecting source/target IDs into
  a HashSet and counting intersecting active entries (acceptable alternative)
