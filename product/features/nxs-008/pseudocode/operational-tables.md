# Component: operational-tables (Wave 2)

## Files Modified

- `crates/unimatrix-store/src/sessions.rs` - 9 SQL columns
- `crates/unimatrix-store/src/injection_log.rs` - 5 SQL columns
- `crates/unimatrix-store/src/signal.rs` - 6 SQL columns + JSON entry_ids
- `crates/unimatrix-store/src/write_ext.rs` - Co-access SQL columns (covered in write-paths)

**Risk**: MEDIUM (RISK-11, RISK-12), HIGH (RISK-07 enums, RISK-08 JSON)
**ADR**: ADR-003 (INTEGER enums), ADR-007 (JSON arrays)

## sessions.rs Rewrite

### SessionLifecycleStatus: Add repr(u8) + TryFrom

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum SessionLifecycleStatus {
    Active = 0,
    Completed = 1,
    TimedOut = 2,
    Abandoned = 3,
}

impl TryFrom<u8> for SessionLifecycleStatus {
    type Error = StoreError;
    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Active),
            1 => Ok(Self::Completed),
            2 => Ok(Self::TimedOut),
            3 => Ok(Self::Abandoned),
            other => Err(StoreError::InvalidStatus(other)),
        }
    }
}
```

### Remove: serialize_session, deserialize_session

These become dead code (moved to migration_compat if needed).

### insert_session Rewrite

```rust
pub fn insert_session(&self, record: &SessionRecord) -> Result<()> {
    let conn = self.lock_conn();
    conn.execute(
        "INSERT OR REPLACE INTO sessions (session_id, feature_cycle, agent_role,
            started_at, ended_at, status, compaction_count, outcome, total_injections)
         VALUES (:sid, :fc, :ar, :sa, :ea, :st, :cc, :oc, :ti)",
        rusqlite::named_params! {
            ":sid": &record.session_id,
            ":fc": &record.feature_cycle,
            ":ar": &record.agent_role,
            ":sa": record.started_at as i64,
            ":ea": record.ended_at.map(|v| v as i64),
            ":st": record.status as u8 as i64,
            ":cc": record.compaction_count as i64,
            ":oc": &record.outcome,
            ":ti": record.total_injections as i64,
        },
    ).map_err(StoreError::Sqlite)?;
    Ok(())
}
```

### get_session Rewrite

```rust
pub fn get_session(&self, session_id: &str) -> Result<Option<SessionRecord>> {
    let conn = self.lock_conn();
    conn.query_row(
        "SELECT session_id, feature_cycle, agent_role, started_at, ended_at,
                status, compaction_count, outcome, total_injections
         FROM sessions WHERE session_id = ?1",
        rusqlite::params![session_id],
        |row| Ok(SessionRecord {
            session_id: row.get("session_id")?,
            feature_cycle: row.get("feature_cycle")?,
            agent_role: row.get("agent_role")?,
            started_at: row.get::<_, i64>("started_at")? as u64,
            ended_at: row.get::<_, Option<i64>>("ended_at")?.map(|v| v as u64),
            status: SessionLifecycleStatus::try_from(
                row.get::<_, u8>("status")?
            ).unwrap_or(SessionLifecycleStatus::Active),
            compaction_count: row.get::<_, i64>("compaction_count")? as u32,
            outcome: row.get("outcome")?,
            total_injections: row.get::<_, i64>("total_injections")? as u32,
        }),
    ).optional().map_err(StoreError::Sqlite)?
    .map_or(Ok(None), |r| Ok(Some(r)))
}
```

### update_session Rewrite

```rust
pub fn update_session(&self, session_id: &str, updater: impl FnOnce(&mut SessionRecord)) -> Result<()> {
    let conn = self.lock_conn();
    conn.execute_batch("BEGIN IMMEDIATE").map_err(StoreError::Sqlite)?;

    let result = (|| -> Result<()> {
        // Read current record from SQL columns
        let mut record = self.get_session_internal(&conn, session_id)?
            .ok_or_else(|| StoreError::Deserialization(format!("session not found: {session_id}")))?;

        // Apply updater
        updater(&mut record);

        // Write back all columns
        conn.execute(
            "UPDATE sessions SET feature_cycle = :fc, agent_role = :ar,
                started_at = :sa, ended_at = :ea, status = :st,
                compaction_count = :cc, outcome = :oc, total_injections = :ti
             WHERE session_id = :sid",
            rusqlite::named_params! { /* all fields */ },
        ).map_err(StoreError::Sqlite)?;
        Ok(())
    })();
    // COMMIT / ROLLBACK pattern
}
```

### scan_sessions_by_feature Rewrite

```rust
// Replace full-table scan + deserialize + filter
// with indexed WHERE query
pub fn scan_sessions_by_feature(&self, feature_cycle: &str) -> Result<Vec<SessionRecord>> {
    let conn = self.lock_conn();
    let mut stmt = conn.prepare(
        "SELECT session_id, feature_cycle, agent_role, started_at, ended_at,
                status, compaction_count, outcome, total_injections
         FROM sessions WHERE feature_cycle = ?1"
    ).map_err(StoreError::Sqlite)?;
    // Map rows to SessionRecord
}
```

### gc_sessions Rewrite

```rust
pub fn gc_sessions(&self, timed_out_threshold: u64, delete_threshold: u64) -> Result<GcStats> {
    let now = current_unix_secs();
    let delete_boundary = now.saturating_sub(delete_threshold);
    let timed_out_boundary = now.saturating_sub(timed_out_threshold);

    let conn = self.lock_conn();
    conn.execute_batch("BEGIN IMMEDIATE")?;

    // Phase 1: Delete injection_log for sessions being deleted (indexed)
    let deleted_logs = conn.execute(
        "DELETE FROM injection_log WHERE session_id IN (
            SELECT session_id FROM sessions WHERE started_at < ?1
        )",
        rusqlite::params![delete_boundary as i64],
    )? as u32;

    // Phase 2: Delete old sessions
    let deleted_sessions = conn.execute(
        "DELETE FROM sessions WHERE started_at < ?1",
        rusqlite::params![delete_boundary as i64],
    )? as u32;

    // Phase 3: Mark timed-out sessions
    let timed_out = conn.execute(
        "UPDATE sessions SET status = ?1 WHERE status = 0 AND started_at < ?2",
        rusqlite::params![
            SessionLifecycleStatus::TimedOut as u8 as i64,
            timed_out_boundary as i64
        ],
    )? as u32;

    conn.execute_batch("COMMIT")?;
    Ok(GcStats { timed_out_count: timed_out, deleted_session_count: deleted_sessions, deleted_injection_log_count: deleted_logs })
}
```

## injection_log.rs Rewrite

### Remove: serialize_injection_log, deserialize_injection_log

Dead code after normalization. Move to migration_compat if needed.

### insert_injection_log_batch Rewrite

```rust
pub fn insert_injection_log_batch(&self, records: &[InjectionLogRecord]) -> Result<()> {
    if records.is_empty() { return Ok(()); }
    let conn = self.lock_conn();
    conn.execute_batch("BEGIN IMMEDIATE")?;

    let result = (|| -> Result<()> {
        let base_id = crate::counters::read_counter(&conn, "next_log_id")?;
        crate::counters::set_counter(&conn, "next_log_id", base_id + records.len() as u64)?;

        let mut stmt = conn.prepare(
            "INSERT INTO injection_log (log_id, session_id, entry_id, confidence, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)"
        )?;

        for (i, record) in records.iter().enumerate() {
            let log_id = base_id + i as u64;
            stmt.execute(rusqlite::params![
                log_id as i64,
                &record.session_id,
                record.entry_id as i64,
                record.confidence,
                record.timestamp as i64,
            ])?;
        }
        Ok(())
    })();
    // COMMIT / ROLLBACK
}
```

### scan_injection_log_by_session Rewrite

```rust
// Replace full-table scan with indexed WHERE
pub fn scan_injection_log_by_session(&self, session_id: &str) -> Result<Vec<InjectionLogRecord>> {
    let conn = self.lock_conn();
    let mut stmt = conn.prepare(
        "SELECT log_id, session_id, entry_id, confidence, timestamp
         FROM injection_log WHERE session_id = ?1 ORDER BY log_id"
    )?;
    // Map rows to InjectionLogRecord
}
```

## signal.rs Rewrite

### SignalType, SignalSource: Add TryFrom<u8>

```rust
impl TryFrom<u8> for SignalType {
    type Error = StoreError;
    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Helpful),
            1 => Ok(Self::Flagged),
            other => Err(StoreError::InvalidStatus(other)),
        }
    }
}
// Same for SignalSource
```

### Remove: serialize_signal, deserialize_signal from runtime

Keep public API (they're re-exported in lib.rs). Move to migration_compat.

### insert_signal Rewrite

```rust
pub fn insert_signal(&self, record: &SignalRecord) -> Result<u64> {
    let conn = self.lock_conn();
    conn.execute_batch("BEGIN IMMEDIATE")?;

    let result = (|| -> Result<u64> {
        let next_id = crate::counters::read_counter(&conn, "next_signal_id")?;
        crate::counters::set_counter(&conn, "next_signal_id", next_id + 1)?;

        // Cap enforcement
        let current_len: i64 = conn.query_row("SELECT COUNT(*) FROM signal_queue", [], |r| r.get(0))?;
        if current_len >= 10_000 {
            conn.execute(
                "DELETE FROM signal_queue WHERE signal_id = (SELECT MIN(signal_id) FROM signal_queue)",
                [],
            )?;
        }

        // Serialize entry_ids as JSON (ADR-007)
        let entry_ids_json = serde_json::to_string(&record.entry_ids)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;

        conn.execute(
            "INSERT INTO signal_queue (signal_id, session_id, created_at, entry_ids, signal_type, signal_source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                next_id as i64,
                &record.session_id,
                record.created_at as i64,
                &entry_ids_json,
                record.signal_type as u8 as i64,
                record.signal_source as u8 as i64,
            ],
        )?;
        Ok(next_id)
    })();
    // COMMIT / ROLLBACK
}
```

### drain_signals Rewrite

```rust
pub fn drain_signals(&self, signal_type: SignalType) -> Result<Vec<SignalRecord>> {
    let conn = self.lock_conn();
    conn.execute_batch("BEGIN IMMEDIATE")?;

    let result = (|| -> Result<Vec<SignalRecord>> {
        // SELECT matching signals (WHERE signal_type = ?)
        let mut stmt = conn.prepare(
            "SELECT signal_id, session_id, created_at, entry_ids, signal_type, signal_source
             FROM signal_queue WHERE signal_type = ?1 ORDER BY signal_id"
        )?;

        let records: Vec<SignalRecord> = stmt.query_map(
            rusqlite::params![signal_type as u8 as i64],
            |row| {
                let entry_ids_json: String = row.get("entry_ids")?;
                let entry_ids: Vec<u64> = serde_json::from_str(&entry_ids_json)
                    .unwrap_or_default();
                Ok(SignalRecord {
                    signal_id: row.get::<_, i64>("signal_id")? as u64,
                    session_id: row.get("session_id")?,
                    created_at: row.get::<_, i64>("created_at")? as u64,
                    entry_ids,
                    signal_type: SignalType::try_from(row.get::<_, u8>("signal_type")?)
                        .unwrap_or(SignalType::Helpful),
                    signal_source: SignalSource::try_from(row.get::<_, u8>("signal_source")?)
                        .unwrap_or(SignalSource::ImplicitOutcome),
                })
            },
        )?.collect::<rusqlite::Result<Vec<_>>>()?;

        // DELETE matching signals
        conn.execute(
            "DELETE FROM signal_queue WHERE signal_type = ?1",
            rusqlite::params![signal_type as u8 as i64],
        )?;

        Ok(records)
    })();
    // COMMIT / ROLLBACK
}
```
