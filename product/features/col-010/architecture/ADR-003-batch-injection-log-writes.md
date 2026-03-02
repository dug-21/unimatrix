# ADR-003: Batch INJECTION_LOG Writes Per ContextSearch Response

**Feature**: col-010
**Status**: Accepted
**Date**: 2026-03-02

## Context

SR-12 identified counter contention risk: if each `InjectionLogRecord` insert allocates a counter increment in its own `WriteTransaction`, a `ContextSearch` response injecting 5 entries produces 5 separate write transactions, each incrementing `next_log_id` in COUNTERS. Under concurrent multi-agent sessions (e.g., 5 agents each injecting at the same time), this serializes 25 write transactions through redb's single-writer model, producing measurable latency spikes.

## Decision

`insert_injection_log_batch(records: &[InjectionLogRecord]) -> Result<()>` allocates a contiguous range of IDs and writes all records in a **single `WriteTransaction`**:

```rust
pub fn insert_injection_log_batch(&self, records: &[InjectionLogRecord]) -> Result<()> {
    if records.is_empty() {
        return Ok(());
    }
    let txn = self.db.begin_write()?;
    {
        let mut counters = txn.open_table(COUNTERS)?;
        let mut log_table = txn.open_table(INJECTION_LOG)?;

        // Allocate ID range atomically
        let start_id: u64 = counters
            .get("next_log_id")?
            .map(|g| g.value())
            .unwrap_or(0);
        let next_id = start_id + records.len() as u64;
        counters.insert("next_log_id", next_id)?;

        // Write all records
        for (i, record) in records.iter().enumerate() {
            let log_id = start_id + i as u64;
            let bytes = serialize_injection_log_record(&InjectionLogRecord { log_id, ..record.clone() })?;
            log_table.insert(log_id, bytes.as_slice())?;
        }
    }
    txn.commit()?;
    Ok(())
}
```

This reduces write transactions per `ContextSearch` response from N to 1.

## Rationale

Batch semantics match the injection event's natural granularity: all entries injected in response to a single query arrive together and belong to the same logical event. Writing them as a batch is both correct and efficient.

redb's single-writer model serializes all write transactions. Reducing the write count from N (per entry) to 1 (per response) directly reduces contention under concurrent sessions.

The batch function is the only public write API for INJECTION_LOG. There is no single-record `insert_injection_log` — this prevents callers from accidentally regressing to per-record writes.

## Consequences

- 1 write transaction per `ContextSearch` response regardless of how many entries are injected (1–5 in typical operation).
- `log_id` values for a batch are allocated as a contiguous range (`start_id` to `start_id + n - 1`). Not a problem for `scan_injection_log_by_session` which filters by `session_id` field, not by `log_id` range.
- Records within a batch share the same `timestamp` (the unix timestamp at batch write time). Sub-second ordering within a single response is not needed.
