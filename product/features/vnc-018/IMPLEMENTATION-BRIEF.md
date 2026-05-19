# vnc-018 Implementation Brief: context_graph (chain, current, neighbors)

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-018/SCOPE.md |
| Architecture | product/features/vnc-018/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-018/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-018/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-018/ALIGNMENT-REPORT.md |
| ADR-001 | product/features/vnc-018/architecture/ADR-001-sql-cte-for-supersession-traversal.md |
| ADR-002 | product/features/vnc-018/architecture/ADR-002-truncated-response-envelope.md |
| ADR-003 | product/features/vnc-018/architecture/ADR-003-graphparams-struct-layout.md |
| ADR-004 | product/features/vnc-018/architecture/ADR-004-edgerecord-type-location.md |
| ADR-005 | product/features/vnc-018/architecture/ADR-005-neighbors-execution-split.md |
| ADR-006 | product/features/vnc-018/architecture/ADR-006-advances-motivates-ppr-bfs.md |
| ADR-007 | product/features/vnc-018/architecture/ADR-007-schema-migration-v27.md |
| ADR-008 | product/features/vnc-018/architecture/ADR-008-node-index-accessor.md |

---

## Goal

Add `context_graph` as the 14th MCP tool in the Unimatrix server, exposing three graph read modes — `chain`, `current`, and `neighbors` — that complete the read surface for the typed knowledge graph established by W1B-1 (vnc-015/017). This delivery also completes the deferred `Advances`/`Motivates` PPR/BFS addition from W1B-1 and adds four schema indexes required for efficient traversal at scale.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| mcp/graph_read.rs | pseudocode/graph_read.md | test-plan/graph_read.md |
| mcp/tools.rs (context_graph handler) | pseudocode/tools_dispatch.md | test-plan/tools_dispatch.md |
| unimatrix-store db.rs (SQL query functions) | pseudocode/store_queries.md | test-plan/store_queries.md |
| graph_ppr.rs + graph_expand.rs (PPR/BFS additions) | pseudocode/ppr_bfs.md | test-plan/ppr_bfs.md |
| migration.rs (v26→v27) | pseudocode/migration.md | test-plan/migration.md |
| unimatrix-engine graph.rs (node_index_for accessor) | pseudocode/graph_read.md | test-plan/graph_read.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| chain/current traversal engine | SQL recursive CTEs on `entries.supersedes`/`entries.superseded_by`; `find_terminal_active` (in-memory) is prohibited for both modes | ADR-001 | architecture/ADR-001-sql-cte-for-supersession-traversal.md |
| truncated response shape | `Truncated { forward: bool, backward: bool }` struct — per-direction, always present, never a flat bool | ADR-002 | architecture/ADR-002-truncated-response-envelope.md |
| GraphParams struct layout | Locked struct with centralized `validate_no_unsupported_params` called inside `handle_graph` before mode dispatch; forward-compat fields error on misuse; `resolve_supersessions=Some(true)` on chain mode also rejected here | ADR-003 | architecture/ADR-003-graphparams-struct-layout.md |
| EdgeRecord type location | Defined in `mcp/graph_read.rs`, re-exported from `mcp/mod.rs`; `metadata: Option<serde_json::Value>` always `None` in vnc-018 | ADR-004 | architecture/ADR-004-edgerecord-type-location.md |
| neighbors execution split | depth=1 → live SQL on GRAPH_EDGES; depth>1 → BFS over in-memory TypedRelationGraph; staleness test required | ADR-005 | architecture/ADR-005-neighbors-execution-split.md |
| Advances and Motivates PPR/BFS | Add both to positive type sets in `graph_ppr.rs` (2 locations) and `graph_expand.rs` (1 location) | ADR-006 | architecture/ADR-006-advances-motivates-ppr-bfs.md |
| Schema migration | v26→v27 index-only; 7 mandatory touch points (schema cascade checklist); delivery agent must `grep -r 'schema_version.*== 26' crates/` before Gate 3b | ADR-007 | architecture/ADR-007-schema-migration-v27.md |
| node_index cross-crate visibility | Add `pub fn node_index_for(&self, id: u64) -> Option<NodeIndex>` accessor to `TypedRelationGraph` in `unimatrix-engine/src/graph.rs`; BFS traversal logic stays in `unimatrix-server` | ADR-008 | architecture/ADR-008-node-index-accessor.md |
| BFS visited set keying | `HashSet<u64>` keyed by `node_id` only — each node appears at most once at its minimum depth; `(node_id, depth)` keying rejected (produces duplicates requiring agent-side deduplication) | SPEC FR-06 | — |
| current mode terminal condition | Terminal requires `superseded_by IS NULL AND status = 'Active'`; orphaned deprecated entry (`superseded_by IS NULL`, `status = 'Deprecated'`) is NOT a valid terminal and triggers "no active terminal found" error | SPEC FR-05 | — |
| current/chain non-existent ID asymmetry | `chain` on non-existent ID → empty result (AC-04); `current` on non-existent ID → error "no active terminal found" (AC-05a); intentional and must not be unified | SPEC Constraints §Safety | — |

---

## Files to Create / Modify

### New files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/mcp/graph_read.rs` | All context_graph mode logic: GraphParams, EdgeRecord, Truncated, ChainResult, handle_graph, validate_no_unsupported_params, handle_chain, handle_current, handle_neighbors, follow_to_current. 500-line limit enforced; split into graph_read_supersession.rs + graph_read_neighbors.rs if needed. |
| `crates/unimatrix-store/src/migration_v26_to_v27.rs` | Migration integration test: asserts all 4 new index names present after migration. |

### Modified files

| File | Change |
|------|--------|
| `crates/unimatrix-engine/src/graph.rs` | Add `pub fn node_index_for(&self, id: u64) -> Option<NodeIndex>` method to `TypedRelationGraph`; ~5 lines (ADR-008) |
| `crates/unimatrix-server/src/mcp/mod.rs` | Add `pub(crate) mod graph_read;` and `pub use graph_read::EdgeRecord;` |
| `crates/unimatrix-server/src/mcp/tools.rs` | Add `context_graph` `#[tool]` handler (dispatch only); fully-qualified `graph_read::handle_graph` call per Pattern #4436 |
| `crates/unimatrix-store/src/db.rs` | Add `query_supersession_chain` and `query_direct_neighbors` async functions; add 4 index DDL to `create_tables_if_needed`; bump schema_version literal to 27 |
| `crates/unimatrix-store/src/migration.rs` | Add v26→v27 block (4 indexes); bump `CURRENT_SCHEMA_VERSION` to 27 |
| `crates/unimatrix-store/src/sqlite_parity.rs` | Update `test_schema_version_is_26` → 27; add 4 index-existence assertions; column-count assertions unchanged |
| `crates/unimatrix-server/src/server.rs` | Update all `assert_eq!(version, 26)` to 27 |
| `crates/unimatrix-store/src/migration_v25_to_v26.rs` | Rename exact-version assertion to `assert!(version >= 26)` |
| `crates/unimatrix-server/src/graph_ppr.rs` | Add `Advances` and `Motivates` `edges_of_type` calls in `personalized_pagerank` (~line 131) and `positive_out_degree_weight` (~line 203); update module doc |
| `crates/unimatrix-server/src/graph_expand.rs` | Add `Advances` and `Motivates` `edges_of_type` calls after RelatedTo block (~line 144); update module doc |
| `product/test/infra-001/suites/test_protocol.py` | P-03: assert 14 context_* tools (was 13) |
| `product/test/infra-001/` | Extend fixtures and Python suite with AC-20 tests covering all three modes; include R-03 staleness test; include AC-05a / R-21 asymmetry pair; include R-20 orphaned-deprecated test |

---

## Data Structures

### GraphParams (wire struct, layout locked by ADR-003)

```rust
pub struct GraphParams {
    pub mode: String,                        // "chain" | "current" | "neighbors"
    pub agent_id: Option<String>,
    pub format: Option<String>,
    pub id: Option<u64>,                     // anchor entry ID — required for all three modes
    pub direction: Option<String>,           // chain: "forward"|"backward"|"both"; neighbors: "incoming"|"outgoing"|"both"
    pub edge_types: Option<Vec<String>>,     // neighbors only; absent/[] = all except Supersedes
    pub depth: Option<u8>,                   // neighbors only; 1..=10, default 1
    pub resolve_supersessions: Option<bool>, // neighbors only; default false; rejected on chain mode
    // Forward-compat — error on misuse in current modes:
    pub seed_ids: Option<Vec<u64>>,          // subgraph mode (#597)
    pub max_nodes: Option<u32>,              // subgraph mode (#597)
    pub from_id: Option<u64>,               // path mode (#598)
    pub to_id: Option<u64>,                 // path mode (#598)
}
```

### EdgeRecord (layout locked by ADR-004)

```rust
pub struct EdgeRecord {
    pub source_id: u64,
    pub target_id: u64,
    pub relation_type: String,
    pub direction: String,                   // "incoming" | "outgoing" relative to traversal anchor
    pub depth: u8,
    pub metadata: Option<serde_json::Value>, // always None in vnc-018; never skip_serializing_if
}
```

### Response envelopes

```rust
pub struct Truncated { pub forward: bool, pub backward: bool }
pub struct ChainResult { pub entries: Vec<EntryRecord>, pub truncated: Truncated }
pub struct CurrentResponse { pub entry: EntryRecord }
pub struct NeighborsResponse { pub edges: Vec<EdgeRecord> }
```

### Store-layer types (in db.rs)

```rust
pub enum ChainDirection { Forward, Backward, Both }
pub enum NeighborDirection { Incoming, Outgoing, Both }
pub struct ChainQueryResult { pub entries: Vec<EntryRecord>, pub forward_capped: bool, pub backward_capped: bool }
pub struct RawEdgeRow { pub source_id: u64, pub target_id: u64, pub relation_type: String }
```

---

## Function Signatures

### graph_read.rs (new)

```rust
pub(crate) async fn handle_graph(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: GraphParams,
    ctx: &ToolContext,
) -> Result<CallToolResult, rmcp::ErrorData>
// Ordering inside: validate_no_unsupported_params first, then mode dispatch.
// Capability check (require_cap) runs in tools.rs BEFORE handle_graph is called.

fn validate_no_unsupported_params(params: &GraphParams) -> Result<(), String>
// Centralized; called at top of handle_graph, before mode dispatch.
// Rejects: forward-compat fields on unsupported modes; resolve_supersessions=Some(true) on chain mode.
// Unrecognized mode → "unrecognized mode" error fires before any field checks (_ arm is fallthrough).

async fn handle_chain(store: &Store, params: &GraphParams) -> ChainResult
// Assumes validate_no_unsupported_params has already run. No resolve_supersessions check needed here.

async fn handle_current(store: &Store, params: &GraphParams) -> Result<CurrentResponse, String>
// CTE terminal condition: superseded_by IS NULL AND status = 'Active'.
// Non-existent ID, orphaned deprecated entry, and 50-hop cap → all return "no active terminal found" error.

async fn handle_neighbors(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: &GraphParams,
) -> NeighborsResponse
// depth=1 → SQL; depth>1 → BFS with HashSet<u64> visited set (node_id only).

async fn follow_to_current(store: &Store, id: u64) -> Option<u64>
// 50-hop cap; None = chain too long or orphaned; caller uses original ID, no error propagated.
```

### db.rs (new functions)

```rust
pub async fn query_supersession_chain(
    pool: &SqlitePool,
    id: u64,
    direction: ChainDirection,
    depth_cap: u8,           // 50
) -> Result<ChainQueryResult, StoreError>

pub async fn query_direct_neighbors(
    pool: &SqlitePool,
    id: u64,
    edge_types: &[&str],     // empty = all except Supersedes
    direction: NeighborDirection,
) -> Result<Vec<RawEdgeRow>, StoreError>
```

### unimatrix-engine graph.rs (new method, ADR-008)

```rust
// On TypedRelationGraph:
pub fn node_index_for(&self, id: u64) -> Option<NodeIndex> {
    self.node_index.get(&id).copied()
}
```

---

## current Mode SQL — Critical Detail

The `current` mode CTE **must** include `AND e.status = 'Active'` in the final SELECT. Without it, an entry deprecated via `context_deprecate` with no successor (`superseded_by IS NULL`, `status = 'Deprecated'`) would be returned as the terminal — silently wrong results (R-20, Critical risk).

```sql
WITH RECURSIVE chain(id, depth) AS (
    SELECT id, 0 FROM entries WHERE id = ?1
    UNION ALL
    SELECT e.superseded_by, c.depth + 1
    FROM entries e JOIN chain c ON e.id = c.id
    WHERE e.superseded_by IS NOT NULL AND c.depth < 50
)
SELECT e.* FROM entries e
JOIN chain c ON e.id = c.id
WHERE e.superseded_by IS NULL
  AND e.status = 'Active'
LIMIT 1;
```

All zero-row outcomes (non-existent ID, orphaned deprecated terminal, chain too long) map to the "no active terminal found" error. The CTE does not distinguish between these cases at the SQL level — the handler reports the same error for all, which is intentional (caller's intent is version resolution, not existence check).

---

## Constraints

### Hard implementation constraints

1. `chain` and `current` modes MUST use SQL recursive CTEs. Using `find_terminal_active` (in-memory) is prohibited — it fails silently on cold-start and is stale within the tick window. (ADR-001)
2. All `context_graph` logic goes in `mcp/graph_read.rs`. `tools.rs` contains only the `#[tool]` dispatch point. 500-line limit on the new module. (SCOPE.md, NFR-05)
3. Validation ordering: `require_cap(Read)` runs in `tools.rs` before `handle_graph` is entered. `validate_no_unsupported_params` runs inside `handle_graph` at the top of the function, before mode dispatch. The correct sequence is: **capability check → parameter validation → mode dispatch**. Do not move `validate_no_unsupported_params` before the capability check. (ARCHITECTURE.md §Component Interactions, ADR-003)
4. `validate_no_unsupported_params` must reject `resolve_supersessions=Some(true)` on chain mode. This check belongs in the centralized function — not inside `handle_chain`. The `handle_chain` implementation can assume the parameter is absent. (ARCHITECTURE.md §Validation Strategy, R-08)
5. `current` mode CTE terminal condition: `superseded_by IS NULL AND status = 'Active'`. The `AND e.status = 'Active'` filter is mandatory — omitting it causes orphaned deprecated entries to be silently returned as valid terminals. (SPEC FR-05, R-20)
6. `current` mode on a non-existent ID returns an error — NOT an empty result. This is intentionally asymmetric with `chain` mode (AC-04 returns empty). Implementations must not unify these behaviors. Both AC-04 and AC-05a must be present as a matched pair in the test suite. (SPEC Constraints §Safety, R-21)
7. BFS visited set must be `HashSet<u64>` keyed by `node_id` only. Do NOT key on `(node_id, depth)`. Each node appears at most once in the result, at its minimum hop depth. (SPEC FR-06, AC-11a, R-18)
8. `Capability::Read` required for all three modes. Capability check runs in `tools.rs` before `handle_graph`. (FR-02)
9. Read operations use `read_pool()` per C-07. Write pool never accessed in context_graph. (SPEC Constraints §4)
10. `Supersedes` excluded from neighbors mode: silently from "all types" default expansion (no warning, no excluded_types field in response); explicitly rejected with exact error message when specified. (FR-07, AC-10a)
11. `EdgeRecord.metadata` must serialize as JSON `null`, not as an absent field. Do NOT use `#[serde(skip_serializing_if = "Option::is_none")]` on this field. (ADR-004, R-15)
12. Every call from `tools.rs` to `graph_read.rs` must use a fully-qualified module path. (Pattern #4436)
13. 50-hop safety cap enforced at CTE level (`WHERE depth < 50`) for chain/current; in the `follow_to_current` loop for neighbors. (NFR-04)
14. `depth` parameter validated to `1..=10` before any BFS executes. (SPEC Constraints §Safety)
15. `neighbors` direction values: `"incoming"` / `"outgoing"` / `"both"`. Reject `"forward"` / `"backward"` with a mode-specific error. (R-17)
16. `node_index_for` accessor must be implemented in `unimatrix-engine/src/graph.rs` as part of this feature (ADR-008). This is not a future-phase task — BFS cannot compile without it.

### Schema cascade (7 mandatory touch points — ADR-007)

All 7 must be complete before Gate 3b:
1. `migration.rs` — v26→v27 block + `CURRENT_SCHEMA_VERSION = 27`
2. `db.rs` — 4 index DDL in `create_tables_if_needed` + schema_version literal → 27
3. `sqlite_parity.rs` — `test_schema_version_is_N` → 27 + 4 index-existence assertions
4. `server.rs` — all `assert_eq!(version, 26)` → 27
5. `migration_v25_to_v26.rs` — exact-version assertion → `assert!(version >= 26)`
6. New `migration_v26_to_v27.rs` — asserts all 4 index names
7. `db.rs::test_schema_version_initialized_to_current_on_fresh_db` — expected value → 27

Delivery agent must run `grep -r 'schema_version.*== 26' crates/` and confirm zero matches.

### Branch dependency (hard gate-0)

Delivery branch must be cut from post-vnc-017 merged main. The codebase must contain: 16 `RelationType` variants in `graph.rs`, `edge_write.rs`, `query_incoming_edges`. Smoke test: `neighbors` with `edge_types=["Advances"]` must not return "unknown edge type" error.

---

## Dependencies

### Hard dependencies (must be merged before delivery branch cut)

| Dependency | Feature | Status |
|------------|---------|--------|
| W1B-1 Typed Edge Write Path | vnc-015, PR #600 | Must be merged |
| Auto-redirect | vnc-017, `feature/vnc-017` | Must be merged to main first — delivery branches from post-vnc-017 state (SR-08) |

### Crates

| Crate / Library | Usage |
|-----------------|-------|
| `sqlx` (workspace) | SQL recursive CTEs for chain/current; composite index queries for neighbors depth=1 |
| `petgraph` (workspace, via unimatrix-vector/core) | TypedRelationGraph BFS for neighbors depth>1 |
| `serde_json` (workspace) | `EdgeRecord.metadata: Option<serde_json::Value>` |
| `rmcp 0.16` (workspace) | `#[tool]` attribute and MCP dispatch |
| `tracing` (workspace) | `tracing::warn!` for unrecognized relation_type strings in BFS |

### Existing components consumed

| Component | File | Usage |
|-----------|------|-------|
| `TypedRelationGraph` | `unimatrix-engine/src/graph.rs` | `edges_of_type()`, `node_index_for(id: u64) -> Option<NodeIndex>` (new accessor, ADR-008) |
| `find_terminal_active` | `graph.rs:523` | NOT used — pattern reference only; SQL CTE path is mandatory |
| `query_incoming_edges` | `edge_write.rs` (vnc-017) | Pattern reference for Supersedes exclusion |
| `McpServerImpl` | `tools.rs` | `#[tool]` handler registration |
| `require_cap` | service layer | Capability gate — runs in `tools.rs` before `handle_graph` |
| `read_pool()` | `db.rs:294` | All read operations |
| `graph_ppr.rs` | `graph_ppr.rs` | Modified for ADR-006 |
| `graph_expand.rs` | `graph_expand.rs` | Modified for ADR-006 |
| infra-001 test suite | `product/test/infra-001/` | Extended for AC-20 + R-03 staleness test + AC-05a/R-21 asymmetry pair + R-20 orphaned-deprecated test |

---

## NOT in Scope

1. `subgraph` mode — multi-seed BFS returning node+edge sets, 200-node cap (W1B-2b, #597)
2. `inverse` mode — antijoin: entries missing expected incoming edges (W1B-2c, #598)
3. `path` mode — shortest path between two entries (W1B-2c, #598)
4. `filter` mode — property and edge-count filter (W1B-2c, #598)
5. `metadata` field population on EdgeRecord — `RelationEdge` does not carry metadata; W1B-2b extends it
6. New `RelationType` variants — all 16 variants are established by W1B-1
7. `resolve_supersessions` on chain mode — semantically circular; rejected with error in `validate_no_unsupported_params`
8. `revision_reason` accessibility via supersession chain — `GRAPH_EDGES` Supersedes rows are skip-loaded; direct SQL only, not addressed here
9. `context_batch_write` — HNSW atomicity open question; out of roadmap scope
10. Research domain configuration (`research-domain.toml`, category provisioning) — separate feature
11. `excluded_types` response field — silent Supersedes exclusion produces no warning and no extra field (AC-10a)

---

## Alignment Status

**Overall: PASS.** Vision alignment confirmed — vnc-018 directly delivers W1B-2a, the read complement to the W1B-1 write surface. All eight ADRs resolve SCOPE.md open questions and design-review findings. Architecture, specification, and risk strategy are internally consistent after the amendment pass.

**Two non-blocking WARNs from the alignment report (delivery agent should treat as resolved):**

- WARN-1 (OQ-01 conflict): SPECIFICATION.md re-opens the question of what `neighbors` mode returns for a non-existent anchor ID. SCOPE.md already resolved this: return empty (consistent with AC-04 for chain mode). Delivery agent must follow SCOPE.md's resolution — return empty `NeighborsResponse`, no error. No additional architect input needed.

- WARN-2 (OQ-02 depth upper bound): SPECIFICATION.md specifies `1..=10` as the authoritative constraint in NFR/Safety Constraints sections, then redundantly marks it as an open question. Treat the NFR/Constraints text as authoritative. Validate `depth` to `1..=10`; error if outside range. No additional input needed.

**Amendment pass changes reflected (6 findings):**

1. `current` mode CTE `AND e.status = 'Active'` filter is now a hard constraint (R-20, Critical). Orphaned deprecated entries produce "no active terminal found" error, not a returned entry.
2. BFS visited set keying resolved: `HashSet<u64>` by `node_id` only (AC-11a added to acceptance map).
3. `validate_no_unsupported_params` rejects `resolve_supersessions=Some(true)` on chain mode in the centralized function — `handle_chain` need not check it.
4. `current` mode non-existent ID returns error (AC-05a added). Intentional asymmetry with chain mode (AC-04 empty). Both must be present as a test pair.
5. Validation ordering corrected: capability check → parameter validation → mode dispatch. `validate_no_unsupported_params` is inside `handle_graph`, not before the capability check.
6. R-07 resolved by ADR-008: `node_index_for` accessor on `TypedRelationGraph` is a delivery-time implementation task, not an open architectural decision. Delivery agent must implement this accessor.

**New risks (R-20, R-21) are reflected in constraints above.**

No variances requiring human approval.

## Tracking

https://github.com/dug-21/unimatrix/issues/608
