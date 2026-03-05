//! Migration compatibility: bincode deserializers for v5 schema blobs.
//!
//! These functions are used exclusively by migrate_v5_to_v6().
//! They retain the bincode v2 serde path for deserializing data
//! written at schema versions v0-v5.
//!
//! ADR-005: Keep in dedicated module to prevent accidental runtime use.

use crate::error::{Result, StoreError};
use crate::injection_log::InjectionLogRecord;
use crate::schema::{AgentRecord, AuditEvent, CoAccessRecord, EntryRecord};
use crate::sessions::SessionRecord;
use crate::signal::SignalRecord;

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
