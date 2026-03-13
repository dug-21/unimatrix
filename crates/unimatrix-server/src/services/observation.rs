//! SqlObservationSource: SQL implementation of ObservationSource trait.
//!
//! Implements the ObservationSource trait (defined in unimatrix-observe)
//! using the SQLite observations and sessions tables. Preserves
//! unimatrix-observe's independence from unimatrix-store (ADR-002).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use unimatrix_observe::error::{ObserveError, Result};
use unimatrix_observe::source::ObservationSource;
use unimatrix_observe::types::{HookType, ObservationRecord, ObservationStats, ParsedSession};
use unimatrix_store::Store;
use unimatrix_store::rusqlite;

/// SQL-backed implementation of ObservationSource.
///
/// Queries the `observations` and `sessions` tables via `Store::lock_conn()`.
pub struct SqlObservationSource {
    store: Arc<Store>,
}

impl SqlObservationSource {
    /// Create a new SqlObservationSource backed by the given Store.
    pub fn new(store: Arc<Store>) -> Self {
        SqlObservationSource { store }
    }
}

impl ObservationSource for SqlObservationSource {
    fn load_feature_observations(&self, feature_cycle: &str) -> Result<Vec<ObservationRecord>> {
        let conn = self.store.lock_conn();

        // Step 1: Get session_ids for this feature from SESSIONS table.
        // Sessions with NULL feature_cycle are excluded by the WHERE clause.
        let mut session_stmt = conn
            .prepare("SELECT session_id FROM sessions WHERE feature_cycle = ?1")
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        let session_ids: Vec<String> = session_stmt
            .query_map(rusqlite::params![feature_cycle], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| ObserveError::Database(e.to_string()))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        drop(session_stmt);

        if session_ids.is_empty() {
            return Ok(vec![]);
        }

        // Step 2: Query observations for those session_ids.
        let placeholders: String = session_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT session_id, ts_millis, hook, tool, input, response_size, response_snippet
             FROM observations
             WHERE session_id IN ({})
             ORDER BY ts_millis ASC",
            placeholders
        );

        let mut obs_stmt = conn
            .prepare(&sql)
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        let rows = obs_stmt
            .query_map(rusqlite::params_from_iter(session_ids.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,         // session_id
                    row.get::<_, i64>(1)?,            // ts_millis
                    row.get::<_, String>(2)?,         // hook
                    row.get::<_, Option<String>>(3)?, // tool
                    row.get::<_, Option<String>>(4)?, // input
                    row.get::<_, Option<i64>>(5)?,    // response_size
                    row.get::<_, Option<String>>(6)?, // response_snippet
                ))
            })
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        let mut records = Vec::new();
        for row_result in rows {
            let (session_id, ts_millis, hook_str, tool, input_str, response_size, response_snippet) =
                row_result.map_err(|e| ObserveError::Database(e.to_string()))?;

            let hook = match hook_str.as_str() {
                "PreToolUse" => HookType::PreToolUse,
                "PostToolUse" => HookType::PostToolUse,
                "SubagentStart" => HookType::SubagentStart,
                "SubagentStop" => HookType::SubagentStop,
                _ => continue, // skip unknown hook types
            };

            // Input deserialization depends on hook type (R-10):
            // - SubagentStart: input is plain text (prompt snippet) -> Value::String
            // - Tool events: input is JSON string -> parse to Value::Object
            let input = match (&hook, input_str) {
                (HookType::SubagentStart, Some(s)) => Some(serde_json::Value::String(s)),
                (_, Some(s)) => serde_json::from_str(&s).ok(),
                (_, None) => None,
            };

            records.push(ObservationRecord {
                ts: ts_millis as u64,
                hook,
                session_id,
                tool,
                input,
                response_size: response_size.map(|v| v as u64),
                response_snippet,
            });
        }

        Ok(records)
    }

    fn discover_sessions_for_feature(&self, feature_cycle: &str) -> Result<Vec<String>> {
        let conn = self.store.lock_conn();

        let mut stmt = conn
            .prepare("SELECT session_id FROM sessions WHERE feature_cycle = ?1")
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        let sessions: Vec<String> = stmt
            .query_map(rusqlite::params![feature_cycle], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| ObserveError::Database(e.to_string()))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        Ok(sessions)
    }

    fn load_unattributed_sessions(&self) -> Result<Vec<ParsedSession>> {
        let conn = self.store.lock_conn();

        // Step 1: Get session_ids where feature_cycle IS NULL.
        let mut session_stmt = conn
            .prepare("SELECT session_id FROM sessions WHERE feature_cycle IS NULL")
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        let session_ids: Vec<String> = session_stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| ObserveError::Database(e.to_string()))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        drop(session_stmt);

        if session_ids.is_empty() {
            return Ok(vec![]);
        }

        // Step 2: Load observations for those sessions, ordered by session then timestamp.
        let placeholders: String = session_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT session_id, ts_millis, hook, tool, input, response_size, response_snippet
             FROM observations
             WHERE session_id IN ({})
             ORDER BY session_id, ts_millis ASC",
            placeholders
        );

        let mut obs_stmt = conn
            .prepare(&sql)
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        let rows = obs_stmt
            .query_map(rusqlite::params_from_iter(session_ids.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,         // session_id
                    row.get::<_, i64>(1)?,            // ts_millis
                    row.get::<_, String>(2)?,         // hook
                    row.get::<_, Option<String>>(3)?, // tool
                    row.get::<_, Option<String>>(4)?, // input
                    row.get::<_, Option<i64>>(5)?,    // response_size
                    row.get::<_, Option<String>>(6)?, // response_snippet
                ))
            })
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        // Step 3: Group into ParsedSession structs.
        let mut sessions_map: std::collections::HashMap<String, Vec<ObservationRecord>> =
            std::collections::HashMap::new();

        for row_result in rows {
            let (session_id, ts_millis, hook_str, tool, input_str, response_size, response_snippet) =
                row_result.map_err(|e| ObserveError::Database(e.to_string()))?;

            let hook = match hook_str.as_str() {
                "PreToolUse" => HookType::PreToolUse,
                "PostToolUse" => HookType::PostToolUse,
                "SubagentStart" => HookType::SubagentStart,
                "SubagentStop" => HookType::SubagentStop,
                _ => continue,
            };

            let input = match (&hook, input_str) {
                (HookType::SubagentStart, Some(s)) => Some(serde_json::Value::String(s)),
                (_, Some(s)) => serde_json::from_str(&s).ok(),
                (_, None) => None,
            };

            sessions_map
                .entry(session_id.clone())
                .or_default()
                .push(ObservationRecord {
                    ts: ts_millis as u64,
                    hook,
                    session_id,
                    tool,
                    input,
                    response_size: response_size.map(|v| v as u64),
                    response_snippet,
                });
        }

        let parsed: Vec<ParsedSession> = sessions_map
            .into_iter()
            .map(|(session_id, records)| ParsedSession {
                session_id,
                records,
            })
            .collect();

        Ok(parsed)
    }

    fn observation_stats(&self) -> Result<ObservationStats> {
        let conn = self.store.lock_conn();

        // Aggregate counts
        let (record_count, session_count, min_ts): (i64, i64, Option<i64>) = conn
            .query_row(
                "SELECT COUNT(*), COUNT(DISTINCT session_id), MIN(ts_millis) FROM observations",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let oldest_record_age_days = match min_ts {
            Some(min) if min > 0 => ((now_millis - min) / 86_400_000).max(0) as u64,
            _ => 0,
        };

        // Sessions approaching 60-day cleanup (records between 45 and 60 days old)
        let forty_five_days_ms = 45_i64 * 86_400_000;
        let sixty_days_ms = 60_i64 * 86_400_000;
        let cutoff_45 = now_millis - forty_five_days_ms;
        let cutoff_60 = now_millis - sixty_days_ms;

        let mut approaching_stmt = conn
            .prepare(
                "SELECT DISTINCT session_id FROM observations
                 WHERE ts_millis <= ?1 AND ts_millis > ?2",
            )
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        let approaching: Vec<String> = approaching_stmt
            .query_map(rusqlite::params![cutoff_45, cutoff_60], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| ObserveError::Database(e.to_string()))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| ObserveError::Database(e.to_string()))?;

        Ok(ObservationStats {
            record_count: record_count as u64,
            session_count: session_count as u64,
            oldest_record_age_days,
            approaching_cleanup: approaching,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_store() -> Arc<Store> {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let store = Store::open(dir.path().join("test.db")).expect("open store");
        Arc::new(store)
    }

    fn insert_session(store: &Store, session_id: &str, feature_cycle: Option<&str>) {
        let conn = store.lock_conn();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        conn.execute(
            "INSERT INTO sessions (session_id, feature_cycle, started_at, status)
             VALUES (?1, ?2, ?3, 0)",
            rusqlite::params![session_id, feature_cycle, now],
        )
        .expect("insert session");
    }

    fn insert_observation(
        store: &Store,
        session_id: &str,
        ts_millis: i64,
        hook: &str,
        tool: Option<&str>,
        input: Option<&str>,
        response_size: Option<i64>,
        response_snippet: Option<&str>,
    ) {
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO observations (session_id, ts_millis, hook, tool, input, response_size, response_snippet)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![session_id, ts_millis, hook, tool, input, response_size, response_snippet],
        )
        .expect("insert observation");
    }

    #[test]
    fn test_load_feature_observations_all_fields() {
        let store = setup_test_store();
        insert_session(&store, "sess-1", Some("col-012"));
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PostToolUse",
            Some("Read"),
            Some(r#"{"file_path":"/tmp/test"}"#),
            Some(1024),
            Some("output text"),
        );

        let source = SqlObservationSource::new(Arc::clone(&store));
        let records = source.load_feature_observations("col-012").unwrap();

        assert_eq!(records.len(), 1);
        let r = &records[0];
        assert_eq!(r.ts, 1700000000000);
        assert_eq!(r.hook, HookType::PostToolUse);
        assert_eq!(r.session_id, "sess-1");
        assert_eq!(r.tool, Some("Read".to_string()));
        assert!(r.input.as_ref().unwrap().get("file_path").is_some());
        assert_eq!(r.response_size, Some(1024));
        assert_eq!(r.response_snippet, Some("output text".to_string()));
    }

    #[test]
    fn test_load_feature_observations_null_optionals() {
        let store = setup_test_store();
        insert_session(&store, "sess-1", Some("col-012"));
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "SubagentStop",
            None,
            None,
            None,
            None,
        );

        let source = SqlObservationSource::new(Arc::clone(&store));
        let records = source.load_feature_observations("col-012").unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].hook, HookType::SubagentStop);
        assert!(records[0].tool.is_none());
        assert!(records[0].input.is_none());
        assert!(records[0].response_size.is_none());
        assert!(records[0].response_snippet.is_none());
    }

    #[test]
    fn test_load_feature_observations_subagent_start_string_input() {
        let store = setup_test_store();
        insert_session(&store, "sess-1", Some("col-012"));
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "SubagentStart",
            Some("uni-pseudocode"),
            Some("Design components for col-012"),
            None,
            None,
        );

        let source = SqlObservationSource::new(Arc::clone(&store));
        let records = source.load_feature_observations("col-012").unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].hook, HookType::SubagentStart);
        assert_eq!(records[0].tool, Some("uni-pseudocode".to_string()));
        assert_eq!(
            records[0].input,
            Some(serde_json::Value::String(
                "Design components for col-012".to_string()
            ))
        );
    }

    #[test]
    fn test_load_feature_observations_json_input_deserialized() {
        let store = setup_test_store();
        insert_session(&store, "sess-1", Some("col-012"));
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Bash"),
            Some(r#"{"command":"ls -la"}"#),
            None,
            None,
        );

        let source = SqlObservationSource::new(Arc::clone(&store));
        let records = source.load_feature_observations("col-012").unwrap();

        assert_eq!(records.len(), 1);
        let input = records[0].input.as_ref().unwrap();
        assert_eq!(input.get("command").unwrap().as_str().unwrap(), "ls -la");
    }

    #[test]
    fn test_null_feature_cycle_excluded() {
        let store = setup_test_store();
        insert_session(&store, "sess-1", Some("col-012"));
        insert_session(&store, "sess-2", None);
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            None,
            None,
            None,
        );
        insert_observation(
            &store,
            "sess-2",
            1700000001000,
            "PreToolUse",
            Some("Write"),
            None,
            None,
            None,
        );

        let source = SqlObservationSource::new(Arc::clone(&store));
        let records = source.load_feature_observations("col-012").unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].session_id, "sess-1");
    }

    #[test]
    fn test_empty_result_nonexistent_feature() {
        let store = setup_test_store();
        let source = SqlObservationSource::new(Arc::clone(&store));
        let records = source.load_feature_observations("nonexistent").unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn test_observation_stats_aggregate() {
        let store = setup_test_store();
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        // Insert 10 observations across 3 sessions
        for i in 0..10 {
            let session = format!("sess-{}", i % 3);
            insert_observation(
                &store,
                &session,
                now_millis - (i * 1000),
                "PreToolUse",
                Some("Read"),
                None,
                None,
                None,
            );
        }

        let source = SqlObservationSource::new(Arc::clone(&store));
        let stats = source.observation_stats().unwrap();

        assert_eq!(stats.record_count, 10);
        assert_eq!(stats.session_count, 3);
        assert_eq!(stats.oldest_record_age_days, 0); // all from today
    }

    #[test]
    fn test_discover_sessions_for_feature() {
        let store = setup_test_store();
        insert_session(&store, "sess-1", Some("col-012"));
        insert_session(&store, "sess-2", Some("col-012"));
        insert_session(&store, "sess-3", Some("nxs-001"));
        insert_session(&store, "sess-4", None);

        let source = SqlObservationSource::new(Arc::clone(&store));
        let sessions = source.discover_sessions_for_feature("col-012").unwrap();

        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains(&"sess-1".to_string()));
        assert!(sessions.contains(&"sess-2".to_string()));
    }

    #[test]
    fn test_load_unattributed_sessions_returns_null_feature_cycle_only() {
        let store = setup_test_store();
        insert_session(&store, "sess-1", None);
        insert_session(&store, "sess-2", Some("col-012"));
        insert_session(&store, "sess-3", None);
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/col-015/SCOPE.md"}"#),
            None,
            None,
        );
        insert_observation(
            &store,
            "sess-2",
            1700000001000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/col-012/SCOPE.md"}"#),
            None,
            None,
        );
        insert_observation(
            &store,
            "sess-3",
            1700000002000,
            "PreToolUse",
            Some("Write"),
            Some(r#"{"file_path":"product/features/crt-013/test.rs"}"#),
            None,
            None,
        );

        let source = SqlObservationSource::new(Arc::clone(&store));
        let sessions = source.load_unattributed_sessions().unwrap();

        // Only sess-1 and sess-3 (NULL feature_cycle), not sess-2
        assert_eq!(sessions.len(), 2);
        let session_ids: Vec<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert!(session_ids.contains(&"sess-1"));
        assert!(session_ids.contains(&"sess-3"));
        assert!(!session_ids.contains(&"sess-2"));
    }

    #[test]
    fn test_load_unattributed_sessions_empty_when_all_attributed() {
        let store = setup_test_store();
        insert_session(&store, "sess-1", Some("col-012"));
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            None,
            None,
            None,
        );

        let source = SqlObservationSource::new(Arc::clone(&store));
        let sessions = source.load_unattributed_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_load_unattributed_sessions_groups_by_session_id() {
        let store = setup_test_store();
        insert_session(&store, "sess-1", None);
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/col-015/SCOPE.md"}"#),
            None,
            None,
        );
        insert_observation(
            &store,
            "sess-1",
            1700000001000,
            "PostToolUse",
            Some("Read"),
            None,
            Some(512),
            Some("file contents"),
        );

        let source = SqlObservationSource::new(Arc::clone(&store));
        let sessions = source.load_unattributed_sessions().unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "sess-1");
        assert_eq!(sessions[0].records.len(), 2);
        // Records should be sorted by timestamp
        assert!(sessions[0].records[0].ts <= sessions[0].records[1].ts);
    }

    #[test]
    fn test_load_unattributed_sessions_empty_when_no_sessions() {
        let store = setup_test_store();
        let source = SqlObservationSource::new(Arc::clone(&store));
        let sessions = source.load_unattributed_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    /// Integration test for the full fallback path: NULL feature_cycle sessions
    /// with observations containing feature file paths are attributed via
    /// content-based attribution and returned for the correct feature (AC-01, AC-02, AC-08).
    #[test]
    fn test_attribution_fallback_end_to_end() {
        use unimatrix_observe::attribute_sessions;

        let store = setup_test_store();

        // Session with NULL feature_cycle but observations referencing col-test
        insert_session(&store, "sess-1", None);
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/col-test/SCOPE.md"}"#),
            None,
            None,
        );
        insert_observation(
            &store,
            "sess-1",
            1700000001000,
            "PreToolUse",
            Some("Write"),
            Some(r#"{"file_path":"product/features/col-test/impl.rs"}"#),
            None,
            None,
        );

        // Session with NULL feature_cycle referencing a different feature
        insert_session(&store, "sess-2", None);
        insert_observation(
            &store,
            "sess-2",
            1700000002000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/nxs-001/SCOPE.md"}"#),
            None,
            None,
        );

        let source = SqlObservationSource::new(Arc::clone(&store));

        // Direct query returns empty (no sessions have feature_cycle = 'col-test')
        let direct = source.load_feature_observations("col-test").unwrap();
        assert!(direct.is_empty());

        // Fallback: load unattributed, run attribution
        let unattributed = source.load_unattributed_sessions().unwrap();
        assert_eq!(unattributed.len(), 2);

        let attributed = attribute_sessions(&unattributed, "col-test");

        // Only sess-1 records attributed to col-test (AC-04: multi-feature partitioning)
        assert_eq!(attributed.len(), 2);
        assert!(attributed.iter().all(|r| r.session_id == "sess-1"));
    }

    /// AC-03: Sessions with populated feature_cycle still use the direct query path.
    #[test]
    fn test_direct_path_preserved_for_populated_feature_cycle() {
        let store = setup_test_store();
        insert_session(&store, "sess-1", Some("col-015"));
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/col-015/SCOPE.md"}"#),
            None,
            None,
        );

        let source = SqlObservationSource::new(Arc::clone(&store));
        let direct = source.load_feature_observations("col-015").unwrap();

        // Direct path returns data -- no fallback needed
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].session_id, "sess-1");
    }

    /// AC-04: Multi-feature session correctly partitioned.
    #[test]
    fn test_multi_feature_session_partitioned_via_fallback() {
        use unimatrix_observe::attribute_sessions;

        let store = setup_test_store();
        insert_session(&store, "sess-1", None);

        // Session touches col-015 then crt-013
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/col-015/SCOPE.md"}"#),
            None,
            None,
        );
        insert_observation(
            &store,
            "sess-1",
            1700000001000,
            "PreToolUse",
            Some("Write"),
            Some(r#"{"file_path":"product/features/col-015/impl.rs"}"#),
            None,
            None,
        );
        insert_observation(
            &store,
            "sess-1",
            1700000002000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/crt-013/SCOPE.md"}"#),
            None,
            None,
        );
        insert_observation(
            &store,
            "sess-1",
            1700000003000,
            "PreToolUse",
            Some("Write"),
            Some(r#"{"file_path":"product/features/crt-013/test.rs"}"#),
            None,
            None,
        );

        let source = SqlObservationSource::new(Arc::clone(&store));
        let unattributed = source.load_unattributed_sessions().unwrap();

        let col015 = attribute_sessions(&unattributed, "col-015");
        assert_eq!(col015.len(), 2);
        assert!(col015.iter().all(|r| r.ts <= 1700000001000));

        let crt013 = attribute_sessions(&unattributed, "crt-013");
        assert_eq!(crt013.len(), 2);
        assert!(crt013.iter().all(|r| r.ts >= 1700000002000));
    }
}
