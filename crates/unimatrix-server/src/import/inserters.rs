//! Per-table INSERT functions for the import pipeline.
//!
//! All functions use `rusqlite::params![]` for parameterized queries.
//! No string interpolation (ADR-002).

use unimatrix_store::rusqlite::{Connection, params};

use crate::format::{
    AgentRegistryRow, AuditLogRow, CoAccessRow, CounterRow, EntryRow, EntryTagRow, FeatureEntryRow,
    OutcomeIndexRow,
};

pub(super) fn insert_counter(
    conn: &Connection,
    r: &CounterRow,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT OR REPLACE INTO counters (name, value) VALUES (?1, ?2)",
        params![r.name, r.value],
    )?;
    Ok(())
}

pub(super) fn insert_entry(
    conn: &Connection,
    r: &EntryRow,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT INTO entries (
            id, title, content, topic, category, source, status, confidence,
            created_at, updated_at, last_accessed_at, access_count,
            supersedes, superseded_by, correction_count, embedding_dim,
            created_by, modified_by, content_hash, previous_hash,
            version, feature_cycle, trust_source,
            helpful_count, unhelpful_count, pre_quarantine_status
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
            ?9, ?10, ?11, ?12,
            ?13, ?14, ?15, ?16,
            ?17, ?18, ?19, ?20,
            ?21, ?22, ?23,
            ?24, ?25, ?26
        )",
        params![
            r.id,
            r.title,
            r.content,
            r.topic,
            r.category,
            r.source,
            r.status,
            r.confidence,
            r.created_at,
            r.updated_at,
            r.last_accessed_at,
            r.access_count,
            r.supersedes,
            r.superseded_by,
            r.correction_count,
            r.embedding_dim,
            r.created_by,
            r.modified_by,
            r.content_hash,
            r.previous_hash,
            r.version,
            r.feature_cycle,
            r.trust_source,
            r.helpful_count,
            r.unhelpful_count,
            r.pre_quarantine_status,
        ],
    )?;
    Ok(())
}

pub(super) fn insert_entry_tag(
    conn: &Connection,
    r: &EntryTagRow,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
        params![r.entry_id, r.tag],
    )?;
    Ok(())
}

pub(super) fn insert_co_access(
    conn: &Connection,
    r: &CoAccessRow,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (?1, ?2, ?3, ?4)",
        params![r.entry_id_a, r.entry_id_b, r.count, r.last_updated],
    )?;
    Ok(())
}

pub(super) fn insert_feature_entry(
    conn: &Connection,
    r: &FeatureEntryRow,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT INTO feature_entries (feature_id, entry_id) VALUES (?1, ?2)",
        params![r.feature_id, r.entry_id],
    )?;
    Ok(())
}

pub(super) fn insert_outcome_index(
    conn: &Connection,
    r: &OutcomeIndexRow,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT INTO outcome_index (feature_cycle, entry_id) VALUES (?1, ?2)",
        params![r.feature_cycle, r.entry_id],
    )?;
    Ok(())
}

pub(super) fn insert_agent_registry(
    conn: &Connection,
    r: &AgentRegistryRow,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT INTO agent_registry (
            agent_id, trust_level, capabilities, allowed_topics,
            allowed_categories, enrolled_at, last_seen_at, active
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            r.agent_id,
            r.trust_level,
            r.capabilities,
            r.allowed_topics,
            r.allowed_categories,
            r.enrolled_at,
            r.last_seen_at,
            r.active,
        ],
    )?;
    Ok(())
}

pub(super) fn insert_audit_log(
    conn: &Connection,
    r: &AuditLogRow,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT INTO audit_log (
            event_id, timestamp, session_id, agent_id,
            operation, target_ids, outcome, detail
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            r.event_id,
            r.timestamp,
            r.session_id,
            r.agent_id,
            r.operation,
            r.target_ids,
            r.outcome,
            r.detail,
        ],
    )?;
    Ok(())
}
