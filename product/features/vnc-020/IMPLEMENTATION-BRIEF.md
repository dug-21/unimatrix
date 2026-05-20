# vnc-020 Implementation Brief
# context_graph — inverse, filter, path Modes (W1B-2c)

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/vnc-020/SCOPE.md |
| Architecture | product/features/vnc-020/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-020/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-020/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-020/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| graph_read.rs (wire types + dispatch + validation) | pseudocode/graph_read.md | test-plan/graph_read.md |
| graph_read_inverse.rs | pseudocode/graph_read_inverse.md | test-plan/graph_read_inverse.md |
| graph_read_filter.rs | pseudocode/graph_read_filter.md | test-plan/graph_read_filter.md |
| graph_read_path.rs | pseudocode/graph_read_path.md | test-plan/graph_read_path.md |
| tools.rs (tool description update) | pseudocode/tools.md | test-plan/tools.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component Map
lists expected components from the architecture — actual file paths are filled during delivery.

---

## Goal

vnc-020 completes the `context_graph` MCP tool series by adding three modes deferred from
vnc-018 and vnc-019: `inverse` (SQL antijoin — entries with no incoming edges of specified
types), `filter` (combined category + property + edge-count correlated subquery), and `path`
(BFS shortest-path over the in-memory `TypedRelationGraph`). All three modes are dispatched
through the existing `context_graph` tool; the total MCP tool count remains 14 and schema
version stays at 27.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Module split strategy | Three sibling modules (`graph_read_inverse.rs`, `graph_read_filter.rs`, `graph_read_path.rs`); `graph_read.rs` retains wire types, dispatch, and centralized `validate_no_unsupported_params`; no handler logic in the parent file | ADR-001 | product/features/vnc-020/architecture/ADR-001-module-split-strategy.md |
| GraphParams field additions | Add 8 new `Option<T>` fields; `from_id`/`to_id`/`depth`/`edge_types` are already present and reused; no field removal or retyping | ADR-002 | product/features/vnc-020/architecture/ADR-002-graphparams-field-additions.md |
| inverse mode AND vs OR semantics | AND semantics — entries missing ALL specified types; OR behavior is composable via two separate inverse queries; tool description must state this with an example | ADR-003 | product/features/vnc-020/architecture/ADR-003-inverse-mode-and-semantics.md |
| depth field reuse for path mode | Reuse existing `depth: Option<u8>` (default 5, range [1,10]) instead of adding `path_max_depth`; `depth` explicitly rejected on chain/current/subgraph/inverse/filter — corrects prior silent-ignore | ADR-004 | product/features/vnc-020/architecture/ADR-004-depth-field-reuse-path-mode.md |
| path response format | `PathResponse { found, from_id, to_id, hops: Vec<PathHop>, length }`; `from_id` is top-level (not in hops); each `PathHop { entry_id, relation_type }` has no null relation_type; `length == hops.len()` | ADR-005 | product/features/vnc-020/architecture/ADR-005-path-response-format.md |
| resolve_supersessions in path mode | Supported; endpoint resolution before BFS (`from_id`/`to_id` resolved via `follow_to_current`); per-hop intermediate resolution reuses `graph_read_subgraph.rs` pattern — no new infrastructure | ADR-006 | product/features/vnc-020/architecture/ADR-006-resolve-supersessions-path-mode.md |
| No raw SQL in filter mode | ASS-057 `where_clause: String` proposal rejected; all property filters are typed `GraphParams` fields bound as sqlx parameters; no SQL injection surface | ADR-007 | product/features/vnc-020/architecture/ADR-007-no-raw-sql-filter-mode.md |

---

## Files to Create / Modify

### New Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/mcp/graph_read_inverse.rs` | `handle_inverse`: validate params, build dynamic N-LEFT-JOIN antijoin SQL, execute via `store.read_pool_server()`, return `InverseResponse` |
| `crates/unimatrix-server/src/mcp/graph_read_filter.rs` | `handle_filter`: validate params, build parameterized correlated subquery SQL with optional property + edge-count clauses, return `FilterResponse` |
| `crates/unimatrix-server/src/mcp/graph_read_path.rs` | `handle_path`: validate params, resolve endpoints via `follow_to_current`, clone `TypedRelationGraph`, run path-carrying BFS outgoing only, return `PathResponse` |

### Modified Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/mcp/graph_read.rs` | Add 8 new `Option<T>` fields to `GraphParams`; add `InverseResponse`, `FilterResponse`, `PathHop`, `PathResponse` structs; extend `handle_graph` dispatch with inverse/filter/path arms; extend `validate_no_unsupported_params` with 3 new arms and 8-field rejection clauses on 4 existing arms; add `#[path]` declarations for the three new sibling modules |
| `crates/unimatrix-server/src/mcp/tools.rs` | Extend `context_graph` tool description to cover inverse, filter, and path modes; include mandatory staleness disclosure text for path mode verbatim; include AND semantics example for inverse mode |
| Integration test file(s) in infra-001 suite | Add AC-27 through AC-31 integration tests (inverse single-type, inverse AND semantics, filter max_edge_count=0, filter min_edge_count≥2, path found/not-found) |

---

## Data Structures

### New GraphParams fields (all `Option<T>`, backward-compatible)

```rust
pub category: Option<String>,            // inverse, filter — required for both
pub missing_edge_types: Option<Vec<String>>, // inverse — required, non-empty
pub limit: Option<u32>,                  // inverse, filter — default 100, range [1, 500]
pub min_age_days: Option<u32>,           // filter — created_at <= NOW - N days
pub min_confidence: Option<f64>,         // filter — confidence >= N
pub max_confidence: Option<f64>,         // filter — confidence <= N
pub min_edge_count: Option<u32>,         // filter — requires edge_types
pub max_edge_count: Option<u32>,         // filter — requires edge_types
```

Pre-existing fields reused by new modes (no changes):
- `from_id: Option<u64>` — path mode start node (stub from vnc-018)
- `to_id: Option<u64>` — path mode destination node (stub from vnc-018)
- `depth: Option<u8>` — path mode hop limit (default 5, range [1,10])
- `edge_types: Option<Vec<String>>` — path/filter type filter; **rejected on inverse** (inverse uses `missing_edge_types` exclusively — AC-03a)

### New response types

```rust
pub struct InverseResponse {
    pub entries: Vec<EntryRecord>,
    pub total_returned: usize,
}

pub struct FilterResponse {
    pub entries: Vec<EntryRecord>,
    pub total_returned: usize,
}

pub struct PathHop {
    pub entry_id: u64,
    pub relation_type: String,  // never null; always one of 16 RelationType variants
}

pub struct PathResponse {
    pub found: bool,
    pub from_id: u64,   // resolved ID when resolve_supersessions=true
    pub to_id: u64,     // resolved ID when resolve_supersessions=true
    pub hops: Vec<PathHop>,
    pub length: u8,     // always equals hops.len()
}
```

---

## Function Signatures

```rust
// graph_read_inverse.rs
pub(super) async fn handle_inverse(
    store: &Store,
    params: &GraphParams,
) -> Result<InverseResponse, ErrorData>

// graph_read_filter.rs
pub(super) async fn handle_filter(
    store: &Store,
    params: &GraphParams,
) -> Result<FilterResponse, ErrorData>

// graph_read_path.rs
pub(super) async fn handle_path(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: &GraphParams,
) -> Result<PathResponse, ErrorData>

// Reused from graph_read_neighbors.rs (pub(super) since vnc-019)
pub(super) async fn follow_to_current(store: &Store, id: u64) -> Option<u64>
pub(super) fn all_non_supersedes_types() -> Vec<RelationType>
```

---

## Constraints

| ID | Constraint |
|----|-----------|
| C1 | vnc-018 (PR #596) must be merged — provides `GraphParams`, `handle_graph`, `validate_no_unsupported_params`, schema v27 indexes |
| C2 | vnc-019 (PR #597) must be merged — provides `follow_to_current` as `pub(super)`, `all_non_supersedes_types` as `pub(super)`, `max_depth` in `GraphParams`, subgraph arm in validation |
| C3 | No schema migration — schema v27 composite indexes (`idx_graph_edges_target_type`, `idx_graph_edges_source_type`) already exist; `CURRENT_SCHEMA_VERSION` stays at 27 |
| C4 | `GraphParams` field removal and retyping are prohibited (ADR-003 vnc-018); only `Option<T>` additions are permitted |
| C5 | `graph_read.rs` must not exceed 500 lines post-expansion; all handler logic lives in sibling modules |
| C6 | All three modes require `Capability::Read`; no new capability introduced |
| C7 | No new MCP tool — all three modes dispatch through existing `context_graph`; tool count stays at 14 |
| C8 | `inverse` and `filter` modes execute SQL against live DB (no staleness); `path` mode uses in-memory `TypedRelationGraph` (tick-window staleness applies) |
| C9 | No raw SQL in filter mode — all clauses built from typed params bound as sqlx parameters; `where_clause: String` is permanently rejected |

---

## Dependencies

### Crates (no new dependencies)

| Crate | Usage |
|-------|-------|
| `petgraph` (already in use) | `TypedRelationGraph.inner` is `StableGraph<u64, RelationEdge>`; BFS uses `edges_of_type` + `Direction::Outgoing`; `node_index_for` is O(1) NodeIndex lookup |
| `sqlx` 0.8 with SQLite (already in use) | Parameterized queries for `inverse` and `filter` modes via `store.read_pool_server()`; `push_bind` pattern for dynamic IN clause binding (pattern #4058) |

### Existing Components Required

| Component | Provided By | Required For |
|-----------|------------|-------------|
| `TypedRelationGraph`, `node_index_for`, `edges_of_type` | `unimatrix-engine/graph.rs` (vnc-018) | path mode BFS |
| `follow_to_current` (`pub(super)`) | `graph_read_neighbors.rs` (vnc-019) | path mode endpoint + per-hop resolution |
| `all_non_supersedes_types` (`pub(super)`) | `graph_read_neighbors.rs` (vnc-019) | path mode default edge type set |
| Schema v27 indexes `idx_graph_edges_target_type`, `idx_graph_edges_source_type` | vnc-018 ADR-007 | inverse/filter SQL performance |
| `validate_no_unsupported_params` | `graph_read.rs` | centralized cross-mode parameter rejection |
| `RelationType::from_str` | `unimatrix-engine/graph.rs` | validation of caller-supplied edge type strings |
| `EntryRecord` | `unimatrix-store` | inverse/filter response entries |
| infra-001 integration test suite | existing | AC-27 through AC-31 integration tests |

---

## NOT in Scope

- Any new `RelationType` enum variants — all 16 exist (vnc-015)
- `subgraph`, `chain`, `current`, `neighbors` mode behavior changes
- `as_of` timestamp support — Phase 3+, deferred per ASS-057
- `context_batch_write` — out of roadmap scope
- NLI `contradicts_category_pairs` scoping — Wave 3
- `metadata: Option<String>` on `RelationEdge` — not required by any new mode
- Multi-hop path enumeration (all paths) — only shortest path is in scope
- `resolve_supersessions` in `inverse` or `filter` modes — SQL-only, silently ignored
- Bidirectional path search — path mode is outgoing only; `direction` param deferred
- New MCP tool — tool count stays at 14
- Schema migration — version stays at 27
- Research-domain configuration (`research-domain.toml`)

---

## Alignment Status

**Overall**: 5 PASS, 1 WARN (resolved before delivery). No FAIL or VARIANCE classifications.

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — directly completes W1B-2 (`context_graph` tool series, issue #598) |
| Milestone Fit | PASS — pure Wave 1B; no Wave 2+ capabilities introduced |
| Scope Gaps | PASS — all SCOPE.md goals, ACs (AC-01 through AC-31 + AC-32), constraints (C1–C9), and OQs addressed |
| Scope Additions | WARN (resolved) — see below |
| Architecture Consistency | PASS |
| Risk Completeness | PASS — 14 risks, 4 Critical, 6 High, 2 Medium, 2 Low |

### WARN Resolutions (both closed, no delivery action required)

**WARN-1 — `resolve_supersessions` on `inverse`/`filter` modes**: SPECIFICATION.md Param/Mode
Rejection Matrix note confirms silent-ignore behavior (SQL reads live DB regardless). This
is consistent with the `agent_id` field behavior across all modes. No AC is needed for
un-testable silent-ignore behavior; it is documented in the spec matrix.

**WARN-2 — `from_id == to_id` self-path not in SPECIFICATION.md**: Resolved by FR-18a and
AC-32 added to the specification during the design phase. When `from_id == to_id`, path mode
returns `{ found: false, hops: [], length: 0 }`. A corresponding unit test is required
(AC-32).

### Key Risks for Delivery

| Risk | Priority | Required Test |
|------|----------|---------------|
| R-01: path mode staleness disclosure absent/incorrect | Critical | Manual inspection of `tools.rs` description; unit test for not-in-snapshot returning `found: false` |
| R-02: `max_edge_count=0` boundary returns wrong results | Critical | Integration test AC-29 with 0/1/2/3-edge entries; verify `<= ?` binding not special-cased |
| R-03: BFS visited set keyed on raw ID causing double-enqueue when deprecated nodes share terminal | Critical | Unit test with forked deprecated supersession graph; verify visited set keyed on resolved ID (pattern #4494) |
| R-04: `validate_no_unsupported_params` rejection matrix incomplete | Critical | At least one wrong-mode rejection test per each of the 8 new fields |
| R-07: `depth` rejection is a behavior change | High | Unit test per each of the 5 newly-rejecting modes (AC-25) |
| IR-04: `edge_types` IN clause binding | Integration | Use `push_bind` pattern #4058; integration test with multi-type edge_types |

---

## Staleness Disclosure Text (mandatory verbatim in `tools.rs`)

> "path mode uses the in-memory graph cache for BFS traversal. The cache is rebuilt each
> tick (typically 30-60 seconds). Edges written within the current tick interval may not
> appear in the result. This is the same staleness contract as neighbors mode at depth>1
> and subgraph mode. If from_id or to_id is not present in the current graph snapshot, the
> result is { found: false } — not an error. Use resolve_supersessions=true to have
> deprecated endpoints resolved to their active successors before BFS begins."

---

## Param/Mode Rejection Matrix (implementation reference)

Rows = parameters. A = accept, R = reject with named-mode hint. Blank = pre-vnc-020, no change.

| Parameter | chain | current | neighbors | subgraph | inverse | filter | path |
|-----------|-------|---------|-----------|----------|---------|--------|------|
| `category` (new) | R→inv/flt | R→inv/flt | R→inv/flt | R→inv/flt | A | A | R→inv/flt |
| `missing_edge_types` (new) | R→inv | R→inv | R→inv | R→inv | A | R→inv | R→inv |
| `limit` (new) | R→inv/flt | R→inv/flt | R→inv/flt | R→inv/flt | A | A | R→inv/flt |
| `min_age_days` (new) | R→flt | R→flt | R→flt | R→flt | R→flt | A | R→flt |
| `min_confidence` (new) | R→flt | R→flt | R→flt | R→flt | R→flt | A | R→flt |
| `max_confidence` (new) | R→flt | R→flt | R→flt | R→flt | R→flt | A | R→flt |
| `min_edge_count` (new) | R→flt | R→flt | R→flt | R→flt | R→flt | A | R→flt |
| `max_edge_count` (new) | R→flt | R→flt | R→flt | R→flt | R→flt | A | R→flt |
| `depth` (existing) | R→nbr/path | R→nbr/path | A | R→nbr/path | R→nbr/path | R→nbr/path | A |
| `from_id` (existing stub) | R→path | R→path | R→path | R→path | R→path | R→path | A |
| `to_id` (existing stub) | R→path | R→path | R→path | R→path | R→path | R→path | A |
| `resolve_supersessions` (existing) | R | A | A | A | — (silent ignore) | — (silent ignore) | A |
