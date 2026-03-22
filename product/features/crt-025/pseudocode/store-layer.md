# Component 6: Store Layer
## Files: `crates/unimatrix-store/src/analytics.rs`, `write_ext.rs`, `db.rs`

---

## Purpose

Three parallel changes in `unimatrix-store`:

1. **`analytics.rs`**: Add `phase: Option<String>` field to `AnalyticsWrite::FeatureEntry` variant; update drain handler INSERT to write the `phase` column.
2. **`write_ext.rs`**: Change `record_feature_entries` to accept `phase: Option<&str>`; update INSERT to include `phase`.
3. **`db.rs`**: Add `SqlxStore::insert_cycle_event` method for direct write-pool inserts into `CYCLE_EVENTS`.

---

## 6a: `analytics.rs` — `AnalyticsWrite::FeatureEntry` variant

### Modified Enum Variant

```
// BEFORE:
FeatureEntry { feature_id: String, entry_id: u64 },

// AFTER:
FeatureEntry {
    feature_id: String,
    entry_id:   u64,
    phase:      Option<String>,   // NEW: snapshot at enqueue time (ADR-001)
},
```

The variant is `#[non_exhaustive]`. This is a **struct variant** not a tuple variant, so external crate catch-all arms `_ => {}` are unaffected. Internal crate match arms on `FeatureEntry` MUST be updated to destructure `phase` explicitly (C-12).

### Modified: `execute_analytics_write` match arm for `FeatureEntry`

```
// BEFORE:
AnalyticsWrite::FeatureEntry { feature_id, entry_id } => {
    sqlx::query(
        "INSERT OR IGNORE INTO feature_entries (feature_id, entry_id) VALUES (?1, ?2)",
    )
    .bind(feature_id)
    .bind(entry_id as i64)
    .execute(&mut **txn)
    .await?;
}

// AFTER:
AnalyticsWrite::FeatureEntry { feature_id, entry_id, phase } => {   // phase destructured
    sqlx::query(
        "INSERT OR IGNORE INTO feature_entries (feature_id, entry_id, phase) VALUES (?1, ?2, ?3)",
    )
    .bind(feature_id)
    .bind(entry_id as i64)
    .bind(phase)                // Option<String> → sqlx encodes as NULL when None
    .execute(&mut **txn)
    .await?;
}
```

### Modified: `variant_name` match arm

```
// variant_name() does not need to change (uses `..` pattern which is always fine for #[non_exhaustive]):
AnalyticsWrite::FeatureEntry { .. } => "FeatureEntry",
```

---

## 6b: `write_ext.rs` — `record_feature_entries` signature change

### Modified Signature

```
// BEFORE:
pub async fn record_feature_entries(
    &self,
    feature_cycle: &str,
    entry_ids:     &[u64],
) -> Result<()>

// AFTER:
pub async fn record_feature_entries(
    &self,
    feature_cycle: &str,
    entry_ids:     &[u64],
    phase:         Option<&str>,   // NEW: phase active at call time
) -> Result<()>
```

### Modified Body

```
FUNCTION record_feature_entries(feature_cycle, entry_ids, phase):
    FOR &entry_id IN entry_ids:
        sqlx::query(
            "INSERT OR IGNORE INTO feature_entries (feature_id, entry_id, phase) VALUES (?1, ?2, ?3)"
        )
        .bind(feature_cycle)
        .bind(entry_id as i64)
        .bind(phase)              // &str or NULL — sqlx handles Option<&str>
        .execute(&self.write_pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?
    return Ok(())
```

Note: This is a **breaking change** at all call sites. All callers must add the `phase` argument. Known call sites:
- `crates/unimatrix-server/src/services/usage.rs` (two locations: `record_mcp_usage`, `record_hook_injection`)
- `crates/unimatrix-server/src/server.rs` (one location, if it calls directly)
- Test fixtures that call `record_feature_entries` directly

See component 8 (context-store-phase-capture) for the server-side call site update.

---

## 6c: `db.rs` — `insert_cycle_event` new method

This is a direct write pool method (not analytics drain) per ADR-003.

### New Method Signature

```
impl SqlxStore:

    /// Insert one row into cycle_events.
    ///
    /// Direct write pool call (not analytics drain). CYCLE_EVENTS is an
    /// append-only audit table; silent queue shedding of audit rows is
    /// unacceptable (ADR-003).
    ///
    /// Called fire-and-forget from UDS listener. Errors are logged by the
    /// caller, not propagated to the MCP response.
    pub async fn insert_cycle_event(
        &self,
        cycle_id:   &str,
        seq:        i64,
        event_type: &str,
        phase:      Option<&str>,
        outcome:    Option<&str>,
        next_phase: Option<&str>,
        timestamp:  i64,
    ) -> Result<()>
```

### Method Body

```
FUNCTION insert_cycle_event(cycle_id, seq, event_type, phase, outcome, next_phase, timestamp):
    sqlx::query(
        "INSERT INTO cycle_events
            (cycle_id, seq, event_type, phase, outcome, next_phase, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
    )
    .bind(cycle_id)
    .bind(seq)
    .bind(event_type)
    .bind(phase)         // Option<&str> → NULL when None
    .bind(outcome)
    .bind(next_phase)
    .bind(timestamp)
    .execute(&self.write_pool)
    .await
    .map_err(|e| StoreError::Database(e.into()))?

    return Ok(())
```

No transaction wrapping. Single-row INSERT. The `id` column is AUTOINCREMENT — no explicit binding needed.

---

## Data Flow Summary

```
context_store call:
    MCP handler snapshots phase from SessionState.current_phase
    → passes to record_feature_entries(feature_cycle, ids, Some("implementation"))
    → INSERT feature_entries (feature_id="crt-025", entry_id=N, phase="implementation")

context_store via analytics drain:
    AnalyticsWrite::FeatureEntry { feature_id="crt-025", entry_id=N, phase: Some("implementation") }
    → execute_analytics_write: INSERT feature_entries (phase="implementation")
    Phase captured at enqueue time, not re-read from SessionState at drain time.

context_cycle hook event:
    UDS listener: insert_cycle_event("crt-025", 1, "cycle_phase_end", "design", None, "implementation", ts)
    → INSERT cycle_events (cycle_id, seq, event_type, phase, outcome, next_phase, timestamp)
```

---

## Error Handling

| Function | Error Type | On Failure |
|----------|-----------|------------|
| `insert_cycle_event` | `StoreError::Database` | Caller logs warn, discards; tool call unaffected |
| `record_feature_entries` | `StoreError::Database` | Propagated to caller (existing behavior) |
| `FeatureEntry` drain arm | `sqlx::Error` | Batch rolled back (existing drain error behavior) |

---

## Key Test Scenarios

1. `insert_cycle_event("crt-025", 0, "cycle_start", None, None, Some("scope"), ts)` → row inserted; `phase IS NULL`
2. `insert_cycle_event("crt-025", 1, "cycle_phase_end", Some("scope"), Some("no issues"), Some("design"), ts)` → all columns populated
3. Three sequential inserts for same `cycle_id` → rows with seq 0, 1, 2 (advisory; test single-session case)
4. `record_feature_entries("crt-025", [1, 2], Some("implementation"))` → both rows have `phase="implementation"`
5. `record_feature_entries("crt-025", [3], None)` → row has `phase IS NULL`
6. Drain path: `FeatureEntry { feature_id: "f", entry_id: 4, phase: Some("scope") }` → `feature_entries.phase="scope"`
7. Drain path: `FeatureEntry { feature_id: "f", entry_id: 5, phase: None }` → `feature_entries.phase IS NULL`
8. `insert_cycle_event` on closed DB → returns Err, caller logs and discards
