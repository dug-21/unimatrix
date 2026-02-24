## ADR-005: Audit Log Full Scan for Rate Tracking

### Context

crt-001 adds a `write_count_since(agent_id, since)` query method on AuditLog for write rate tracking. The AUDIT_LOG table is keyed by monotonic `event_id`, not by `agent_id` or `timestamp`. To count writes for a specific agent in a time window, the method must scan events.

Options:
1. Full table scan: iterate all AUDIT_LOG entries, deserialize each, check agent_id + operation + timestamp.
2. Secondary index: add a new table `AGENT_AUDIT_INDEX: (&str, u64) -> ()` keyed by (agent_id, event_id) for efficient agent-specific queries.
3. Reverse scan: iterate from the latest event backwards until timestamp < since, checking agent_id. This is efficient if the time window is recent.

### Decision

Use Option 3: reverse scan from the latest event. The AUDIT_LOG is ordered by monotonic event_id, which correlates with timestamp (events are inserted in time order). Scanning backwards from the latest event and stopping when `timestamp < since` gives us the correct result without scanning the entire history.

Implementation:
```rust
pub fn write_count_since(&self, agent_id: &str, since: u64) -> Result<u64, ServerError> {
    let txn = self.store.begin_read()?;
    let table = txn.open_table(AUDIT_LOG)?;
    let mut count = 0u64;
    // Iterate in reverse (highest event_id first)
    for result in table.iter()?.rev() {
        let (_, value) = result?;
        let event = deserialize_audit_event(value.value())?;
        if event.timestamp < since {
            break; // All remaining events are older
        }
        if event.agent_id == agent_id && is_write_operation(&event.operation) {
            count += 1;
        }
    }
    Ok(count)
}
```

`is_write_operation` checks for `"context_store"` and `"context_correct"` operation strings.

### Consequences

- **No secondary index needed.** Saves a table and the write overhead of maintaining an index on every audit event.
- **Efficient for recent windows.** Rate limiting queries typically look at the last N minutes/hours. The reverse scan stops early because recent events are at the end.
- **Degrades on long windows.** If `since` is very old (e.g., beginning of time), the scan reads the entire AUDIT_LOG. This is acceptable because rate limiting uses short windows, and `context_status` (which might want historical data) is already expected to be slow.
- **Deserialization cost.** Each event must be deserialized to check agent_id and timestamp. At hundreds of events per session, this is negligible (<1ms). At millions, it would need a secondary index -- but Unimatrix won't reach millions of audit events in normal operation.
- **`deserialize_audit_event` must be pub(crate).** Currently `#[cfg(test)]` only. crt-001 promotes it to `pub(crate)` for use in `write_count_since`.
