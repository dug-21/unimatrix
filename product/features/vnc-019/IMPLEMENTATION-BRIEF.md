# vnc-019 Implementation Brief: context_graph subgraph Mode

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-019/SCOPE.md |
| Architecture | product/features/vnc-019/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-019/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-019/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-019/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| graph_read.rs (GraphParams + SubgraphResponse + dispatch) | pseudocode/graph_read.md | test-plan/graph_read.md |
| graph_read_subgraph.rs (BFS + metadata hydration) | pseudocode/graph_read_subgraph.md | test-plan/graph_read_subgraph.md |
| graph_read_neighbors.rs (follow_to_current visibility) | pseudocode/graph_read_neighbors.md | test-plan/graph_read_neighbors.md |
| tools.rs (tool description update) | pseudocode/tools.md | test-plan/tools.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Add `subgraph` mode to the existing `context_graph` MCP tool, enabling bounded multi-hop BFS from one or more seed entries that returns both the discovered entry records and the typed edges between them. The result set gives consuming agents enough information to reconstruct the full subgraph locally without additional queries, supporting the research-domain traversal patterns (Goal evidence graph, Thesis evidence chain, Contradiction surface) mandated by Wave 1B of the product roadmap.

---

## Hard Delivery Constraint

**vnc-019 delivery is blocked until vnc-018 PR #596 merges.** `graph_read.rs` is a stub returning `INTERNAL_ERROR` on branch `feature/vnc-018`. The full chain/current/neighbors implementation (including `TypedRelationGraph`, `node_index_for`, `edges_of_type`, `EdgeRecord`, `GraphParams`, `validate_no_unsupported_params`, `follow_to_current`, `all_non_supersedes_types`, and schema v27 indexes) must be present before any vnc-019 delivery work begins.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| `max_depth` field location | Add `max_depth: Option<u8>` to `GraphParams` as an `Option<T>` backward-compatible extension. Rejected on chain/current/neighbors with message `"max_depth is not supported in {mode} mode — use subgraph mode"`. Default 3 when absent, valid range [1, 10]. | SCOPE.md OQ-01, Specification FR-06 | architecture/ADR-001-graphparams-max-depth-extension.md |
| `handle_subgraph` file placement | New `graph_read_subgraph.rs` declared as a `#[path]`-submodule of `graph_read.rs`. `SubgraphResponse` defined in `graph_read.rs` alongside other response envelopes. Tests in `graph_read_subgraph_tests.rs` declared via `#[path]` inside `graph_read_subgraph.rs`. | SCOPE.md Constraint 3, SR-04 | architecture/ADR-002-file-split-graph-read-subgraph.md |
| Post-BFS metadata strategy | Single batch GRAPH_EDGES query using a dynamically built OR-chain after BFS completes. O(1) round-trips. Skipped entirely when `collected_edges` is empty. Empty-edge guard is a correctness requirement, not an optimization. | SCOPE.md Background / EdgeRecord metadata, Specification FR-14 | architecture/ADR-003-post-bfs-metadata-batch-query.md |
| Staleness disclosure mechanism | Tool description text only. `SubgraphResponse` does NOT include `graph_rebuilt_at` or `graph_age_ms`. `depth_reached` and `truncated` are the caller's traversal-bound signals. | SCOPE-RISK-ASSESSMENT SR-01, Specification FR-19 / C-08 | architecture/ADR-004-staleness-disclosure-no-timestamp.md |
| In-memory BFS only | `TypedRelationGraph` for hop enumeration; no SQL fallback for depth > 1. Cold-start returns empty result, not error. | Inherited from vnc-018 ADR-005 (Unimatrix #4479) | N/A — inherited decision |
| `EdgeRecord` unchanged structurally | `metadata` field populated for first time via post-BFS SQL; not through `RelationEdge` engine struct. | Inherited from vnc-018 ADR-004 (Unimatrix #4478) | N/A — inherited decision |
| `GraphParams` struct lock | Field removal/retyping prohibited. `Option<T>` additions permitted for backward compatibility. | Inherited from vnc-018 ADR-003 (Unimatrix #4477) | N/A — inherited decision |
| `max_nodes > 200` behavior | **RESOLVED by alignment review**: Reject with validation error `"max_nodes must be in range 1..=200, got {value}"`. Consistent with `max_depth` range validation. Silent clamping is prohibited. Document in tool description. | ALIGNMENT-REPORT.md variance FR-07 | N/A — resolved in brief |
| `follow_to_current` re-use | `pub(super)` in `graph_read_neighbors.rs`. No private copy permitted. A stale copy would drift silently if the 50-hop guard or `Store::get` signature changes. | Specification FR-08, Architecture Component 3 | N/A — visibility rule |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/mcp/graph_read.rs` | Modify | Add `max_depth: Option<u8>` to `GraphParams`; define `SubgraphResponse`; add `"subgraph"` arm to `validate_no_unsupported_params` (permitting `seed_ids`, `max_nodes`, `max_depth`; rejecting `from_id`, `to_id`); add `"subgraph"` dispatch arm to `handle_graph`; update unrecognized-mode error to list `subgraph` as a supported mode |
| `crates/unimatrix-server/src/mcp/graph_read_subgraph.rs` | Create (new) | BFS traversal loop, parameter validation, `resolve_supersessions` substitution, `max_nodes` cap enforcement, post-BFS dangling-edge filter, batch node hydration, post-BFS metadata batch query, `SubgraphResponse` construction |
| `crates/unimatrix-server/src/mcp/graph_read_subgraph_tests.rs` | Create (new) | Unit and integration tests for subgraph mode; covers all Critical and High risks from RISK-TEST-STRATEGY |
| `crates/unimatrix-server/src/mcp/graph_read_neighbors.rs` | Modify | Change `follow_to_current` and `all_non_supersedes_types` visibility from private to `pub(super)` so sibling subgraph module can import them |
| `crates/unimatrix-server/src/mcp/tools.rs` | Modify | Update `context_graph` tool description to include: subgraph mode section, staleness disclosure text (FR-19), `direction` always `"outgoing"` note, truncation semantics, unknown seed behavior, `max_nodes > 200` rejection behavior |

No other files are modified. No new crates, no new tables, no new migrations.

---

## Data Structures

### `GraphParams` extension (in `graph_read.rs`)

```rust
pub struct GraphParams {
    // ... existing fields (from_id, to_id, mode, direction, edge_types,
    //                       seed_ids, max_nodes) unchanged ...

    /// subgraph mode only: BFS max depth 1..=10 (default 3 when absent).
    /// Error if passed to chain, current, or neighbors modes.
    pub max_depth: Option<u8>,
}
```

### `SubgraphResponse` (in `graph_read.rs`)

```rust
#[derive(serde::Serialize)]
pub struct SubgraphResponse {
    pub nodes: Vec<EntryRecord>,
    pub edges: Vec<EdgeRecord>,
    pub truncated: bool,
    pub seed_ids: Vec<u64>,
    pub depth_reached: u8,
}
```

`SubgraphResponse` is defined adjacent to `ChainResult`, `CurrentResponse`, and `NeighborsResponse` in `graph_read.rs`. Wire format: JSON object with those five fields.

### BFS Internal State (within `handle_subgraph`)

```rust
let mut visited: HashSet<u64>;                        // keyed on effective_id (post-substitution)
let mut frontier: VecDeque<(NodeIndex, u64, u8)>;     // (graph_idx, entry_id, current_depth)
let mut collected_edges: Vec<(u64, u64, String, u8)>; // (source_id, target_id, rel_type, depth)
let mut collected_node_ids: Vec<u64>;
let mut edge_set: HashSet<(u64, u64, String)>;        // dedup by canonical triple
let mut truncated: bool = false;
```

### Metadata Batch Map (within `handle_subgraph`)

```rust
HashMap<(u64, u64, String), Option<serde_json::Value>>
```

Built from the post-BFS GRAPH_EDGES query result. Used to populate `EdgeRecord.metadata` before constructing the response.

---

## Function Signatures

### `handle_subgraph` (new, in `graph_read_subgraph.rs`)

```rust
pub(super) async fn handle_subgraph(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: &GraphParams,
) -> Result<SubgraphResponse, ErrorData>
```

### `follow_to_current` (existing, visibility change in `graph_read_neighbors.rs`)

```rust
pub(super) async fn follow_to_current(store: &Store, id: u64) -> Option<u64>
```

### `all_non_supersedes_types` (existing, visibility change in `graph_read_neighbors.rs`)

```rust
pub(super) fn all_non_supersedes_types() -> Vec<RelationType>
```

Returns 15 `RelationType` variants (all except `Supersedes`). This is the default `edge_types` expansion when the caller omits or empties `edge_types`.

### Post-BFS metadata batch query (inline in `graph_read_subgraph.rs`)

```rust
// Builds and executes:
// SELECT source_id, target_id, relation_type, metadata
// FROM graph_edges
// WHERE (source_id = ?1 AND target_id = ?2 AND relation_type = ?3)
//    OR (source_id = ?4 AND target_id = ?5 AND relation_type = ?6)
//    -- one OR clause per collected edge
// Issued via store.read_pool_server() only when collected_edges is non-empty.
```

Metadata deserialization: `serde_json::from_str(text).ok()` — returns `None` on malformed JSON without panic (SEC-05).

---

## BFS Algorithm Contract

The BFS in `handle_subgraph` follows this exact ordering (from ARCHITECTURE.md):

1. **Validate** — `seed_ids` non-empty; `max_depth` in [1, 10]; `max_nodes` in [1, 200] (reject > 200 with validation error); each `edge_type` parses via `RelationType::from_str`; `direction` in ["incoming", "outgoing", "both"]; expand empty/absent `edge_types` to `all_non_supersedes_types()`.

2. **Acquire graph** — `std::sync::RwLock` read lock; clone `TypedRelationGraph`; release before any async work.

3. **Seed phase** — for each `seed_id`: if `resolve_supersessions`, call `follow_to_current` → use terminal (fallback: original). If not in `visited` and `collected_node_ids.len() < max_nodes`: add to `visited`, `collected_node_ids`; push to frontier at depth=0. If seeds alone reach `max_nodes`: `truncated=true`, skip BFS.

4. **BFS phase** — while frontier non-empty: pop `(current_idx, current_id, current_depth)`. If `current_depth >= max_depth`: continue. For each `rel_type` × `petgraph_dir`: call `edges_of_type`. Build canonical edge key `(edge_src, edge_tgt, rel_type_str)` using stored direction (source→target). Dedup by `edge_set`. Record edge with `depth = current_depth + 1`. Compute `effective_id` (apply supersession resolution). If `effective_id` not in visited: if `collected_node_ids.len() >= max_nodes`: `truncated=true`, goto POST_BFS. Otherwise: add to `visited`, `collected_node_ids`, push to frontier.

5. **Dangling-edge filter (POST_BFS, required correctness step)** — remove any edge from `collected_edges` whose `source_id` or `target_id` is not in `collected_node_ids`. This prevents `EdgeRecord`s referencing nodes absent from `nodes` when the cap fires mid-hop.

6. **Batch node hydration** — `store.get_many(collected_node_ids)` — single query.

7. **Post-BFS metadata batch** — if `collected_edges` non-empty: issue OR-chain SQL against `GRAPH_EDGES`; build `metadata_map`; populate `EdgeRecord.metadata`. Skip entirely when `collected_edges` is empty.

8. **Compute `depth_reached`** — `collected_edges.iter().map(|e| e.depth).max().unwrap_or(0)`.

9. **Return** `SubgraphResponse { nodes, edges, truncated, seed_ids, depth_reached }`.

---

## Critical Risk Implementation Notes

**R-01 — resolve_supersessions ordering**: Substitution (`follow_to_current`) MUST happen BEFORE the `visited` check. The `visited` set is keyed on `effective_id` (post-substitution), not on the original deprecated node ID. This prevents deprecated nodes from appearing in results and prevents the terminal node from being double-enqueued via multiple paths.

**R-02 — direction="both" dedup**: The `edge_key` for deduplication MUST be built from the canonical stored direction `(source→target)`, not from the iteration variable's perspective. When traversing an incoming edge `A→B` from node B, the canonical key is still `(A, B, rel_type)`, not `(B, A, rel_type)`. The `direction` field on all returned `EdgeRecord`s is always `"outgoing"`. See Unimatrix lesson #4077.

**R-03 — seed count at cap boundary**: Seeds are added to `collected_node_ids` before BFS begins. If seeds alone equal or exceed `max_nodes`, BFS must NOT execute, and the response must have `truncated=true, depth_reached=0`.

**R-04 — empty OR-chain guard**: The metadata batch query MUST be skipped when `collected_edges` is empty. An empty `WHERE` clause in the dynamically built SQL is a syntax error or full-table scan.

**R-05 — validate_no_unsupported_params regression**: After adding the `"subgraph"` arm, the test `test_validate_unrecognized_mode_fires_before_field_check` in `graph_read_subgraph_tests.rs` (or its vnc-018 source) must be updated: remove the `mode="subgraph"` case from the unrecognized-mode test; add it to the recognized-mode acceptance test. This is part of FR-20 delivery.

---

## Validation Error Messages (exact strings)

| Condition | Error Message |
|-----------|---------------|
| `seed_ids` absent or empty on subgraph mode | `"subgraph mode requires at least one entry ID in seed_ids"` |
| `max_depth` out of range | `"max_depth must be in range 1..=10, got {depth}"` |
| `max_nodes` above 200 | `"max_nodes must be in range 1..=200, got {value}"` |
| unknown `edge_type` string | `"unrecognized edge_type '{value}' — recognized types: Supports, Contradicts, ..."` (list all 16) |
| `direction` invalid | `"direction must be one of: incoming, outgoing, both"` |
| `seed_ids` on chain/current/neighbors | existing validation error from `validate_no_unsupported_params` |
| `max_depth` on chain/current/neighbors | `"max_depth is not supported in {mode} mode — use subgraph mode"` |
| `from_id` or `to_id` on subgraph | existing validation error from `validate_no_unsupported_params` |
| unrecognized mode | `"unrecognized mode '{x}' — supported modes: chain, current, neighbors, subgraph"` |

---

## Wire Response Format

```json
{
  "nodes": [
    { /* full EntryRecord — same shape as other context_graph modes */ }
  ],
  "edges": [
    {
      "source_id": 42,
      "target_id": 57,
      "relation_type": "Supports",
      "direction": "outgoing",
      "depth": 1,
      "metadata": null
    }
  ],
  "truncated": false,
  "seed_ids": [42],
  "depth_reached": 2
}
```

`direction` is always `"outgoing"` for every `EdgeRecord` in subgraph mode, reflecting the canonical stored direction (`source_id → target_id`). This is documented in the tool description (FR-19).

Empty-result response (cold-start or all seeds absent from graph):

```json
{
  "nodes": [],
  "edges": [],
  "truncated": false,
  "seed_ids": [42],
  "depth_reached": 0
}
```

---

## Tool Description Requirements (FR-19)

The `context_graph` tool description in `tools.rs` must include the following text in the subgraph mode section (exact text or equivalent preserving all facts):

> "subgraph mode uses the in-memory graph cache for BFS traversal. The cache is rebuilt each tick (typically 30-60 seconds). Edges written within the current tick interval may not appear in the result. This is the same staleness contract as neighbors mode at depth>1. The `depth_reached` field reports the actual maximum BFS depth traversed; `truncated: true` indicates the `max_nodes` cap was reached before BFS completed. Seed IDs not present in the graph return an empty result — not an error. The `direction` field in returned EdgeRecords is always `outgoing`, reflecting the canonical stored direction (source_id → target_id) regardless of the traversal direction parameter. `max_nodes` must be in range 1..=200; values above 200 are rejected with a validation error."

---

## Constraints

1. **vnc-018 PR #596 must merge before delivery begins** (SR-06, C-01). `graph_read.rs` is a stub until then.
2. **Schema v27 required; no new migration** (C-02). Four indexes from vnc-018 migration v26→v27 are prerequisites.
3. **`GraphParams` is a wire contract** (C-03). Field removal and retyping are prohibited. `max_depth: Option<u8>` is a permitted backward-compatible addition.
4. **500-line file limit on `graph_read.rs`** (C-04). Resolved by ADR-002: `handle_subgraph` goes in `graph_read_subgraph.rs`.
5. **In-memory BFS only; no SQL fallback** (C-05). Cold-start returns empty result, not error.
6. **No engine changes** (C-06). `unimatrix-engine`, `TypedRelationGraph`, `RelationEdge` — not modified.
7. **No new MCP tool** (C-07). Tool count remains 14.
8. **No `graph_rebuilt_at` in response** (C-08, ADR-004). Staleness disclosed via tool description only.

---

## Dependencies

### Crates Modified

- `unimatrix-server` — the only crate modified

### Crates Consumed Read-Only

- `unimatrix-engine` — `TypedRelationGraph`, `node_index_for`, `node_id_for_index`, `edges_of_type`, `RelationType`, `Direction`
- `unimatrix-store` — `Store::get_many` (batch hydration), `store.read_pool_server()` (metadata query)
- `petgraph` — already linked via `unimatrix-engine`; no new dependency
- `sqlx` — already used in `unimatrix-server` for direct SQL
- `serde_json` — already used for `EdgeRecord.metadata`
- `rmcp 0.16.0` — `CallToolResult::success` pattern unchanged

### External Services

None.

### Schema

- `GRAPH_EDGES` table: `source_id`, `target_id`, `relation_type`, `metadata` columns
- `ENTRIES` table: batch node hydration
- `idx_graph_edges_source_type (source_id, relation_type)` — schema v27 (vnc-018 required)
- `idx_graph_edges_target_type (target_id, relation_type)` — schema v27 (vnc-018 required)

---

## NOT in Scope

- `inverse` mode, `path` mode, `filter` mode — all deferred to W1B-2c (#598)
- Any new `RelationType` enum variants (all 16 exist from vnc-015)
- Adding `metadata: Option<String>` to `RelationEdge` in `unimatrix-engine`
- SQL-only BFS fallback path for depth > 1
- `as_of` timestamp parameter for historical subgraph queries
- `graph_rebuilt_at` or `graph_age_ms` field in `SubgraphResponse`
- Structured truncation reason (seed saturation vs. BFS expansion) — W1B-2c
- Batch supersession pre-resolution before BFS
- `research-domain.toml` configuration
- NLI `contradicts_category_pairs` scoping
- `context_batch_write` MCP tool
- Any change to `unimatrix-engine`, `unimatrix-store`, `unimatrix-vector`, or `unimatrix-embed`
- Any new MCP tool, route, or database migration

---

## Alignment Status

**Vision Alignment**: PASS — subgraph mode directly implements "Goal → Decision → Outcome audit subgraphs in a single query" from the product vision. In-memory hot-path rule respected (BFS uses `TypedRelationGraph`; SQL only post-BFS). Single-binary non-negotiable respected (no new crate, tool, or table).

**Milestone Fit**: PASS — correctly scoped as W1B-2b (#597). No W1B-2c capabilities pulled forward. Dependency on vnc-018 (W1B-2a) documented as hard constraint.

**Architecture Consistency**: PASS — all ADRs inherit from or extend vnc-018 correctly. Lock discipline, BFS traversal surface, and `resolve_supersessions` ordering are all consistent with established patterns.

**Variances**: One WARN resolved in this brief.

**FR-07 max_nodes > 200 behavior (WARN → RESOLVED in brief)**: The alignment report flagged that the specification left the clamp-vs-reject behavior to the "architect decision." This brief resolves it: values above 200 are **rejected** with a validation error (`"max_nodes must be in range 1..=200, got {value}"`). This is consistent with `max_depth` range validation (FR-06) and with the product vision's requirement for predictable tool contracts. Silent clamping is prohibited. The tool description must document this behavior explicitly. No silent behavioral surprises for callers.
