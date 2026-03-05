# Component: server-tables (Wave 3)

## Files Modified

- `crates/unimatrix-store/src/schema.rs` - Add AgentRecord, AuditEvent, TrustLevel, Capability, Outcome types
- `crates/unimatrix-server/src/infra/registry.rs` - Rewrite to SQL columns + JSON
- `crates/unimatrix-server/src/infra/audit.rs` - Rewrite to SQL columns + JSON

**Risk**: MEDIUM (RISK-13, RISK-14), HIGH (RISK-08 JSON)
**ADR**: ADR-003 (INTEGER enums), ADR-007 (JSON arrays)

## Type Movements (store::schema)

Move these types from server crate to store crate (done in Wave 0 for migration_compat):

```rust
// In crates/unimatrix-store/src/schema.rs:

/// Agent trust hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum TrustLevel {
    System = 0,
    Privileged = 1,
    Internal = 2,
    Restricted = 3,
}

impl TryFrom<u8> for TrustLevel { /* 0-3 mapping */ }

/// Atomic permission unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Capability {
    Read = 0,
    Write = 1,
    Search = 2,
    Admin = 3,
    SessionWrite = 4,
}

impl TryFrom<u8> for Capability { /* 0-4 mapping */ }

/// Result of an audited operation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum Outcome {
    Success = 0,
    Denied = 1,
    Error = 2,
    NotImplemented = 3,
}

impl TryFrom<u8> for Outcome { /* 0-3 mapping */ }

/// An enrolled agent's identity and capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRecord {
    pub agent_id: String,
    pub trust_level: TrustLevel,
    pub capabilities: Vec<Capability>,
    pub allowed_topics: Option<Vec<String>>,
    pub allowed_categories: Option<Vec<String>>,
    pub enrolled_at: u64,
    pub last_seen_at: u64,
    pub active: bool,
}

/// An immutable record of a single MCP request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEvent {
    pub event_id: u64,
    pub timestamp: u64,
    pub session_id: String,
    pub agent_id: String,
    pub operation: String,
    pub target_ids: Vec<u64>,
    pub outcome: Outcome,
    pub detail: String,
}
```

Server crate then re-imports:
```rust
use unimatrix_store::{AgentRecord, TrustLevel, Capability, AuditEvent, Outcome};
```

## registry.rs Rewrite

### Remove: bincode serialize/deserialize

### bootstrap_defaults Rewrite

```rust
pub fn bootstrap_defaults(&self) -> Result<(), ServerError> {
    let now = current_unix_seconds();
    let conn = self.store.lock_conn();
    conn.execute_batch("BEGIN IMMEDIATE")?;

    // Check if system exists
    let exists: bool = conn.query_row(
        "SELECT 1 FROM agent_registry WHERE agent_id = 'system'",
        [], |_| Ok(true),
    ).optional()?.unwrap_or(false);

    if !exists {
        let caps_json = serde_json::to_string(&vec![
            Capability::Read as u8, Capability::Write as u8,
            Capability::Search as u8, Capability::Admin as u8,
            Capability::SessionWrite as u8,
        ])?;
        conn.execute(
            "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
                allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
             VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4, 1)",
            rusqlite::params!["system", TrustLevel::System as u8 as i64, &caps_json, now as i64],
        )?;
    }

    // Similar for "human"
    conn.execute_batch("COMMIT")?;
    Ok(())
}
```

### resolve_or_enroll Rewrite

```rust
pub fn resolve_or_enroll(&self, agent_id: &str) -> Result<AgentRecord, ServerError> {
    let conn = self.store.lock_conn();

    // Try to read existing
    let record = conn.query_row(
        "SELECT agent_id, trust_level, capabilities, allowed_topics,
                allowed_categories, enrolled_at, last_seen_at, active
         FROM agent_registry WHERE agent_id = ?1",
        rusqlite::params![agent_id],
        |row| {
            let caps_json: String = row.get("capabilities")?;
            let caps: Vec<u8> = serde_json::from_str(&caps_json).unwrap_or_default();
            let capabilities: Vec<Capability> = caps.iter()
                .filter_map(|&v| Capability::try_from(v).ok())
                .collect();

            let topics_json: Option<String> = row.get("allowed_topics")?;
            let allowed_topics = topics_json.map(|j| serde_json::from_str(&j).unwrap_or_default());

            let cats_json: Option<String> = row.get("allowed_categories")?;
            let allowed_categories = cats_json.map(|j| serde_json::from_str(&j).unwrap_or_default());

            Ok(AgentRecord {
                agent_id: row.get("agent_id")?,
                trust_level: TrustLevel::try_from(row.get::<_, u8>("trust_level")?)
                    .unwrap_or(TrustLevel::Restricted),
                capabilities,
                allowed_topics,
                allowed_categories,
                enrolled_at: row.get::<_, i64>("enrolled_at")? as u64,
                last_seen_at: row.get::<_, i64>("last_seen_at")? as u64,
                active: row.get::<_, i64>("active")? != 0,
            })
        },
    ).optional()?;

    match record {
        Some(mut r) => {
            // Update last_seen_at
            let now = current_unix_seconds();
            conn.execute(
                "UPDATE agent_registry SET last_seen_at = ?1 WHERE agent_id = ?2",
                rusqlite::params![now as i64, agent_id],
            )?;
            r.last_seen_at = now;
            Ok(r)
        }
        None => {
            // Auto-enroll with Restricted trust
            // ... INSERT with default capabilities ...
        }
    }
}
```

## audit.rs Rewrite

### Remove: bincode serialize/deserialize

### log_event Rewrite

```rust
pub fn log_event(&self, event: AuditEvent) -> Result<(), ServerError> {
    let conn = self.store.lock_conn();
    conn.execute_batch("BEGIN IMMEDIATE")?;

    let current_id = crate::counters::read_counter(&conn, "next_audit_event_id")
        .unwrap_or(1);
    let id = if current_id == 0 { 1 } else { current_id };
    crate::counters::set_counter(&conn, "next_audit_event_id", id + 1)?;

    let target_ids_json = serde_json::to_string(&event.target_ids)?;
    let now = current_unix_seconds();

    conn.execute(
        "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
            operation, target_ids, outcome, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            id as i64,
            now as i64,
            &event.session_id,
            &event.agent_id,
            &event.operation,
            &target_ids_json,
            event.outcome as u8 as i64,
            &event.detail,
        ],
    )?;

    conn.execute_batch("COMMIT")?;
    Ok(())
}
```

### write_in_txn Rewrite

```rust
pub fn write_in_txn(
    &self,
    txn: &SqliteWriteTransaction<'_>,
    event: AuditEvent,
) -> Result<u64, ServerError> {
    let conn = &*txn.guard;

    // Read and increment counter within the existing transaction
    let current_id = crate::counters::read_counter(conn, "next_audit_event_id")
        .unwrap_or(1);
    let id = if current_id == 0 { 1 } else { current_id };
    crate::counters::set_counter(conn, "next_audit_event_id", id + 1)?;

    let target_ids_json = serde_json::to_string(&event.target_ids)?;
    let now = current_unix_seconds();

    conn.execute(
        "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
            operation, target_ids, outcome, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            id as i64, now as i64, &event.session_id, &event.agent_id,
            &event.operation, &target_ids_json, event.outcome as u8 as i64,
            &event.detail,
        ],
    )?;

    Ok(id)
}
```

### write_count_since Rewrite

```rust
pub fn write_count_since(&self, agent_id: &str, since: u64) -> Result<u64, ServerError> {
    let conn = self.store.lock_conn();
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM audit_log
         WHERE agent_id = ?1 AND timestamp >= ?2
         AND operation IN ('context_store', 'context_correct')",
        rusqlite::params![agent_id, since as i64],
        |row| row.get(0),
    )?;
    Ok(count as u64)
}
```

## JSON Serialization Pattern

For capabilities stored as integer array:
```rust
// Write: Vec<Capability> -> Vec<u8> -> JSON string
let cap_ints: Vec<u8> = capabilities.iter().map(|c| *c as u8).collect();
let json = serde_json::to_string(&cap_ints)?;

// Read: JSON string -> Vec<u8> -> Vec<Capability>
let ints: Vec<u8> = serde_json::from_str(&json)?;
let caps: Vec<Capability> = ints.iter()
    .filter_map(|&v| Capability::try_from(v).ok())
    .collect();
```

For allowed_topics/allowed_categories (Option<Vec<String>>):
```rust
// Write: None -> SQL NULL, Some(vec) -> JSON string
let json: Option<String> = allowed_topics.as_ref()
    .map(|v| serde_json::to_string(v))
    .transpose()?;

// Read: SQL NULL -> None, JSON string -> Some(vec)
let json: Option<String> = row.get("allowed_topics")?;
let topics: Option<Vec<String>> = json.map(|j| serde_json::from_str(&j))
    .transpose()?;
```
