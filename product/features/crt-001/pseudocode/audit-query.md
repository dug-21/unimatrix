# Pseudocode: C7 Audit Log Query

## File: crates/unimatrix-server/src/audit.rs

### Promote deserialize_audit_event

Change from `#[cfg(test)]` to `pub(crate)`:

```
/// Deserialize an AuditEvent from bincode bytes.
pub(crate) fn deserialize_audit_event(bytes: &[u8]) -> Result<AuditEvent, ServerError> {
    let (event, _) =
        bincode::serde::decode_from_slice::<AuditEvent, _>(bytes, bincode::config::standard())
            .map_err(|e| ServerError::Audit(format!("deserialization failed: {e}")))?;
    Ok(event)
}
```

### AuditLog::write_count_since

```
impl AuditLog {
    /// Count write operations by a specific agent since a given timestamp.
    ///
    /// Scans AUDIT_LOG for entries where `agent_id` matches and `operation`
    /// is a write tool (context_store, context_correct) with `timestamp >= since`.
    /// Returns the count.
    pub fn write_count_since(&self, agent_id: &str, since: u64) -> Result<u64, ServerError> {
        let txn = self.store.begin_read()
            .map_err(|e| ServerError::Audit(e.to_string()))?;
        let table = txn.open_table(AUDIT_LOG)
            .map_err(|e| ServerError::Audit(e.to_string()))?;

        let mut count = 0u64;

        // Iterate in reverse (newest first) for efficiency
        // Stop when we hit events older than `since`
        for result in table.iter().map_err(|e| ServerError::Audit(e.to_string()))? {
            let (_, value) = result.map_err(|e| ServerError::Audit(e.to_string()))?;
            let event = deserialize_audit_event(value.value())?;

            // Skip events before the time window
            // Note: events are ordered by event_id, not necessarily by timestamp,
            // but in practice they're monotonically increasing.
            // We cannot break early because event ordering is by ID not timestamp.
            // Full scan is acceptable for current scale.
            if event.timestamp < since {
                continue;
            }

            // Check agent_id match
            if event.agent_id != agent_id {
                continue;
            }

            // Check if it's a write operation
            if is_write_operation(&event.operation) {
                count += 1;
            }
        }

        Ok(count)
    }
}

/// Check if an operation name is a write operation.
fn is_write_operation(operation: &str) -> bool {
    matches!(operation, "context_store" | "context_correct")
}
```

Note on scan direction: ADR-005 specifies reverse scan for efficiency, but redb table iteration is forward by default. Since events are keyed by monotonic ID and timestamps are monotonically increasing in practice, we could use reverse iteration. However, for correctness (no assumption about timestamp monotonicity), a full forward scan is simpler and correct for the current scale. If performance becomes an issue, reverse iteration with early termination can be added.

For the current implementation: forward scan, no early termination, O(N) where N = total audit events. At hundreds to low thousands of events per session, this is fast enough.
