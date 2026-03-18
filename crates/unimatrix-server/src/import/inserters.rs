//! Per-table INSERT functions for the import pipeline.
//!
//! All functions are async and use sqlx parameterized queries.
//! No string interpolation (ADR-002).
//!
//! All functions accept `&mut SqliteConnection` (not `&SqlitePool`) because they
//! execute within a `BEGIN IMMEDIATE` transaction. Using the pool would dispatch
//! each INSERT to a potentially different connection, causing SQLITE_BUSY (code 5)
//! as that second connection cannot acquire a write lock while the first holds it.

use sqlx::sqlite::SqliteConnection;

use crate::format::{
    AgentRegistryRow, AuditLogRow, CoAccessRow, CounterRow, EntryRow, EntryTagRow, FeatureEntryRow,
    OutcomeIndexRow,
};

pub(super) async fn insert_counter(
    conn: &mut SqliteConnection,
    r: &CounterRow,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES (?1, ?2)")
        .bind(&r.name)
        .bind(r.value)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub(super) async fn insert_entry(
    conn: &mut SqliteConnection,
    r: &EntryRow,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
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
    )
    .bind(r.id)
    .bind(&r.title)
    .bind(&r.content)
    .bind(&r.topic)
    .bind(&r.category)
    .bind(&r.source)
    .bind(r.status)
    .bind(r.confidence)
    .bind(r.created_at)
    .bind(r.updated_at)
    .bind(r.last_accessed_at)
    .bind(r.access_count)
    .bind(r.supersedes)
    .bind(r.superseded_by)
    .bind(r.correction_count)
    .bind(r.embedding_dim)
    .bind(&r.created_by)
    .bind(&r.modified_by)
    .bind(&r.content_hash)
    .bind(&r.previous_hash)
    .bind(r.version)
    .bind(&r.feature_cycle)
    .bind(&r.trust_source)
    .bind(r.helpful_count)
    .bind(r.unhelpful_count)
    .bind(r.pre_quarantine_status)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(super) async fn insert_entry_tag(
    conn: &mut SqliteConnection,
    r: &EntryTagRow,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)")
        .bind(r.entry_id)
        .bind(&r.tag)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub(super) async fn insert_co_access(
    conn: &mut SqliteConnection,
    r: &CoAccessRow,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(r.entry_id_a)
    .bind(r.entry_id_b)
    .bind(r.count)
    .bind(r.last_updated)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(super) async fn insert_feature_entry(
    conn: &mut SqliteConnection,
    r: &FeatureEntryRow,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("INSERT INTO feature_entries (feature_id, entry_id) VALUES (?1, ?2)")
        .bind(&r.feature_id)
        .bind(r.entry_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub(super) async fn insert_outcome_index(
    conn: &mut SqliteConnection,
    r: &OutcomeIndexRow,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("INSERT INTO outcome_index (feature_cycle, entry_id) VALUES (?1, ?2)")
        .bind(&r.feature_cycle)
        .bind(r.entry_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub(super) async fn insert_agent_registry(
    conn: &mut SqliteConnection,
    r: &AgentRegistryRow,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        "INSERT INTO agent_registry (
            agent_id, trust_level, capabilities, allowed_topics,
            allowed_categories, enrolled_at, last_seen_at, active
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&r.agent_id)
    .bind(r.trust_level)
    .bind(&r.capabilities)
    .bind(&r.allowed_topics)
    .bind(&r.allowed_categories)
    .bind(r.enrolled_at)
    .bind(r.last_seen_at)
    .bind(r.active)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(super) async fn insert_audit_log(
    conn: &mut SqliteConnection,
    r: &AuditLogRow,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        "INSERT INTO audit_log (
            event_id, timestamp, session_id, agent_id,
            operation, target_ids, outcome, detail
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(r.event_id)
    .bind(r.timestamp)
    .bind(&r.session_id)
    .bind(&r.agent_id)
    .bind(&r.operation)
    .bind(&r.target_ids)
    .bind(r.outcome)
    .bind(&r.detail)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
