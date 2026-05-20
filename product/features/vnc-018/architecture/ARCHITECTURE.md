# vnc-018 Architecture: context_graph Tool (14th MCP Tool)

## System Overview

vnc-018 delivers the first three traversal modes of the `context_graph` MCP tool
(GH #596), completing the read surface for the typed knowledge graph. W1B-1
(`vnc-015`, PR #600) established the graph write surface — all 16 `RelationType`
variants, `GRAPH_EDGES` persistence, and `context_edge` as the 13th tool. vnc-018
adds the 14th tool and consumes the write surface for the first time through read
operations.

Within the server, `context_graph` follows the module extraction pattern established
by `edge_write.rs` (ADR-005, vnc-015): `tools.rs` contains only the `#[tool]`
dispatch point; all mode logic lives in a new sibling module `mcp/graph_read.rs`.

**Dependency gate**: vnc-017 (auto-redirect, `feature/vnc-017`) must be merged to
main before any vnc-018 delivery branch is cut. The post-vnc-017 codebase state
(`graph.rs` with 16 variants, `query_incoming_edges`, `edge_write.rs`) is the
required base. See SR-08.

---

## Component Breakdown

### 1. `mcp/graph_read.rs` (new module)

Owns all `context_graph` dispatch and mode logic. Exposes one public entry point
called from `tools.rs`:

```rust
pub(crate) async fn handle_graph(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: GraphParams,
    ctx: &ToolContext,
) -> Result<CallToolResult, rmcp::ErrorData>
```

Internally routes via `match params.mode.as_str()` to:
- `handle_chain(&store, &params) -> ChainResult`
- `handle_current(&store, &params) -> CurrentResult`
- `handle_neighbors(&store, typed_graph_state, &params) -> NeighborsResult`

**Responsibility**: parameter validation, mode dispatch, response formatting.
The module must not exceed 500 lines; if it approaches the limit, split
`handle_chain`/`handle_current` into `mcp/graph_read_supersession.rs` and
`handle_neighbors` into `mcp/graph_read_neighbors.rs`.

### 2. `mcp/tools.rs` — `context_graph` handler (addition only)

Adds one `#[tool(...)]` attributed method to `McpServerImpl`. The body follows the
established ceremony: build context, require cap, delegate to `graph_read::handle_graph`.
No mode logic in `tools.rs`. Full module path qualifier required per Pattern #4436.

### 3. `unimatrix-store` — SQL query functions

Two new async functions added to `db.rs` (or a new `graph_read.rs` submodule of
`unimatrix-store`):

```rust
pub async fn query_supersession_chain(
    pool: &SqlitePool,
    id: u64,
    direction: ChainDirection,  // Forward | Backward | Both
    depth_cap: u8,              // 50
) -> Result<ChainQueryResult, StoreError>

pub async fn query_direct_neighbors(
    pool: &SqlitePool,
    id: u64,
    edge_types: &[&str],        // empty = all except Supersedes
    direction: NeighborDirection, // Incoming | Outgoing | Both
) -> Result<Vec<RawEdgeRow>, StoreError>
```

`query_supersession_chain` uses recursive CTEs on `entries.supersedes` /
`entries.superseded_by`. `query_direct_neighbors` uses composite indexes
`idx_graph_edges_source_type` / `idx_graph_edges_target_type`.

### 4. `graph_ppr.rs` and `graph_expand.rs` — PPR/BFS additions

Add `Advances` and `Motivates` to the positive-type sets. Approximately 16 lines
across both files. These are targeted surgical additions; the files are not
restructured.

### 5. Schema migration (v26 → v27)

Four new indexes added in a single migration step. No table DDL changes.

---

## Component Interactions

```
tools.rs (context_graph handler)
  │  require_cap(Read)               ← capability check runs FIRST (in tools.rs)
  │  build_context_with_external_identity
  ▼
graph_read.rs (handle_graph)
  │  validate_no_unsupported_params  ← parameter validation runs SECOND (inside handle_graph)
  │
  ├─ mode="chain" ──────────────────► handle_chain
  │                                      │ SQL recursive CTE
  │                                      ▼
  │                                   query_supersession_chain (db.rs)
  │                                      │ uses idx_entries_supersedes /
  │                                      │ idx_entries_superseded_by
  │                                      ▼
  │                                   ChainResult { entries, truncated }
  │
  ├─ mode="current" ────────────────► handle_current
  │                                      │ SQL recursive CTE
  │                                      ▼
  │                                   query_supersession_chain (db.rs)
  │                                      │ single forward-only walk
  │                                      ▼
  │                                   EntryRecord | error
  │
  └─ mode="neighbors" ──────────────► handle_neighbors
       │
       ├─ depth=1 ──────────────────► query_direct_neighbors (db.rs)
       │                                uses idx_graph_edges_source_type /
       │                                idx_graph_edges_target_type
       │
       └─ depth>1 ──────────────────► BFS over TypedRelationGraph
                                       Arc<RwLock<TypedGraphState>>
                                       edges_of_type() per type per hop
                                       follow_to_current() if resolve_supersessions=true
```

**Validation ordering — intentional and correct**: `require_cap(Read)` runs in
`tools.rs` before `handle_graph` is called. `validate_no_unsupported_params` runs
inside `handle_graph` at the top of the function, before mode dispatch. This ordering
(capability check → parameter validation → mode dispatch) is the established pattern
in this codebase. The delivery agent must NOT move `validate_no_unsupported_params`
before the capability check; the current position inside `handle_graph` is correct.

---

## Technology Decisions

### SQL Recursive CTEs for chain/current modes (ADR-001)

Both `chain` and `current` modes use SQLite recursive CTEs on `entries.supersedes`
/ `entries.superseded_by`, not the in-memory `TypedRelationGraph`. See ADR-001.

### `truncated` response shape — per-direction struct (ADR-002)

`chain` mode response carries a `Truncated` struct with two booleans. See ADR-002.

### `GraphParams` forward-compat struct layout (ADR-003)

Struct layout locked with validation-on-misuse for forward-compat fields. See ADR-003.

### `EdgeRecord` location — `mcp/graph_read.rs` with re-export (ADR-004)

`EdgeRecord` is defined in `graph_read.rs`, re-exported from `mcp/mod.rs` for
#597/#598 consumers. See ADR-004.

### neighbors mode execution split — SQL at depth=1, in-memory BFS at depth>1 (ADR-005)

Resolved in SCOPE.md OQ-01. depth=1 is always live-database; depth>1 uses the
in-memory `TypedRelationGraph` with a documented tick-window staleness constraint.
See ADR-005.

### `Advances` and `Motivates` added to PPR/BFS positive types (ADR-006)

Completes the write-only deferral from W1B-1 (vnc-015 ADR-006). See ADR-006.

### Schema migration v26→v27 (ADR-007)

Four indexes added as a single migration step, no table DDL changes. See ADR-007.

### `node_index_for` accessor on `TypedRelationGraph` (ADR-008)

Cross-crate visibility for BFS traversal resolved by adding a `pub fn node_index_for`
accessor to `TypedRelationGraph` in `unimatrix-engine`. BFS traversal logic stays in
`unimatrix-server`. See ADR-008.

---

## Integration Points

### Existing components consumed

| Component | What's used | Notes |
|-----------|-------------|-------|
| `unimatrix-engine::graph::RelationType` | `from_str()` for edge_types validation | All 16 variants already registered |
| `unimatrix-engine::graph::TypedRelationGraph` | `edges_of_type()`, `node_index_for(id: u64) -> Option<NodeIndex>` | depth>1 neighbors only; `node_index_for` accessor added per ADR-008 |
| `unimatrix-engine::graph::find_terminal_active` | NOT used by chain/current | SQL CTE path instead; used as pattern reference only |
| `unimatrix-store::db` | New SQL query functions | query_supersession_chain, query_direct_neighbors |
| `unimatrix-core::Store` | `read_pool()` accessor | All read operations use read_pool() per C-07 |
| `mcp::tools::ToolContext` | Identity + audit ceremony | Standard vnc-008 pattern |
| `mcp::edge_write` | Not called; read-only tool | EdgeValidationError variants are a reference model |
| `services::typed_graph::TypedGraphState` | Arc<RwLock<>> access | depth>1 neighbors only |

### Components modified

| File | Change |
|------|--------|
| `mcp/mod.rs` | Add `pub(crate) mod graph_read` |
| `mcp/tools.rs` | Add `context_graph` `#[tool]` handler (dispatch only) |
| `graph_ppr.rs` | Add Advances + Motivates to positive type set (lines ~107–136, ~185–210) |
| `graph_expand.rs` | Add Advances + Motivates to BFS positive type set (lines ~130–148) |
| `migration.rs` | Add v26→v27 block; bump CURRENT_SCHEMA_VERSION to 27 |
| `db.rs` | Add 4 indexes to `create_tables_if_needed`; bump schema_version literal |
| `test_protocol.py` | P-03: assert 14 tools (was 13) |

### Downstream consumers (future)

| Consumer | What they import from vnc-018 |
|----------|-------------------------------|
| vnc-018 subgraph mode (#597) | `EdgeRecord`, `GraphParams` (seed_ids, max_nodes fields) |
| vnc-018 path/inverse/filter modes (#598) | `GraphParams` (from_id, to_id fields), composite indexes |

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `GraphParams` | `struct GraphParams` — full field set locked in ADR-003 | `mcp/tools.rs` (wire struct) |
| `EdgeRecord` | `struct EdgeRecord { source_id: u64, target_id: u64, relation_type: String, direction: String, depth: u8, metadata: Option<serde_json::Value> }` | `mcp/graph_read.rs` |
| `Truncated` | `struct Truncated { forward: bool, backward: bool }` | `mcp/graph_read.rs` |
| `handle_graph` | `pub(crate) async fn handle_graph(store: &Store, typed_graph_state: &Arc<RwLock<TypedGraphState>>, params: GraphParams, ctx: &ToolContext) -> Result<CallToolResult, rmcp::ErrorData>` | `mcp/graph_read.rs` |
| `query_supersession_chain` | `pub async fn query_supersession_chain(pool: &SqlitePool, id: u64, direction: ChainDirection, depth_cap: u8) -> Result<ChainQueryResult, StoreError>` | `unimatrix-store/src/db.rs` |
| `query_direct_neighbors` | `pub async fn query_direct_neighbors(pool: &SqlitePool, id: u64, edge_types: &[&str], direction: NeighborDirection) -> Result<Vec<RawEdgeRow>, StoreError>` | `unimatrix-store/src/db.rs` |
| `follow_to_current` | `async fn follow_to_current(store: &Store, id: u64) -> Option<u64>` (store-layer helper, 50-hop cap) | `mcp/graph_read.rs` (private) |
| `node_index_for` | `pub fn node_index_for(&self, id: u64) -> Option<NodeIndex>` — cross-crate accessor for BFS; see ADR-008 | `unimatrix-engine/src/graph.rs` on `TypedRelationGraph` |
| `idx_entries_supersedes` | `CREATE INDEX IF NOT EXISTS idx_entries_supersedes ON entries(supersedes)` | migration v27 |
| `idx_entries_superseded_by` | `CREATE INDEX IF NOT EXISTS idx_entries_superseded_by ON entries(superseded_by)` | migration v27 |
| `idx_graph_edges_source_type` | `CREATE INDEX IF NOT EXISTS idx_graph_edges_source_type ON graph_edges(source_id, relation_type)` | migration v27 |
| `idx_graph_edges_target_type` | `CREATE INDEX IF NOT EXISTS idx_graph_edges_target_type ON graph_edges(target_id, relation_type)` | migration v27 |

---

## Mode Implementation Details

### chain mode

SQL recursive CTE approach. Two CTE sub-queries for the two traversal directions:

**Forward (descendants — entries that supersede X, toward newer):**
```sql
WITH RECURSIVE chain(id, depth) AS (
    SELECT id, 0 FROM entries WHERE id = ?1
    UNION ALL
    SELECT e.id, c.depth + 1
    FROM entries e JOIN chain c ON e.supersedes = c.id
    WHERE c.depth < 50
)
SELECT e.*, c.depth FROM entries e JOIN chain c ON e.id = c.id
ORDER BY c.depth ASC;
```

**Backward (ancestors — entries X supersedes, toward older):**
```sql
WITH RECURSIVE chain(id, depth) AS (
    SELECT id, 0 FROM entries WHERE id = ?1
    UNION ALL
    SELECT e.id, c.depth + 1
    FROM entries e JOIN chain c ON e.superseded_by = c.id
    WHERE c.depth < 50
)
SELECT e.*, c.depth FROM entries e JOIN chain c ON e.id = c.id
ORDER BY c.depth ASC;
```

For `direction="both"`, run both CTEs, union results, dedup by entry ID (seed
appears in both), order by depth. Each direction independently reports whether its
50-hop cap fired. The `Truncated` struct encodes this per direction (see ADR-002).

If the seed ID does not exist: both CTEs return zero rows → empty result, not an error (AC-04).

**direction semantics**: `"forward"` = toward newer (entries that supersede X);
`"backward"` = toward older (entries X supersedes). These names follow the timeline,
not the field-pointer direction. The tool description must include parenthetical
clarification: "forward: returns descendants (entries that supersede X); backward:
returns ancestors (entries X supersedes)."

### current mode

Single recursive CTE following `superseded_by` until a terminal with no
`superseded_by` is found:

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

The `AND e.status = 'Active'` filter is required. Without it, an entry that was
deprecated via `context_deprecate` (not `context_correct`) can have
`superseded_by IS NULL` but `status = 'Deprecated'`. Such an entry is an orphaned
deprecated terminal — the CTE would return it as the terminal, violating the spec's
requirement that `current` returns "the terminal active entry." The status filter
ensures the result is truly active, not merely an end-of-chain artifact.

Failure modes for `current` mode:

| Condition | CTE result | Handler behavior |
|-----------|-----------|-----------------|
| Non-existent ID | Anchor SELECT returns empty; zero rows from final SELECT | "no active terminal found" error |
| ID exists; chain terminates at orphaned deprecated entry (`superseded_by IS NULL`, `status = 'Deprecated'`) | status filter eliminates the row; zero rows | "no active terminal found" error |
| ID exists; chain loops or exceeds 50 hops | CTE depth cap fires; no terminal reachable; zero rows | AC-07: "chain exceeds 50-hop depth cap" error |
| Input ID is already active | CTE returns it immediately at depth=0; status filter passes | Entry returned unchanged (AC-05) |
| Input ID is deprecated; valid chain to active terminal | CTE follows chain; status filter passes on active terminal | Active terminal returned (AC-06) |

Note: the "non-existent ID" and "orphaned deprecated terminal" cases both produce
zero rows, so they share the same error path in the handler. The error message should
say "no active terminal found" (not "entry not found") because the caller's intent is
to resolve a current version, not to check existence. This is intentionally distinct
from `chain` mode: `chain` on a non-existent ID returns an empty result (AC-04);
`current` on a non-existent ID returns an error — asking for the current version of
something that doesn't exist is semantically an error, not an empty set.

`find_terminal_active` in `graph.rs:523` is NOT used — it operates on the in-memory
graph with a read lock and fails silently on cold-start. The SQL CTE path is
mandatory per ADR-001.

### neighbors mode

**depth=1 (live SQL path):**

Query `GRAPH_EDGES` using composite indexes. Builds an `IN (?, ?, ...)` clause for
relation types or uses the full non-Supersedes type list when `edge_types` is empty.
Direction determines whether `source_id = ?` (outgoing), `target_id = ?` (incoming),
or union of both (both):

```sql
-- Outgoing, specific types:
SELECT source_id, target_id, relation_type FROM graph_edges
WHERE source_id = ?1 AND relation_type IN (?2, ?3, ...)

-- Incoming, specific types:
SELECT source_id, target_id, relation_type FROM graph_edges
WHERE target_id = ?1 AND relation_type IN (?2, ?3, ...)
```

The composite indexes `idx_graph_edges_source_type` and `idx_graph_edges_target_type`
make these single-range scans.

**depth>1 (in-memory BFS path):**

BFS over `TypedRelationGraph` (acquired via `Arc<RwLock<TypedGraphState>>::read()`).
Uses `edges_of_type(node_idx, rel_type, direction)` per requested type per frontier
node. Depth tracked per node in the BFS queue. Result accumulates `EdgeRecord` items
per hop.

**Visited set keying (resolved design decision):** The visited set is a
`HashSet<u64>` keyed by `node_id` only — standard BFS where each node appears at
most once, at its minimum depth. The alternative (keying by `(node_id, depth)`) was
rejected: it allows the same node to appear at multiple depths across different paths,
producing duplicate `EdgeRecord` entries that callers would need to deduplicate. The
`node_id`-only key is correct: a node reached first at depth N is the shortest-path
entry for that node; any path reaching it at depth N+k carries no new information
and is skipped.

If `resolve_supersessions=true`: after resolving each hop's target node ID, call
`follow_to_current(store, target_id)` — if it returns `Some(live_id)` and
`live_id != target_id`, substitute `live_id` in the `EdgeRecord` and continue BFS
from `live_id`.

**Tool description text (exact, required in `#[tool(description = "...")]`):**

> "depth=1 queries the live database and reflects all committed writes immediately.
> depth>1 queries the in-memory graph cache, which may lag recent writes by up to
> one tick interval (typically 30–60 seconds). This asymmetry is intentional:
> depth=1 is the precise lookup case where freshness matters; depth>1 is exploratory
> multi-hop traversal where a tick-window lag is acceptable."

**Supersedes exclusion rules:**
- `edge_types=[]` or absent: traverse all types except `Supersedes` (silent exclusion).
- `edge_types=["Supersedes"]` or any list containing `"Supersedes"`: reject with error:
  `"Supersedes edges are not traversable via neighbors mode — use chain or current modes for supersession navigation."`
- Unknown type string in `edge_types`: reject before any traversal with:
  `"unknown edge type '{x}' — valid types: Advances, Cites, ..."`

---

## PPR and BFS Additions

### graph_ppr.rs

Two insertion points, both adding two `edges_of_type` calls after the existing
`RelatedTo` block:

1. **In `personalized_pagerank` (~line 131–136)** — after the RelatedTo call in the
   neighbor-contribution loop. Add:
   ```rust
   for edge_ref in graph.edges_of_type(node_idx, RelationType::Advances, Direction::Outgoing) {
       neighbor_contribution += outgoing_contribution(&current_scores, &edge_ref, out_degree, graph);
   }
   for edge_ref in graph.edges_of_type(node_idx, RelationType::Motivates, Direction::Outgoing) {
       neighbor_contribution += outgoing_contribution(&current_scores, &edge_ref, out_degree, graph);
   }
   ```

2. **In `positive_out_degree_weight` (~line 203)** — after the RelatedTo call. Add:
   ```rust
   for edge_ref in graph.edges_of_type(node_idx, RelationType::Advances, Direction::Outgoing) {
       total += edge_ref.weight().weight;
   }
   for edge_ref in graph.edges_of_type(node_idx, RelationType::Motivates, Direction::Outgoing) {
       total += edge_ref.weight().weight;
   }
   ```

### graph_expand.rs

One insertion point **after the RelatedTo block (~line 144–148)**:
```rust
for edge_ref in graph.edges_of_type(node_idx, RelationType::Advances, Direction::Outgoing) {
    neighbors.push(graph.inner[edge_ref.target()]);
}
for edge_ref in graph.edges_of_type(node_idx, RelationType::Motivates, Direction::Outgoing) {
    neighbors.push(graph.inner[edge_ref.target()]);
}
```

Update module-level doc comments in both files to remove the "write-only until Phase 2"
note for Advances and Motivates and add the vnc-018 attribution.

---

## Schema Migration Sequencing

Current schema: v26. vnc-018 adds v27.

Migration block structure in `migration.rs` (follows the established `if current_version < N` pattern):

```rust
// v26 → v27: four indexes for context_graph supersession and neighbor queries (vnc-018).
if current_version < 27 {
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_entries_supersedes ON entries(supersedes)")
        .execute(&mut **txn).await.map_err(...)?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_entries_superseded_by ON entries(superseded_by)")
        .execute(&mut **txn).await.map_err(...)?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_graph_edges_source_type ON graph_edges(source_id, relation_type)")
        .execute(&mut **txn).await.map_err(...)?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_graph_edges_target_type ON graph_edges(target_id, relation_type)")
        .execute(&mut **txn).await.map_err(...)?;

    sqlx::query("UPDATE counters SET value = 27 WHERE name = 'schema_version'")
        .execute(&mut **txn).await.map_err(...)?;
}
```

All four indexes are `CREATE INDEX IF NOT EXISTS` — idempotent. They are also added
to `db.rs::create_tables_if_needed()` for fresh databases.

**Schema cascade checklist** (Pattern #4373 — MANDATORY):
- `CURRENT_SCHEMA_VERSION` in `migration.rs`: 26 → 27
- `db.rs` `create_tables_if_needed()`: add 4 index DDL calls; bump schema_version INSERT literal to 27
- `sqlite_parity.rs`: update `test_schema_version_is_N` → 27; add 4 index existence assertions
- `server.rs`: update all `assert_eq!(version, 26)` to 27
- Previous migration test file (`migration_v25_to_v26.rs`): rename
  `test_current_schema_version_is_26` → `test_current_schema_version_is_at_least_26`
  with `assert!(... >= 26)`
- New migration test file: `migration_v26_to_v27.rs` — asserts all 4 index names present

Note: v27 is an index-only migration. No new tables, no new columns, no data back-fill.
The column-count assertions in `sqlite_parity.rs` do NOT change. Only the
schema-version and index-existence assertions change.

---

## Validation Strategy for Forward-Compat Fields

Forward-compat fields in `GraphParams` (`seed_ids`, `from_id`, `to_id`, `max_nodes`)
use a centralized validation function called at the top of `handle_graph`, before
mode dispatch:

```rust
fn validate_no_unsupported_params(params: &GraphParams) -> Result<(), String> {
    match params.mode.as_str() {
        "chain" => {
            if params.seed_ids.is_some() {
                return Err("seed_ids is not supported in chain mode — use subgraph mode".to_string());
            }
            if params.from_id.is_some() {
                return Err("from_id is not supported in chain mode — use path mode".to_string());
            }
            if params.to_id.is_some() {
                return Err("to_id is not supported in chain mode — use path mode".to_string());
            }
            if params.max_nodes.is_some() {
                return Err("max_nodes is not supported in chain mode — use subgraph mode".to_string());
            }
            if params.resolve_supersessions == Some(true) {
                return Err("resolve_supersessions is not supported in chain mode — chain mode traverses supersession links directly; use neighbors mode for cross-type traversal with supersession resolution".to_string());
            }
            Ok(())
        }
        "current" | "neighbors" => {
            if params.seed_ids.is_some() {
                return Err(format!("seed_ids is not supported in {} mode — use subgraph mode", params.mode));
            }
            if params.from_id.is_some() {
                return Err(format!("from_id is not supported in {} mode — use path mode", params.mode));
            }
            if params.to_id.is_some() {
                return Err(format!("to_id is not supported in {} mode — use path mode", params.mode));
            }
            if params.max_nodes.is_some() {
                return Err(format!("max_nodes is not supported in {} mode — use subgraph mode", params.mode));
            }
            Ok(())
        }
        // Future modes will be added here as their handlers are implemented.
        _ => Err(format!("unrecognized mode '{}' — supported modes: chain, current, neighbors", params.mode)),
    }
}
```

This is a **centralized function, not per-mode guards**. Rationale: when #597 adds
`subgraph` mode, the function's `match` arm for `"subgraph"` will permit `seed_ids`
and `max_nodes` without requiring changes to the `"neighbors"` arm. Per-mode guards
scatter the contract and make it easy to forget adding a guard when a new forward-compat
field is added. Centralization means adding a new field touches exactly one function.

`resolve_supersessions=Some(true)` is rejected for `chain` mode specifically because
`chain` already traverses supersession links — the parameter is redundant and would
create ambiguity about whether supersession links are being followed via the CTE or
as a post-hoc node substitution. The check belongs here in the centralized validation
function so the `handle_chain` implementation can assume the parameter is absent.

---

## Capability and Audit

- `context_graph` requires `Capability::Read` (all three modes are read-only).
- Audit log entry: `capability_used = "read"`, `operation = "context_graph"`.
- No write queue involvement. No confidence updates. No usage recording side effects
  beyond the standard `record_usage` fire-and-forget.

---

## Open Questions

None that block delivery. The following design questions were resolved in SCOPE.md
or in this architecture:

- SR-05 (truncated shape): resolved — ADR-002 defines `Truncated { forward: bool, backward: bool }`.
- SR-03 (GraphParams layout): resolved — ADR-003 locks the struct and validation strategy.
- OQ-01 (depth=1 SQL vs depth>1 in-memory): resolved in SCOPE.md — confirmed here in ADR-005.
- OQ-02 (flat list): resolved in SCOPE.md.
- OQ-03 through OQ-06: resolved in SCOPE.md.
- R-07 (`node_index` cross-crate visibility): resolved — ADR-008 mandates a
  `pub fn node_index_for(id: u64) -> Option<NodeIndex>` accessor on
  `TypedRelationGraph` in `unimatrix-engine`. BFS traversal logic stays in
  `unimatrix-server`. The delivery agent implements this accessor as approximately
  5 lines in `unimatrix-engine/src/graph.rs`.
