# Test Plan: store-v22

## Component Summary

`store-v22` covers:
- Schema v21→v22 migration (new `goal_clusters` table + index)
- `db.rs` `create_tables_if_needed` update (byte-identical DDL to migration)
- Three new store methods: `get_cycle_start_goal_embedding`, `insert_goal_cluster`,
  `query_goal_clusters_by_embedding`
- `sqlite_parity.rs` tests for the new table
- `server.rs` version assertion update

Risks addressed: R-05 (migration cascade), R-06 (partial persistence), R-07
(recency cap), R-14 (async fn compliance), R-15 (DDL mismatch).

---

## Unit Tests — `crates/unimatrix-store/src/db.rs`

### Schema Version (AC-17, R-05)

```rust
#[tokio::test]
async fn test_schema_version_initialized_to_22_on_fresh_db()
```
- Arrange: open fresh store via `open_test_store()`.
- Assert: `SELECT value FROM counters WHERE name = 'schema_version'` returns 22.
- Note: renames the existing `test_schema_version_initialized_to_21_on_fresh_db`
  (migration cascade site 4).

### `goal_clusters` Table Exists (R-05, R-15)

```rust
#[tokio::test]
async fn test_create_tables_goal_clusters_exists()
```
- Arrange: open fresh store.
- Assert: `SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='goal_clusters'` returns 1.
- Located in `sqlite_parity.rs` or `db.rs` tests (cascade site 5).

### `goal_clusters` Column Count = 7 (R-15, AC-12)

```rust
#[tokio::test]
async fn test_create_tables_goal_clusters_schema()
```
- Arrange: open fresh store.
- Assert: `SELECT COUNT(*) FROM pragma_table_info('goal_clusters')` returns 7.
- This verifies that `create_tables_if_needed` produces the same 7-column schema
  as the migration block (cascade site 5, DDL parity guarantee).

### `idx_goal_clusters_created_at` Index Exists

```rust
#[tokio::test]
async fn test_create_tables_goal_clusters_index_exists()
```
- Arrange: open fresh store.
- Assert: `SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_goal_clusters_created_at'` returns 1.

---

## Unit Tests — `crates/unimatrix-store/src/goal_clusters.rs`

### `insert_goal_cluster` — new row returns true (AC-05)

```rust
#[tokio::test]
async fn test_insert_goal_cluster_new_row_returns_true()
```
- Arrange: open test store; prepare dummy 384-dim embedding.
- Act: call `store.insert_goal_cluster("fc-001", embedding, None, "[]", None, 1000)`.
- Assert: returns `Ok(true)`.
- Assert: `SELECT COUNT(*) FROM goal_clusters` returns 1.

### `insert_goal_cluster` — UNIQUE conflict returns false (AC-02, R-06)

```rust
#[tokio::test]
async fn test_insert_goal_cluster_duplicate_returns_false()
```
- Arrange: insert row for "fc-001".
- Act: insert again for "fc-001" with different entry_ids_json.
- Assert: returns `Ok(false)`.
- Assert: `SELECT COUNT(*) FROM goal_clusters` still returns 1 (first write wins).
- Assert: `entry_ids_json` in DB is the original value (not overwritten).

### `insert_goal_cluster` — special character feature_cycle (E-06)

```rust
#[tokio::test]
async fn test_insert_goal_cluster_special_chars_in_feature_cycle()
```
- Arrange: feature_cycle = "crt-046/sub-test".
- Act: insert; then query by feature_cycle.
- Assert: row round-trips without SQL escaping issues.

### `query_goal_clusters_by_embedding` — returns matching rows above threshold

```rust
#[tokio::test]
async fn test_query_goal_clusters_by_embedding_returns_above_threshold()
```
- Arrange: encode a unit-vector embedding V; insert 3 rows:
  - Row A: embedding = V (cosine 1.0 to query)
  - Row B: embedding = orthogonal(V) (cosine 0.0)
  - Row C: embedding = near(V, cos=0.85) (above 0.80 threshold)
- Act: call `query_goal_clusters_by_embedding(V, 0.80, 100)`.
- Assert: results contain rows A and C; row B absent.
- Assert: results sorted descending by similarity (A first, C second).

### `query_goal_clusters_by_embedding` — recency cap enforced (AC-11, R-07)

```rust
#[tokio::test]
async fn test_query_goal_clusters_recency_cap_100()
```
- Arrange: insert 150 rows with ascending `created_at` values; row 1 (oldest)
  has embedding = V (cosine 1.0 to query); rows 2..150 have cosine 0.0.
- Act: call `query_goal_clusters_by_embedding(V, 0.0, 100)`.
  (threshold=0.0 ensures all rows above threshold — recency cap is the only filter)
- Assert: result count ≤ 100.
- Assert: row 1 (oldest by created_at) is NOT in results (outside recency window).
- Assert: rows 51..150 (most recent 100) are scanned; if any have cosine > 0.0
  they appear.

### `query_goal_clusters_by_embedding` — threshold boundary at exactly 0.80 (E-07)

```rust
#[tokio::test]
async fn test_query_goal_clusters_threshold_boundary_inclusive()
```
- Arrange: insert row with embedding at exactly 0.80 cosine to query vector.
- Act: call with threshold=0.80.
- Assert: that row IS included in results (≥ not >).

### `query_goal_clusters_by_embedding` — empty table returns empty vec (AC-09)

```rust
#[tokio::test]
async fn test_query_goal_clusters_empty_table_returns_empty()
```
- Assert: `query_goal_clusters_by_embedding(V, 0.80, 100)` returns `Ok(Vec::new())`.

### `query_goal_clusters_by_embedding` — empty entry_ids_json row (E-05)

```rust
#[tokio::test]
async fn test_query_goal_clusters_empty_entry_ids_row()
```
- Arrange: insert row with `entry_ids_json = "[]"` and cosine above threshold.
- Act: call query.
- Assert: row returned in results; no panic; `entry_ids_json` field is `"[]"`.

### `get_cycle_start_goal_embedding` — returns embedding from cycle_start event (AC-05)

```rust
#[tokio::test]
async fn test_get_cycle_start_goal_embedding_returns_embedding()
```
- Arrange: seed a `cycle_events` row with `event_type='cycle_start'`,
  `goal_embedding=encode_goal_embedding(V)`.
- Act: call `store.get_cycle_start_goal_embedding("fc-001")`.
- Assert: returns `Ok(Some(V))` (decoded embedding matches input).

### `get_cycle_start_goal_embedding` — returns None when no cycle_start event (AC-06, E-08)

```rust
#[tokio::test]
async fn test_get_cycle_start_goal_embedding_no_event_returns_none()
```
- Assert: on empty cycle_events table, returns `Ok(None)`.

### `get_cycle_start_goal_embedding` — returns None when NULL BLOB (AC-08)

```rust
#[tokio::test]
async fn test_get_cycle_start_goal_embedding_null_blob_returns_none()
```
- Arrange: seed cycle_start row with `goal_embedding = NULL`.
- Assert: returns `Ok(None)`.

### `get_cycle_start_goal_embedding` — returns None on malformed BLOB (E-03)

```rust
#[tokio::test]
async fn test_get_cycle_start_goal_embedding_malformed_blob_returns_none()
```
- Arrange: seed cycle_start row with arbitrary non-embedding bytes in
  `goal_embedding`.
- Assert: does NOT panic; returns `Ok(None)` or `Err` (either is acceptable as
  long as no panic propagates).

---

## Migration Tests — `crates/unimatrix-store/src/migration.rs`

### v21→v22 migration creates goal_clusters (AC-12, R-05)

```rust
#[tokio::test]
async fn test_v21_to_v22_migration_creates_goal_clusters()
```
- Arrange: create a v21 fixture DB programmatically:
  1. Open a fresh `SqliteConnection` to a temp file.
  2. Run the full v21 DDL manually (all tables from `create_tables_if_needed` up
     to but not including the v22 block).
  3. Set `counters.schema_version = 21`.
  4. Close the connection.
- Act: call `SqlxStore::open()` on the fixture path (triggers migration).
- Assert: `SELECT value FROM counters WHERE name = 'schema_version'` returns 22.
- Assert: `SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='goal_clusters'`
  returns 1.
- Assert: `SELECT COUNT(*) FROM pragma_table_info('goal_clusters')` returns 7.
- Assert: `SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_goal_clusters_created_at'`
  returns 1.

### Prior migration test renamed (R-05, cascade site 8)

```rust
// Existing test renamed:
// test_current_schema_version_is_21 → test_current_schema_version_is_at_least_21
#[tokio::test]
async fn test_current_schema_version_is_at_least_21()
```
- Assert: `read_schema_version(&store) >= 21`.
- Note: uses `>=` predicate so the test remains valid after future version bumps.

---

## AC-17 — Shell Verification (Gate 3a Checklist)

```bash
grep -r 'schema_version.*== 21' crates/
```
Must return zero matches. This is a Gate 3a blocking check, not a Rust test.
The tester must run this command explicitly and include the result in the
RISK-COVERAGE-REPORT.md.

---

## R-14 — Async fn Compliance (Code Review)

All three new store methods must be `async fn` called with `.await`:
- `get_cycle_start_goal_embedding` — `async fn` on `SqlxStore`
- `insert_goal_cluster` — `async fn` on `SqlxStore`
- `query_goal_clusters_by_embedding` — `async fn` on `SqlxStore`

Verify: search for `spawn_blocking` in `goal_clusters.rs` and `db.rs` around
these methods. Must find zero occurrences. Any `spawn_blocking` wrapping these
calls is a bug (entries #2266, #2249).

---

## Assertions Summary

| Test | Function under test | Key assertion |
|------|---------------------|---------------|
| `test_schema_version_initialized_to_22_on_fresh_db` | `create_tables_if_needed` | schema_version == 22 |
| `test_create_tables_goal_clusters_exists` | `create_tables_if_needed` | table exists |
| `test_create_tables_goal_clusters_schema` | `create_tables_if_needed` | column count == 7 |
| `test_insert_goal_cluster_new_row_returns_true` | `insert_goal_cluster` | returns Ok(true) |
| `test_insert_goal_cluster_duplicate_returns_false` | `insert_goal_cluster` | returns Ok(false) on conflict |
| `test_query_goal_clusters_recency_cap_100` | `query_goal_clusters_by_embedding` | oldest row excluded |
| `test_query_goal_clusters_threshold_boundary_inclusive` | `query_goal_clusters_by_embedding` | row at 0.80 included |
| `test_v21_to_v22_migration_creates_goal_clusters` | `migrate_if_needed` | version 22, 7 columns, index |
| `test_current_schema_version_is_at_least_21` | `migrate_if_needed` | version >= 21 |
