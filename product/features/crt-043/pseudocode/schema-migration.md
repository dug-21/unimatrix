# crt-043: Schema Migration — Pseudocode

## Purpose

Add the v20→v21 migration block to `run_main_migrations`, providing two new nullable columns:
- `cycle_events.goal_embedding BLOB` — stores bincode-serialized `Vec<f32>` goal embeddings
- `observations.phase TEXT` — stores the active session phase at observation write time

Provide `encode_goal_embedding` / `decode_goal_embedding` helpers as the canonical SQLite
embedding blob serialization API for this codebase (ADR-001).

Add `update_cycle_start_goal_embedding` as the UPDATE method for the goal embedding write path.

---

## Files

| File | Action |
|------|--------|
| `crates/unimatrix-store/src/migration.rs` | Modify: update `CURRENT_SCHEMA_VERSION`, add v21 block, add composite index |
| `crates/unimatrix-store/src/db.rs` | Modify: add `update_cycle_start_goal_embedding`, add `phase` bind to observation INSERT functions |
| `crates/unimatrix-store/src/embedding.rs` | Create: `encode_goal_embedding`, `decode_goal_embedding`, round-trip unit tests |
| `crates/unimatrix-store/src/lib.rs` | Modify: declare `mod embedding` |

---

## migration.rs Changes

### CURRENT_SCHEMA_VERSION Constant

```
// Before:
pub const CURRENT_SCHEMA_VERSION: u64 = 20;

// After:
pub const CURRENT_SCHEMA_VERSION: u64 = 21;
```

### v20 → v21 Migration Block

Appended to `run_main_migrations` before the final `INSERT OR REPLACE INTO counters` statement.

```
// v20 → v21: goal_embedding BLOB on cycle_events + phase TEXT on observations (crt-043).
//
// Both ADD COLUMN statements run inside the outer transaction from migrate_if_needed().
// If either fails, the entire transaction rolls back — schema_version stays at 20 (ADR-003).
//
// Both pragma_table_info checks run first, before either ALTER TABLE, so that a partially-
// applied previous attempt (one column added, no version bump) is recovered correctly:
//   - column present → skip ALTER (idempotent)
//   - column absent → execute ALTER
//
// Pattern: entry #1264 (pragma_table_info pre-check).
if current_version < 21 {

    // Pre-check 1: cycle_events.goal_embedding
    let has_goal_embedding: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_events') WHERE name = 'goal_embedding'"
    )
    .fetch_one(&mut **txn)
    .await
    .map(|count| count > 0)
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    // Pre-check 2: observations.phase
    let has_phase: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'phase'"
    )
    .fetch_one(&mut **txn)
    .await
    .map(|count| count > 0)
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    // ALTER 1: cycle_events.goal_embedding (only if absent)
    if !has_goal_embedding {
        sqlx::query("ALTER TABLE cycle_events ADD COLUMN goal_embedding BLOB")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
    }

    // ALTER 2: observations.phase (only if absent)
    if !has_phase {
        sqlx::query("ALTER TABLE observations ADD COLUMN phase TEXT")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
    }

    // Composite index on (topic_signal, phase) for Group 6 S6/S7 phase-stratification queries.
    // CREATE INDEX IF NOT EXISTS: idempotent on re-run (no pre-check needed).
    // topic_signal first (higher cardinality, primary filter); phase second.
    // Decision rationale: see OVERVIEW.md FR-C-07 Resolution.
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_observations_topic_phase ON observations (topic_signal, phase)"
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    // No backfill: pre-v21 rows have goal_embedding = NULL and phase = NULL.
    // NULL is the accepted cold-start baseline for both columns (NFR-04).

    // Bump schema_version to 21 within the outer transaction.
    // If the outer transaction rolls back, this UPDATE rolls back with it.
    sqlx::query("UPDATE counters SET value = 21 WHERE name = 'schema_version'")
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
}
```

Note: the existing final `INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', ?1)` statement at the bottom of `run_main_migrations` is retained and correctly sets the version to 21 via `CURRENT_SCHEMA_VERSION`. The in-block `UPDATE counters SET value = 21` is required so that if a v22 block is added later, it sees the correct intermediate version when `current_version < 21` evaluates from an already-partially-migrated state. This follows the established pattern from v18, v19, and v20 blocks.

---

## embedding.rs (New File)

Location: `crates/unimatrix-store/src/embedding.rs`

```
//! SQLite embedding blob serialization helpers (ADR-001, crt-043).
//!
//! These are the canonical encode/decode helpers for Vec<f32> BLOB columns in SQLite.
//! Every new embedding BLOB column introduced after crt-043 must have analogous paired
//! helpers (`encode_X_embedding` / `decode_X_embedding`) defined in the same PR as
//! the write path.
//!
//! Serialization: bincode v2 with serde, config::standard().
//! Rationale: self-describing length prefix, model-upgrade-safe, no new dependency (ADR-001).
//!
//! Visibility: pub(crate). Group 6 consumes embeddings via store query methods that
//! decode internally — not by calling these helpers directly from unimatrix-server.
//! If Group 6 ever requires direct cross-crate access, promote to pub at that time.

use bincode::error::{DecodeError, EncodeError};

/// Serialize a Vec<f32> embedding to a SQLite BLOB using bincode standard config.
///
/// Uses `bincode::serde::encode_to_vec(vec, config::standard())`.
/// Returns an error if bincode serialization fails (should be unreachable for a
/// valid Vec<f32> with standard config, but the Result is propagated per FR-B-04).
pub(crate) fn encode_goal_embedding(vec: Vec<f32>) -> Result<Vec<u8>, EncodeError> {
    bincode::serde::encode_to_vec(vec, bincode::config::standard())
}

/// Deserialize a SQLite BLOB back to Vec<f32> using bincode standard config.
///
/// Uses `bincode::serde::decode_from_slice(bytes, config::standard())`.
/// The `_len` (bytes consumed) return value is discarded; only the Vec<f32> is returned.
/// Returns DecodeError if the bytes are malformed or use a different config.
///
/// Read sites (Group 6/7) MUST call this via a store query method, not directly.
/// See OVERVIEW.md WARN-2 Resolution for access pattern.
pub(crate) fn decode_goal_embedding(bytes: &[u8]) -> Result<Vec<f32>, DecodeError> {
    let (vec, _len): (Vec<f32>, usize) =
        bincode::serde::decode_from_slice(bytes, bincode::config::standard())?;
    Ok(vec)
}

#[cfg(test)]
mod tests {
    use super::*;

    // R-02 scenario 1: round-trip test (AC-14).
    // Encodes a known Vec<f32>, decodes it back, asserts element-wise equality.
    // Float equality is exact because no lossy transform occurs in bincode round-trip.
    #[test]
    fn test_encode_decode_round_trip() {
        let original: Vec<f32> = (0..384).map(|i| i as f32 * 0.001).collect();
        let bytes = encode_goal_embedding(original.clone())
            .expect("encode should not fail for valid Vec<f32>");
        let decoded = decode_goal_embedding(&bytes)
            .expect("decode should not fail for bytes produced by encode");
        assert_eq!(original, decoded, "round-trip encode→decode must be lossless");
    }

    // R-02 scenario 2: negative test — malformed bytes produce DecodeError, not panic.
    #[test]
    fn test_decode_malformed_bytes_returns_error() {
        let bad_bytes: Vec<u8> = vec![0xFF, 0xFE, 0x01, 0x02];
        let result = decode_goal_embedding(&bad_bytes);
        assert!(result.is_err(), "malformed bytes must return DecodeError, not Ok");
    }

    // R-02 scenario 3: helper is a thin wrapper — cross-call with direct bincode API
    // produces identical results.
    #[test]
    fn test_encode_matches_direct_bincode_call() {
        let vec: Vec<f32> = vec![1.0, 2.0, 3.0];
        let via_helper = encode_goal_embedding(vec.clone()).unwrap();
        let via_direct = bincode::serde::encode_to_vec(&vec, bincode::config::standard()).unwrap();
        assert_eq!(via_helper, via_direct, "helper must produce same bytes as direct bincode call");
    }

    // Additional: zero-length vector round-trips correctly.
    #[test]
    fn test_encode_decode_empty_vec() {
        let empty: Vec<f32> = vec![];
        let bytes = encode_goal_embedding(empty.clone()).unwrap();
        let decoded = decode_goal_embedding(&bytes).unwrap();
        assert_eq!(empty, decoded);
    }
}
```

### lib.rs Addition

Add the module declaration in `crates/unimatrix-store/src/lib.rs`:

```
// Add alongside other pub(crate) / private modules:
pub(crate) mod embedding;
```

The helpers are `pub(crate)` within the `embedding` module, so `pub(crate) mod embedding` is
sufficient. No re-export in `lib.rs` is needed.

---

## db.rs Changes

### New Method: `update_cycle_start_goal_embedding`

Add to the `impl SqlxStore` block, alongside `insert_cycle_event` and `get_cycle_start_goal`.

```
/// Write the bincode-encoded goal embedding blob to the cycle_start row (crt-043).
///
/// Issues:
///   UPDATE cycle_events SET goal_embedding = ?1
///   WHERE cycle_id = ?2 AND event_type = 'cycle_start'
///
/// Matching on both cycle_id and event_type ensures only the start row is updated.
/// The existing idx_cycle_events_cycle_id index makes this UPDATE cheap (O(log N) lookup).
///
/// If the cycle_start row does not yet exist (INSERT/UPDATE race, ADR-002), SQLite
/// returns zero rows affected — this is a silent no-op, not an error. The column
/// stays NULL, which is the same outcome as the embed-service-unavailable path.
///
/// If multiple cycle_start rows exist for the same cycle_id (data anomaly), all are
/// updated. This is not expected but is documented as a known edge case.
///
/// Called from a fire-and-forget tokio::spawn in handle_cycle_event (Step 6).
/// Uses the write pool directly (same as insert_cycle_event), not the analytics drain.
pub async fn update_cycle_start_goal_embedding(
    &self,
    cycle_id: &str,
    embedding_bytes: Vec<u8>,
) -> Result<()> {
    let mut conn = self.write_pool
        .acquire()
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

    sqlx::query(
        "UPDATE cycle_events SET goal_embedding = ?1
         WHERE cycle_id = ?2 AND event_type = 'cycle_start'"
    )
    .bind(embedding_bytes)
    .bind(cycle_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| StoreError::Database(e.into()))?;

    Ok(())
}
```

### Modified: `insert_observation` (in listener.rs, not db.rs)

Note: `insert_observation` and `insert_observations_batch` are private functions in
`unimatrix-server/src/uds/listener.rs`, not in `db.rs`. They use `store.write_pool_server()`
via raw sqlx. Their modification is documented in the `phase-capture.md` component file.

The `observations.rs` in `unimatrix-store` has a separate `ObservationRow` (the read-side
struct for the server tick path). That struct does not need modification — it is used for
fetching observations for the NLI detection tick, not for writing. The write-side struct
is the private `ObservationRow` in `listener.rs`.

---

## create_tables_if_needed Synchronization

The DDL in `create_tables_if_needed` (db.rs) creates tables from scratch for fresh databases.
The `cycle_events` CREATE TABLE must be updated to include `goal_embedding BLOB`, and the
`observations` CREATE TABLE must be updated to include `phase TEXT` and the composite index.

Both DDL statements in `create_tables_if_needed` use `CREATE TABLE IF NOT EXISTS` with all
columns. After crt-043, they must include the new columns so that fresh databases start at v21
with the correct schema.

```
// In create_tables_if_needed, cycle_events table must add:
goal_embedding BLOB  -- crt-043

// In create_tables_if_needed, observations table must add:
phase TEXT           -- crt-043

// In create_tables_if_needed, add index:
CREATE INDEX IF NOT EXISTS idx_observations_topic_phase ON observations (topic_signal, phase)
```

---

## Error Handling

| Function | Error | Handling |
|----------|-------|---------|
| `encode_goal_embedding` | `bincode::error::EncodeError` | Propagated to caller (fire-and-forget task) which emits `tracing::warn!` and skips UPDATE |
| `decode_goal_embedding` | `bincode::error::DecodeError` | Propagated to caller (store query method, Group 6) which handles NULL-like degradation |
| `update_cycle_start_goal_embedding` | `StoreError::Database` | Propagated to caller (fire-and-forget task) which emits `tracing::warn!` with cycle_id |
| Migration block (ALTER TABLE) | `StoreError::Migration` | Outer transaction in `migrate_if_needed` rolls back; Store::open() returns Err; server fails to start |

---

## Key Test Scenarios

For full test scenarios see `test-plan/schema-migration.md`. Required scenarios (AC-01, AC-07, FR-M-04):

1. **v20 fixture migration** — open a real v20 database through `Store::open()`. Assert:
   - `pragma_table_info('cycle_events')` contains a row with `name = 'goal_embedding'` and `type = 'BLOB'`
   - `pragma_table_info('observations')` contains a row with `name = 'phase'` and `type = 'TEXT'`
   - `sqlite_master` contains an index row for `idx_observations_topic_phase`
   - `SELECT value FROM counters WHERE name = 'schema_version'` returns 21

2. **Idempotency** (AC-11) — call `Store::open()` on an already-v21 database. Assert no error, schema_version still 21, no duplicate columns.

3. **Partial-apply recovery** (R-05 scenario 2) — open a v20 database where `goal_embedding` was manually added. Assert migration adds `phase` without error, schema_version = 21.

4. **Round-trip encode/decode** (AC-14, R-02) — see `embedding.rs` unit tests above.

5. **update_cycle_start_goal_embedding no-op on missing row** (R-08 scenario 1) — call the method with a non-existent cycle_id. Assert `Ok(())` returned, no panic.

6. **Fresh schema at v21** (R-06 scenario 2) — create a new store from scratch. Assert both columns present and schema_version = 21 in `create_tables_if_needed` path.
