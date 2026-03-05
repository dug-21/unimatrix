# ADR-005: Migration Compatibility Module for Bincode Deserializers

**Status**: Accepted
**Context**: nxs-008
**Mitigates**: SR-01 (Migration Data Fidelity)

## Decision

Create a `migration_compat.rs` module in `unimatrix-store` that retains all bincode deserializers needed for the v5-to-v6 migration. This module is compiled but not used at runtime after migration completes.

### Problem

The v5-to-v6 migration must deserialize every existing bincode blob to extract field values for SQL column insertion. But nxs-008 also removes the bincode serialization infrastructure from the runtime paths. If deserializers are removed before migration code is written and tested, the migration cannot read old data.

### Design

1. `migration_compat.rs` contains:
   - `deserialize_entry_v5(bytes: &[u8]) -> Result<EntryRecord>` — re-export of current `deserialize_entry`
   - `deserialize_co_access_v5(bytes: &[u8]) -> Result<CoAccessRecord>`
   - `deserialize_session_v5(bytes: &[u8]) -> Result<SessionRecord>`
   - `deserialize_injection_log_v5(bytes: &[u8]) -> Result<InjectionLogRecord>`
   - `deserialize_signal_v5(bytes: &[u8]) -> Result<SignalRecord>`
   - `deserialize_agent_v5(bytes: &[u8]) -> Result<AgentRecord>` (moved from server crate)
   - `deserialize_audit_event_v5(bytes: &[u8]) -> Result<AuditEvent>` (moved from server crate)

2. The module is created in **Wave 1** before any runtime bincode removal begins

3. `migration.rs` imports only from `migration_compat`, never from runtime modules

4. After migration runs on a database, the compat module is dead code until the next schema migration. It stays compiled to prevent bitrot.

5. Server-crate deserializers (`deserialize_agent`, `deserialize_audit_event`) are duplicated into `migration_compat.rs` in the store crate. The server-crate types (`AgentRecord`, `AuditEvent`) must be accessible to the store crate for migration. This requires either:
   - **(a)** Moving `AgentRecord` and `AuditEvent` structs to the store crate (preferred — they are data types, not server logic), or
   - **(b)** Using serde_json intermediate format: deserialize blob to `serde_json::Value`, re-serialize as SQL columns

   Option (a) is recommended. The structs can remain in server for runtime use via re-export.

### Migration Sequence

```
Wave 0 (prep):
  1. Create migration_compat.rs with all deserializers
  2. Write v5-to-v6 migration code in migration.rs using migration_compat
  3. Write round-trip migration tests

Wave 1-3 (normalization):
  4. Remove runtime bincode from each table's implementation
  5. migration_compat is untouched

Wave 4 (cleanup):
  6. Verify migration tests still pass
  7. migration_compat stays (dead code outside of migration path)
```

### Automatic Backup

The v5-to-v6 migration copies the database file before starting:
```rust
let backup_path = db_path.with_extension("db.v5-backup");
std::fs::copy(&db_path, &backup_path)?;
```

This provides a rollback path despite the migration being a one-way door.

## Consequences

- Migration code can be written and tested before bincode removal
- Bincode dependency remains in Cargo.toml (needed by migration_compat and OBSERVATION_METRICS)
- ~200 lines of deserializer code retained as migration infrastructure
- `AgentRecord` and `AuditEvent` types may move to store crate
