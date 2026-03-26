# Specification: col-029 — Graph Cohesion Metrics in context_status

GH Issue: #413

## Objective

The `context_status` tool has no visibility into the topology of the `GRAPH_EDGES`
table. This feature adds six graph cohesion metrics to `StatusReport` so operators
can observe whether automated NLI-based edge inference (GH #412) is producing a
connected, cross-category graph that PPR can exploit. The metrics are computed
per-call via SQL over `GRAPH_EDGES` joined to `entries`, with no background-tick
caching and no schema migration.

---

## Functional Requirements

### FR-01: GraphCohesionMetrics store function

`Store::compute_graph_cohesion_metrics()` must be added to
`crates/unimatrix-store/src/read.rs` as a `pub async fn` returning
`Result<GraphCohesionMetrics>`. It must use `write_pool_server()`, consistent
with `compute_status_aggregates`. The function must complete in two SQL queries
(not one per metric).

### FR-02: bootstrap_only=0 filter on all metrics

All six metrics must exclude rows where `bootstrap_only = 1`. Rows with
`bootstrap_only = 1` are not real inference edges; including them would
misrepresent graph health independently of #412's activity.

### FR-03: Active-only entry join

Any metric that joins `GRAPH_EDGES` to the `entries` table must restrict the
join to rows where `entries.status = 0` (Active). Deprecated (status=1) and
quarantined (status=3) entries must not influence `isolated_entry_count`,
`graph_connectivity_rate`, `cross_category_edge_count`, or `mean_entry_degree`.

### FR-04: graph_connectivity_rate computation

`graph_connectivity_rate` is `connected_active_entry_count / total_active_entry_count`
expressed as a value in `[0.0, 1.0]`. An "active entry" is any `entries` row with
`status = 0`. A "connected entry" is an active entry that appears as `source_id`
or `target_id` in at least one `GRAPH_EDGES` row with `bootstrap_only = 0`. If
`total_active_entry_count = 0`, return `0.0`.

### FR-05: isolated_entry_count computation

`isolated_entry_count` is the count of active entries (status=0) that have no
corresponding row in `GRAPH_EDGES` with `bootstrap_only = 0` on either endpoint.
This is the complement of FR-04's connected count: `total_active - connected_active`.

### FR-06: cross_category_edge_count computation

`cross_category_edge_count` counts `GRAPH_EDGES` rows (bootstrap_only=0) where
the source entry and the target entry have different `category` values. Both
endpoints must have `status = 0` (Active) — if either endpoint is
deprecated/quarantined the edge is excluded from this count. This requires joining
`entries` twice (once on `source_id`, once on `target_id`) with `status = 0` on
both aliases. A CTE or subquery must be used to avoid a cartesian product.

### FR-07: supports_edge_count computation

`supports_edge_count` counts `GRAPH_EDGES` rows where `relation_type = 'Supports'`
and `bootstrap_only = 0`. No join to `entries` is required for this metric.

### FR-08: mean_entry_degree computation

`mean_entry_degree` is the in+out degree average across active entries. Each
non-bootstrap edge contributes 1 to its `source_id` entry's degree and 1 to its
`target_id` entry's degree. `mean_entry_degree = (2 * non_bootstrap_edge_count) /
total_active_entry_count`. If `total_active_entry_count = 0`, return `0.0`.

### FR-09: inferred_edge_count computation

`inferred_edge_count` counts `GRAPH_EDGES` rows where `source = 'nli'` and
`bootstrap_only = 0`. The string literal `'nli'` is the confirmed source tag used
by `nli_detection.rs`. No join to `entries` is required.

### FR-10: StatusReport struct fields

Six fields must be added to the `StatusReport` struct in
`crates/unimatrix-server/src/mcp/response/status.rs`, appended after the existing
graph-adjacent fields (`graph_stale_ratio`, `graph_quality_score`,
`graph_compacted`):

```
pub graph_connectivity_rate: f64,
pub isolated_entry_count: u64,
pub cross_category_edge_count: u64,
pub supports_edge_count: u64,
pub mean_entry_degree: f64,
pub inferred_edge_count: u64,
```

All six must also appear in `StatusReport::default()` with values `0.0` (f64) or
`0` (u64). Omitting any field from the `default()` block is a compile error.

### FR-11: Service layer assignment in compute_report() Phase 5

`StatusService::compute_report()` in `crates/unimatrix-server/src/services/status.rs`
must call `self.store.compute_graph_cohesion_metrics().await` in Phase 5 (Coherence
dimensions), after the existing HNSW graph quality score computation. On error the
call must be handled non-fatally: log a warning via `tracing::warn!` and leave the
six fields at their `Default` zero values so the `context_status` response is still
returned. Rationale: follows the Phase 4 co-access precedent; failing the entire
status call at the moment an operator needs it (e.g. during NLI write contention)
is worse than returning partial data. The six returned values must be assigned to the
corresponding `StatusReport` fields before the report is formatted.

### FR-12: Summary format output

The `format_status_report` Summary path in `mcp/response/status.rs` must append a
graph cohesion line whenever `total_active > 0`. The line must include at minimum
`graph_connectivity_rate` (as a percentage), `isolated_entry_count`, and
`cross_category_edge_count`. Suggested format:

```
Graph cohesion: {:.1}% connected, {} isolated, {} cross-category, {} inferred
```

The line is omitted (or shows zeros without misleading phrasing) when all six
metrics are zero, consistent with the existing conditional append pattern (e.g.,
`graph_stale_ratio > 0.0` guard).

### FR-13: Markdown format output — Graph Cohesion sub-section

The `format_status_report` Markdown path must include a `### Graph Cohesion`
sub-section nested within the existing Coherence block (not a new top-level
section). The sub-section must label all six metrics. Suggested layout:

```markdown
### Graph Cohesion
- Connectivity: {:.1}% ({}/{} active entries connected)
- Isolated entries: {}
- Cross-category edges: {}
- Supports edges: {}
- Mean entry degree: {:.2}
- Inferred (NLI) edges: {}
```

### FR-14: Unit tests for compute_graph_cohesion_metrics

Unit tests must be added to `crates/unimatrix-store/src/read.rs` in the existing
`#[cfg(test)]` block using `open_test_store()`. The following scenarios must each
have a dedicated test function:

1. All isolated: no edges — connectivity_rate=0.0, isolated_entry_count=total_active
2. All connected: every active entry has at least one edge — connectivity_rate=1.0,
   isolated_entry_count=0
3. Mixed: some entries connected, some isolated — partial connectivity_rate
4. Cross-category: edges between entries of different categories —
   cross_category_edge_count reflects only those edges with active endpoints
5. Same-category-only: edges only between same-category entries —
   cross_category_edge_count=0
6. NLI-source edges: rows with source='nli' — inferred_edge_count matches exactly
7. Bootstrap-only edges excluded: rows with bootstrap_only=1 do not affect any metric

---

## Non-Functional Requirements

### NFR-01: Per-call computation, no tick caching

`compute_graph_cohesion_metrics()` is called only within `compute_report()` when
`context_status` is invoked directly. It must NOT be called from the background
maintenance tick (`maintenance_tick`) or stored in `MaintenanceDataSnapshot`. This
is by design: the metrics give a live snapshot at query time for post-inference
debugging, not a cached value from up to 15 minutes prior.

### NFR-02: Two-query maximum

The implementation must use at most two SQL queries. Query 1 operates on
`GRAPH_EDGES` alone (for `supports_edge_count`, `inferred_edge_count`, and the
total non-bootstrap edge count for `mean_entry_degree`). Query 2 joins `GRAPH_EDGES`
to `entries` (for `graph_connectivity_rate`, `isolated_entry_count`,
`cross_category_edge_count`). A single query combining all six is acceptable if it
uses a CTE and avoids a cartesian product; a query per metric is not acceptable.

### NFR-03: No schema migration

All data required by the six metrics exists in the current schema (GRAPH_EDGES at
schema v13, entries at schema v17). No new columns, tables, or indexes are added.
The feature must compile and pass tests against the existing schema without migration.

### NFR-04: No new crate dependency

The implementation uses only `sqlx` and scalar arithmetic already present in
`unimatrix-store`. No new entries in `Cargo.toml`.

### NFR-05: Function and struct size limit

The `GraphCohesionMetrics` struct definition and `compute_graph_cohesion_metrics()`
function together must not exceed 50 lines in `read.rs`. If `read.rs` reaches or
exceeds 500 lines after the addition, the new code must be placed in a new
`read_graph.rs` submodule instead.

### NFR-06: No lambda (coherence scalar) change

The six metrics are informational only. `coherence` (lambda), `graph_quality_score`,
and the four coherence dimension scores must not be modified by this feature.

### NFR-07: write_pool_server() usage

All SQL in `compute_graph_cohesion_metrics()` must use `write_pool_server()`,
consistent with `compute_status_aggregates` and all other server-layer raw queries.

---

## Acceptance Criteria

Each criterion carries its scope AC-ID. Additional criteria (AC-14 through AC-16)
address SR-04 and integration gaps not fully specified in SCOPE.md.

| AC-ID | Criterion | Verification Method |
|-------|-----------|---------------------|
| AC-01 | `StatusReport` contains all six fields with types: `graph_connectivity_rate: f64`, `isolated_entry_count: u64`, `cross_category_edge_count: u64`, `supports_edge_count: u64`, `mean_entry_degree: f64`, `inferred_edge_count: u64` | Compile check; `grep` for all six field names in struct definition |
| AC-02 | All-isolated store (no non-bootstrap edges): `connectivity_rate = 0.0`, `isolated_entry_count = total_active`, `cross_category_edge_count = 0` | Unit test `test_graph_cohesion_all_isolated` |
| AC-03 | All-connected store (every active entry has ≥1 non-bootstrap edge): `connectivity_rate = 1.0`, `isolated_entry_count = 0` | Unit test `test_graph_cohesion_all_connected` |
| AC-04 | `cross_category_edge_count` counts only edges where both endpoints are Active (status=0) and have different `category` values; same-category edges not counted | Unit tests `test_graph_cohesion_cross_category` and `test_graph_cohesion_same_category_only` |
| AC-05 | `inferred_edge_count` equals exactly the count of GRAPH_EDGES rows with `source = 'nli'` and `bootstrap_only = 0`; rows with `bootstrap_only = 1` are excluded even if `source = 'nli'` | Unit test `test_graph_cohesion_nli_source` |
| AC-06 | `supports_edge_count` equals exactly the count of GRAPH_EDGES rows with `relation_type = 'Supports'` and `bootstrap_only = 0` | Unit test `test_graph_cohesion_supports_edges` (part of AC-13 mixed test or dedicated) |
| AC-07 | `mean_entry_degree = 0.0` when no non-bootstrap edges exist; for non-empty connected graph equals `(2 * non_bootstrap_edge_count) / active_entry_count` | Unit tests AC-02 (zero case) and AC-03 (non-zero case) with degree assertion |
| AC-08 | Deprecated and quarantined entries are excluded from `isolated_entry_count` and `graph_connectivity_rate`; an edge whose endpoint is deprecated does not count as connecting an active entry | Unit test with mixed-status store: deprecated entry with edge, active entry without — deprecated must not appear in connected count |
| AC-09 | Summary format response includes `isolated_entry_count` and `cross_category_edge_count` values (numerically, not just the field names) | Manual invocation of `context_status` with summary format; assert substring present |
| AC-10 | Markdown format response includes a `### Graph Cohesion` sub-section with all six metric values individually labeled | Manual invocation of `context_status` with markdown format; assert all six labels present |
| AC-11 | `compute_graph_cohesion_metrics()` is called in `compute_report()` Phase 5 and all six fields are assigned to `StatusReport` before `format_status_report` is called | Code review: call site in `services/status.rs` Phase 5 block; `grep` for all six assignments |
| AC-12 | All six fields default to `0` / `0.0` in `StatusReport::default()` — no field omitted from the `default()` block | Compile check; `grep` for all six field names in `default()` impl |
| AC-13 | Unit tests cover all seven scenarios: all-isolated, all-connected, mixed connectivity, cross-category edges, same-category-only edges, NLI-source edges, bootstrap-only edges excluded | Test file review: seven test functions present in `read.rs` `#[cfg(test)]` block |
| AC-14 | `cross_category_edge_count` SQL joins `entries` twice (once on `source_id`, once on `target_id`) with `status = 0` on both aliases; an edge where one endpoint is deprecated/quarantined is excluded from the count | Code review of SQL in `compute_graph_cohesion_metrics`; confirmed by AC-08 unit test |
| AC-15 | `compute_graph_cohesion_metrics()` is NOT called from the background maintenance tick (`maintenance_tick`) or written into `MaintenanceDataSnapshot` | Code review: `grep` for `compute_graph_cohesion_metrics` confirms single call site in `compute_report()` |
| AC-16 | Bootstrap-only edges (bootstrap_only=1) are excluded from all six metrics, including `inferred_edge_count` for any `source='nli'` rows that happen to have `bootstrap_only=1` | Unit test with a bootstrap_only=1 NLI-sourced edge: `inferred_edge_count` must remain 0 |

---

## Domain Models

### GraphCohesionMetrics (new struct, `crates/unimatrix-store/src/read.rs`)

| Field | Type | Semantics |
|-------|------|-----------|
| `connectivity_rate` | `f64` | Fraction of active entries that appear in at least one non-bootstrap edge. Range [0.0, 1.0]. |
| `isolated_entry_count` | `u64` | Count of active entries with no non-bootstrap edges on either endpoint. |
| `cross_category_edge_count` | `u64` | Count of non-bootstrap edges where both active endpoints have different `category` values. |
| `supports_edge_count` | `u64` | Count of non-bootstrap GRAPH_EDGES rows with `relation_type = 'Supports'`. |
| `mean_entry_degree` | `f64` | Average in+out degree across all active entries. Equals `(2 * non_bootstrap_edge_count) / active_entry_count`. |
| `inferred_edge_count` | `u64` | Count of non-bootstrap GRAPH_EDGES rows with `source = 'nli'`. |

### StatusReport (existing struct extended, `crates/unimatrix-server/src/mcp/response/status.rs`)

The struct is a plain Rust struct with a manual `Default` impl. No derives. Six
new fields are appended after `graph_compacted`. Default values: f64 → `0.0`,
u64 → `0`. These fields are display-only; they do not feed into `coherence` (lambda).

### Key Ubiquitous Language

| Term | Definition |
|------|-----------|
| Active entry | An `entries` row with `status = 0`. Deprecated (1) and quarantined (3) are excluded from all cohesion metrics. |
| Non-bootstrap edge | A `GRAPH_EDGES` row with `bootstrap_only = 0`. Bootstrap-only edges (bootstrap_only=1) are treated as non-existent for all cohesion purposes. |
| Connected entry | An active entry that appears as `source_id` or `target_id` in at least one non-bootstrap edge. |
| Isolated entry | An active entry that is not a connected entry. |
| Inferred edge | A non-bootstrap edge with `source = 'nli'`. Written by the NLI detection service (#412). |
| Cross-category edge | A non-bootstrap edge where the `category` of the source active entry differs from the `category` of the target active entry. |
| In+out degree | For a given entry, the count of non-bootstrap edges where it appears as either endpoint (source or target). Each edge contributes 1 to source degree and 1 to target degree. |
| Phase 5 | The "Coherence dimensions" phase of `compute_report()` in `services/status.rs`. This is where `graph_quality_score` is computed; the new cohesion call slots in after it. |

---

## User Workflows

### Workflow 1: Operator checks graph health after NLI inference run

1. Operator invokes `context_status` (format: markdown or default).
2. Response includes `### Graph Cohesion` sub-section showing connectivity rate,
   isolated entry count, and inferred edge count.
3. Operator observes `inferred_edge_count` has grown and `isolated_entry_count`
   has dropped, confirming #412 is building the graph.
4. Operator observes `cross_category_edge_count` to verify NLI is linking across
   category boundaries (cross-category PPR propagation prerequisite).

### Workflow 2: Operator calls context_status in Summary format

1. Operator invokes `context_status` with format=summary (or default summary path).
2. A single-line "Graph cohesion:" string appears in the summary output.
3. Operator can spot-check isolation and cross-category counts without parsing
   full markdown.

### Workflow 3: CI / automated gate checks

1. After a delivery wave runs, the test suite executes unit tests for
   `compute_graph_cohesion_metrics`.
2. All seven scenarios (AC-13) must pass.
3. No integration test additions are required for this feature beyond the unit
   tests if the SQL is fully exercised there.

---

## Constraints

- **SQLite via sqlx only**: all queries use `sqlx::query` with `write_pool_server()`.
  No direct `rusqlite` connections.
- **bootstrap_only=0 filter mandatory on all six metrics**: matches the
  semantics of `TypedRelationGraph.inner` which already excludes them.
- **Active-only join (status=0)**: deprecated/quarantined entries are invisible
  to PPR and must be invisible to cohesion metrics.
- **No schema migration**: data is already present in GRAPH_EDGES (v13) and
  entries (v17). No new columns, tables, or indexes.
- **File size**: `read.rs` is large; the new struct and function must not exceed
  50 lines. If `read.rs` reaches 500 lines, split graph cohesion into `read_graph.rs`.
- **No new crate dependency**: uses only `sqlx` and scalar arithmetic.
- **No lambda modification**: `coherence` scalar and its four dimension scores
  are untouched.
- **No tick caching**: cohesion metrics are not stored in `MaintenanceDataSnapshot`
  or any background-tick cache handle. (SR-03/SR-06 risk is resolved by the
  per-call design decision in the scope.)
- **source='nli' string literal**: `inferred_edge_count` uses the literal `'nli'`.
  SR-01 risk (divergence if #412 changes the string) must be addressed by defining
  `EDGE_SOURCE_NLI: &str = "nli"` as a named constant shared with `nli_detection.rs`.
  The architect must confirm the constant location before implementation.

---

## Dependencies

| Dependency | Kind | Notes |
|------------|------|-------|
| `sqlx` | Crate (existing) | All SQL queries use the existing sqlx pool |
| `GRAPH_EDGES` table | Schema (existing, v13) | No changes required |
| `entries` table | Schema (existing, v17) | Joined on `status = 0` and `category` |
| `compute_status_aggregates` | Pattern reference | New function follows this pattern exactly |
| `StatusReport::default()` | Struct contract | All six fields must be enumerated in the default block |
| GH #412 (NLI inference) | Integration dependency | Defines `source='nli'` convention; SR-01 requires named constant coordination |
| `nli_detection.rs` | Source reference | Confirms `source='nli'` string; constant must be extracted before or during this feature |

---

## NOT in Scope

- Changes to the lambda (coherence) scalar or its four dimension weights
- Replacement or modification of `graph_quality_score` / `graph_stale_ratio`
- Caching cohesion metrics in the background maintenance tick or `MaintenanceDataSnapshot`
- Per-category or per-relation-type breakdowns beyond the six specified metrics
- Distinguishing Supports vs. Contradicts among NLI-inferred edges
- Alerting thresholds or automated maintenance triggered by cohesion metric values
- Changes to `TypedRelationGraph` in-memory structure or `build_typed_relation_graph`
- New schema migration, new columns, or new indexes
- Per-entry degree distribution (histogram); only the mean is exposed
- Integration tests beyond unit tests for `compute_graph_cohesion_metrics`

---

## Open Questions

1. **SR-01 — Named constant location**: Where should `EDGE_SOURCE_NLI: &str = "nli"`
   live? Options: (a) `unimatrix-store/src/schema.rs`, (b) a new
   `unimatrix-store/src/graph_constants.rs`, (c) `unimatrix-server/src/services/nli_detection.rs`
   re-exported. The architect must decide before implementation to prevent silent
   divergence when #412 ships.

2. **SR-04 — SQL strategy for cross_category_edge_count**: The double-join on entries
   (once for source, once for target) must be reviewed by the architect to confirm
   the query plan does not produce a cartesian product on large graphs. A CTE approach
   may be preferable. This is a design decision for the architect, not the implementer.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for `context_status tool interface response format StatusReport` — found Generic Formatter Pattern (#298), Response Formatting Convention (#307)
- Queried: /uni-query-patterns for `StatusReport field naming health metric output format` — found Response Formatting Convention (#307), Validation Function Convention (#308)
- Queried: /uni-query-patterns for `unit test patterns store layer SQL functions` — found TestHarness Server Integration Pattern (#748), Test Gateway Pattern (#315 — new_permissive() with throwaway store), Extract spawn_blocking body into named sync helper (#1758)
- Queried: /uni-query-patterns for `graph cohesion connectivity GRAPH_EDGES maintenance tick cache` — found TypedRelationGraph pattern (#2476, bootstrap exclusion), compute_report tick inflation lesson (#1777 — confirms per-call design is correct), ContradictionScanCacheHandle ADR pattern referenced in scope risk assessment
- Queried: /uni-query-patterns for `ContradictionScanCacheHandle Arc RwLock background tick compute_report` — confirmed per-call approach avoids the tick-cost bug documented in lesson #1777; no cache handle needed given per-call design decision
