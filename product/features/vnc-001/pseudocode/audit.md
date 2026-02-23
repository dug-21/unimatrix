# Pseudocode: audit.rs (C6 — Audit Log)

## Purpose

Append-only request logging using the AUDIT_LOG redb table. Assigns monotonic event IDs via the existing COUNTERS table.

## Types

```
struct AuditLog {
    store: Arc<Store>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditEvent {
    event_id: u64,
    timestamp: u64,         // unix seconds
    session_id: String,
    agent_id: String,
    operation: String,      // tool name: "context_search", "context_store", etc.
    target_ids: Vec<u64>,   // entry IDs affected (empty for search/stubs)
    outcome: Outcome,
    detail: String,         // human-readable detail
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum Outcome {
    Success,
    Denied,
    Error,
    NotImplemented,
}
```

## Serialization

AuditEvent uses bincode v2 serde path:
- `bincode::serde::encode_to_vec(event, bincode::config::standard())`
- `bincode::serde::decode_from_slice::<AuditEvent, _>(bytes, bincode::config::standard())`

## Functions

### AuditLog::new(store: Arc<Store>) -> Self

```
AuditLog { store }
```

### AuditLog::log_event(&self, event: AuditEvent) -> Result<(), ServerError>

```
write_txn = self.store.db.begin_write()?
{
    // Step 1: Get next event ID from COUNTERS
    counters = write_txn.open_table(COUNTERS)?
    current_id = match counters.get("next_audit_id")? {
        Some(guard) => guard.value(),
        None => 1,  // first event ever
    }

    // Step 2: Increment counter
    counters.insert("next_audit_id", current_id + 1)?

    // Step 3: Assign event_id and timestamp
    final_event = AuditEvent {
        event_id: current_id,
        timestamp: current_unix_seconds(),
        ..event
    }

    // Step 4: Serialize and insert into AUDIT_LOG
    audit_table = write_txn.open_table(AUDIT_LOG)?
    bytes = serialize_audit_event(&final_event)?
    audit_table.insert(current_id, bytes.as_slice())?
}
write_txn.commit()?
Ok(())
```

Key design:
- event_id and timestamp are set by `log_event`, not the caller. Caller provides all other fields.
- The counter read + increment + log insert happen in a single write transaction (atomic).
- First event gets ID 1 (COUNTERS["next_audit_id"] starts unset, defaults to 1).

### Helper: serialize/deserialize AuditEvent

```
fn serialize_audit_event(event: &AuditEvent) -> Result<Vec<u8>, ServerError>
fn deserialize_audit_event(bytes: &[u8]) -> Result<AuditEvent, ServerError>
```

Both use bincode serde path. `deserialize_audit_event` is not used in vnc-001 (audit is append-only) but is provided for future read operations and testing.

## Error Handling

- All redb errors mapped to `ServerError::Audit(msg)`
- Audit log write failure should NOT crash tool handlers. The Delivery Leader's main.rs and tool handlers should log a warning and continue if `log_event` fails. This is per NFR-05 and FM-04.

## Key Test Scenarios

1. First event gets event_id = 1
2. 10 rapid events produce strictly increasing IDs (1..=10)
3. Cross-session continuity: close store, reopen, next event ID continues
4. AuditEvent round-trips through bincode
5. All Outcome variants serialize correctly
6. Timestamp is set by log_event (not caller's value)
7. Event with empty target_ids serializes correctly
8. Event with multiple target_ids serializes correctly
