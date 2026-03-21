//! Migration compatibility: bincode deserializers for historical schema blobs.
//!
//! v5 deserializers used by migrate_v5_to_v6().
//! v8 deserializer used by migrate_v8_to_v9() (nxs-009).
//! They retain the bincode v2 serde path for deserializing data
//! written at prior schema versions.
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
    let (record, _) =
        bincode::serde::decode_from_slice::<EntryRecord, _>(bytes, bincode::config::standard())
            .map_err(|e| StoreError::Deserialization(format!("entry v5: {e}")))?;
    Ok(record)
}

/// Deserialize a CoAccessRecord from v5 bincode blob.
pub(crate) fn deserialize_co_access_v5(bytes: &[u8]) -> Result<CoAccessRecord> {
    let (record, _) =
        bincode::serde::decode_from_slice::<CoAccessRecord, _>(bytes, bincode::config::standard())
            .map_err(|e| StoreError::Deserialization(format!("co_access v5: {e}")))?;
    Ok(record)
}

/// Deserialize a SessionRecord from v5 bincode blob.
pub(crate) fn deserialize_session_v5(bytes: &[u8]) -> Result<SessionRecord> {
    let (record, _) =
        bincode::serde::decode_from_slice::<SessionRecord, _>(bytes, bincode::config::standard())
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
    let (record, _) =
        bincode::serde::decode_from_slice::<SignalRecord, _>(bytes, bincode::config::standard())
            .map_err(|e| StoreError::Deserialization(format!("signal v5: {e}")))?;
    Ok(record)
}

/// Deserialize an AgentRecord from v5 bincode blob.
pub(crate) fn deserialize_agent_v5(bytes: &[u8]) -> Result<AgentRecord> {
    let (record, _) =
        bincode::serde::decode_from_slice::<AgentRecord, _>(bytes, bincode::config::standard())
            .map_err(|e| StoreError::Deserialization(format!("agent v5: {e}")))?;
    Ok(record)
}

/// Deserialize an AuditEvent from v5 bincode blob.
pub(crate) fn deserialize_audit_event_v5(bytes: &[u8]) -> Result<AuditEvent> {
    let (record, _) =
        bincode::serde::decode_from_slice::<AuditEvent, _>(bytes, bincode::config::standard())
            .map_err(|e| StoreError::Deserialization(format!("audit v5: {e}")))?;
    Ok(record)
}

// === v8 MetricVector deserializer (nxs-009, ADR-002) ===
//
// Self-contained snapshot of the v8 MetricVector format.
// These structs are frozen and must not track changes to the live types.

use serde::Deserialize as SerdeDeserialize;
use std::collections::BTreeMap;

#[derive(SerdeDeserialize)]
struct MetricVectorV8 {
    #[serde(default)]
    computed_at: u64,
    #[serde(default)]
    universal: UniversalMetricsV8,
    #[serde(default)]
    phases: BTreeMap<String, PhaseMetricsV8>,
}

#[derive(SerdeDeserialize, Default)]
struct UniversalMetricsV8 {
    #[serde(default)]
    total_tool_calls: u64,
    #[serde(default)]
    total_duration_secs: u64,
    #[serde(default)]
    session_count: u64,
    #[serde(default)]
    search_miss_rate: f64,
    #[serde(default)]
    edit_bloat_total_kb: f64,
    #[serde(default)]
    edit_bloat_ratio: f64,
    #[serde(default)]
    permission_friction_events: u64,
    #[serde(default)]
    bash_for_search_count: u64,
    #[serde(default)]
    cold_restart_events: u64,
    #[serde(default)]
    coordinator_respawn_count: u64,
    #[serde(default)]
    parallel_call_rate: f64,
    #[serde(default)]
    context_load_before_first_write_kb: f64,
    #[serde(default)]
    total_context_loaded_kb: f64,
    #[serde(default)]
    post_completion_work_pct: f64,
    #[serde(default)]
    follow_up_issues_created: u64,
    #[serde(default)]
    knowledge_entries_stored: u64,
    #[serde(default)]
    sleep_workaround_count: u64,
    #[serde(default)]
    agent_hotspot_count: u64,
    #[serde(default)]
    friction_hotspot_count: u64,
    #[serde(default)]
    session_hotspot_count: u64,
    #[serde(default)]
    scope_hotspot_count: u64,
}

#[derive(SerdeDeserialize, Default)]
struct PhaseMetricsV8 {
    #[serde(default)]
    duration_secs: u64,
    #[serde(default)]
    tool_call_count: u64,
}

/// Deserialize a MetricVector from a v8 bincode blob.
///
/// Uses `bincode::config::standard()` matching the production serializer
/// in `unimatrix-observe::serialize_metric_vector()` (R-05).
pub(crate) fn deserialize_metric_vector_v8(bytes: &[u8]) -> Result<crate::metrics::MetricVector> {
    let (v8, _) =
        bincode::serde::decode_from_slice::<MetricVectorV8, _>(bytes, bincode::config::standard())
            .map_err(|e| StoreError::Deserialization(format!("metric_vector v8: {e}")))?;

    let phases: BTreeMap<String, crate::metrics::PhaseMetrics> = v8
        .phases
        .into_iter()
        .map(|(name, p)| {
            (
                name,
                crate::metrics::PhaseMetrics {
                    duration_secs: p.duration_secs,
                    tool_call_count: p.tool_call_count,
                },
            )
        })
        .collect();

    Ok(crate::metrics::MetricVector {
        computed_at: v8.computed_at,
        universal: crate::metrics::UniversalMetrics {
            total_tool_calls: v8.universal.total_tool_calls,
            total_duration_secs: v8.universal.total_duration_secs,
            session_count: v8.universal.session_count,
            search_miss_rate: v8.universal.search_miss_rate,
            edit_bloat_total_kb: v8.universal.edit_bloat_total_kb,
            edit_bloat_ratio: v8.universal.edit_bloat_ratio,
            permission_friction_events: v8.universal.permission_friction_events,
            bash_for_search_count: v8.universal.bash_for_search_count,
            cold_restart_events: v8.universal.cold_restart_events,
            coordinator_respawn_count: v8.universal.coordinator_respawn_count,
            parallel_call_rate: v8.universal.parallel_call_rate,
            context_load_before_first_write_kb: v8.universal.context_load_before_first_write_kb,
            total_context_loaded_kb: v8.universal.total_context_loaded_kb,
            post_completion_work_pct: v8.universal.post_completion_work_pct,
            follow_up_issues_created: v8.universal.follow_up_issues_created,
            knowledge_entries_stored: v8.universal.knowledge_entries_stored,
            sleep_workaround_count: v8.universal.sleep_workaround_count,
            agent_hotspot_count: v8.universal.agent_hotspot_count,
            friction_hotspot_count: v8.universal.friction_hotspot_count,
            session_hotspot_count: v8.universal.session_hotspot_count,
            scope_hotspot_count: v8.universal.scope_hotspot_count,
        },
        phases,
        domain_metrics: std::collections::HashMap::new(),
    })
}
