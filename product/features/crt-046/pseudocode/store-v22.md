# crt-046 — Component: store-v22

## Purpose

Bump schema from v21 to v22. Add the `goal_clusters` table. Add three new async methods
on `SqlxStore`. Add `GoalClusterRow` struct. Add three `InferenceConfig` fields.
Update all nine schema cascade touchpoints.

Wave: 1 (no dependencies on other crt-046 components).

---

## Schema v22 Cascade Checklist (9 touchpoints — all must ship together)

Before Gate 3a, `grep -r 'schema_version.*== 21' crates/` must return zero matches (AC-17).

| # | File | Change |
|---|------|--------|
| 1 | `crates/unimatrix-store/src/migration.rs` | Add `if current_version < 22` block |
| 2 | `crates/unimatrix-store/src/db.rs` | Add `goal_clusters` DDL to `create_tables_if_needed()` |
| 3 | `crates/unimatrix-store/src/db.rs` | Bump hardcoded schema_version INSERT integer from 21 to 22 |
| 4 | `crates/unimatrix-store/src/db.rs` | Rename `test_schema_version_initialized_to_21_on_fresh_db` → `_22` |
| 5 | `crates/unimatrix-store/src/tests/sqlite_parity.rs` | Add `test_create_tables_goal_clusters_exists` and `test_create_tables_goal_clusters_schema` (7 columns) |
| 6 | `crates/unimatrix-store/src/tests/sqlite_parity.rs` | Update `test_schema_version_is_N` to 22 |
| 7 | `crates/unimatrix-server/src/server.rs` | Update both `assert_eq!(version, 21)` sites to 22 |
| 8 | Migration tests | Rename `test_current_schema_version_is_21` → `test_current_schema_version_is_at_least_21` with `>= 21` predicate |
| 9 | Migration tests | Grep column-count assertions referencing old total; update if affected |

---

## goal_clusters DDL (byte-identical in migration.rs AND db.rs)

```sql
CREATE TABLE IF NOT EXISTS goal_clusters (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    feature_cycle   TEXT    NOT NULL UNIQUE,
    goal_embedding  BLOB    NOT NULL,
    phase           TEXT,
    entry_ids_json  TEXT    NOT NULL,
    outcome         TEXT,
    created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_goal_clusters_created_at
    ON goal_clusters(created_at DESC);
```

7 columns: id, feature_cycle, goal_embedding, phase, entry_ids_json, outcome, created_at.
The `UNIQUE` constraint on `feature_cycle` enforces INSERT OR IGNORE semantics.
The `created_at DESC` index supports the recency-capped `ORDER BY created_at DESC LIMIT 100`
query (ADR-003).

---

## New File: `crates/unimatrix-store/src/goal_clusters.rs`

### Struct: GoalClusterRow

```
struct GoalClusterRow {
    id:             i64,
    feature_cycle:  String,
    goal_embedding: Vec<f32>,           // decoded at query time; never stored as Vec<f32>
    phase:          Option<String>,
    entry_ids_json: String,             // raw JSON array of u64 IDs as stored in DB
    outcome:        Option<String>,
    created_at:     i64,                // Unix millis
    similarity:     f32,                // computed at query time; NOT a DB column
}
```

`similarity` is set during `query_goal_clusters_by_embedding`; it is 0.0 for rows returned
by other queries. `entry_ids_json` is unparsed TEXT; callers parse it with `serde_json`.

### Function: insert_goal_cluster

```
async fn insert_goal_cluster(
    &self,
    feature_cycle: &str,
    goal_embedding: Vec<f32>,   // will be encoded via encode_goal_embedding
    phase: Option<&str>,
    entry_ids_json: &str,       // caller has already serialized the Vec<u64> to JSON
    outcome: Option<&str>,
    created_at: i64,            // Unix millis; caller provides
) -> Result<bool>
```

Algorithm:
1. Encode `goal_embedding` to BLOB via `encode_goal_embedding(goal_embedding)?`.
   On encode error: return `Err(StoreError::InvalidInput { ... })`.
2. Execute on `write_pool_server()` (NOT analytics drain — structural write, ADR-002):
   ```sql
   INSERT OR IGNORE INTO goal_clusters
       (feature_cycle, goal_embedding, phase, entry_ids_json, outcome, created_at)
   VALUES (?1, ?2, ?3, ?4, ?5, ?6)
   ```
3. Check `query_result.rows_affected()`:
   - `== 1` → return `Ok(true)`   (new row inserted)
   - `== 0` → return `Ok(false)`  (UNIQUE conflict on feature_cycle; INSERT OR IGNORE silent no-op)
4. On SQL error: return `Err(StoreError::from(e))`.

Note: `Ok(false)` is NOT an error. Caller (populate_goal_cluster) logs at debug level.
No INSERT OR REPLACE exists anywhere in this feature.

Error handling: propagate `encode_goal_embedding` errors and SQL errors as `StoreError`.

### Function: query_goal_clusters_by_embedding

```
async fn query_goal_clusters_by_embedding(
    &self,
    embedding: &[f32],      // current session goal embedding
    threshold: f32,         // from InferenceConfig.goal_cluster_similarity_threshold (default 0.80)
    recency_limit: u64,     // RECENCY_CAP constant = 100
) -> Result<Vec<GoalClusterRow>>
```

Algorithm:
1. Fetch at most `recency_limit` rows from DB using `read_pool()`:
   ```sql
   SELECT id, feature_cycle, goal_embedding, phase, entry_ids_json, outcome, created_at
   FROM goal_clusters
   ORDER BY created_at DESC
   LIMIT ?1
   ```
   Bind `recency_limit as i64`.
2. For each row, decode the `goal_embedding` BLOB via `decode_goal_embedding(&blob)`.
   On decode error for a single row: log `warn!` and skip that row (continue loop).
   Do NOT abort the entire query.
3. Compute cosine similarity in-process (not in spawn_blocking — O(100×384) is trivial):
   ```
   cosine_similarity(embedding, &decoded_embedding) -> f32
   ```
   Use the helper function defined in this module (see cosine_similarity below).
4. Filter: keep row only if `similarity >= threshold`.
5. Build `GoalClusterRow` for kept rows, setting `similarity` field from step 3.
6. Sort the kept rows by `similarity` descending.
7. Return `Ok(Vec<GoalClusterRow>)`.

On SQL error: return `Err`.
Returns empty Vec when table is empty or no row meets the threshold — not an error.

### Helper: cosine_similarity (module-private)

```
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32
```

Algorithm:
1. If `a.len() != b.len()` or either is empty: return 0.0.
2. Compute dot product: `dot = sum(a[i] * b[i] for i in 0..len)`.
3. Compute magnitudes: `mag_a = sqrt(sum(a[i]^2))`, `mag_b = sqrt(sum(b[i]^2))`.
4. If `mag_a == 0.0 || mag_b == 0.0`: return 0.0.
5. Return `dot / (mag_a * mag_b)`.
6. Clamp result to [0.0, 1.0] to handle floating-point rounding artifacts
   (use `f32::min(result, 1.0).max(0.0)`).

Note on E-07: threshold comparison is `>= threshold` (inclusive), so similarity exactly
equal to the threshold IS included.

---

## Modified: `crates/unimatrix-store/src/db.rs`

### New Method: get_cycle_start_goal_embedding

```
async fn get_cycle_start_goal_embedding(
    &self,
    cycle_id: &str,
) -> Result<Option<Vec<f32>>>
```

Algorithm:
1. Query using `read_pool()`:
   ```sql
   SELECT goal_embedding
   FROM cycle_events
   WHERE cycle_id = ?1
     AND event_type = 'cycle_start'
     AND goal_embedding IS NOT NULL
   ORDER BY timestamp ASC
   LIMIT 1
   ```
2. If no row: return `Ok(None)`.
3. If row found but `goal_embedding` column is NULL (sqlx may return `None`): return `Ok(None)`.
4. Decode via `decode_goal_embedding(&blob)`.
   On decode error: log `warn!("get_cycle_start_goal_embedding: decode failed for {}: {}", cycle_id, e)`;
   return `Ok(None)` (treat decode failure as absence — cold-start path).
5. Return `Ok(Some(decoded_vec))`.

Pattern mirrors existing `get_cycle_start_goal` method in db.rs (uses same index
`idx_cycle_events_cycle_id` implicitly via `cycle_id = ?1`).

### Changes to create_tables_if_needed()

Add the `goal_clusters` DDL block (byte-identical to migration.rs) to the
`create_tables_if_needed()` function, after the existing table definitions.
Do not alter any existing DDL.

### Schema Version Integer

Update the hardcoded schema_version INSERT from 21 to 22:
```sql
-- Find: INSERT INTO counters (name, value) VALUES ('schema_version', 21)
-- Replace with:
INSERT INTO counters (name, value) VALUES ('schema_version', 22)
```

### Test Rename

Rename `test_schema_version_initialized_to_21_on_fresh_db`
to `test_schema_version_initialized_to_22_on_fresh_db`.
Update the `assert_eq!(version, 21)` assertion inside to `assert_eq!(version, 22)`.

---

## Modified: `crates/unimatrix-store/src/migration.rs`

### CURRENT_SCHEMA_VERSION

Change:
```rust
pub const CURRENT_SCHEMA_VERSION: u64 = 21;
```
to:
```rust
pub const CURRENT_SCHEMA_VERSION: u64 = 22;
```

### New Migration Block

In `run_main_migrations`, after the existing `if current_version < 21` block, add:

```
if current_version < 22 {
    execute SQL on &mut txn:
        CREATE TABLE IF NOT EXISTS goal_clusters (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            feature_cycle   TEXT    NOT NULL UNIQUE,
            goal_embedding  BLOB    NOT NULL,
            phase           TEXT,
            entry_ids_json  TEXT    NOT NULL,
            outcome         TEXT,
            created_at      INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_goal_clusters_created_at
            ON goal_clusters(created_at DESC);
        UPDATE counters SET value = 22 WHERE name = 'schema_version';

    on error: return Err(StoreError::Migration { source: Box::new(e) })
}
```

The DDL strings must be byte-identical to the `goal_clusters` DDL in `db.rs`.

---

## Modified: `crates/unimatrix-store/src/lib.rs`

Add module declaration after existing module declarations:
```rust
pub mod goal_clusters;
```

Re-export `GoalClusterRow` so server crate can use it:
```rust
pub use goal_clusters::GoalClusterRow;
```

---

## Modified: `crates/unimatrix-server/src/infra/config.rs` — InferenceConfig

Add three fields to `InferenceConfig` struct with `#[serde(default)]`:

```
/// Cosine similarity threshold for goal_clusters matching at briefing time.
/// Defaults to 0.80. Passed to query_goal_clusters_by_embedding at call time — not a constant.
#[serde(default = "default_goal_cluster_similarity_threshold")]
pub goal_cluster_similarity_threshold: f32,

/// Weight applied to EntryRecord.confidence (Wilson-score) in the cluster_score formula.
/// cluster_score = (EntryRecord.confidence × w_goal_cluster_conf) + (goal_cosine × w_goal_boost).
/// NOTE: this is EntryRecord.confidence (Wilson-score), NOT IndexEntry.confidence (raw cosine).
#[serde(default = "default_w_goal_cluster_conf")]
pub w_goal_cluster_conf: f32,

/// Weight applied to goal_cosine (GoalClusterRow.similarity) in the cluster_score formula.
#[serde(default = "default_w_goal_boost")]
pub w_goal_boost: f32,
```

Add default functions:
```
fn default_goal_cluster_similarity_threshold() -> f32 { 0.80 }
fn default_w_goal_cluster_conf() -> f32 { 0.35 }
fn default_w_goal_boost() -> f32 { 0.25 }
```

---

## Modified: `crates/unimatrix-server/src/server.rs`

Two `assert_eq!(version, 21)` sites must both become `assert_eq!(version, 22)`.
These appear inside the test at the bottom of server.rs (lines ~2137 and ~2162 in the
current codebase). Update both. Do not change the surrounding test logic.

---

## New Tests: `crates/unimatrix-store/src/tests/sqlite_parity.rs`

### test_create_tables_goal_clusters_exists

```
#[tokio::test]
async fn test_create_tables_goal_clusters_exists() {
    let dir = TempDir::new();
    let store = open_test_store(&dir).await;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='goal_clusters'"
    )
    .fetch_one(store.read_pool_test())
    .await
    .unwrap();

    assert_eq!(count, 1, "goal_clusters table must exist after create_tables_if_needed");
}
```

### test_create_tables_goal_clusters_schema

```
#[tokio::test]
async fn test_create_tables_goal_clusters_schema() {
    let dir = TempDir::new();
    let store = open_test_store(&dir).await;

    let rows = sqlx::query("PRAGMA table_info(goal_clusters)")
        .fetch_all(store.read_pool_test())
        .await
        .unwrap();

    assert_eq!(
        rows.len(), 7,
        "goal_clusters must have exactly 7 columns: id, feature_cycle, goal_embedding, \
         phase, entry_ids_json, outcome, created_at"
    );
}
```

### test_schema_version_is_N (update to 22)

Find the existing test asserting schema version and change the expected value from 21 to 22.

---

## New Test: Migration Integration (migration_tests.rs or equivalent)

### test_v22_migration_creates_goal_clusters

```
#[tokio::test]
async fn test_v22_migration_creates_goal_clusters() {
    // Open a v21 fixture database (must be prepared separately as test fixture).
    let dir = TempDir::new();
    let v21_db_path = copy_v21_fixture_to(&dir);

    let mut conn = open_migration_connection(&v21_db_path).await;
    apply_pragmas(&mut conn).await;
    migrate_if_needed(&mut conn, &v21_db_path).await.unwrap();
    drop(conn);

    // Open the store to verify.
    let store = open_store_at(&v21_db_path).await;

    // Assert version is 22.
    let version: i64 = sqlx::query_scalar(
        "SELECT value FROM counters WHERE name = 'schema_version'"
    )
    .fetch_one(store.read_pool_test())
    .await
    .unwrap();
    assert!(version >= 22, "schema version must be at least 22 after migration");

    // Assert goal_clusters table exists with 7 columns.
    let col_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('goal_clusters')"
    )
    .fetch_one(store.read_pool_test())
    .await
    .unwrap();
    assert_eq!(col_count, 7);
}
```

### test_current_schema_version_is_at_least_21 (rename from _21)

Rename existing `test_current_schema_version_is_21` to
`test_current_schema_version_is_at_least_21`.
Change `assert_eq!(version, 21)` to `assert!(version >= 21)`.

---

## Key Test Scenarios (store-v22)

| Scenario | Assertion |
|----------|-----------|
| Fresh DB create_tables | `goal_clusters` table exists with 7 columns |
| Fresh DB schema version | version == 22 |
| v21 → v22 migration | version >= 22, goal_clusters exists, 7 columns |
| insert_goal_cluster first write | returns Ok(true) |
| insert_goal_cluster duplicate | returns Ok(false), no error, single row in DB |
| query_goal_clusters empty table | returns Ok(Vec::new()) |
| query_goal_clusters below threshold | filtered out, empty result |
| query_goal_clusters 101 rows | oldest row excluded (recency cap = 100) |
| get_cycle_start_goal_embedding no event | returns Ok(None) |
| get_cycle_start_goal_embedding NULL blob | returns Ok(None) |
| get_cycle_start_goal_embedding malformed blob | returns Ok(None), logs warn! |
| cosine similarity exactly at threshold | included (>= not >) |
| server.rs version assertions | both assert_eq!(version, 22) pass |
| grep 'schema_version.*== 21' | zero matches (AC-17) |
