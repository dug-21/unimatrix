# Component: AuditEvent + audit.rs (unimatrix-store)

## Purpose

Add four new compliance fields to the `AuditEvent` struct and update the
SQL INSERT and SELECT in `audit.rs` to bind and read them. This component
is the foundation that all other components depend on — it must be
implemented first.

**Files modified:**
- `crates/unimatrix-store/src/schema.rs` — struct definition + `Default` impl
- `crates/unimatrix-store/src/audit.rs` — `log_audit_event` INSERT, `read_audit_event` SELECT

---

## New / Modified Types

### `AuditEvent` struct (schema.rs)

Append four new fields after the existing `detail: String` field.
All four carry `#[serde(default)]` for legacy deserialization compatibility.

```
pub struct AuditEvent {
    // existing 8 fields — unchanged
    pub event_id:   u64,
    pub timestamp:  u64,
    pub session_id: String,
    pub agent_id:   String,
    pub operation:  String,
    pub target_ids: Vec<u64>,
    pub outcome:    Outcome,
    pub detail:     String,

    // vnc-014 / ASS-050 additions — append after detail
    #[serde(default)]
    pub credential_type:   String,   // sentinel: "none" (code), SQL DEFAULT 'none'
    #[serde(default)]
    pub capability_used:   String,   // sentinel: "" (no gate), SQL DEFAULT ''
    #[serde(default)]
    pub agent_attribution: String,   // transport-attested client name, SQL DEFAULT ''
    #[serde(default)]
    pub metadata:          String,   // JSON object string, SQL DEFAULT '{}'
}
```

**serde(default) semantics for these fields:**
`#[serde(default)]` calls `String::default()` which is `""` for all four.
This is intentional for legacy JSON deserialization (pre-migration records
will deserialize with empty strings). The sentinels (`"none"`, `"{}"`) are
code-construction defaults, not serde defaults.

### `Default` impl for `AuditEvent` (schema.rs)

The `Default` impl sets construction-time sentinels. It differs from serde defaults.
Non-tool-call construction sites use `..AuditEvent::default()` struct update syntax.

```
impl Default for AuditEvent {
    fn default() -> Self {
        AuditEvent {
            event_id:          0,
            timestamp:         0,
            session_id:        String::new(),
            agent_id:          String::new(),
            operation:         String::new(),
            target_ids:        Vec::new(),
            outcome:           Outcome::Success,
            detail:            String::new(),
            // vnc-014 sentinels:
            credential_type:   "none".to_string(),
            capability_used:   String::new(),
            agent_attribution: String::new(),
            metadata:          "{}".to_string(),
        }
    }
}
```

---

## Modified Functions

### `SqlxStore::log_audit_event` (audit.rs)

Extend the INSERT to include four new columns as `?9`..`?12`.

```
fn log_audit_event(store, event: AuditEvent) -> Result<u64>:
    pool = store.write_pool_server()
    txn = pool.begin().await?

    current_id = counters::read_counter(txn, "next_audit_event_id").await?
    id = if current_id == 0 { 1 } else { current_id }
    counters::set_counter(txn, "next_audit_event_id", id + 1).await?

    target_ids_json = serde_json::to_string(&event.target_ids)?
    now = current_unix_seconds()

    sqlx::query(
        "INSERT INTO audit_log
            (event_id, timestamp, session_id, agent_id,
             operation, target_ids, outcome, detail,
             credential_type, capability_used, agent_attribution, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
    )
    .bind(id as i64)          // ?1
    .bind(now as i64)         // ?2
    .bind(&event.session_id)  // ?3
    .bind(&event.agent_id)    // ?4
    .bind(&event.operation)   // ?5
    .bind(&target_ids_json)   // ?6
    .bind(event.outcome as u8 as i64)  // ?7
    .bind(&event.detail)      // ?8
    .bind(&event.credential_type)   // ?9
    .bind(&event.capability_used)   // ?10
    .bind(&event.agent_attribution) // ?11
    .bind(&event.metadata)          // ?12
    .execute(&mut *txn).await?

    txn.commit().await?
    return Ok(id)
```

**GUARD**: `event.metadata` must never be empty string. Callers must supply
`"{}"` as the minimum. If defensive hardening is desired, log a WARN and
substitute `"{}"` inside this function when `event.metadata.is_empty()`.

### `SqlxStore::read_audit_event` (audit.rs)

Extend the SELECT projection and struct reconstruction to include four new columns.

```
fn read_audit_event(store, event_id: u64) -> Result<Option<AuditEvent>>:
    row = sqlx::query(
        "SELECT event_id, timestamp, session_id, agent_id, operation,
                target_ids, outcome, detail,
                credential_type, capability_used, agent_attribution, metadata
         FROM audit_log WHERE event_id = ?1"
    )
    .bind(event_id as i64)
    .fetch_optional(store.read_pool()).await?

    match row:
        None -> return Ok(None)
        Some(r):
            target_ids_json: String = r.get("target_ids")
            target_ids: Vec<u64> = serde_json::from_str(&target_ids_json).unwrap_or_default()
            outcome_byte = r.get::<i64, _>("outcome") as u8
            outcome = Outcome::try_from(outcome_byte).unwrap_or(Outcome::Error)
            return Ok(Some(AuditEvent {
                event_id:          r.get::<i64, _>("event_id") as u64,
                timestamp:         r.get::<i64, _>("timestamp") as u64,
                session_id:        r.get("session_id"),
                agent_id:          r.get("agent_id"),
                operation:         r.get("operation"),
                target_ids,
                outcome,
                detail:            r.get("detail"),
                credential_type:   r.get("credential_type"),
                capability_used:   r.get("capability_used"),
                agent_attribution: r.get("agent_attribution"),
                metadata:          r.get("metadata"),
            }))
```

---

## Error Handling

- `log_audit_event`: existing error propagation unchanged. The four new
  `.bind()` calls cannot fail (String values). No new error paths.
- `read_audit_event`: `r.get("credential_type")` etc. use the same
  `sqlx::Row::get` pattern as existing columns. If the column is absent
  (schema mismatch), sqlx panics — the migration must run before the server
  starts, so this is acceptable.

---

## Key Test Scenarios

1. **Round-trip (AC-05)**: `log_audit_event` with all four fields set, then
   `read_audit_event` — confirm each field reads back exactly as stored.
   Cover: `credential_type="none"`, `capability_used="write"`,
   `agent_attribution="codex-mcp-client"`,
   `metadata=r#"{"client_type":"codex-mcp-client"}"#`.

2. **Default sentinel (R-06)**: `AuditEvent::default()` has
   `credential_type="none"`, `capability_used=""`, `agent_attribution=""`,
   `metadata="{}"`. Assert none are empty string for credential_type and metadata.

3. **serde round-trip (R-13)**: Deserialize an 8-field JSON AuditEvent (no
   new fields) — verify four new fields are `""` (serde default, not sentinel).
   Then verify `AuditEvent::default()` gives sentinels. These paths are distinct.

4. **metadata non-empty constraint**: `log_audit_event` with `metadata=""`
   should either WARN+substitute `"{}"` or be a documented caller contract.
   Test that the stored value is never empty.

5. **Column ordering**: After migration to v25, `pragma_table_info('audit_log')`
   confirms the four new columns exist at positions 9..12. Implicit in migration
   tests but stated here for completeness.
