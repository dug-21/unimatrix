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
    AgentRegistryRow, AuditLogRow, CoAccessRow, CounterRow, CycleEventRow, EntryRow, EntryTagRow,
    FeatureEntryRow, GraphEdgeRow, ObservationRow, OutcomeIndexRow,
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

/// Insert a graph edge row (ADR-005: plain INSERT, no id column).
///
/// Uses plain `INSERT INTO` (not `INSERT OR IGNORE`) so that duplicate
/// (source_id, target_id, relation_type) tuples surface as UNIQUE
/// constraint violations — serving as data corruption detection.
/// The synthetic AUTOINCREMENT `id` is omitted; SQLite assigns fresh values.
pub(super) async fn insert_graph_edge(
    conn: &mut SqliteConnection,
    r: &GraphEdgeRow,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        "INSERT INTO graph_edges (
            source_id, target_id, relation_type, weight,
            created_at, created_by, source, bootstrap_only, metadata
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(r.source_id)
    .bind(r.target_id)
    .bind(&r.relation_type)
    .bind(r.weight)
    .bind(r.created_at)
    .bind(&r.created_by)
    .bind(&r.source)
    .bind(r.bootstrap_only)
    .bind(&r.metadata)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Insert an observation row with explicit id binding (ADR-006).
///
/// `id` is preserved through export/import for watermark/ordering significance.
/// Plain INSERT surfaces duplicate id as PRIMARY KEY violation.
pub(super) async fn insert_observation(
    conn: &mut SqliteConnection,
    r: &ObservationRow,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        "INSERT INTO observations (
            id, session_id, ts_millis, hook, tool, input,
            response_size, response_snippet, topic_signal, phase
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(r.id)
    .bind(&r.session_id)
    .bind(r.ts_millis)
    .bind(&r.hook)
    .bind(&r.tool)
    .bind(&r.input)
    .bind(r.response_size)
    .bind(&r.response_snippet)
    .bind(&r.topic_signal)
    .bind(&r.phase)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Insert a cycle event row with explicit id binding (ADR-006) and NULL goal_embedding (ADR-004).
///
/// `id` is preserved through export/import for sequencing significance.
/// `goal_embedding` is excluded from export; the INSERT includes it in the column
/// list but binds literal NULL in the VALUES clause. 9 bind parameters + 1 NULL = 10 values.
pub(super) async fn insert_cycle_event(
    conn: &mut SqliteConnection,
    r: &CycleEventRow,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        "INSERT INTO cycle_events (
            id, cycle_id, seq, event_type, phase, outcome,
            next_phase, timestamp, goal, goal_embedding
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
    )
    .bind(r.id)
    .bind(&r.cycle_id)
    .bind(r.seq)
    .bind(&r.event_type)
    .bind(&r.phase)
    .bind(&r.outcome)
    .bind(&r.next_phase)
    .bind(r.timestamp)
    .bind(&r.goal)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{CycleEventRow, GraphEdgeRow, ObservationRow};
    use sqlx::Row;
    use unimatrix_store::test_helpers::open_test_store;

    // --- Helper: acquire a connection from a test store ---

    /// Create a test store and return the pool for connection acquisition.
    async fn test_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        let pool = store.write_pool_test().clone();
        (pool, dir)
    }

    fn make_graph_edge_row(metadata: Option<String>) -> GraphEdgeRow {
        GraphEdgeRow {
            source_id: 1,
            target_id: 2,
            relation_type: "Supports".to_string(),
            weight: 0.85,
            created_at: 1700000000,
            created_by: "agent-x".to_string(),
            source: "runtime".to_string(),
            bootstrap_only: 0,
            metadata,
        }
    }

    fn make_observation_row(id: i64) -> ObservationRow {
        ObservationRow {
            id,
            session_id: "sess-001".to_string(),
            ts_millis: 1700000000,
            hook: "on_tool".to_string(),
            tool: Some("context_store".to_string()),
            input: Some("test input".to_string()),
            response_size: Some(1024),
            response_snippet: Some("ok".to_string()),
            topic_signal: Some("testing".to_string()),
            phase: Some("active".to_string()),
        }
    }

    fn make_cycle_event_row(id: i64) -> CycleEventRow {
        CycleEventRow {
            id,
            cycle_id: "nxs-012".to_string(),
            seq: 1,
            event_type: "cycle_start".to_string(),
            phase: Some("design".to_string()),
            outcome: Some("complete".to_string()),
            next_phase: Some("delivery".to_string()),
            timestamp: 1700000000,
            goal: Some("extend export".to_string()),
        }
    }

    // ===== insert_graph_edge tests =====

    #[tokio::test]
    async fn test_insert_graph_edge_all_columns() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row = make_graph_edge_row(Some(r#"{"score": 0.9}"#.to_string()));
        insert_graph_edge(&mut conn, &row).await.unwrap();

        let db_row = sqlx::query(
            "SELECT source_id, target_id, relation_type, weight, created_at, \
             created_by, source, bootstrap_only, metadata, id FROM graph_edges",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        assert_eq!(db_row.get::<i64, _>("source_id"), 1);
        assert_eq!(db_row.get::<i64, _>("target_id"), 2);
        assert_eq!(db_row.get::<String, _>("relation_type"), "Supports");
        assert!((db_row.get::<f64, _>("weight") - 0.85).abs() < f64::EPSILON);
        assert_eq!(db_row.get::<i64, _>("created_at"), 1700000000);
        assert_eq!(db_row.get::<String, _>("created_by"), "agent-x");
        assert_eq!(db_row.get::<String, _>("source"), "runtime");
        assert_eq!(db_row.get::<i64, _>("bootstrap_only"), 0);
        assert_eq!(db_row.get::<String, _>("metadata"), r#"{"score": 0.9}"#);
        // Verify id was auto-assigned (not controlled by inserter)
        let auto_id: i64 = db_row.get("id");
        assert!(auto_id > 0, "auto-assigned id should be positive");
    }

    #[tokio::test]
    async fn test_insert_graph_edge_nullable_metadata_null() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row = make_graph_edge_row(None);
        insert_graph_edge(&mut conn, &row).await.unwrap();

        let db_row = sqlx::query("SELECT metadata FROM graph_edges")
            .fetch_one(&mut *conn)
            .await
            .unwrap();

        let metadata: Option<String> = db_row.get("metadata");
        assert!(metadata.is_none(), "metadata should be SQL NULL");
    }

    #[tokio::test]
    async fn test_insert_graph_edge_nullable_metadata_populated() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row = make_graph_edge_row(Some(r#"{"score": 0.9}"#.to_string()));
        insert_graph_edge(&mut conn, &row).await.unwrap();

        let db_row = sqlx::query("SELECT metadata FROM graph_edges")
            .fetch_one(&mut *conn)
            .await
            .unwrap();

        let metadata: Option<String> = db_row.get("metadata");
        assert_eq!(metadata.as_deref(), Some(r#"{"score": 0.9}"#));
    }

    #[tokio::test]
    async fn test_insert_graph_edge_plain_insert_not_ignore() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row = make_graph_edge_row(None);
        insert_graph_edge(&mut conn, &row).await.unwrap();

        // Second insert with same (source_id, target_id, relation_type) must fail
        let result = insert_graph_edge(&mut conn, &row).await;
        assert!(
            result.is_err(),
            "duplicate natural key should produce UNIQUE constraint error"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("UNIQUE") || err.contains("constraint"),
            "error should mention constraint violation: {err}"
        );
    }

    #[tokio::test]
    async fn test_insert_graph_edge_duplicate_different_relation() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row1 = make_graph_edge_row(None);
        insert_graph_edge(&mut conn, &row1).await.unwrap();

        // Same (source_id, target_id) but different relation_type should succeed
        let row2 = GraphEdgeRow {
            relation_type: "Contradicts".to_string(),
            ..make_graph_edge_row(None)
        };
        insert_graph_edge(&mut conn, &row2).await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    // ===== insert_observation tests =====

    #[tokio::test]
    async fn test_insert_observation_all_columns() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row = make_observation_row(42);
        insert_observation(&mut conn, &row).await.unwrap();

        let db_row = sqlx::query(
            "SELECT id, session_id, ts_millis, hook, tool, input, \
             response_size, response_snippet, topic_signal, phase FROM observations",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        assert_eq!(db_row.get::<i64, _>("id"), 42);
        assert_eq!(db_row.get::<String, _>("session_id"), "sess-001");
        assert_eq!(db_row.get::<i64, _>("ts_millis"), 1700000000);
        assert_eq!(db_row.get::<String, _>("hook"), "on_tool");
        assert_eq!(
            db_row.get::<Option<String>, _>("tool"),
            Some("context_store".to_string())
        );
        assert_eq!(
            db_row.get::<Option<String>, _>("input"),
            Some("test input".to_string())
        );
        assert_eq!(db_row.get::<Option<i64>, _>("response_size"), Some(1024));
        assert_eq!(
            db_row.get::<Option<String>, _>("response_snippet"),
            Some("ok".to_string())
        );
        assert_eq!(
            db_row.get::<Option<String>, _>("topic_signal"),
            Some("testing".to_string())
        );
        assert_eq!(
            db_row.get::<Option<String>, _>("phase"),
            Some("active".to_string())
        );
    }

    #[tokio::test]
    async fn test_insert_observation_nullable_fields_null() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row = ObservationRow {
            id: 1,
            session_id: "sess-002".to_string(),
            ts_millis: 1700000000,
            hook: "on_response".to_string(),
            tool: None,
            input: None,
            response_size: None,
            response_snippet: None,
            topic_signal: None,
            phase: None,
        };
        insert_observation(&mut conn, &row).await.unwrap();

        let db_row = sqlx::query(
            "SELECT tool, input, response_size, response_snippet, \
             topic_signal, phase FROM observations",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        assert!(db_row.get::<Option<String>, _>("tool").is_none());
        assert!(db_row.get::<Option<String>, _>("input").is_none());
        assert!(db_row.get::<Option<i64>, _>("response_size").is_none());
        assert!(
            db_row
                .get::<Option<String>, _>("response_snippet")
                .is_none()
        );
        assert!(db_row.get::<Option<String>, _>("topic_signal").is_none());
        assert!(db_row.get::<Option<String>, _>("phase").is_none());
    }

    #[tokio::test]
    async fn test_insert_observation_id_preserved() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row = make_observation_row(999);
        insert_observation(&mut conn, &row).await.unwrap();

        let id: i64 = sqlx::query_scalar("SELECT id FROM observations")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(id, 999);
    }

    #[tokio::test]
    async fn test_insert_observation_id_collision() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row1 = make_observation_row(1);
        insert_observation(&mut conn, &row1).await.unwrap();

        let row2 = ObservationRow {
            session_id: "sess-other".to_string(),
            ..make_observation_row(1)
        };
        let result = insert_observation(&mut conn, &row2).await;
        assert!(
            result.is_err(),
            "duplicate id should produce PRIMARY KEY constraint error"
        );
    }

    // ===== insert_cycle_event tests =====

    #[tokio::test]
    async fn test_insert_cycle_event_all_columns() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row = make_cycle_event_row(77);
        insert_cycle_event(&mut conn, &row).await.unwrap();

        let db_row = sqlx::query(
            "SELECT id, cycle_id, seq, event_type, phase, outcome, \
             next_phase, timestamp, goal, goal_embedding FROM cycle_events",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        assert_eq!(db_row.get::<i64, _>("id"), 77);
        assert_eq!(db_row.get::<String, _>("cycle_id"), "nxs-012");
        assert_eq!(db_row.get::<i64, _>("seq"), 1);
        assert_eq!(db_row.get::<String, _>("event_type"), "cycle_start");
        assert_eq!(
            db_row.get::<Option<String>, _>("phase"),
            Some("design".to_string())
        );
        assert_eq!(
            db_row.get::<Option<String>, _>("outcome"),
            Some("complete".to_string())
        );
        assert_eq!(
            db_row.get::<Option<String>, _>("next_phase"),
            Some("delivery".to_string())
        );
        assert_eq!(db_row.get::<i64, _>("timestamp"), 1700000000);
        assert_eq!(
            db_row.get::<Option<String>, _>("goal"),
            Some("extend export".to_string())
        );
        // ADR-004: goal_embedding must be NULL
        let embedding: Option<Vec<u8>> = db_row.get("goal_embedding");
        assert!(embedding.is_none(), "goal_embedding must be NULL");
    }

    #[tokio::test]
    async fn test_insert_cycle_event_goal_embedding_null() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row = make_cycle_event_row(1);
        insert_cycle_event(&mut conn, &row).await.unwrap();

        let embedding: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT goal_embedding FROM cycle_events")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert!(
            embedding.is_none(),
            "goal_embedding must be NULL after insert"
        );
    }

    #[tokio::test]
    async fn test_insert_cycle_event_nullable_fields_null() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row = CycleEventRow {
            id: 1,
            cycle_id: "c".to_string(),
            seq: 0,
            event_type: "t".to_string(),
            phase: None,
            outcome: None,
            next_phase: None,
            timestamp: 0,
            goal: None,
        };
        insert_cycle_event(&mut conn, &row).await.unwrap();

        let db_row = sqlx::query("SELECT phase, outcome, next_phase, goal FROM cycle_events")
            .fetch_one(&mut *conn)
            .await
            .unwrap();

        assert!(db_row.get::<Option<String>, _>("phase").is_none());
        assert!(db_row.get::<Option<String>, _>("outcome").is_none());
        assert!(db_row.get::<Option<String>, _>("next_phase").is_none());
        assert!(db_row.get::<Option<String>, _>("goal").is_none());
    }

    #[tokio::test]
    async fn test_insert_cycle_event_id_preserved() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row = make_cycle_event_row(888);
        insert_cycle_event(&mut conn, &row).await.unwrap();

        let id: i64 = sqlx::query_scalar("SELECT id FROM cycle_events")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(id, 888);
    }

    #[tokio::test]
    async fn test_insert_cycle_event_id_collision() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let row1 = make_cycle_event_row(1);
        insert_cycle_event(&mut conn, &row1).await.unwrap();

        let row2 = CycleEventRow {
            cycle_id: "other".to_string(),
            ..make_cycle_event_row(1)
        };
        let result = insert_cycle_event(&mut conn, &row2).await;
        assert!(
            result.is_err(),
            "duplicate id should produce PRIMARY KEY constraint error"
        );
    }
}
