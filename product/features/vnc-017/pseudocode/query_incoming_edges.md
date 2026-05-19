# Component: query_incoming_edges

## Purpose

Provides the store-layer read function that the redirect loop needs to discover
which `graph_edges` rows point at a given entry. This is the only new function
in `unimatrix-store`; the redirect writes are handled by the existing
`redirect_graph_edge` in `unimatrix-server`.

## File Location

`crates/unimatrix-store/src/read.rs`

Appended to the `impl Store` block. The file is 3,465 lines; this addition is
~30 lines (struct definition + function). No module split required (ADR-002,
NFR-06).

## New Struct: IncomingEdgeRow

Place this struct definition immediately before (or after) the `query_incoming_edges`
function. Keep it `pub` so that `unimatrix-server` can receive it across the
crate boundary.

```
pub struct IncomingEdgeRow {
    pub source_id:     u64,
    pub relation_type: String,
    pub created_at:    u64,
}
```

`target_id` is the query parameter; it is implicit and not included in the row struct.

## Function: query_incoming_edges

### Signature

```
pub async fn query_incoming_edges(
    &self,
    target_id: u64,
) -> Result<Vec<IncomingEdgeRow>>
```

### Doc Comment

```
/// Return all `graph_edges` rows pointing at `target_id`, excluding `Supersedes`
/// relation types.
///
/// Used by `context_correct`'s auto-redirect loop (vnc-017) to discover stale
/// incoming edges before redirecting them to the new active entry.
///
/// # Supersedes exclusion
/// `Supersedes` rows are excluded at the SQL level (ADR-002 vnc-017). They are
/// derived from `entries.supersedes` and are rebuilt by the graph tick automatically.
/// Redirecting them would assert incorrect semantic claims (e.g. C supersedes B
/// when only C superseded A). Future callers that require Supersedes rows must
/// issue a separate query.
///
/// # Pool
/// Uses `read_pool()`. Both `read_pool()` and `write_pool_server()` currently alias
/// the same underlying pool (`db.rs:294`); use canonical accessor name per C-07.
///
/// # Index
/// `idx_graph_edges_target_id` covers `WHERE target_id = ?` efficiently (migration v12→v13).
```

### Pseudocode Body

```
FUNCTION query_incoming_edges(self, target_id: u64) -> Result<Vec<IncomingEdgeRow>>:

    SQL =
        "SELECT source_id, relation_type, created_at
         FROM graph_edges
         WHERE target_id = ?1
           AND relation_type != 'Supersedes'
           -- Supersedes rows are derived from entries.supersedes; redirecting them would
           -- assert incorrect semantic claims (e.g. C supersedes B when only C superseded A).
           -- They are rebuilt by the graph tick automatically on the next cycle. ADR-002 vnc-017."

    rows = sqlx::query(SQL)
        .bind(target_id as i64)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?

    rows.into_iter()
        .map(|row| {
            Ok(IncomingEdgeRow {
                source_id:     row.try_get::<i64, _>("source_id")
                                   .map_err(|e| StoreError::Database(e.into()))? as u64,
                relation_type: row.try_get("relation_type")
                                   .map_err(|e| StoreError::Database(e.into()))?,
                created_at:    row.try_get::<i64, _>("created_at")
                                   .map_err(|e| StoreError::Database(e.into()))? as u64,
            })
        })
        .collect::<Result<Vec<_>>>()
```

### Notes on Binding

- `target_id` is `u64` but SQLite columns are `i64` (BIGINT). Cast to `i64` for
  `.bind()` and back to `u64` when reading, matching the pattern used throughout
  `read.rs` (see `query_graph_edges`, `query_stale_prerequisite_edges_for_cycle`).
- `created_at` is similarly stored as `i64` (seconds since epoch) and cast back to
  `u64` on read.

## Data Flow

- Input: `target_id: u64` — the original (now-deprecated) entry ID from the
  `context_correct` call. Provided by the redirect loop after `correct_result` commits.
- Output: `Vec<IncomingEdgeRow>` — zero or more rows. Empty means no stale edges.
- Side effects: none (read-only).
- Error: `Err(StoreError)` on SQL/pool failure. The redirect loop in `tools.rs`
  handles this by logging a warn and skipping the loop entirely (correction still succeeds).

## Error Handling

| Condition | Behavior |
|-----------|----------|
| SQL success, 0 rows | Returns `Ok(vec![])` |
| SQL success, N rows | Returns `Ok(Vec<IncomingEdgeRow>)` with N items |
| Pool error | Returns `Err(StoreError::Database(_))` |
| Row deserialization error | Returns `Err(StoreError::Database(_))` from the `.map()` |

The caller (`tools.rs` redirect loop) treats `Err` from `query_incoming_edges` as a
warn-and-skip: it logs the error and does not execute the loop body, preserving the
correction success. The pseudocode for that handling is in `redirect_loop.md`.

## Key Test Scenarios

**AC-05 — Basic correctness (unit test, `unimatrix-store`)**
- Seed 3 `graph_edges` rows with `target_id = T`, relation types Prerequisite/Informs/Supports
- Seed 1 row with `target_id = T`, relation type Supersedes
- Seed 2 rows with a different `target_id`
- Call `query_incoming_edges(T)`
- Assert: returns 3 rows (Supersedes excluded, other target excluded)
- Assert: returned fields match seeded values exactly

**R-02 / R-07 — SQL-level Supersedes exclusion (unit test)**
- Seed only a Supersedes row with `target_id = T`
- Call `query_incoming_edges(T)`
- Assert: returns `Ok(vec![])` — structural proof that exclusion is at SQL level,
  not loop level

**R-03 — High-cardinality filter correctness (unit test)**
- Seed 1,000 rows with `target_id = OTHER`
- Seed 3 rows with `target_id = T`
- Call `query_incoming_edges(T)`
- Assert: returns exactly 3 rows (not 1,003)

**Pool accessor check (code review gate)**
- Verify `read_pool()` is used, not `write_pool_server()`
- Verify the C-07 comment is present at the call site
