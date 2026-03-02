# Pseudocode: storage-layer

Component: Storage Layer (P0)
Files: `schema.rs`, `sessions.rs` (new), `injection_log.rs` (new), `migration.rs`

---

## Purpose

Add SESSIONS (16th table) and INJECTION_LOG (17th table) to the redb store with schema v5 migration. Provide typed store operations for session lifecycle management and injection event persistence.

---

## 1. schema.rs Changes

Add two new table constants after SIGNAL_QUEUE:

```
// Table 16: Session lifecycle records
pub const SESSIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("sessions")

// Table 17: Injection event log
pub const INJECTION_LOG: TableDefinition<u64, &[u8]> = TableDefinition::new("injection_log")
```

Update comment: `// -- Table Definitions (17 total after schema v5) --`

---

## 2. sessions.rs (new file)

### Imports
```
use serde::{Serialize, Deserialize}
use crate::error::{Result, StoreError}
use crate::schema::{SESSIONS, INJECTION_LOG, COUNTERS}
use crate::db::Store
use std::time::{SystemTime, UNIX_EPOCH}
```

### Types

```
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionRecord {
    pub session_id: String,
    pub feature_cycle: Option<String>,
    pub agent_role: Option<String>,
    pub started_at: u64,          // unix epoch seconds
    pub ended_at: Option<u64>,
    pub status: SessionLifecycleStatus,
    pub compaction_count: u32,
    pub outcome: Option<String>,      // "success" | "rework" | "abandoned"
    pub total_injections: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SessionLifecycleStatus {
    Active,
    Completed,    // Success or Rework outcome
    TimedOut,     // GC-marked; was Active > 24h
    Abandoned,    // ADR-001: distinct for retrospective filtering
}

pub struct GcStats {
    pub timed_out_count: u32,
    pub deleted_session_count: u32,
    pub deleted_injection_log_count: u32,
}

pub const TIMED_OUT_THRESHOLD_SECS: u64 = 24 * 3600
pub const DELETE_THRESHOLD_SECS: u64 = 30 * 24 * 3600
```

### Serialization helpers

```
fn serialize_session(record: &SessionRecord) -> Result<Vec<u8>>:
    bincode::serde::encode_to_vec(record, bincode::config::standard())
    map_err to StoreError::Serialization

fn deserialize_session(bytes: &[u8]) -> Result<SessionRecord>:
    bincode::serde::decode_from_slice(bytes, bincode::config::standard())
    map_err to StoreError::Serialization
    return first element (ignore consumed count)
```

### insert_session

```
impl Store {
    pub fn insert_session(&self, record: &SessionRecord) -> Result<()>:
        bytes = serialize_session(record)?
        txn = self.db.begin_write()?
        {
            let mut table = txn.open_table(SESSIONS)?
            table.insert(record.session_id.as_str(), bytes.as_slice())?
        }
        txn.commit()?
        Ok(())
}
```

Error: propagates `redb::Error` via `StoreError::Database`.

### update_session

```
pub fn update_session(
    &self,
    session_id: &str,
    updater: impl FnOnce(&mut SessionRecord),
) -> Result<()>:
    txn = self.db.begin_write()?
    {
        let mut table = txn.open_table(SESSIONS)?
        let guard = table.get(session_id)?
        match guard:
            None => return Err(StoreError::NotFound(session_id.to_string()))
            Some(value_guard) =>
                bytes = value_guard.value()
                record = deserialize_session(bytes)?
                drop(value_guard)  // must release borrow before mut borrow
                updater(&mut record)
                new_bytes = serialize_session(&record)?
                table.insert(session_id, new_bytes.as_slice())?
    }
    txn.commit()?
    Ok(())
```

Note: redb requires releasing the read guard before inserting. Pattern: read bytes, deserialize, drop guard, modify, re-serialize, insert.

### get_session

```
pub fn get_session(&self, session_id: &str) -> Result<Option<SessionRecord>>:
    txn = self.db.begin_read()?
    table = txn.open_table(SESSIONS)?
    match table.get(session_id)?:
        None => Ok(None)
        Some(guard) => Ok(Some(deserialize_session(guard.value())?))
```

### scan_sessions_by_feature

```
pub fn scan_sessions_by_feature(&self, feature_cycle: &str) -> Result<Vec<SessionRecord>>:
    txn = self.db.begin_read()?
    table = txn.open_table(SESSIONS)?
    results = Vec::new()
    for entry in table.iter()?:
        (key_guard, value_guard) = entry?
        record = deserialize_session(value_guard.value())?
        if record.feature_cycle.as_deref() == Some(feature_cycle):
            results.push(record)
    Ok(results)
```

Note: Full scan acceptable at current volumes. Primary-key lookup is by session_id; no secondary index by feature_cycle.

### scan_sessions_by_feature_with_status

```
pub fn scan_sessions_by_feature_with_status(
    &self,
    feature_cycle: &str,
    status_filter: Option<SessionLifecycleStatus>,
) -> Result<Vec<SessionRecord>>:
    txn = self.db.begin_read()?
    table = txn.open_table(SESSIONS)?
    results = Vec::new()
    for entry in table.iter()?:
        (_, value_guard) = entry?
        record = deserialize_session(value_guard.value())?
        if record.feature_cycle.as_deref() != Some(feature_cycle):
            continue
        match &status_filter:
            None => results.push(record)
            Some(filter) =>
                if &record.status == filter:
                    results.push(record)
    Ok(results)
```

### gc_sessions

```
pub fn gc_sessions(
    &self,
    timed_out_threshold_secs: u64,
    delete_threshold_secs: u64,
) -> Result<GcStats>:
    now = unix_now_secs()
    timed_out_boundary = now.saturating_sub(timed_out_threshold_secs)
    delete_boundary = now.saturating_sub(delete_threshold_secs)

    txn = self.db.begin_write()?
    stats = GcStats { timed_out_count: 0, deleted_session_count: 0, deleted_injection_log_count: 0 }

    // Phase 1: collect session_ids to delete
    sessions_to_delete: Vec<String> = []
    {
        let table = txn.open_table(SESSIONS)?  // read-only scan
        for entry in table.iter()?:
            (key_guard, value_guard) = entry?
            record = deserialize_session(value_guard.value())?
            if record.started_at < delete_boundary:
                sessions_to_delete.push(record.session_id.clone())
    }

    // Phase 2: full scan INJECTION_LOG; collect log_ids for deleted sessions
    log_ids_to_delete: Vec<u64> = []
    {
        let log_table = txn.open_table(INJECTION_LOG)?
        for entry in log_table.iter()?:
            (key_guard, value_guard) = entry?
            log_record = deserialize_injection_log(value_guard.value())?
            if sessions_to_delete.contains(&log_record.session_id):
                log_ids_to_delete.push(log_record.log_id)
    }

    // Phase 3: delete INJECTION_LOG entries
    {
        let mut log_table = txn.open_table(INJECTION_LOG)?
        for log_id in &log_ids_to_delete:
            log_table.remove(*log_id)?
            stats.deleted_injection_log_count += 1
    }

    // Phase 4: delete SESSIONS entries
    {
        let mut sessions_table = txn.open_table(SESSIONS)?
        for session_id in &sessions_to_delete:
            sessions_table.remove(session_id.as_str())?
            stats.deleted_session_count += 1
    }

    // Phase 5: mark Active sessions with started_at < timed_out_boundary as TimedOut
    {
        // Collect IDs to update first (avoid borrow conflicts)
        let table = txn.open_table(SESSIONS)?
        timed_out_updates: Vec<(String, Vec<u8>)> = []
        for entry in table.iter()?:
            (_, value_guard) = entry?
            record = deserialize_session(value_guard.value())?
            if record.status == SessionLifecycleStatus::Active
               && record.started_at < timed_out_boundary
               && !sessions_to_delete.contains(&record.session_id):
                let mut updated = record.clone()
                updated.status = SessionLifecycleStatus::TimedOut
                timed_out_updates.push((updated.session_id.clone(), serialize_session(&updated)?))
                stats.timed_out_count += 1
        drop(table)  // release immutable borrow

        let mut sessions_table = txn.open_table(SESSIONS)?
        for (id, bytes) in timed_out_updates:
            sessions_table.insert(id.as_str(), bytes.as_slice())?
    }

    txn.commit()?
    Ok(stats)
```

Note: All 5 phases in one write transaction (ADR-002 atomicity guarantee).

---

## 3. injection_log.rs (new file)

### Imports
```
use serde::{Serialize, Deserialize}
use crate::error::{Result, StoreError}
use crate::schema::{INJECTION_LOG, COUNTERS}
use crate::db::Store
```

### Types

```
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InjectionLogRecord {
    pub log_id: u64,          // monotonic; allocated by insert_injection_log_batch
    pub session_id: String,
    pub entry_id: u64,
    pub confidence: f64,      // reranked score at injection time
    pub timestamp: u64,       // unix epoch seconds
}
```

### Serialization helpers

```
fn serialize_injection_log(record: &InjectionLogRecord) -> Result<Vec<u8>>:
    bincode::serde::encode_to_vec(record, bincode::config::standard())

fn deserialize_injection_log(bytes: &[u8]) -> Result<InjectionLogRecord>:
    bincode::serde::decode_from_slice(bytes, bincode::config::standard())
    return .0 (first element)
```

### insert_injection_log_batch

```
pub fn insert_injection_log_batch(&self, records: &[InjectionLogRecord]) -> Result<()>:
    if records.is_empty():
        return Ok(())

    txn = self.db.begin_write()?
    {
        let mut counters = txn.open_table(COUNTERS)?
        // Atomically allocate a contiguous ID range
        base_id: u64 = match counters.get("next_log_id")?:
            Some(guard) => guard.value()
            None => 0  // should not happen after migration; defensive fallback

        let next_id = base_id + records.len() as u64
        counters.insert("next_log_id", next_id)?

        let mut log_table = txn.open_table(INJECTION_LOG)?
        for (i, record) in records.iter().enumerate():
            let mut r = record.clone()
            r.log_id = base_id + i as u64
            bytes = serialize_injection_log(&r)?
            log_table.insert(r.log_id, bytes.as_slice())?
    }
    txn.commit()?
    Ok(())
```

Key: counter and all writes in one transaction. The `log_id` in incoming records is ignored (overwritten with allocated IDs).

### scan_injection_log_by_session

```
pub fn scan_injection_log_by_session(
    &self,
    session_id: &str,
) -> Result<Vec<InjectionLogRecord>>:
    txn = self.db.begin_read()?
    table = txn.open_table(INJECTION_LOG)?
    results = Vec::new()
    for entry in table.iter()?:
        (_, value_guard) = entry?
        record = deserialize_injection_log(value_guard.value())?
        if record.session_id == session_id:
            results.push(record)
    Ok(results)
```

Note: Full scan + in-process filter. Acceptable at <5K records/day. Secondary index deferred.

---

## 4. migration.rs Changes

### Bump version constant

```
pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 5
```

### Add migrate_v4_to_v5

```
fn migrate_v4_to_v5(txn: &redb::WriteTransaction) -> Result<()>:
    // Open tables to trigger creation (redb creates on first open in write txn)
    txn.open_table(SESSIONS)?
    txn.open_table(INJECTION_LOG)?

    // Write next_log_id = 0 only if key does not already exist (idempotency)
    {
        let mut counters = txn.open_table(COUNTERS)?
        if counters.get("next_log_id")?.is_none():
            counters.insert("next_log_id", 0u64)?
    }

    Ok(())
```

### Update migrate_if_needed chain

After the existing `if current_version <= 3 { migrate_v3_to_v4(&txn)?; }` block, add:

```
if current_version <= 4:
    migrate_v4_to_v5(&txn)?
```

---

## 5. lib.rs Changes (unimatrix-store)

Re-export new modules:

```
pub mod sessions;
pub mod injection_log;

// Re-export key types at crate root
pub use sessions::{
    SessionRecord, SessionLifecycleStatus, GcStats,
    TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS,
};
pub use injection_log::InjectionLogRecord;
```

---

## Error Handling

| Operation | Error Type | Condition |
|-----------|-----------|-----------|
| `insert_session` | `StoreError::Database` | redb write failure |
| `update_session` | `StoreError::NotFound` | session_id not in SESSIONS |
| `update_session` | `StoreError::Serialization` | corrupt bytes |
| `insert_injection_log_batch` | `StoreError::Database` | redb write failure |
| `gc_sessions` | `StoreError::Database` | redb write failure mid-transaction |

All errors propagate up; callers (spawn_blocking wrappers) log errors but don't fail the parent operation.

---

## Key Test Scenarios

1. `insert_session` → `get_session` roundtrip: returns identical record.
2. `update_session` changes status and sets `ended_at` atomically.
3. `insert_injection_log_batch` with N=3: allocates IDs [0,1,2], `next_log_id` becomes 3.
4. Two separate `insert_injection_log_batch` calls: IDs are contiguous, no overlap.
5. `scan_injection_log_by_session("A")` with records for A and B: returns only A's.
6. `gc_sessions`: Active session at 25h → status=TimedOut (not deleted).
7. `gc_sessions`: Session at 31 days → deleted; associated injection log records deleted.
8. `gc_sessions` atomicity: if any phase fails, entire transaction rolls back.
9. `migrate_v4_to_v5` idempotency: calling twice on same transaction doesn't reset `next_log_id`.
10. Schema v5 migration preserves all existing ENTRIES and SIGNAL_QUEUE records.
