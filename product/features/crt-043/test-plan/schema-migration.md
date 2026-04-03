# Test Plan: schema-migration

Component covers: `crates/unimatrix-store/src/migration.rs`, `crates/unimatrix-store/src/db.rs`, and `crates/unimatrix-store/src/embedding.rs`.

Test file: `crates/unimatrix-store/tests/migration_v20_v21.rs`

---

## Fixture Builder

**Problem:** No pre-built v20 `.db` file exists. The project convention (established by `migration_v19_v20.rs`) is to build the prior-version DB programmatically.

**`create_v20_database(path: &Path)`** — async function that:
1. Opens a raw `SqliteConnectOptions` connection (not through SqlxStore)
2. Executes all DDL statements for v20 schema tables and indexes
3. Seeds counters with `schema_version = 20`

The v20 DDL is the v19 DDL (from `migration_v19_v20.rs`) verbatim — the v19→v20 migration adds no new columns, only inserts back-fill data into `graph_edges`. The `cycle_events` table at v20 has these columns: `id, cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal` — no `goal_embedding`. The `observations` table at v20 has: `id, session_id, ts_millis, hook, tool, input, response_size, response_snippet, topic_signal` — no `phase`.

**Pattern from existing test:**
```rust
async fn create_v20_database(path: &Path) {
    let opts = SqliteConnectOptions::new().filename(path).create_if_missing(true);
    let mut conn = opts.connect().await.expect("open v20 setup conn");
    // ... same DDL as v19 plus schema_version counter seeded at 20
}
```

---

## Unit Test Expectations

### MIG-V21-U-01: CURRENT_SCHEMA_VERSION constant

```rust
#[test]
fn test_current_schema_version_is_21() {
    assert_eq!(unimatrix_store::migration::CURRENT_SCHEMA_VERSION, 21);
}
```

- Non-async, no fixture.
- Fails if the constant is not bumped from 20.

---

### MIG-V21-U-02: Fresh database creates schema v21

```rust
#[tokio::test]
async fn test_fresh_db_creates_schema_v21() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::default())
        .await.expect("open fresh store");
    assert_eq!(read_schema_version(&store).await, 21);
    store.close().await.unwrap();
}
```

- Verifies fresh databases initialize directly to v21 (create_tables_if_needed path).
- Also asserts that `goal_embedding` and `phase` columns exist on the respective tables via `pragma_table_info`.

---

### MIG-V21-U-03: v20 → v21 migration — both columns present (R-05, AC-01, AC-07)

```rust
#[tokio::test]
async fn test_v20_to_v21_both_columns_present() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v20_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await.expect("open after v20→v21 migration");

    // Assert schema_version = 21
    assert_eq!(read_schema_version(&store).await, 21);

    // Assert goal_embedding BLOB on cycle_events
    let has_goal_embedding: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_events') WHERE name = 'goal_embedding'"
    ).fetch_one(store.read_pool_test()).await.expect("pragma cycle_events");
    assert_eq!(has_goal_embedding, 1, "goal_embedding column must be present on cycle_events");

    // Assert phase TEXT on observations
    let has_phase: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'phase'"
    ).fetch_one(store.read_pool_test()).await.expect("pragma observations");
    assert_eq!(has_phase, 1, "phase column must be present on observations");

    store.close().await.unwrap();
}
```

**This is the mandatory AC-01/AC-07 test (R-05, FR-M-04).**

---

### MIG-V21-U-04: Partial apply recovery — pre-existing goal_embedding column (R-05 scenario 2)

```rust
#[tokio::test]
async fn test_v20_to_v21_partial_apply_recovery() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v20_database(&db_path).await;

    // Simulate partial application: add goal_embedding manually, leave schema_version at 20.
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        sqlx::query("ALTER TABLE cycle_events ADD COLUMN goal_embedding BLOB")
            .execute(&mut conn).await.expect("pre-add goal_embedding");
    }

    // Act: open triggers v20→v21 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await.expect("open with partial state");

    // Assert: no error; both columns present; schema_version = 21.
    assert_eq!(read_schema_version(&store).await, 21);
    // pragma_table_info checks (same as MIG-V21-U-03)
    let has_goal_embedding: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_events') WHERE name = 'goal_embedding'"
    ).fetch_one(store.read_pool_test()).await.expect("pragma goal_embedding");
    assert_eq!(has_goal_embedding, 1);
    let has_phase: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'phase'"
    ).fetch_one(store.read_pool_test()).await.expect("pragma phase");
    assert_eq!(has_phase, 1);

    store.close().await.unwrap();
}
```

Validates that the `pragma_table_info` idempotency pre-check skips the already-present column and proceeds to add the second.

---

### MIG-V21-U-05: Idempotency — re-open v21 database (R-06, AC-11)

```rust
#[tokio::test]
async fn test_v21_migration_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v20_database(&db_path).await;

    // First open triggers migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await.expect("first open");
    assert_eq!(read_schema_version(&store).await, 21);
    store.close().await.unwrap();

    // Second open must be a no-op.
    let store2 = SqlxStore::open(&db_path, PoolConfig::default())
        .await.expect("second open must not error");
    assert_eq!(read_schema_version(&store2).await, 21, "schema_version must remain 21");
    store2.close().await.unwrap();
}
```

---

### MIG-V21-U-06: Composite index decision (R-13)

If the delivery agent adds a `(topic_signal, phase)` composite index to `observations`, add this test:

```rust
// Conditional: only if composite index is added by delivery agent.
#[tokio::test]
async fn test_v21_composite_index_present() {
    // Verify index via sqlite_master:
    // SELECT name FROM sqlite_master WHERE type='index'
    //   AND tbl_name='observations' AND name='idx_observations_topic_signal_phase';
    // assert_eq!(count, 1)
}
```

If the delivery agent decides not to add the index, this test is omitted. The written decision must be in the PR description (FR-C-07).

---

## Serialization Helpers (embedding.rs)

These tests belong in `crates/unimatrix-store/src/embedding.rs` (or `db.rs`) alongside the helpers.

### EMBED-U-01: Round-trip encode→decode (R-02, R-11, AC-14)

```rust
#[test]
fn test_encode_decode_goal_embedding_round_trip() {
    let original: Vec<f32> = (0..384).map(|i| i as f32 * 0.001).collect();
    let bytes = encode_goal_embedding(original.clone()).expect("encode must succeed");
    let decoded = decode_goal_embedding(&bytes).expect("decode must succeed");
    assert_eq!(decoded, original, "decoded must exactly match original (no lossy transform)");
}
```

- 384-element vector matches actual embed pipeline output dimension.
- Exact float equality is valid (no lossy transform in bincode serialization).

### EMBED-U-02: Malformed blob → DecodeError, not panic (R-02)

```rust
#[test]
fn test_decode_goal_embedding_malformed_bytes() {
    let garbage: &[u8] = &[0x00, 0xFF, 0x42, 0x13, 0x37];
    let result = decode_goal_embedding(garbage);
    assert!(result.is_err(), "malformed bytes must return DecodeError, not Ok");
}
```

### EMBED-U-03: Cross-call consistency — helper matches raw bincode (R-02)

```rust
#[test]
fn test_encode_matches_raw_bincode_standard() {
    let vec: Vec<f32> = vec![1.0, 2.0, 3.0];
    let via_helper = encode_goal_embedding(vec.clone()).expect("helper encode");
    let via_raw = bincode::serde::encode_to_vec(&vec, bincode::config::standard())
        .expect("raw encode");
    assert_eq!(via_helper, via_raw, "helper must be a thin wrapper over standard() config");
}
```

---

## store method: update_cycle_start_goal_embedding

### STORE-U-01: Non-existent cycle_id → Ok, zero rows (R-08)

```rust
#[tokio::test]
async fn test_update_goal_embedding_nonexistent_cycle_id() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::default())
        .await.expect("open");
    let bytes = encode_goal_embedding(vec![1.0, 2.0]).expect("encode");
    let result = store.update_cycle_start_goal_embedding("nonexistent-cycle-id", bytes).await;
    assert!(result.is_ok(), "zero rows affected must return Ok, not Err");
    store.close().await.unwrap();
}
```

### STORE-U-02: Existing cycle_start row → blob written (AC-03)

```rust
#[tokio::test]
async fn test_update_goal_embedding_writes_blob() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::default())
        .await.expect("open");

    // Insert a cycle_start row manually (or via store method).
    // Then call update_cycle_start_goal_embedding.
    // Read back via raw SQL and assert non-NULL blob.
    // Decode and assert Vec<f32> matches the original.
}
```

---

## Edge Cases

- **Empty Vec<f32>** — `encode_goal_embedding(vec![])` must succeed (bincode handles empty Vec). `decode_goal_embedding` on the resulting bytes must return `Ok(vec![])`. (Not required by spec but validates the helper is not fragile.)
- **Large vector (768 dims)** — round-trip test with 768-element vector validates future model-upgrade safety (R-02 edge note about 384 vs 768 dimensions).

---

## Code Review Assertions (Static)

These are not automated tests but are required at PR review gate:

1. `CURRENT_SCHEMA_VERSION` constant is exactly 21 in `migration.rs`.
2. Both `pragma_table_info` pre-checks appear before the corresponding `ALTER TABLE` statements.
3. Both ALTER TABLE statements are inside the existing outer transaction (no new `BEGIN`/`COMMIT`).
4. Schema version bump to 21 appears after both ALTER statements, inside the same transaction.
5. `encode_goal_embedding` and `decode_goal_embedding` both use `config::standard()` — no `config::legacy()` or other variant.
6. Both helpers are marked `pub` with re-export from `lib.rs`, per WARN-2 resolution in pseudocode/OVERVIEW.md: Group 6 accesses decoded embeddings via a future store query method (`get_cycle_start_embedding`), not by calling `decode_goal_embedding` directly from `unimatrix-server`. However, helpers are promoted to `pub` now to avoid a breaking change when Group 6 ships. ADR-001 `pub(crate)` baseline is superseded by this delivery decision.
