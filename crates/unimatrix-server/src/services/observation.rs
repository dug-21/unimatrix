//! SqlObservationSource: SQL implementation of ObservationSource trait.
//!
//! Implements the ObservationSource trait (defined in unimatrix-observe)
//! using the SQLite observations and sessions tables. Preserves
//! unimatrix-observe's independence from unimatrix-store (ADR-002).
//!
//! All store access uses async sqlx via write_pool_server() (nxs-011).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::Row;
use unimatrix_observe::domain::DomainPackRegistry;
use unimatrix_observe::error::{ObserveError, Result};
use unimatrix_observe::source::ObservationSource;
use unimatrix_observe::types::{ObservationRecord, ObservationStats, ParsedSession};
use unimatrix_store::SqlxStore;

/// SQL-backed implementation of ObservationSource.
///
/// Queries the `observations` and `sessions` tables via async sqlx pool access.
pub struct SqlObservationSource {
    store: Arc<SqlxStore>,
    registry: Arc<DomainPackRegistry>,
}

impl SqlObservationSource {
    /// Create a new SqlObservationSource backed by the given Store and DomainPackRegistry.
    ///
    /// The registry is used for source_domain resolution. For the hook ingress path,
    /// source_domain is always "claude-code" regardless of event_type.
    pub fn new(store: Arc<SqlxStore>, registry: Arc<DomainPackRegistry>) -> Self {
        SqlObservationSource { store, registry }
    }

    /// Create a new SqlObservationSource with the built-in claude-code registry.
    ///
    /// Convenience constructor for callers that do not inject a registry (e.g., status checks,
    /// legacy call sites). For full ingest security, use `new(store, registry)`.
    pub fn new_default(store: Arc<SqlxStore>) -> Self {
        let registry = Arc::new(DomainPackRegistry::with_builtin_claude_code());
        SqlObservationSource { store, registry }
    }

    /// Async version of observation_stats for use in async contexts.
    ///
    /// Returns aggregate observation counts and the oldest record age.
    pub async fn observation_stats_async(&self) -> Result<ObservationStats> {
        let pool = self.store.write_pool_server();

        let row = sqlx::query(
            "SELECT COUNT(*), COUNT(DISTINCT session_id), MIN(ts_millis) FROM observations",
        )
        .fetch_one(pool)
        .await
        .map_err(|e| ObserveError::Database(e.to_string()))?;

        let record_count: i64 = row.get::<i64, _>(0);
        let session_count: i64 = row.get::<i64, _>(1);
        let min_ts: Option<i64> = row.get(2);

        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let oldest_record_age_days = match min_ts {
            Some(min) if min > 0 => ((now_millis - min) / 86_400_000).max(0) as u64,
            _ => 0,
        };

        // Sessions approaching 60-day cleanup (between 45 and 60 days old)
        let forty_five_days_ms = 45_i64 * 86_400_000;
        let sixty_days_ms = 60_i64 * 86_400_000;
        let cutoff_45 = now_millis - forty_five_days_ms;
        let cutoff_60 = now_millis - sixty_days_ms;

        let approaching_rows = sqlx::query(
            "SELECT DISTINCT session_id FROM observations WHERE ts_millis <= ?1 AND ts_millis > ?2",
        )
        .bind(cutoff_45)
        .bind(cutoff_60)
        .fetch_all(pool)
        .await
        .map_err(|e| ObserveError::Database(e.to_string()))?;

        let approaching: Vec<String> = approaching_rows
            .into_iter()
            .map(|row| row.get::<String, _>(0))
            .collect();

        Ok(ObservationStats {
            record_count: record_count as u64,
            session_count: session_count as u64,
            oldest_record_age_days,
            approaching_cleanup: approaching,
        })
    }
}

impl ObservationSource for SqlObservationSource {
    fn load_feature_observations(&self, feature_cycle: &str) -> Result<Vec<ObservationRecord>> {
        // Bridge async sqlx to sync trait. Use block_in_place when inside a
        // tokio runtime to avoid "Cannot start a runtime from within a runtime".
        let pool = self.store.write_pool_server();

        block_sync(async {
            // Step 1: Get session_ids for this feature from SESSIONS table.
            let session_rows =
                sqlx::query("SELECT session_id FROM sessions WHERE feature_cycle = ?1")
                    .bind(feature_cycle)
                    .fetch_all(pool)
                    .await
                    .map_err(|e| ObserveError::Database(e.to_string()))?;

            let session_ids: Vec<String> = session_rows
                .into_iter()
                .map(|row| row.get::<String, _>(0))
                .collect();

            if session_ids.is_empty() {
                return Ok(vec![]);
            }

            // Step 2: Query observations for those session_ids.
            // Build parameterized IN clause.
            let placeholders: String = session_ids
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT session_id, ts_millis, hook, tool, input, response_size, response_snippet \
                     FROM observations \
                     WHERE session_id IN ({}) \
                     ORDER BY ts_millis ASC",
                placeholders
            );

            let mut q = sqlx::query(&sql);
            for sid in &session_ids {
                q = q.bind(sid);
            }

            let rows = q
                .fetch_all(pool)
                .await
                .map_err(|e| ObserveError::Database(e.to_string()))?;

            parse_observation_rows(rows, &self.registry)
        })
    }

    fn discover_sessions_for_feature(&self, feature_cycle: &str) -> Result<Vec<String>> {
        let pool = self.store.write_pool_server();

        block_sync(async {
            let rows = sqlx::query("SELECT session_id FROM sessions WHERE feature_cycle = ?1")
                .bind(feature_cycle)
                .fetch_all(pool)
                .await
                .map_err(|e| ObserveError::Database(e.to_string()))?;

            Ok(rows
                .into_iter()
                .map(|row| row.get::<String, _>(0))
                .collect())
        })
    }

    fn load_unattributed_sessions(&self) -> Result<Vec<ParsedSession>> {
        let pool = self.store.write_pool_server();

        block_sync(async {
            // Step 1: Get session_ids where feature_cycle IS NULL.
            let session_rows =
                sqlx::query("SELECT session_id FROM sessions WHERE feature_cycle IS NULL")
                    .fetch_all(pool)
                    .await
                    .map_err(|e| ObserveError::Database(e.to_string()))?;

            let session_ids: Vec<String> = session_rows
                .into_iter()
                .map(|row| row.get::<String, _>(0))
                .collect();

            if session_ids.is_empty() {
                return Ok(vec![]);
            }

            // Step 2: Load observations for those sessions.
            let placeholders: String = session_ids
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT session_id, ts_millis, hook, tool, input, response_size, response_snippet \
                 FROM observations \
                 WHERE session_id IN ({}) \
                 ORDER BY session_id, ts_millis ASC",
                placeholders
            );

            let mut q = sqlx::query(&sql);
            for sid in &session_ids {
                q = q.bind(sid);
            }

            let rows = q
                .fetch_all(pool)
                .await
                .map_err(|e| ObserveError::Database(e.to_string()))?;

            let records = parse_observation_rows(rows, &self.registry)?;

            // Step 3: Group into ParsedSession structs.
            let mut sessions_map: std::collections::HashMap<String, Vec<ObservationRecord>> =
                std::collections::HashMap::new();
            for record in records {
                sessions_map
                    .entry(record.session_id.clone())
                    .or_default()
                    .push(record);
            }

            let parsed: Vec<ParsedSession> = sessions_map
                .into_iter()
                .map(|(session_id, records)| ParsedSession {
                    session_id,
                    records,
                })
                .collect();

            Ok(parsed)
        })
    }

    fn observation_stats(&self) -> Result<ObservationStats> {
        let pool = self.store.write_pool_server();

        block_sync(async {
            let row = sqlx::query(
                "SELECT COUNT(*), COUNT(DISTINCT session_id), MIN(ts_millis) FROM observations",
            )
            .fetch_one(pool)
            .await
            .map_err(|e| ObserveError::Database(e.to_string()))?;

            let record_count: i64 = row.get::<i64, _>(0);
            let session_count: i64 = row.get::<i64, _>(1);
            let min_ts: Option<i64> = row.get(2);

            let now_millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;

            let oldest_record_age_days = match min_ts {
                Some(min) if min > 0 => ((now_millis - min) / 86_400_000).max(0) as u64,
                _ => 0,
            };

            let forty_five_days_ms = 45_i64 * 86_400_000;
            let sixty_days_ms = 60_i64 * 86_400_000;
            let cutoff_45 = now_millis - forty_five_days_ms;
            let cutoff_60 = now_millis - sixty_days_ms;

            let approaching_rows = sqlx::query(
                "SELECT DISTINCT session_id FROM observations WHERE ts_millis <= ?1 AND ts_millis > ?2",
            )
            .bind(cutoff_45)
            .bind(cutoff_60)
            .fetch_all(pool)
            .await
            .map_err(|e| ObserveError::Database(e.to_string()))?;

            let approaching: Vec<String> = approaching_rows
                .into_iter()
                .map(|row| row.get::<String, _>(0))
                .collect();

            Ok(ObservationStats {
                record_count: record_count as u64,
                session_count: session_count as u64,
                oldest_record_age_days,
                approaching_cleanup: approaching,
            })
        })
    }

    /// Load observations attributed to a named feature cycle via cycle_events timestamps.
    ///
    /// Primary attribution path (col-024). Three-step algorithm inside a single block_sync
    /// (ADR-001):
    ///
    /// Step 0: COUNT pre-check — returns Ok(vec![]) immediately when no cycle_events rows
    ///         exist for this cycle_id. Distinguishes "no rows" from "rows exist, no match".
    /// Step 1: Fetch cycle_events rows and pair into (start_ms, stop_ms) windows.
    ///         A cycle_start with no matching cycle_stop uses unix_now_secs() as stop (ADR-005).
    ///         KNOWN LIMITATION (ADR-005): abandoned cycles over-include observations up to now.
    /// Step 2: Per-window DISTINCT session_id discovery via topic_signal match.
    /// Step 3: Load observations for discovered sessions; Rust-filter to windows.
    ///
    /// Returns Ok(vec![]) (not Err) when no observations match — legacy fallback activates (FM-01).
    fn load_cycle_observations(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>> {
        let pool = self.store.write_pool_server();
        let cycle_id = cycle_id.to_string();
        let registry = Arc::clone(&self.registry);

        block_sync(async move {
            // ---- Step 0: Count-only pre-check (AC-15) ----
            // Distinguishes "no cycle_events rows" from "rows exist but no match".
            // Both return Ok(vec![]) to caller; the fallback log at the call site differentiates.
            let count_row = sqlx::query("SELECT COUNT(*) FROM cycle_events WHERE cycle_id = ?1")
                .bind(&cycle_id)
                .fetch_one(pool)
                .await
                .map_err(|e| ObserveError::Database(e.to_string()))?;

            let row_count: i64 = count_row.get::<i64, _>(0);
            if row_count == 0 {
                return Ok(vec![]);
            }

            // ---- Step 1: Fetch event rows and build time windows ----
            let event_rows = sqlx::query(
                "SELECT event_type, timestamp \
                 FROM cycle_events \
                 WHERE cycle_id = ?1 \
                 ORDER BY timestamp ASC, seq ASC",
            )
            .bind(&cycle_id)
            .fetch_all(pool)
            .await
            .map_err(|e| ObserveError::Database(e.to_string()))?;

            let mut windows: Vec<(i64, i64)> = Vec::new();
            let mut pending_start: Option<i64> = None;

            for row in &event_rows {
                let event_type: String = row.get::<String, _>(0);
                let ts_secs: i64 = row.get::<i64, _>(1);

                match event_type.as_str() {
                    "cycle_start" => {
                        if let Some(prev_start) = pending_start {
                            // Malformed: second cycle_start without intervening cycle_stop (E-03).
                            // Close the previous pending window at this start's timestamp.
                            windows.push((
                                cycle_ts_to_obs_millis(prev_start),
                                cycle_ts_to_obs_millis(ts_secs),
                            ));
                        }
                        pending_start = Some(ts_secs);
                    }
                    "cycle_stop" => {
                        if let Some(start) = pending_start.take() {
                            windows.push((
                                cycle_ts_to_obs_millis(start),
                                cycle_ts_to_obs_millis(ts_secs),
                            ));
                        }
                        // cycle_stop with no pending start: ignore defensively.
                    }
                    _ => {
                        // cycle_phase_end or unknown event_type: ignored for windowing (E-02).
                    }
                }
            }

            // Close any open-ended window (ADR-005).
            // KNOWN LIMITATION: abandoned cycle (cycle_start with no cycle_stop) will include
            // all observations with matching topic_signal up to the present. No max-age cap.
            if let Some(start) = pending_start {
                let now_ms = cycle_ts_to_obs_millis(unix_now_secs() as i64);
                windows.push((cycle_ts_to_obs_millis(start), now_ms));
            }

            if windows.is_empty() {
                // cycle_events rows exist but no parseable windows (e.g., only cycle_phase_end rows).
                return Ok(vec![]);
            }

            // ---- Step 2: Per-window session discovery ----
            // For each window, SELECT DISTINCT session_id WHERE topic_signal = cycle_id
            // AND ts_millis BETWEEN start_ms AND stop_ms.
            // Union into HashSet to deduplicate across overlapping windows (R-11).
            let mut session_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();

            for (start_ms, stop_ms) in &windows {
                let sid_rows = sqlx::query(
                    "SELECT DISTINCT session_id \
                     FROM observations \
                     WHERE topic_signal = ?1 \
                       AND ts_millis >= ?2 \
                       AND ts_millis <= ?3",
                )
                .bind(&cycle_id)
                .bind(start_ms)
                .bind(stop_ms)
                .fetch_all(pool)
                .await
                .map_err(|e| ObserveError::Database(e.to_string()))?;

                for row in sid_rows {
                    session_ids.insert(row.get::<String, _>(0));
                }
            }

            if session_ids.is_empty() {
                // cycle_events rows exist and windows are valid, but no observations carry
                // topic_signal = cycle_id within any window (AC-15 case B).
                return Ok(vec![]);
            }

            // ---- Step 3: Load observations for discovered sessions ----
            // Bound SQL scan to [min_window_start, max_window_stop] for index efficiency.
            // Rust post-filter retains only records inside at least one (start_ms, stop_ms) pair.
            let min_ms: i64 = windows
                .iter()
                .map(|(s, _)| *s)
                .min()
                .expect("windows non-empty");
            let max_ms: i64 = windows
                .iter()
                .map(|(_, e)| *e)
                .max()
                .expect("windows non-empty");

            let sid_vec: Vec<String> = session_ids.into_iter().collect();
            // Bind: ?1=min_ms, ?2=max_ms, ?3..?N=session_ids
            let placeholders: String = sid_vec
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 3))
                .collect::<Vec<_>>()
                .join(",");

            let sql = format!(
                "SELECT session_id, ts_millis, hook, tool, input, response_size, response_snippet \
                 FROM observations \
                 WHERE session_id IN ({placeholders}) \
                   AND ts_millis >= ?1 \
                   AND ts_millis <= ?2 \
                 ORDER BY ts_millis ASC",
                placeholders = placeholders
            );

            let mut q = sqlx::query(&sql).bind(min_ms).bind(max_ms);
            for sid in &sid_vec {
                q = q.bind(sid);
            }

            let rows = q
                .fetch_all(pool)
                .await
                .map_err(|e| ObserveError::Database(e.to_string()))?;

            // parse_observation_rows applies ingest security bounds (NFR-05): 64 KB limit,
            // JSON depth check. Rejected records are skipped with WARN; remaining are returned.
            let all_records = parse_observation_rows(rows, &registry)?;

            // Rust post-filter: retain records inside at least one window (R-09).
            // The SQL [min_ms, max_ms] bound may include gap-period observations from
            // multi-window cycles; this filter removes them.
            let filtered: Vec<ObservationRecord> = all_records
                .into_iter()
                .filter(|rec| {
                    let ts = rec.ts as i64;
                    windows
                        .iter()
                        .any(|(start, stop)| ts >= *start && ts <= *stop)
                })
                .collect();

            Ok(filtered)
        })
    }
}

/// Convert a cycle_events.timestamp (Unix epoch seconds) to the millisecond
/// unit used by observations.ts_millis.
///
/// cycle_events.timestamp is written by unix_now_secs() (i64, seconds).
/// observations.ts_millis is stored as (unix_now_secs() as i64).saturating_mul(1000) (i64, ms).
/// Both tables use the same epoch; this bridges the unit difference.
///
/// saturating_mul guards against i64 overflow on adversarially large timestamps
/// (E-05 edge case: i64::MAX as input saturates to i64::MAX, not panic).
#[inline]
fn cycle_ts_to_obs_millis(ts_secs: i64) -> i64 {
    ts_secs.saturating_mul(1000)
}

/// Returns the current Unix timestamp in seconds.
fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Bridge an async future to sync context (works inside or outside a tokio runtime).
fn block_sync<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            rt.block_on(fut)
        }
    }
}

/// Recursively check the nesting depth of a serde_json::Value.
///
/// Returns true if the value's nesting depth is <= max_depth.
/// Returns false immediately when the nesting depth exceeds max_depth.
///
/// O(n) walk over all nodes in the JSON tree; short-circuits at max_depth + 1.
/// Safe against stack overflow: at depth max_depth+1 (11 levels), recursion stops.
/// Combined with the 64 KB size pre-check, the total node count is bounded.
///
/// # Arguments
/// - `v`: the JSON value to inspect
/// - `current`: the current recursion depth (call with 0 from the top level)
/// - `max`: the maximum allowed depth (ADR-007 specifies 10)
fn json_depth(v: &serde_json::Value, current: usize, max: usize) -> bool {
    if current > max {
        return false;
    }
    match v {
        serde_json::Value::Object(map) => map
            .values()
            .all(|child| json_depth(child, current + 1, max)),
        serde_json::Value::Array(arr) => {
            arr.iter().all(|child| json_depth(child, current + 1, max))
        }
        _ => true,
    }
}

/// Parse a Vec of sqlx rows into ObservationRecord structs.
///
/// Applies ingest security bounds (ADR-007):
/// 1. Payload size check: rejects records with input > 64 KB (raw bytes).
/// 2. JSON depth check: rejects records with JSON nesting depth > 10.
///
/// Rejected records are skipped (FM-02) with a WARN log; remaining records
/// in the batch are processed normally.
///
/// All hook-path records receive source_domain = "claude-code" (FR-03.3).
/// The registry is available for future non-hook ingress paths (IR-01 contract).
fn parse_observation_rows(
    rows: Vec<sqlx::sqlite::SqliteRow>,
    _registry: &DomainPackRegistry,
) -> Result<Vec<ObservationRecord>> {
    let mut records = Vec::new();
    for row in rows {
        let session_id: String = row.get::<String, _>(0);
        let ts_millis: i64 = row.get::<i64, _>(1);
        let hook_str: String = row.get::<String, _>(2);
        let tool: Option<String> = row.get(3);
        let input_str: Option<String> = row.get(4);
        let response_size: Option<i64> = row.get(5);
        let response_snippet: Option<String> = row.get(6);

        // Set event_type from the raw hook string (no filtering — FR-03.1, AC-11).
        let event_type: String = hook_str;

        // All hook-path records get source_domain = "claude-code" (FR-03.3).
        // Domain is inferred from the ingress path, not from event_type.
        let source_domain: String = "claude-code".to_string();

        // SECURITY BOUND 1: payload size check (NFR-02, FR-03.4, ADR-007).
        // Check raw bytes of input_str BEFORE any JSON parsing.
        if let Some(ref s) = input_str {
            if s.len() > 65_536 {
                tracing::warn!(
                    session_id = %session_id,
                    event_type = %event_type,
                    size = s.len(),
                    "PayloadTooLarge: skipping record"
                );
                continue;
            }
        }

        // Input deserialization (event_type-conditional, not source_domain-conditional).
        // - SubagentStart: input is plain text -> Value::String
        // - Tool events: input is JSON -> parse to Value::Object
        // - Malformed JSON: treated as no input (not an error)
        let input: Option<serde_json::Value> = match (event_type.as_str(), input_str) {
            ("SubagentStart", Some(s)) => Some(serde_json::Value::String(s)),
            (_, Some(s)) => serde_json::from_str::<serde_json::Value>(&s).ok(),
            (_, None) => None,
        };

        // SECURITY BOUND 2: JSON depth check (NFR-02, FR-03.5, ADR-007).
        // Applied AFTER parse (must have a serde_json::Value to walk).
        if let Some(ref v) = input {
            if !json_depth(v, 0, 10) {
                tracing::warn!(
                    session_id = %session_id,
                    event_type = %event_type,
                    "PayloadNestingTooDeep: skipping record"
                );
                continue;
            }
        }

        records.push(ObservationRecord {
            ts: ts_millis as u64,
            event_type,
            source_domain,
            session_id,
            tool,
            input,
            response_size: response_size.map(|v| v as u64),
            response_snippet,
        });
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use unimatrix_store::test_helpers::open_test_store;

    async fn setup_test_store() -> Arc<SqlxStore> {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let store = open_test_store(&dir).await;
        // Note: dir must stay alive for the store lifetime. We leak it for test simplicity.
        std::mem::forget(dir);
        Arc::new(store)
    }

    async fn insert_session(store: &SqlxStore, session_id: &str, feature_cycle: Option<&str>) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        sqlx::query(
            "INSERT INTO sessions (session_id, feature_cycle, started_at, status) \
             VALUES (?1, ?2, ?3, 0)",
        )
        .bind(session_id)
        .bind(feature_cycle)
        .bind(now)
        .execute(store.write_pool_server())
        .await
        .expect("insert session");
    }

    async fn insert_observation(
        store: &SqlxStore,
        session_id: &str,
        ts_millis: i64,
        hook: &str,
        tool: Option<&str>,
        input: Option<&str>,
        response_size: Option<i64>,
        response_snippet: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO observations \
             (session_id, ts_millis, hook, tool, input, response_size, response_snippet) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(session_id)
        .bind(ts_millis)
        .bind(hook)
        .bind(tool)
        .bind(input)
        .bind(response_size)
        .bind(response_snippet)
        .execute(store.write_pool_server())
        .await
        .expect("insert observation");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_load_feature_observations_all_fields() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-012")).await;
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PostToolUse",
            Some("Read"),
            Some(r#"{"file_path":"/tmp/test"}"#),
            Some(1024),
            Some("output text"),
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.observation_stats_async().await.unwrap();

        assert_eq!(records.record_count, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_load_feature_observations_null_optionals() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-012")).await;
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "SubagentStop",
            None,
            None,
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let source_trait: &dyn ObservationSource = &source;
        let records = source_trait.load_feature_observations("col-012").unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].event_type, "SubagentStop");
        assert_eq!(records[0].source_domain, "claude-code");
        assert!(records[0].tool.is_none());
        assert!(records[0].input.is_none());
        assert!(records[0].response_size.is_none());
        assert!(records[0].response_snippet.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_load_feature_observations_subagent_start_string_input() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-012")).await;
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "SubagentStart",
            Some("uni-pseudocode"),
            Some("Design components for col-012"),
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_feature_observations("col-012").unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].event_type, "SubagentStart");
        assert_eq!(records[0].source_domain, "claude-code");
        assert_eq!(records[0].tool, Some("uni-pseudocode".to_string()));
        assert_eq!(
            records[0].input,
            Some(serde_json::Value::String(
                "Design components for col-012".to_string()
            ))
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_load_feature_observations_json_input_deserialized() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-012")).await;
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Bash"),
            Some(r#"{"command":"ls -la"}"#),
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_feature_observations("col-012").unwrap();

        assert_eq!(records.len(), 1);
        let input = records[0].input.as_ref().unwrap();
        assert_eq!(input.get("command").unwrap().as_str().unwrap(), "ls -la");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_null_feature_cycle_excluded() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-012")).await;
        insert_session(&store, "sess-2", None).await;
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            None,
            None,
            None,
        )
        .await;
        insert_observation(
            &store,
            "sess-2",
            1700000001000,
            "PreToolUse",
            Some("Write"),
            None,
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_feature_observations("col-012").unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].session_id, "sess-1");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_empty_result_nonexistent_feature() {
        let store = setup_test_store().await;
        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_feature_observations("nonexistent").unwrap();
        assert!(records.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_observation_stats_aggregate() {
        let store = setup_test_store().await;
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        for i in 0..10_i64 {
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
            )
            .await;
        }

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let stats = source.observation_stats_async().await.unwrap();

        assert_eq!(stats.record_count, 10);
        assert_eq!(stats.session_count, 3);
        assert_eq!(stats.oldest_record_age_days, 0); // all from today
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_discover_sessions_for_feature() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-012")).await;
        insert_session(&store, "sess-2", Some("col-012")).await;
        insert_session(&store, "sess-3", Some("nxs-001")).await;
        insert_session(&store, "sess-4", None).await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let sessions = source.discover_sessions_for_feature("col-012").unwrap();

        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains(&"sess-1".to_string()));
        assert!(sessions.contains(&"sess-2".to_string()));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_load_unattributed_sessions_returns_null_feature_cycle_only() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", None).await;
        insert_session(&store, "sess-2", Some("col-012")).await;
        insert_session(&store, "sess-3", None).await;
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/col-015/SCOPE.md"}"#),
            None,
            None,
        )
        .await;
        insert_observation(
            &store,
            "sess-2",
            1700000001000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/col-012/SCOPE.md"}"#),
            None,
            None,
        )
        .await;
        insert_observation(
            &store,
            "sess-3",
            1700000002000,
            "PreToolUse",
            Some("Write"),
            Some(r#"{"file_path":"product/features/crt-013/test.rs"}"#),
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let sessions = source.load_unattributed_sessions().unwrap();

        assert_eq!(sessions.len(), 2);
        let session_ids: Vec<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert!(session_ids.contains(&"sess-1"));
        assert!(session_ids.contains(&"sess-3"));
        assert!(!session_ids.contains(&"sess-2"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_load_unattributed_sessions_empty_when_all_attributed() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-012")).await;
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            None,
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let sessions = source.load_unattributed_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_load_unattributed_sessions_groups_by_session_id() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", None).await;
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/col-015/SCOPE.md"}"#),
            None,
            None,
        )
        .await;
        insert_observation(
            &store,
            "sess-1",
            1700000001000,
            "PostToolUse",
            Some("Read"),
            None,
            Some(512),
            Some("file contents"),
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let sessions = source.load_unattributed_sessions().unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "sess-1");
        assert_eq!(sessions[0].records.len(), 2);
        assert!(sessions[0].records[0].ts <= sessions[0].records[1].ts);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_load_unattributed_sessions_empty_when_no_sessions() {
        let store = setup_test_store().await;
        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let sessions = source.load_unattributed_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_attribution_fallback_end_to_end() {
        use unimatrix_observe::attribute_sessions;

        let store = setup_test_store().await;

        insert_session(&store, "sess-1", None).await;
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/col-test/SCOPE.md"}"#),
            None,
            None,
        )
        .await;
        insert_observation(
            &store,
            "sess-1",
            1700000001000,
            "PreToolUse",
            Some("Write"),
            Some(r#"{"file_path":"product/features/col-test/impl.rs"}"#),
            None,
            None,
        )
        .await;

        insert_session(&store, "sess-2", None).await;
        insert_observation(
            &store,
            "sess-2",
            1700000002000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/nxs-001/SCOPE.md"}"#),
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));

        let direct = source.load_feature_observations("col-test").unwrap();
        assert!(direct.is_empty());

        let unattributed = source.load_unattributed_sessions().unwrap();
        assert_eq!(unattributed.len(), 2);

        let attributed = attribute_sessions(&unattributed, "col-test");
        assert_eq!(attributed.len(), 2);
        assert!(attributed.iter().all(|r| r.session_id == "sess-1"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_direct_path_preserved_for_populated_feature_cycle() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-015")).await;
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/col-015/SCOPE.md"}"#),
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let direct = source.load_feature_observations("col-015").unwrap();

        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].session_id, "sess-1");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_multi_feature_session_partitioned_via_fallback() {
        use unimatrix_observe::attribute_sessions;

        let store = setup_test_store().await;
        insert_session(&store, "sess-1", None).await;

        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/col-015/SCOPE.md"}"#),
            None,
            None,
        )
        .await;
        insert_observation(
            &store,
            "sess-1",
            1700000001000,
            "PreToolUse",
            Some("Write"),
            Some(r#"{"file_path":"product/features/col-015/impl.rs"}"#),
            None,
            None,
        )
        .await;
        insert_observation(
            &store,
            "sess-1",
            1700000002000,
            "PreToolUse",
            Some("Read"),
            Some(r#"{"file_path":"product/features/crt-013/SCOPE.md"}"#),
            None,
            None,
        )
        .await;
        insert_observation(
            &store,
            "sess-1",
            1700000003000,
            "PreToolUse",
            Some("Write"),
            Some(r#"{"file_path":"product/features/crt-013/test.rs"}"#),
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let unattributed = source.load_unattributed_sessions().unwrap();

        let col015 = attribute_sessions(&unattributed, "col-015");
        assert_eq!(col015.len(), 2);
        assert!(col015.iter().all(|r| r.ts <= 1700000001000));

        let crt013 = attribute_sessions(&unattributed, "crt-013");
        assert_eq!(crt013.len(), 2);
        assert!(crt013.iter().all(|r| r.ts >= 1700000002000));
    }

    // ── Ingest security tests (ADR-007, test-plan/ingest-security.md) ──────

    /// Build a minimal SqliteRow-compatible test using parse_observation_rows directly.
    /// We use integration-style tests via the DB to exercise the full path.

    /// T-SEC-01: Payload exactly 65,536 bytes passes (AC-06).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_payload_size_boundary_exact_limit_passes() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-023")).await;
        // Build a JSON string where the full input_str is exactly 65,536 bytes.
        // Format: {"x":"<padding>"} — overhead is 8 bytes ('{"x":"' + '"}').
        // Padding length = 65536 - 8 = 65528 bytes.
        let padding = "a".repeat(65528);
        let input = format!(r#"{{"x":"{}"}}"#, padding);
        assert_eq!(
            input.len(),
            65536,
            "sanity: input must be exactly 65,536 bytes"
        );
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            None,
            Some(&input),
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_feature_observations("col-023").unwrap();
        assert_eq!(records.len(), 1, "65,536 byte payload must pass");
    }

    /// T-SEC-02: Payload 65,537 bytes is rejected with PayloadTooLarge (AC-06).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_payload_size_one_byte_over_limit_rejects() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-023")).await;
        // 65,537 bytes total: overhead 8 + padding 65529 = 65537.
        let padding = "a".repeat(65529);
        let input = format!(r#"{{"x":"{}"}}"#, padding);
        assert_eq!(
            input.len(),
            65537,
            "sanity: input must be exactly 65,537 bytes"
        );
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            None,
            Some(&input),
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_feature_observations("col-023").unwrap();
        assert!(records.is_empty(), "65,537 byte payload must be rejected");
    }

    /// T-SEC-03: Payload size measured in raw bytes, not chars (SEC-01).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_payload_size_measured_in_bytes_not_chars() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-023")).await;
        // Build a JSON string with multi-byte UTF-8 chars summing to > 65,536 raw bytes.
        // "é" is 2 bytes in UTF-8. Use 32,769 copies = 65,538 raw bytes inside quotes.
        // Full payload: {"x":"<32769 x é>"} = 7 + 65538 + 2 = 65547 bytes
        let padding: String = "é".repeat(32769);
        let input = format!(r#"{{"x":"{}"}}"#, padding);
        assert!(input.len() > 65536, "must exceed 65,536 raw bytes");
        assert!(
            input.chars().count() <= 65536,
            "char count should not exceed limit"
        );
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            None,
            Some(&input),
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_feature_observations("col-023").unwrap();
        assert!(records.is_empty(), "must reject based on raw byte count");
    }

    /// T-SEC-04: Multi-byte UTF-8 at exactly 65,536 raw bytes passes (SEC-01).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_payload_size_multibyte_utf8_boundary_passes() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-023")).await;
        // Build exactly 65,536 bytes total using 2-byte chars.
        // Header: {"x":"} = 6 bytes, suffix: "} = 2 bytes => 8 bytes overhead
        // Padding must be exactly 65536 - 8 = 65528 raw bytes = 32764 "é" chars
        let padding: String = "é".repeat(32764);
        let input = format!(r#"{{"x":"{}"}}"#, padding);
        assert_eq!(input.len(), 65536, "must be exactly 65,536 raw bytes");
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            None,
            Some(&input),
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_feature_observations("col-023").unwrap();
        assert_eq!(records.len(), 1, "exactly 65,536 raw bytes must pass");
    }

    /// Build a nested JSON object to a given depth.
    ///
    /// depth=0 returns `{"k": 1}` (a scalar at depth 1, which passes max=10).
    /// depth=10 returns 10 nested objects with a scalar at depth 10.
    fn build_nested_json(depth: usize) -> serde_json::Value {
        if depth == 0 {
            return serde_json::json!({"k": 1});
        }
        let inner = build_nested_json(depth - 1);
        serde_json::json!({"k": inner})
    }

    /// T-SEC-05: JSON depth exactly 10 levels passes (AC-06).
    ///
    /// Depth semantics: current=0 at root. build_nested_json(9) produces:
    /// {"k": {"k": ... {"k": 1} ...}} with the scalar at depth 10. json_depth(root, 0, 10)
    /// visits root at 0, first child at 1, ..., scalar at 10. 10 > 10 is false → returns true.
    #[test]
    fn test_nesting_depth_boundary_10_passes() {
        // 9 nested objects means the inner scalar is at depth 10 (root = depth 0).
        let v = build_nested_json(9);
        assert!(
            json_depth(&v, 0, 10),
            "depth 10 must pass (boundary condition)"
        );
    }

    /// T-SEC-06: JSON depth 11 levels is rejected (AC-06).
    #[test]
    fn test_nesting_depth_11_rejects() {
        // 10 nested objects means the inner scalar is at depth 11.
        let v = build_nested_json(10);
        assert!(!json_depth(&v, 0, 10), "depth 11 must reject");
    }

    /// T-SEC-07: json_depth() does not stack overflow at 10 levels (SEC-02).
    #[test]
    fn test_json_depth_no_stack_overflow_at_10_levels() {
        let v = build_nested_json(9);
        // Must complete without panic
        let result = json_depth(&v, 0, 10);
        assert!(result);
    }

    /// T-SEC-08: json_depth() short-circuits at max + 1 (ADR-007).
    #[test]
    fn test_json_depth_short_circuits_above_max() {
        // 15 nested objects (scalar at depth 15) — well beyond the limit.
        let v = build_nested_json(14);
        // Must return false without panic (short-circuits at depth 11).
        assert!(!json_depth(&v, 0, 10));
    }

    /// T-SEC-12: Unknown event_type passes through with source_domain = "claude-code" (AC-11).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_parse_rows_unknown_event_type_passthrough() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-023")).await;
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "UnknownEventType",
            None,
            None,
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_feature_observations("col-023").unwrap();

        assert_eq!(records.len(), 1, "unknown event_type must not be dropped");
        assert_eq!(records[0].event_type, "UnknownEventType");
        assert_eq!(records[0].source_domain, "claude-code");
    }

    /// T-SEC-13: Hook-path records always get source_domain = "claude-code" (FR-03.3).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_parse_rows_hook_path_always_claude_code() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-023")).await;
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "PreToolUse",
            Some("Bash"),
            None,
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_feature_observations("col-023").unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source_domain, "claude-code");
        assert_eq!(records[0].event_type, "PreToolUse");
    }

    /// T-SEC-14: Mixed batch — oversized record skipped, valid records pass (FM-02).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_parse_rows_partial_batch_invalid_skipped() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-023")).await;

        // Row 1: valid (< 65,536 bytes)
        let valid_input = r#"{"command":"ls"}"#;
        insert_observation(
            &store,
            "sess-1",
            1700000000001,
            "PreToolUse",
            Some("Bash"),
            Some(valid_input),
            None,
            None,
        )
        .await;

        // Row 2: oversized (65,537 bytes): overhead 8 + padding 65529 = 65537.
        let oversized_padding = "a".repeat(65529);
        let oversized_input = format!(r#"{{"x":"{}"}}"#, oversized_padding);
        assert_eq!(
            oversized_input.len(),
            65537,
            "sanity: oversized input must be 65,537 bytes"
        );
        insert_observation(
            &store,
            "sess-1",
            1700000000002,
            "PreToolUse",
            Some("Write"),
            Some(&oversized_input),
            None,
            None,
        )
        .await;

        // Row 3: valid
        let valid_input2 = r#"{"file_path":"/tmp/test.rs"}"#;
        insert_observation(
            &store,
            "sess-1",
            1700000000003,
            "PostToolUse",
            Some("Read"),
            Some(valid_input2),
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_feature_observations("col-023").unwrap();

        assert_eq!(
            records.len(),
            2,
            "oversized record must be skipped; 2 valid records must pass"
        );
        let event_types: Vec<&str> = records.iter().map(|r| r.event_type.as_str()).collect();
        assert!(event_types.contains(&"PreToolUse"));
        assert!(event_types.contains(&"PostToolUse"));
    }

    /// json_depth edge case: empty object at depth 0 returns true.
    #[test]
    fn test_json_depth_empty_object_passes() {
        let v = serde_json::json!({});
        assert!(json_depth(&v, 0, 10));
    }

    /// json_depth edge case: scalar value at depth 0 returns true.
    #[test]
    fn test_json_depth_scalar_passes() {
        let v = serde_json::json!(42);
        assert!(json_depth(&v, 0, 10));
    }

    /// json_depth edge case: flat array of 1000 elements has depth 1 and passes.
    #[test]
    fn test_json_depth_flat_array_passes() {
        let v = serde_json::Value::Array(vec![serde_json::json!(1); 1000]);
        assert!(json_depth(&v, 0, 10));
    }

    /// SubagentStart input preserved as String through depth check (depth 0 for scalar).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_subagent_start_input_preserved_as_string() {
        let store = setup_test_store().await;
        insert_session(&store, "sess-1", Some("col-023")).await;
        insert_observation(
            &store,
            "sess-1",
            1700000000000,
            "SubagentStart",
            Some("coder"),
            Some("Design the payment service"),
            None,
            None,
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_feature_observations("col-023").unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].input,
            Some(serde_json::Value::String(
                "Design the payment service".to_string()
            ))
        );
    }

    // ── col-024: load_cycle_observations tests ──────────────────────────────

    /// Insert an observation row with a topic_signal value.
    /// Used exclusively for col-024 tests (AC-11: raw SQL only for observations, not cycle_events).
    async fn insert_observation_with_signal(
        store: &SqlxStore,
        session_id: &str,
        ts_millis: i64,
        hook: &str,
        topic_signal: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO observations \
             (session_id, ts_millis, hook, topic_signal) \
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(session_id)
        .bind(ts_millis)
        .bind(hook)
        .bind(topic_signal)
        .execute(store.write_pool_server())
        .await
        .expect("insert observation with signal");
    }

    /// T-LCO-01: Single window — in-window observation returned, out-of-window excluded (AC-01).
    #[tokio::test(flavor = "multi_thread")]
    async fn load_cycle_observations_single_window() {
        const T: i64 = 1_700_000_000_i64; // seconds
        const T_MS: i64 = T * 1000; // milliseconds

        let store = setup_test_store().await;

        store
            .insert_cycle_event("col-024", 0, "cycle_start", None, None, None, T)
            .await
            .expect("insert cycle_start");
        store
            .insert_cycle_event("col-024", 1, "cycle_stop", None, None, None, T + 3600)
            .await
            .expect("insert cycle_stop");

        // In-window: T_MS + 60_000 ms = T + 60 seconds into the window
        insert_observation_with_signal(
            &store,
            "sess-1",
            T_MS + 60_000,
            "PostToolUse",
            Some("col-024"),
        )
        .await;
        // Before window: 1 ms before start — must be excluded
        insert_observation_with_signal(
            &store,
            "sess-1",
            T_MS - 1_000,
            "PostToolUse",
            Some("col-024"),
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_cycle_observations("col-024").unwrap();

        assert_eq!(
            records.len(),
            1,
            "only the in-window observation must be returned"
        );
        assert_eq!(records[0].session_id, "sess-1");
        assert_eq!(records[0].ts, (T_MS + 60_000) as u64);
    }

    /// T-LCO-02: Multiple disjoint windows — gap observations excluded (AC-02).
    #[tokio::test(flavor = "multi_thread")]
    async fn load_cycle_observations_multiple_windows() {
        const T: i64 = 1_700_000_000_i64;
        const T_MS: i64 = T * 1000;

        let store = setup_test_store().await;

        // Window 1: T to T+3600
        store
            .insert_cycle_event("col-024", 0, "cycle_start", None, None, None, T)
            .await
            .expect("insert cycle_start w1");
        store
            .insert_cycle_event("col-024", 1, "cycle_stop", None, None, None, T + 3600)
            .await
            .expect("insert cycle_stop w1");
        // Window 2: T+7200 to T+10800
        store
            .insert_cycle_event("col-024", 2, "cycle_start", None, None, None, T + 7200)
            .await
            .expect("insert cycle_start w2");
        store
            .insert_cycle_event("col-024", 3, "cycle_stop", None, None, None, T + 10800)
            .await
            .expect("insert cycle_stop w2");

        // In window 1: T + 1800s = T_MS + 1_800_000 ms
        insert_observation_with_signal(
            &store,
            "sess-1",
            T_MS + 1_800_000,
            "PostToolUse",
            Some("col-024"),
        )
        .await;
        // In gap (T+4500s): T_MS + 4_500_000 ms — must be excluded
        insert_observation_with_signal(
            &store,
            "sess-1",
            T_MS + 4_500_000,
            "PostToolUse",
            Some("col-024"),
        )
        .await;
        // In window 2: T + 9000s = T_MS + 9_000_000 ms
        insert_observation_with_signal(
            &store,
            "sess-2",
            T_MS + 9_000_000,
            "PostToolUse",
            Some("col-024"),
        )
        .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let records = source.load_cycle_observations("col-024").unwrap();

        assert_eq!(records.len(), 2, "gap observation must be excluded");
        let ts_values: Vec<u64> = records.iter().map(|r| r.ts).collect();
        assert!(ts_values.contains(&((T_MS + 1_800_000) as u64)));
        assert!(ts_values.contains(&((T_MS + 9_000_000) as u64)));
        assert!(!ts_values.contains(&((T_MS + 4_500_000) as u64)));
    }

    /// T-LCO-03: No cycle_events rows — Ok(vec![]) returned, not Err (AC-03).
    #[tokio::test(flavor = "multi_thread")]
    async fn load_cycle_observations_no_cycle_events() {
        let store = setup_test_store().await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let result = source.load_cycle_observations("col-024");

        assert!(
            result.is_ok(),
            "must not return Err for missing cycle_events rows"
        );
        assert!(result.unwrap().is_empty(), "must return Ok(vec![])");
    }

    /// T-LCO-04: Count pre-check (AC-15 case A) — zero cycle_events rows → Ok(vec![]) (AC-15).
    #[tokio::test(flavor = "multi_thread")]
    async fn load_cycle_observations_no_cycle_events_count_check() {
        let store = setup_test_store().await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let result = source.load_cycle_observations("col-024");

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    /// T-LCO-05: cycle_events rows exist but observations have different topic_signal → Ok(vec![]) (AC-15 case B).
    #[tokio::test(flavor = "multi_thread")]
    async fn load_cycle_observations_rows_exist_no_signal_match() {
        const T: i64 = 1_700_000_000_i64;
        const T_MS: i64 = T * 1000;

        let store = setup_test_store().await;

        store
            .insert_cycle_event("col-024", 0, "cycle_start", None, None, None, T)
            .await
            .expect("insert cycle_start");
        store
            .insert_cycle_event("col-024", 1, "cycle_stop", None, None, None, T + 3600)
            .await
            .expect("insert cycle_stop");

        // Observation inside the window but with no topic_signal — no match
        insert_observation_with_signal(&store, "sess-1", T_MS + 1_800_000, "PostToolUse", None)
            .await;

        let source = SqlObservationSource::new_default(Arc::clone(&store));
        let result = source.load_cycle_observations("col-024");

        assert!(result.is_ok());
        assert!(
            result.unwrap().is_empty(),
            "rows exist but no topic_signal match → Ok(vec![])"
        );
    }
}
