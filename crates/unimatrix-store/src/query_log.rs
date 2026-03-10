//! Query log persistence for nxs-010.
//!
//! Provides insert and scan operations on the `query_log` table.
//! The table uses SQLite AUTOINCREMENT for primary key allocation (ADR-001).
//! All operations are synchronous; callers in async contexts use `spawn_blocking`.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::db::Store;
use crate::error::{Result, StoreError};

// -- Types --

/// A single query log entry capturing search telemetry.
///
/// `query_id` is 0 on insert (AUTOINCREMENT allocates) and populated on read.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryLogRecord {
    /// Auto-allocated primary key. Set to 0 on insert; populated on read.
    pub query_id: i64,
    /// Session that issued this query.
    pub session_id: String,
    /// The raw query text.
    pub query_text: String,
    /// Unix epoch seconds when the query was executed.
    pub ts: u64,
    /// Number of results returned.
    pub result_count: i64,
    /// JSON array of entry IDs returned (e.g. `"[1,2,3]"`).
    pub result_entry_ids: String,
    /// JSON array of similarity scores (e.g. `"[0.95,0.87]"`).
    pub similarity_scores: String,
    /// Retrieval mode: `"strict"` or `"flexible"`.
    pub retrieval_mode: String,
    /// Source transport: `"uds"` or `"mcp"`.
    pub source: String,
}

// -- Shared constructor (FR-08.1) --

impl QueryLogRecord {
    /// Construct a new `QueryLogRecord` with consistent field population.
    ///
    /// Both UDS and MCP paths must use this constructor to ensure field parity (FR-08.1).
    /// The `query_id` is set to 0 and will be allocated by AUTOINCREMENT on insert.
    /// Timestamp is captured as the current system time.
    pub fn new(
        session_id: String,
        query_text: String,
        entry_ids: &[u64],
        similarity_scores: &[f64],
        retrieval_mode: &str,
        source: &str,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        QueryLogRecord {
            query_id: 0,
            session_id,
            query_text,
            ts: now,
            result_count: entry_ids.len() as i64,
            result_entry_ids: serde_json::to_string(entry_ids).unwrap_or_default(),
            similarity_scores: serde_json::to_string(similarity_scores).unwrap_or_default(),
            retrieval_mode: retrieval_mode.to_string(),
            source: source.to_string(),
        }
    }
}

// -- Row helper --

fn row_to_query_log(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueryLogRecord> {
    Ok(QueryLogRecord {
        query_id: row.get(0)?,
        session_id: row.get(1)?,
        query_text: row.get(2)?,
        ts: row.get::<_, i64>(3)? as u64,
        result_count: row.get(4)?,
        result_entry_ids: row.get(5)?,
        similarity_scores: row.get(6)?,
        retrieval_mode: row.get(7)?,
        source: row.get(8)?,
    })
}

// -- Store methods --

impl Store {
    /// Insert a query log record. The `query_id` field is ignored;
    /// SQLite AUTOINCREMENT allocates the primary key (ADR-001).
    pub fn insert_query_log(&self, record: &QueryLogRecord) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO query_log \
                (session_id, query_text, ts, result_count, \
                 result_entry_ids, similarity_scores, retrieval_mode, source) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                record.session_id,
                record.query_text,
                record.ts as i64,
                record.result_count,
                record.result_entry_ids,
                record.similarity_scores,
                record.retrieval_mode,
                record.source,
            ],
        )
        .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    /// Scan query log records for multiple sessions, ordered by timestamp ascending.
    ///
    /// Session IDs are batched into chunks of 50 to avoid large IN clauses (R-11).
    /// Returns an empty Vec if `session_ids` is empty or no rows match.
    pub fn scan_query_log_by_sessions(&self, session_ids: &[&str]) -> Result<Vec<QueryLogRecord>> {
        if session_ids.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.lock_conn();
        let mut all_results: Vec<QueryLogRecord> = Vec::new();

        for chunk in session_ids.chunks(50) {
            let placeholders: String = chunk
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect::<Vec<_>>()
                .join(",");

            let sql = format!(
                "SELECT query_id, session_id, query_text, ts, result_count, \
                        result_entry_ids, similarity_scores, retrieval_mode, source \
                 FROM query_log \
                 WHERE session_id IN ({placeholders}) \
                 ORDER BY ts ASC"
            );

            let mut stmt = conn.prepare(&sql).map_err(StoreError::Sqlite)?;
            let params: Vec<Box<dyn rusqlite::types::ToSql>> = chunk
                .iter()
                .map(|id| Box::new(id.to_string()) as Box<dyn rusqlite::types::ToSql>)
                .collect();

            let rows = stmt
                .query_map(rusqlite::params_from_iter(params.iter()), row_to_query_log)
                .map_err(StoreError::Sqlite)?;

            for row in rows {
                all_results.push(row.map_err(StoreError::Sqlite)?);
            }
        }

        Ok(all_results)
    }

    /// Scan all query log records for a given session, ordered by timestamp ascending.
    ///
    /// Returns an empty Vec if no rows match (not an error).
    pub fn scan_query_log_by_session(&self, session_id: &str) -> Result<Vec<QueryLogRecord>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare(
                "SELECT query_id, session_id, query_text, ts, result_count, \
                        result_entry_ids, similarity_scores, retrieval_mode, source \
                 FROM query_log \
                 WHERE session_id = ?1 \
                 ORDER BY ts ASC",
            )
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params![session_id], row_to_query_log)
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }
}

// -- Tests --

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::TestDb;

    /// Helper: build a QueryLogRecord with explicit field values for testing.
    fn make_record(
        session_id: &str,
        query_text: &str,
        ts: u64,
        entry_ids: &[u64],
        scores: &[f64],
        retrieval_mode: &str,
        source: &str,
    ) -> QueryLogRecord {
        QueryLogRecord {
            query_id: 0,
            session_id: session_id.to_string(),
            query_text: query_text.to_string(),
            ts,
            result_count: entry_ids.len() as i64,
            result_entry_ids: serde_json::to_string(entry_ids).unwrap(),
            similarity_scores: serde_json::to_string(scores).unwrap(),
            retrieval_mode: retrieval_mode.to_string(),
            source: source.to_string(),
        }
    }

    #[test]
    fn test_insert_query_log_autoincrement() {
        let db = TestDb::new();
        let store = db.store();

        let r1 = make_record(
            "sess-1",
            "query one",
            1000,
            &[1, 2],
            &[0.9, 0.8],
            "strict",
            "uds",
        );
        let r2 = make_record("sess-1", "query two", 2000, &[3], &[0.7], "flexible", "mcp");

        store.insert_query_log(&r1).unwrap();
        store.insert_query_log(&r2).unwrap();

        let rows = store.scan_query_log_by_session("sess-1").unwrap();
        assert_eq!(rows.len(), 2);
        assert!(
            rows[0].query_id > 0,
            "first query_id should be auto-allocated"
        );
        assert!(
            rows[1].query_id > rows[0].query_id,
            "query_ids should be monotonically increasing"
        );
    }

    #[test]
    fn test_insert_query_log_ignores_provided_query_id() {
        let db = TestDb::new();
        let store = db.store();

        let mut record = make_record("sess-1", "query", 1000, &[1], &[0.5], "strict", "uds");
        record.query_id = 999;

        store.insert_query_log(&record).unwrap();

        let rows = store.scan_query_log_by_session("sess-1").unwrap();
        assert_eq!(rows.len(), 1);
        // On a fresh DB, AUTOINCREMENT starts at 1, not 999.
        assert_eq!(rows[0].query_id, 1);
    }

    #[test]
    fn test_scan_query_log_by_session_ordered_by_ts_asc() {
        let db = TestDb::new();
        let store = db.store();

        // Insert out of order: 300, 100, 200
        store
            .insert_query_log(&make_record("sess-1", "q1", 300, &[], &[], "strict", "uds"))
            .unwrap();
        store
            .insert_query_log(&make_record("sess-1", "q2", 100, &[], &[], "strict", "uds"))
            .unwrap();
        store
            .insert_query_log(&make_record("sess-1", "q3", 200, &[], &[], "strict", "uds"))
            .unwrap();

        let rows = store.scan_query_log_by_session("sess-1").unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].ts, 100);
        assert_eq!(rows[1].ts, 200);
        assert_eq!(rows[2].ts, 300);
    }

    #[test]
    fn test_scan_query_log_by_session_filters_correctly() {
        let db = TestDb::new();
        let store = db.store();

        store
            .insert_query_log(&make_record(
                "sess-a",
                "qa1",
                100,
                &[],
                &[],
                "strict",
                "uds",
            ))
            .unwrap();
        store
            .insert_query_log(&make_record(
                "sess-a",
                "qa2",
                200,
                &[],
                &[],
                "strict",
                "uds",
            ))
            .unwrap();
        store
            .insert_query_log(&make_record(
                "sess-b",
                "qb1",
                300,
                &[],
                &[],
                "strict",
                "mcp",
            ))
            .unwrap();
        store
            .insert_query_log(&make_record(
                "sess-b",
                "qb2",
                400,
                &[],
                &[],
                "strict",
                "mcp",
            ))
            .unwrap();
        store
            .insert_query_log(&make_record(
                "sess-b",
                "qb3",
                500,
                &[],
                &[],
                "strict",
                "mcp",
            ))
            .unwrap();

        let rows_a = store.scan_query_log_by_session("sess-a").unwrap();
        assert_eq!(rows_a.len(), 2);
        assert!(rows_a.iter().all(|r| r.session_id == "sess-a"));
    }

    #[test]
    fn test_scan_query_log_by_session_empty() {
        let db = TestDb::new();
        let store = db.store();

        let rows = store.scan_query_log_by_session("nonexistent").unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_query_log_json_round_trip_empty_results() {
        let db = TestDb::new();
        let store = db.store();

        let record = make_record("sess-1", "empty query", 1000, &[], &[], "strict", "uds");
        store.insert_query_log(&record).unwrap();

        let rows = store.scan_query_log_by_session("sess-1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].result_count, 0);

        let ids: Vec<u64> = serde_json::from_str(&rows[0].result_entry_ids).unwrap();
        assert!(ids.is_empty());

        let scores: Vec<f64> = serde_json::from_str(&rows[0].similarity_scores).unwrap();
        assert!(scores.is_empty());
    }

    #[test]
    fn test_query_log_json_round_trip_multiple_results() {
        let db = TestDb::new();
        let store = db.store();

        let entry_ids = vec![1u64, 2, 3, 100];
        let scores = vec![0.95, 0.87, 0.0, 1.0];
        let record = make_record(
            "sess-1",
            "multi query",
            1000,
            &entry_ids,
            &scores,
            "flexible",
            "mcp",
        );
        store.insert_query_log(&record).unwrap();

        let rows = store.scan_query_log_by_session("sess-1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].result_count, 4);

        let read_ids: Vec<u64> = serde_json::from_str(&rows[0].result_entry_ids).unwrap();
        assert_eq!(read_ids, vec![1, 2, 3, 100]);

        let read_scores: Vec<f64> = serde_json::from_str(&rows[0].similarity_scores).unwrap();
        assert_eq!(read_scores, vec![0.95, 0.87, 0.0, 1.0]);
    }

    #[test]
    fn test_query_log_json_round_trip_single_result() {
        let db = TestDb::new();
        let store = db.store();

        let record = make_record("sess-1", "single", 1000, &[42], &[0.5], "strict", "uds");
        store.insert_query_log(&record).unwrap();

        let rows = store.scan_query_log_by_session("sess-1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].result_count, 1);

        let ids: Vec<u64> = serde_json::from_str(&rows[0].result_entry_ids).unwrap();
        assert_eq!(ids, vec![42]);

        let scores: Vec<f64> = serde_json::from_str(&rows[0].similarity_scores).unwrap();
        assert_eq!(scores, vec![0.5]);
    }

    #[test]
    fn test_query_log_all_fields_round_trip() {
        let db = TestDb::new();
        let store = db.store();

        let record = QueryLogRecord {
            query_id: 0,
            session_id: "test-session".to_string(),
            query_text: "how to handle errors in Rust".to_string(),
            ts: 1_700_000_000,
            result_count: 2,
            result_entry_ids: "[1,2]".to_string(),
            similarity_scores: "[0.9,0.8]".to_string(),
            retrieval_mode: "strict".to_string(),
            source: "uds".to_string(),
        };
        store.insert_query_log(&record).unwrap();

        let rows = store.scan_query_log_by_session("test-session").unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert!(row.query_id > 0);
        assert_eq!(row.session_id, "test-session");
        assert_eq!(row.query_text, "how to handle errors in Rust");
        assert_eq!(row.ts, 1_700_000_000);
        assert_eq!(row.result_count, 2);
        assert_eq!(row.result_entry_ids, "[1,2]");
        assert_eq!(row.similarity_scores, "[0.9,0.8]");
        assert_eq!(row.retrieval_mode, "strict");
        assert_eq!(row.source, "uds");
    }

    #[test]
    fn test_query_log_source_values() {
        let db = TestDb::new();
        let store = db.store();

        store
            .insert_query_log(&make_record(
                "sess-1",
                "q1",
                1000,
                &[],
                &[],
                "strict",
                "uds",
            ))
            .unwrap();
        store
            .insert_query_log(&make_record(
                "sess-1",
                "q2",
                2000,
                &[],
                &[],
                "strict",
                "mcp",
            ))
            .unwrap();

        let rows = store.scan_query_log_by_session("sess-1").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].source, "uds");
        assert_eq!(rows[1].source, "mcp");
    }

    #[test]
    fn test_query_log_retrieval_mode_values() {
        let db = TestDb::new();
        let store = db.store();

        store
            .insert_query_log(&make_record(
                "sess-1",
                "q1",
                1000,
                &[],
                &[],
                "strict",
                "uds",
            ))
            .unwrap();
        store
            .insert_query_log(&make_record(
                "sess-1",
                "q2",
                2000,
                &[],
                &[],
                "flexible",
                "mcp",
            ))
            .unwrap();

        let rows = store.scan_query_log_by_session("sess-1").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].retrieval_mode, "strict");
        assert_eq!(rows[1].retrieval_mode, "flexible");
    }

    #[test]
    fn test_query_log_new_constructor_field_parity() {
        let entry_ids: Vec<u64> = vec![10, 20, 30];
        let scores: Vec<f64> = vec![0.9, 0.8, 0.7];

        let record = QueryLogRecord::new(
            "sess-ctor".to_string(),
            "constructor test query".to_string(),
            &entry_ids,
            &scores,
            "flexible",
            "mcp",
        );

        // result_count derived from entry_ids length
        assert_eq!(record.result_count, 3);

        // query_id is 0 (to be allocated on insert)
        assert_eq!(record.query_id, 0);

        // ts is recent (within last 5 seconds)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(record.ts <= now);
        assert!(record.ts >= now - 5);

        // JSON arrays are valid
        let read_ids: Vec<u64> = serde_json::from_str(&record.result_entry_ids).unwrap();
        assert_eq!(read_ids, vec![10, 20, 30]);

        let read_scores: Vec<f64> = serde_json::from_str(&record.similarity_scores).unwrap();
        assert_eq!(read_scores, vec![0.9, 0.8, 0.7]);

        // Other fields
        assert_eq!(record.session_id, "sess-ctor");
        assert_eq!(record.query_text, "constructor test query");
        assert_eq!(record.retrieval_mode, "flexible");
        assert_eq!(record.source, "mcp");

        // Verify round-trip through store
        let db = TestDb::new();
        let store = db.store();
        store.insert_query_log(&record).unwrap();

        let rows = store.scan_query_log_by_session("sess-ctor").unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].query_id > 0);
        assert_eq!(rows[0].result_count, 3);
    }

    // -- scan_query_log_by_sessions tests (col-020 C4) --

    #[test]
    fn test_scan_query_log_by_sessions_returns_matching() {
        let db = TestDb::new();
        let store = db.store();

        // Insert 5 rows across 3 sessions
        store
            .insert_query_log(&make_record("s1", "q1", 100, &[1], &[0.9], "strict", "uds"))
            .unwrap();
        store
            .insert_query_log(&make_record("s1", "q2", 200, &[2], &[0.8], "strict", "uds"))
            .unwrap();
        store
            .insert_query_log(&make_record("s2", "q3", 300, &[3], &[0.7], "strict", "mcp"))
            .unwrap();
        store
            .insert_query_log(&make_record(
                "s3",
                "q4",
                400,
                &[4],
                &[0.6],
                "flexible",
                "uds",
            ))
            .unwrap();
        store
            .insert_query_log(&make_record(
                "s3",
                "q5",
                500,
                &[5],
                &[0.5],
                "flexible",
                "mcp",
            ))
            .unwrap();

        // Query for s1 and s2 only
        let rows = store.scan_query_log_by_sessions(&["s1", "s2"]).unwrap();
        assert_eq!(rows.len(), 3);
        assert!(
            rows.iter()
                .all(|r| r.session_id == "s1" || r.session_id == "s2")
        );
        // Verify ordering by ts ascending
        assert!(rows[0].ts <= rows[1].ts);
        assert!(rows[1].ts <= rows[2].ts);
    }

    #[test]
    fn test_scan_query_log_by_sessions_empty_ids() {
        let db = TestDb::new();
        let store = db.store();

        store
            .insert_query_log(&make_record("s1", "q1", 100, &[], &[], "strict", "uds"))
            .unwrap();

        let rows = store.scan_query_log_by_sessions(&[]).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_scan_query_log_by_sessions_no_matching() {
        let db = TestDb::new();
        let store = db.store();

        store
            .insert_query_log(&make_record("s1", "q1", 100, &[], &[], "strict", "uds"))
            .unwrap();

        let rows = store.scan_query_log_by_sessions(&["s99"]).unwrap();
        assert!(rows.is_empty());
    }
}
