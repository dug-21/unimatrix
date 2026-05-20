# Pseudocode: unimatrix-store/src/db.rs — SQL Query Functions

## Purpose

Additions to `crates/unimatrix-store/src/db.rs`. Provides two new async query
functions for `context_graph` traversal, plus four new index DDL statements added
to `create_tables_if_needed`, and the schema_version literal bump from 26 to 27.

All new functions use `read_pool()` (C-07). The write pool is never accessed by
`context_graph`.

---

## New Types

These types are defined in `db.rs` (or a new `graph_types.rs` submodule of
`unimatrix-store`, re-exported from `lib.rs`). They are consumed by `graph_read.rs`
in `unimatrix-server`.

```
// Traversal direction for supersession chain queries
pub enum ChainDirection {
    Forward,    // Follow entries.supersedes: find entries that supersede X (toward newer)
    Backward,   // Follow entries.superseded_by: find ancestors (toward older)
    Both,       // Run both directions; dedup by entry ID; separate cap tracking
}

// Traversal direction for neighbor queries
pub enum NeighborDirection {
    Incoming,   // graph_edges WHERE target_id = anchor
    Outgoing,   // graph_edges WHERE source_id = anchor
    Both,       // Union of incoming and outgoing
}

// Result of query_supersession_chain
pub struct ChainQueryResult {
    pub entries: Vec<EntryRecord>,
    pub forward_capped: bool,    // true if forward direction hit 50-hop cap
    pub backward_capped: bool,   // true if backward direction hit 50-hop cap
}

// A single row from graph_edges (raw, before direction annotation)
pub struct RawEdgeRow {
    pub source_id: u64,
    pub target_id: u64,
    pub relation_type: String,
}
```

---

## New/Modified Functions

### `query_supersession_chain` — SQL Recursive CTE (ADR-001)

```
pub async fn query_supersession_chain(
    pool: &SqlitePool,
    id: u64,
    direction: ChainDirection,
    depth_cap: u8,  // always 50 for chain/current modes
) -> Result<ChainQueryResult, StoreError>
```

**Purpose**: Walks the supersession chain for `id` using SQL recursive CTEs on
`entries.supersedes` and/or `entries.superseded_by`. Used by both `chain` mode
(returns full chain) and as the SQL engine for `current` mode.

**Note on current mode**: `current` mode needs a DIFFERENT CTE from chain mode —
it needs `AND e.status = 'Active'` in the final SELECT and returns only the terminal
entry. Rather than overloading this function, add a separate `query_current_terminal`
function below.

**Body**:

```
let (forward_entries, forward_capped, backward_entries, backward_capped) =
    match direction {
        ChainDirection::Forward => {
            let (entries, capped) = run_forward_chain_cte(pool, id, depth_cap).await?
            (entries, capped, vec![], false)
        }
        ChainDirection::Backward => {
            let (entries, capped) = run_backward_chain_cte(pool, id, depth_cap).await?
            (vec![], false, entries, capped)
        }
        ChainDirection::Both => {
            // Run both CTEs independently
            // Each direction independently enforces the depth cap
            let (fwd_entries, fwd_capped) = run_forward_chain_cte(pool, id, depth_cap).await?
            let (bwd_entries, bwd_capped) = run_backward_chain_cte(pool, id, depth_cap).await?
            (fwd_entries, fwd_capped, bwd_entries, bwd_capped)
        }
    }

// Merge forward and backward results, dedup by entry ID, order oldest→newest
// The seed entry (id) appears in BOTH CTEs (at depth=0). After dedup, it appears once.
let mut all_entries = merge_and_dedup(backward_entries, forward_entries)
// Order: backward chain from oldest to seed, then forward chain from seed to newest
// This produces the "oldest ancestor to newest descendant" order (AC-01)

Ok(ChainQueryResult {
    entries: all_entries,
    forward_capped,
    backward_capped,
})
```

**Forward CTE** (follows entries that supersede X — toward newer):

```sql
-- run_forward_chain_cte: find descendants (entries that supersede X)
WITH RECURSIVE chain(id, depth) AS (
    SELECT id, 0 FROM entries WHERE id = ?1
    UNION ALL
    SELECT e.id, c.depth + 1
    FROM entries e
    JOIN chain c ON e.supersedes = c.id
    WHERE c.depth < ?2   -- depth_cap (50)
)
SELECT e.*, c.depth
FROM entries e
JOIN chain c ON e.id = c.id
ORDER BY c.depth ASC;
```

Detect cap fire: if any row has depth = depth_cap - 1 AND has children in the CTE
that were cut off. Simpler approach: if the number of rows with depth = depth_cap - 1
is > 0, check if there are entries with supersedes pointing to any of those row IDs.
Alternatively, run a second query: SELECT COUNT(*) FROM entries WHERE supersedes IN (...)
for the last-depth rows. If count > 0 → capped = true.

**Simpler cap detection approach**: The forward CTE returns rows up to depth_cap.
If any row at depth = depth_cap was returned, the cap fired (because the CTE stopped
there). Track `forward_capped = returned_rows.iter().any(|r| r.depth == depth_cap as i64)`.

Wait — the CTE uses `WHERE c.depth < depth_cap`, so rows with depth == depth_cap are
NOT returned. Cap fired if there are entries at depth_cap - 1 that have successors.
Simplest reliable approach: use depth_cap as the limit; if the CTE returns any entry
at depth == depth_cap (using `WHERE c.depth <= ?2` instead), mark as capped.

**IMPLEMENTATION NOTE**: Use `WHERE c.depth < 50` in the recursive step (as specified
in ARCHITECTURE.md and SPEC FR-04). This means the CTE returns entries at depths 0 to 49
at most. To detect cap fire: check if any returned row's entry has a successor that was
NOT included (i.e., query `SELECT COUNT(*) FROM entries WHERE supersedes IN (entries_at_max_depth)`).
Alternatively, and simpler: run the CTE with `LIMIT 51` (one extra) and if 51 rows come
back from a forward query, the cap fired. Adjust depth_cap logic accordingly.

**Recommended implementation**: Follow the exact CTE structure from ARCHITECTURE.md:

```sql
WITH RECURSIVE chain(id, depth) AS (
    SELECT id, 0 FROM entries WHERE id = ?1
    UNION ALL
    SELECT e.id, c.depth + 1
    FROM entries e
    JOIN chain c ON e.supersedes = c.id
    WHERE c.depth < 50
)
SELECT e.*, c.depth
FROM entries e
JOIN chain c ON e.id = c.id
ORDER BY c.depth ASC;
```

Cap detection: after the query, run a follow-up:
```sql
SELECT COUNT(*) FROM entries e
JOIN chain_at_max c ON e.supersedes = c.id
```
where `chain_at_max` is the set of IDs returned at depth=49. If count > 0, cap fired.

OR: for simplicity, detect by querying: does the seed entry have a chain of length > 0
at depth 49? If any row was returned at depth 49, attempt to check for successors.

**Simplest correct approach** (recommended for implementation): Run the CTE. If
the deepest depth row equals 49, run a quick `SELECT 1 FROM entries WHERE supersedes = ?
LIMIT 1` for each row at depth 49. If any returns a row, cap = true.

**Backward CTE** (follows entries that X supersedes — toward older):

```sql
WITH RECURSIVE chain(id, depth) AS (
    SELECT id, 0 FROM entries WHERE id = ?1
    UNION ALL
    SELECT e.id, c.depth + 1
    FROM entries e
    JOIN chain c ON e.superseded_by = c.id
    WHERE c.depth < 50
)
SELECT e.*, c.depth
FROM entries e
JOIN chain c ON e.id = c.id
ORDER BY c.depth ASC;
```

Same cap detection approach as forward.

**Non-existent ID behavior**: The anchor `SELECT id, 0 FROM entries WHERE id = ?1`
returns zero rows when `id` does not exist. The CTE produces zero rows. The result
is `ChainQueryResult { entries: [], forward_capped: false, backward_capped: false }`.
No error — `chain` mode on a non-existent ID returns empty (AC-04).

**Merge and dedup for `direction=Both`**: Combine backward entries (ordered oldest→seed)
and forward entries (ordered seed→newest). The seed entry appears in both at depth=0.
Dedup by entry ID (keep first occurrence). Since backward is processed first, the seed
entry from backward is kept. Final order: backward chain reversed (oldest first) →
seed → forward chain (newest last). This matches "oldest ancestor to newest descendant."

---

### `query_current_terminal` — Terminal Active Entry Lookup (FR-05, R-20)

```
pub async fn query_current_terminal(
    pool: &SqlitePool,
    id: u64,
) -> Result<Option<EntryRecord>, StoreError>
```

**Purpose**: Follows `superseded_by` from `id` to the terminal entry where
`superseded_by IS NULL AND status = 'Active'`. Used by `handle_current` in
`graph_read.rs`. Returns `None` for non-existent IDs, orphaned deprecated terminals,
and chains exceeding 50 hops — all three map to the same "no active terminal found"
error at the handler layer.

**CTE** (from ARCHITECTURE.md §current mode — critical, `AND e.status = 'Active'` MANDATORY):

```sql
WITH RECURSIVE chain(id, depth) AS (
    SELECT id, 0 FROM entries WHERE id = ?1
    UNION ALL
    SELECT e.superseded_by, c.depth + 1
    FROM entries e
    JOIN chain c ON e.id = c.id
    WHERE e.superseded_by IS NOT NULL AND c.depth < 50
)
SELECT e.*
FROM entries e
JOIN chain c ON e.id = c.id
WHERE e.superseded_by IS NULL
  AND e.status = 'Active'
LIMIT 1;
```

**Critical**: `AND e.status = 'Active'` in the final SELECT is MANDATORY. Without it,
an orphaned deprecated entry (`superseded_by IS NULL, status = 'Deprecated'`) would be
returned as the terminal — silently wrong results (R-20, Critical risk). This single
filter is the only guard against this defect.

**Body**:

```
let row = sqlx::query_as::<_, EntryRecord>(
    "WITH RECURSIVE chain(id, depth) AS (
         SELECT id, 0 FROM entries WHERE id = ?1
         UNION ALL
         SELECT e.superseded_by, c.depth + 1
         FROM entries e
         JOIN chain c ON e.id = c.id
         WHERE e.superseded_by IS NOT NULL AND c.depth < 50
     )
     SELECT e.*
     FROM entries e
     JOIN chain c ON e.id = c.id
     WHERE e.superseded_by IS NULL
       AND e.status = 'Active'
     LIMIT 1"
)
.bind(id as i64)
.fetch_optional(pool)
.await
.map_err(|e| StoreError::Query { source: Box::new(e) })?

Ok(row)
// Some(EntryRecord) → terminal active entry found
// None → non-existent ID, orphaned deprecated, or chain too long
//        (handler maps all None cases to "no active terminal found" error)
```

---

### `query_direct_neighbors` — Live SQL Neighbor Query (ADR-005)

```
pub async fn query_direct_neighbors(
    pool: &SqlitePool,
    id: u64,
    edge_types: &[&str],  // empty = all except Supersedes (caller validates this)
    direction: NeighborDirection,
) -> Result<Vec<RawEdgeRow>, StoreError>
```

**Purpose**: Queries `GRAPH_EDGES` for entries connected to `id` via typed edges
at depth=1. Uses composite indexes `idx_graph_edges_source_type` (outgoing) and
`idx_graph_edges_target_type` (incoming). Always live database — no tick-window
staleness.

**Precondition**: `edge_types` does NOT contain "Supersedes" — caller (`handle_neighbors`)
validates this before calling. `edge_types` is empty if and only if all non-Supersedes
types should be returned (but at depth=1, this means no filter on relation_type needed,
or filter IN all-15-types).

**Body**:

```
// Determine query shape based on direction and edge_types
// Build the SQL query dynamically based on parameters.
// Use parameterized binding for all values (no string interpolation in WHERE clauses).

match direction {
    NeighborDirection::Outgoing => {
        run_outgoing_query(pool, id, edge_types).await
    }
    NeighborDirection::Incoming => {
        run_incoming_query(pool, id, edge_types).await
    }
    NeighborDirection::Both => {
        let mut outgoing = run_outgoing_query(pool, id, edge_types).await?
        let incoming = run_incoming_query(pool, id, edge_types).await?
        outgoing.extend(incoming)
        Ok(outgoing)
    }
}
```

**Outgoing query** (source_id = anchor, uses `idx_graph_edges_source_type`):

```sql
-- When edge_types is empty (all non-Supersedes types):
SELECT source_id, target_id, relation_type
FROM graph_edges
WHERE source_id = ?1
  AND relation_type != 'Supersedes'

-- When edge_types is non-empty (specific types; Supersedes already excluded by caller):
SELECT source_id, target_id, relation_type
FROM graph_edges
WHERE source_id = ?1
  AND relation_type IN (?2, ?3, ...)
```

**Incoming query** (target_id = anchor, uses `idx_graph_edges_target_type`):

```sql
-- When edge_types is empty:
SELECT source_id, target_id, relation_type
FROM graph_edges
WHERE target_id = ?1
  AND relation_type != 'Supersedes'

-- When edge_types is non-empty:
SELECT source_id, target_id, relation_type
FROM graph_edges
WHERE target_id = ?1
  AND relation_type IN (?2, ?3, ...)
```

**Dynamic IN clause**: sqlx does not natively support binding a `Vec<&str>` into an
`IN (...)` clause. The implementation must either:

- Use a compile-time maximum and fill remaining slots with dummy values (not recommended)
- Build the query string dynamically with `{repeat("?,", n)}` and bind each value separately
- Use `sqlx::query()` with a hand-constructed SQL string where the placeholders are
  generated based on `edge_types.len()`

Recommended pattern (mirrors existing usage in the codebase for similar IN clauses):

```
let placeholders = (0..edge_types.len())
    .map(|i| format!("?{}", i + 2))  // ?2, ?3, ... (after ?1 for id)
    .collect::<Vec<_>>()
    .join(", ")
let sql = format!(
    "SELECT source_id, target_id, relation_type FROM graph_edges WHERE source_id = ?1 AND relation_type IN ({placeholders})"
)
let mut query = sqlx::query_as::<_, RawEdgeRow>(&sql).bind(id as i64)
for type_str in edge_types {
    query = query.bind(*type_str)
}
query.fetch_all(pool).await.map_err(|e| StoreError::Query { source: Box::new(e) })
```

**Non-existent anchor ID**: `GRAPH_EDGES` has no row with `source_id = id` or
`target_id = id` → returns empty `Vec<RawEdgeRow>`. No error. Consistent with
`chain` mode and the OQ-01 resolution (empty for unknown anchor in neighbors mode).

**Supersedes exclusion**: when `edge_types` is empty, the SQL includes
`AND relation_type != 'Supersedes'`. When specific types are requested, Supersedes
is excluded by the caller (rejected in `handle_neighbors` before this function is
called). The silent exclusion at the SQL level is a fallback safety net, not the
primary mechanism — the primary rejection is in `handle_neighbors`.

---

## Modified: `create_tables_if_needed` — 4 Index DDL (ADR-007)

Add these four `CREATE INDEX IF NOT EXISTS` statements after the existing index
DDL statements in `create_tables_if_needed` (for fresh database consistency):

```sql
CREATE INDEX IF NOT EXISTS idx_entries_supersedes ON entries(supersedes);
CREATE INDEX IF NOT EXISTS idx_entries_superseded_by ON entries(superseded_by);
CREATE INDEX IF NOT EXISTS idx_graph_edges_source_type ON graph_edges(source_id, relation_type);
CREATE INDEX IF NOT EXISTS idx_graph_edges_target_type ON graph_edges(target_id, relation_type);
```

These are idempotent — safe to include for both fresh databases and migrated ones.

## Modified: `create_tables_if_needed` — schema_version Literal Bump

In the `INSERT INTO counters (name, value) VALUES ('schema_version', 26)` statement
(or equivalent), change the literal from `26` to `27`.

This ensures that freshly created databases start at schema version 27 (with all
four indexes already present from `create_tables_if_needed`), consistent with
migration landing databases at the same state.

---

## Error Handling

| Error | Type | Behavior |
|-------|------|---------|
| sqlx query error | `StoreError::Query` | Propagated to caller |
| No rows returned | `Ok(None)` / `Ok([])` | Caller interprets as empty/not found |

All functions return `Result<T, StoreError>`. The callers in `graph_read.rs` handle
`StoreError` by mapping to `rmcp::ErrorData` or returning empty results as appropriate.

---

## Key Test Scenarios

### Store-layer unit tests (isolated from MCP layer)

1. `query_supersession_chain(id, Forward, 50)` with empty database → `entries: []`, not an error
2. `query_supersession_chain(id, Both, 50)` with 5-entry chain, seed at middle → all 5 entries returned
3. `query_supersession_chain(id, Both, 50)` with 60-entry forward chain → forward_capped=true, backward_capped=false
4. `query_current_terminal(id)` with non-existent id → None
5. `query_current_terminal(id)` with active entry (no superseded_by) → Some(that entry)
6. `query_current_terminal(id)` with deprecated entry having active successor → Some(successor)
7. `query_current_terminal(id)` with orphaned deprecated entry (superseded_by IS NULL, status=Deprecated) → None (R-20 critical)
8. `query_direct_neighbors(id, [], Outgoing)` with no edges → empty Vec
9. `query_direct_neighbors(id, ["Supports"], Outgoing)` with two Supports edges → two RawEdgeRow
10. `query_direct_neighbors(id, [], Both)` with both incoming and outgoing edges → correct union
11. Schema version after `create_tables_if_needed` on fresh DB → 27
12. All four indexes present after `create_tables_if_needed` (via sqlite_master query)

### Migration tests

13. AC-19: after `migrate_if_needed`, all four index names present in sqlite_master
14. Schema version after migration from 26 → 27 (migration_v26_to_v27.rs)
