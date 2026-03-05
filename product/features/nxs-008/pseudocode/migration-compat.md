# Component: migration-compat (Wave 0)

## File: `crates/unimatrix-store/src/migration_compat.rs`

**Action**: CREATE (~120 lines)
**Risk**: Low
**ADR**: ADR-005

## Purpose

Retain bincode deserializers for v5 blob data in a dedicated module. These are used ONLY by the v5-to-v6 migration. After migration, they become dead code but are retained for any future database recovery needs.

## Pseudocode

```rust
//! Migration compatibility: bincode deserializers for v5 schema blobs.
//!
//! These functions are used exclusively by migrate_v5_to_v6().
//! They retain the bincode v2 serde path for deserializing data
//! written at schema versions v0-v5.
//!
//! ADR-005: Keep in dedicated module to prevent accidental runtime use.

use crate::error::{Result, StoreError};
use crate::schema::{EntryRecord, CoAccessRecord};
use crate::sessions::SessionRecord;
use crate::injection_log::InjectionLogRecord;
use crate::signal::SignalRecord;

// Server-crate types moved to store::schema in Wave 3.
// Until then, we define local compat structs or import from schema.
// After Wave 3: use crate::schema::{AgentRecord, AuditEvent};

/// Deserialize an EntryRecord from v5 bincode blob.
///
/// Uses serde path with standard() config. All #[serde(default)] fields
/// handle entries written at schema versions v0-v5.
pub(crate) fn deserialize_entry_v5(bytes: &[u8]) -> Result<EntryRecord> {
    let (record, _) = bincode::serde::decode_from_slice::<EntryRecord, _>(
        bytes,
        bincode::config::standard(),
    )
    .map_err(|e| StoreError::Deserialization(format!("entry v5: {e}")))?;
    Ok(record)
}

/// Deserialize a CoAccessRecord from v5 bincode blob.
pub(crate) fn deserialize_co_access_v5(bytes: &[u8]) -> Result<CoAccessRecord> {
    let (record, _) = bincode::serde::decode_from_slice::<CoAccessRecord, _>(
        bytes,
        bincode::config::standard(),
    )
    .map_err(|e| StoreError::Deserialization(format!("co_access v5: {e}")))?;
    Ok(record)
}

/// Deserialize a SessionRecord from v5 bincode blob.
pub(crate) fn deserialize_session_v5(bytes: &[u8]) -> Result<SessionRecord> {
    let (record, _) = bincode::serde::decode_from_slice::<SessionRecord, _>(
        bytes,
        bincode::config::standard(),
    )
    .map_err(|e| StoreError::Deserialization(format!("session v5: {e}")))?;
    Ok(record)
}

/// Deserialize an InjectionLogRecord from v5 bincode blob.
pub(crate) fn deserialize_injection_log_v5(bytes: &[u8]) -> Result<InjectionLogRecord> {
    let (record, _) = bincode::serde::decode_from_slice::<InjectionLogRecord, _>(
        bytes,
        bincode::config::standard(),
    )
    .map_err(|e| StoreError::Deserialization(format!("injection_log v5: {e}")))?;
    Ok(record)
}

/// Deserialize a SignalRecord from v5 bincode blob.
pub(crate) fn deserialize_signal_v5(bytes: &[u8]) -> Result<SignalRecord> {
    let (record, _) = bincode::serde::decode_from_slice::<SignalRecord, _>(
        bytes,
        bincode::config::standard(),
    )
    .map_err(|e| StoreError::Deserialization(format!("signal v5: {e}")))?;
    Ok(record)
}

/// Deserialize an AgentRecord from v5 bincode blob.
///
/// AgentRecord type must be accessible from store crate (moved in Wave 3).
/// During Wave 0, we use a local struct that mirrors the server's AgentRecord.
pub(crate) fn deserialize_agent_v5(bytes: &[u8]) -> Result<AgentRecord> {
    let (record, _) = bincode::serde::decode_from_slice::<AgentRecord, _>(
        bytes,
        bincode::config::standard(),
    )
    .map_err(|e| StoreError::Deserialization(format!("agent v5: {e}")))?;
    Ok(record)
}

/// Deserialize an AuditEvent from v5 bincode blob.
pub(crate) fn deserialize_audit_event_v5(bytes: &[u8]) -> Result<AuditEvent> {
    let (record, _) = bincode::serde::decode_from_slice::<AuditEvent, _>(
        bytes,
        bincode::config::standard(),
    )
    .map_err(|e| StoreError::Deserialization(format!("audit v5: {e}")))?;
    Ok(record)
}
```

## Implementation Note

The AgentRecord and AuditEvent types are currently defined in unimatrix-server. For migration_compat to use them, one of two approaches:

1. **Move types first** (recommended, part of Wave 0): Move `AgentRecord`, `TrustLevel`, `Capability`, `AuditEvent`, `Outcome` structs and their Serialize/Deserialize derives to `crate::schema`. Server re-imports them. This makes migration_compat straightforward.

2. **Duplicate struct definitions**: Define identical structs in migration_compat with bincode serde derives. Risk of divergence but decouples waves.

Decision: Use approach 1 -- move types to schema.rs in Wave 0 (they're pure data types, not server logic). This is consistent with ADR-008 (wave ordering) and the IMPLEMENTATION-BRIEF which lists type movement.

## Changes to lib.rs (Wave 0)

```rust
mod migration_compat;
```
