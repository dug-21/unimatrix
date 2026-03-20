//! Scenario extraction from a snapshot database (D2, nan-007).
//!
//! Scans the `query_log` table in a read-only snapshot and writes one JSONL
//! line per row as a `ScenarioRecord`. Supports filtering by `source`
//! (`mcp`, `uds`, `all`) and an optional row limit.
//!
//! This module never calls `SqlxStore::open()` (C-02). All DB access uses
//! a raw `SqlitePool` opened with `SqliteConnectOptions::read_only(true)`.
//! Async sqlx queries are bridged to the synchronous CLI dispatch path via
//! `block_export_sync` (C-09, ADR-005).
//!
//! # Notes on actual `query_log` schema
//!
//! The real schema (from `migration.rs`) has these columns:
//! `query_id`, `session_id`, `query_text`, `ts`, `result_count`,
//! `result_entry_ids`, `similarity_scores`, `retrieval_mode`, `source`.
//!
//! There is no `agent_id` or `feature_cycle` column. The pseudocode assumed
//! those columns — they are absent. `ScenarioContext.agent_id` is populated
//! from `session_id` as the closest available identifier, and `feature_cycle`
//! defaults to `""`.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqliteConnectOptions;

use crate::export::block_export_sync;
use crate::project;

// ---------------------------------------------------------------------------
// ScenarioSource
// ---------------------------------------------------------------------------

/// Filter for `eval scenarios --source`.
///
/// Controls which `query_log` rows are included in the output JSONL based
/// on the `source` column value (`"mcp"` or `"uds"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ScenarioSource {
    /// Include only rows with `source = "mcp"`.
    Mcp,
    /// Include only rows with `source = "uds"`.
    Uds,
    /// Include all rows regardless of source.
    All,
}

impl ScenarioSource {
    /// Returns the SQL literal to match against `source`, or `None` for `All`.
    pub fn to_sql_filter(self) -> Option<&'static str> {
        match self {
            ScenarioSource::Mcp => Some("mcp"),
            ScenarioSource::Uds => Some("uds"),
            ScenarioSource::All => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ScenarioRecord and sub-types
// ---------------------------------------------------------------------------

/// A single eval scenario derived from a `query_log` row.
///
/// Written as one JSONL line per record. `expected` is always `null` for
/// query-log-sourced scenarios (hand-authored scenarios may set it non-null,
/// but that is not produced by this module).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioRecord {
    /// Unique scenario identifier, formatted as `"qlog-{query_id}"`.
    pub id: String,
    /// The query text from the log.
    pub query: String,
    /// Execution context metadata.
    pub context: ScenarioContext,
    /// Baseline search results at log time, or `null` if no results were returned.
    pub baseline: Option<ScenarioBaseline>,
    /// Source transport: `"mcp"` or `"uds"`.
    pub source: String,
    /// Hard labels for the expected result set. Always `null` for log-sourced scenarios.
    pub expected: Option<Vec<u64>>,
}

/// Execution context metadata extracted from the query log row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioContext {
    /// Agent identifier. Populated from `session_id` (no dedicated column exists).
    pub agent_id: String,
    /// Feature cycle. Empty string — not stored in `query_log`.
    pub feature_cycle: String,
    /// Session identifier from `query_log.session_id`.
    pub session_id: String,
    /// Retrieval mode: `"flexible"` or `"strict"`. Defaults to `"flexible"` if absent.
    pub retrieval_mode: String,
}

/// Baseline search results captured at query time.
///
/// `entry_ids` and `scores` are parallel arrays; their lengths are always equal
/// (enforced at extraction time per R-16).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioBaseline {
    /// Ordered list of result entry IDs.
    pub entry_ids: Vec<u64>,
    /// Similarity scores parallel to `entry_ids`.
    pub scores: Vec<f32>,
}

// ---------------------------------------------------------------------------
// run_scenarios (public entry point)
// ---------------------------------------------------------------------------

/// Extract scenarios from a snapshot database and write JSONL to `out`.
///
/// This is the entry point called from `run_eval_command` via `main.rs`.
/// It is synchronous (pre-tokio dispatch per ADR-005) and bridges to async
/// sqlx via `block_export_sync`.
///
/// # Live-DB path guard (C-13)
///
/// If the supplied `db` path resolves to the active daemon database, an error
/// is returned before any I/O is performed. The guard is best-effort: if the
/// project directory is not found (e.g. CI without a home directory), the guard
/// is skipped and execution continues.
pub fn run_scenarios(
    db: &Path,
    source: ScenarioSource,
    limit: Option<usize>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // ----------------------------------------------------------------
    // Step 1: Live-DB path guard (C-13, mirrors snapshot guard)
    // ----------------------------------------------------------------
    if let Ok(paths) = project::ensure_data_directory(None, None) {
        let active_db =
            std::fs::canonicalize(&paths.db_path).unwrap_or_else(|_| paths.db_path.clone());

        if let Ok(db_resolved) = std::fs::canonicalize(db) {
            if db_resolved == active_db {
                return Err(format!(
                    "eval scenarios --db resolves to the active database\n  \
                     supplied: {}\n  \
                     active:   {}\n  \
                     use a snapshot, not the live database",
                    db.display(),
                    active_db.display()
                )
                .into());
            }
        }
    }

    // ----------------------------------------------------------------
    // Step 2: Validate output parent directory exists
    // ----------------------------------------------------------------
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(format!("output directory does not exist: {}", parent.display()).into());
        }
    }

    // ----------------------------------------------------------------
    // Step 3: Bridge to async
    // ----------------------------------------------------------------
    block_export_sync(async { do_scenarios(db, source, limit, out).await })
}

// ---------------------------------------------------------------------------
// do_scenarios (private async body)
// ---------------------------------------------------------------------------

async fn do_scenarios(
    db: &Path,
    source: ScenarioSource,
    limit: Option<usize>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // ----------------------------------------------------------------
    // Open read-only pool (C-02, FR-11)
    // ----------------------------------------------------------------
    let opts = SqliteConnectOptions::new().filename(db).read_only(true);

    let pool = sqlx::SqlitePool::connect_with(opts).await?;

    // ----------------------------------------------------------------
    // Open output file
    // ----------------------------------------------------------------
    let file = File::create(out)?;
    let mut writer = BufWriter::new(file);

    // ----------------------------------------------------------------
    // Build SQL with optional source filter and limit
    //
    // Note: source filter uses a literal string comparison. Using string
    // interpolation here is safe because ScenarioSource::to_sql_filter()
    // only returns static string literals ("mcp" or "uds") — no user
    // input reaches the SQL directly.
    // ----------------------------------------------------------------
    let source_clause = match source.to_sql_filter() {
        Some(s) => format!(" AND source = '{s}'"),
        None => String::new(),
    };

    let limit_clause = match limit {
        Some(n) => format!(" LIMIT {n}"),
        None => String::new(),
    };

    let sql = format!(
        "SELECT query_id, session_id, query_text, retrieval_mode, source, \
                result_entry_ids, similarity_scores \
         FROM query_log \
         WHERE 1=1{source_clause} \
         ORDER BY query_id ASC{limit_clause}"
    );

    // ----------------------------------------------------------------
    // Execute query and stream rows
    // ----------------------------------------------------------------
    let rows = sqlx::query(&sql).fetch_all(&pool).await?;

    let mut scenario_count: usize = 0;

    for row in rows {
        let record = build_scenario_record(&row)?;
        let json_line = serde_json::to_string(&record)?;
        writeln!(writer, "{json_line}")?;
        scenario_count += 1;
    }

    // ----------------------------------------------------------------
    // Flush output
    // ----------------------------------------------------------------
    writer.flush()?;

    // ----------------------------------------------------------------
    // Report stats to stderr
    // ----------------------------------------------------------------
    eprintln!(
        "eval scenarios: wrote {scenario_count} scenarios to {}",
        out.display()
    );

    // ----------------------------------------------------------------
    // Close pool
    // ----------------------------------------------------------------
    pool.close().await;

    Ok(())
}

// ---------------------------------------------------------------------------
// build_scenario_record (private)
// ---------------------------------------------------------------------------

/// Map a `query_log` row to a `ScenarioRecord`.
///
/// Handles length parity enforcement (R-16): if `result_entry_ids` and
/// `similarity_scores` arrays differ in length, both are truncated to the
/// minimum length and a warning is printed to stderr.
fn build_scenario_record(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<ScenarioRecord, Box<dyn std::error::Error>> {
    let query_id: i64 = row.try_get("query_id")?;
    let session_id: String = row.try_get("session_id")?;
    let query_text: String = row.try_get("query_text")?;
    let retrieval_mode: String = row
        .try_get::<Option<String>, _>("retrieval_mode")?
        .unwrap_or_else(|| "flexible".to_string());
    let source: String = row.try_get("source")?;

    // Parse entry_ids JSON array (may be NULL)
    let entry_ids_json: String = row
        .try_get::<Option<String>, _>("result_entry_ids")?
        .unwrap_or_default();

    // Parse scores JSON array (may be NULL)
    let scores_json: String = row
        .try_get::<Option<String>, _>("similarity_scores")?
        .unwrap_or_default();

    let mut entry_ids: Vec<u64> = if entry_ids_json.is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(&entry_ids_json)
            .map_err(|e| format!("failed to parse result_entry_ids for row {query_id}: {e}"))?
    };

    let mut scores: Vec<f32> = if scores_json.is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(&scores_json)
            .map_err(|e| format!("failed to parse similarity_scores for row {query_id}: {e}"))?
    };

    // Length parity check (R-16)
    if !entry_ids.is_empty() && entry_ids.len() != scores.len() {
        eprintln!(
            "WARN: query_log row {query_id}: entry_ids.len()={} != scores.len()={}, \
             truncating to min",
            entry_ids.len(),
            scores.len()
        );
        let min_len = std::cmp::min(entry_ids.len(), scores.len());
        entry_ids.truncate(min_len);
        scores.truncate(min_len);
    }

    // Build baseline only when results exist
    let baseline = if entry_ids.is_empty() {
        None
    } else {
        Some(ScenarioBaseline { entry_ids, scores })
    };

    Ok(ScenarioRecord {
        id: format!("qlog-{query_id}"),
        query: query_text,
        context: ScenarioContext {
            // No agent_id column in query_log; use session_id as proxy.
            agent_id: session_id.clone(),
            // No feature_cycle column in query_log.
            feature_cycle: String::new(),
            session_id,
            retrieval_mode,
        },
        baseline,
        source,
        expected: None,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use tempfile::TempDir;
    use unimatrix_store::pool_config::PoolConfig;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Create a migrated snapshot DB and return (dir, db_path).
    async fn make_snapshot_db() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("snapshot.db");
        let _store = unimatrix_store::SqlxStore::open(&path, PoolConfig::default())
            .await
            .expect("open snapshot");
        (dir, path)
    }

    /// Insert a `query_log` row directly via a raw pool.
    async fn insert_query_log_row(
        pool: &sqlx::SqlitePool,
        session_id: &str,
        query_text: &str,
        retrieval_mode: &str,
        source: &str,
        entry_ids_json: Option<&str>,
        scores_json: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO query_log \
             (session_id, query_text, ts, result_count, result_entry_ids, \
              similarity_scores, retrieval_mode, source) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(session_id)
        .bind(query_text)
        .bind(0_i64)
        .bind(0_i64)
        .bind(entry_ids_json)
        .bind(scores_json)
        .bind(retrieval_mode)
        .bind(source)
        .execute(pool)
        .await
        .expect("insert query_log");
    }

    /// Open a write pool for a snapshot DB path.
    async fn open_write_pool(db_path: &Path) -> sqlx::SqlitePool {
        let opts = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(false);
        sqlx::SqlitePool::connect_with(opts)
            .await
            .expect("open write pool")
    }

    /// Read all lines from an output file as serde_json Values.
    fn read_jsonl(path: &Path) -> Vec<serde_json::Value> {
        let content = std::fs::read_to_string(path).expect("read output");
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("parse JSON line"))
            .collect()
    }

    // -----------------------------------------------------------------------
    // ScenarioSource tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_scenario_source_to_sql_filter_mcp() {
        assert_eq!(ScenarioSource::Mcp.to_sql_filter(), Some("mcp"));
    }

    #[test]
    fn test_scenario_source_to_sql_filter_uds() {
        assert_eq!(ScenarioSource::Uds.to_sql_filter(), Some("uds"));
    }

    #[test]
    fn test_scenario_source_to_sql_filter_all_is_none() {
        assert_eq!(ScenarioSource::All.to_sql_filter(), None);
    }

    // -----------------------------------------------------------------------
    // AC-03: valid JSONL with all required fields
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_scenarios_produces_valid_jsonl() {
        let (dir, db_path) = make_snapshot_db().await;
        let pool = open_write_pool(&db_path).await;
        insert_query_log_row(
            &pool,
            "sess-1",
            "what is context_search?",
            "flexible",
            "mcp",
            Some("[1,2,3]"),
            Some("[0.9,0.8,0.7]"),
        )
        .await;
        insert_query_log_row(
            &pool,
            "sess-2",
            "how does context_store work?",
            "strict",
            "mcp",
            Some("[4,5]"),
            Some("[0.85,0.75]"),
        )
        .await;
        pool.close().await;

        let out = dir.path().join("scenarios.jsonl");
        run_scenarios(&db_path, ScenarioSource::All, None, &out)
            .expect("run_scenarios must succeed");

        let lines = read_jsonl(&out);
        assert_eq!(lines.len(), 2, "expected 2 scenario lines");

        for line in &lines {
            // id present and non-empty
            assert!(
                line["id"].as_str().is_some_and(|s| s.starts_with("qlog-")),
                "id must be qlog-N, got: {:?}",
                line["id"]
            );
            // query present
            assert!(line["query"].as_str().is_some(), "query must be a string");
            // context with required subfields
            let ctx = &line["context"];
            assert!(ctx["agent_id"].as_str().is_some(), "context.agent_id");
            assert!(
                ctx["feature_cycle"].as_str().is_some(),
                "context.feature_cycle"
            );
            assert!(ctx["session_id"].as_str().is_some(), "context.session_id");
            assert!(
                ctx["retrieval_mode"].as_str().is_some(),
                "context.retrieval_mode"
            );
            // source present
            assert!(line["source"].as_str().is_some(), "source must be a string");
            // expected is null (query-log-sourced)
            assert!(line["expected"].is_null(), "expected must be null");
            // baseline present (we inserted IDs)
            assert!(line["baseline"].is_object(), "baseline must be an object");
            let entry_ids = &line["baseline"]["entry_ids"];
            let scores = &line["baseline"]["scores"];
            assert!(entry_ids.is_array(), "baseline.entry_ids must be array");
            assert!(scores.is_array(), "baseline.scores must be array");
            // length parity (AC-03, R-16)
            assert_eq!(
                entry_ids.as_array().unwrap().len(),
                scores.as_array().unwrap().len(),
                "entry_ids and scores must have equal length"
            );
        }
    }

    // -----------------------------------------------------------------------
    // R-16: length parity enforcement
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_scenarios_length_parity() {
        let (dir, db_path) = make_snapshot_db().await;
        let pool = open_write_pool(&db_path).await;
        // 3 entry_ids but only 2 scores — mismatched
        insert_query_log_row(
            &pool,
            "sess-parity",
            "parity test query",
            "flexible",
            "mcp",
            Some("[10,20,30]"),
            Some("[0.9,0.8]"),
        )
        .await;
        pool.close().await;

        let out = dir.path().join("parity.jsonl");
        run_scenarios(&db_path, ScenarioSource::All, None, &out)
            .expect("run_scenarios must succeed with mismatched row");

        let lines = read_jsonl(&out);
        assert_eq!(lines.len(), 1);

        let entry_ids = lines[0]["baseline"]["entry_ids"].as_array().unwrap();
        let scores = lines[0]["baseline"]["scores"].as_array().unwrap();
        assert_eq!(
            entry_ids.len(),
            scores.len(),
            "lengths must be equal after truncation"
        );
        assert_eq!(entry_ids.len(), 2, "truncated to min(3,2)=2");
    }

    // -----------------------------------------------------------------------
    // AC-04: source filter — mcp
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_scenarios_source_filter_mcp() {
        let (dir, db_path) = make_snapshot_db().await;
        let pool = open_write_pool(&db_path).await;
        insert_query_log_row(
            &pool,
            "sess-m",
            "mcp query",
            "flexible",
            "mcp",
            Some("[1]"),
            Some("[0.9]"),
        )
        .await;
        insert_query_log_row(
            &pool,
            "sess-u",
            "uds query",
            "flexible",
            "uds",
            Some("[2]"),
            Some("[0.8]"),
        )
        .await;
        pool.close().await;

        let out = dir.path().join("mcp.jsonl");
        run_scenarios(&db_path, ScenarioSource::Mcp, None, &out)
            .expect("run_scenarios must succeed");

        let lines = read_jsonl(&out);
        assert_eq!(lines.len(), 1, "only 1 mcp row expected");
        assert_eq!(lines[0]["source"].as_str().unwrap(), "mcp");
    }

    // -----------------------------------------------------------------------
    // AC-04: source filter — uds
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_scenarios_source_filter_uds() {
        let (dir, db_path) = make_snapshot_db().await;
        let pool = open_write_pool(&db_path).await;
        insert_query_log_row(
            &pool,
            "sess-m2",
            "mcp query 2",
            "flexible",
            "mcp",
            Some("[1]"),
            Some("[0.9]"),
        )
        .await;
        insert_query_log_row(
            &pool,
            "sess-u2",
            "uds query 2",
            "flexible",
            "uds",
            Some("[2]"),
            Some("[0.8]"),
        )
        .await;
        pool.close().await;

        let out = dir.path().join("uds.jsonl");
        run_scenarios(&db_path, ScenarioSource::Uds, None, &out)
            .expect("run_scenarios must succeed");

        let lines = read_jsonl(&out);
        assert_eq!(lines.len(), 1, "only 1 uds row expected");
        assert_eq!(lines[0]["source"].as_str().unwrap(), "uds");
    }

    // -----------------------------------------------------------------------
    // AC-04: source filter — all
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_scenarios_source_filter_all() {
        let (dir, db_path) = make_snapshot_db().await;
        let pool = open_write_pool(&db_path).await;
        insert_query_log_row(
            &pool,
            "sess-ma",
            "mcp query a",
            "flexible",
            "mcp",
            Some("[1]"),
            Some("[0.9]"),
        )
        .await;
        insert_query_log_row(
            &pool,
            "sess-ua",
            "uds query a",
            "flexible",
            "uds",
            Some("[2]"),
            Some("[0.8]"),
        )
        .await;
        pool.close().await;

        let out = dir.path().join("all.jsonl");
        run_scenarios(&db_path, ScenarioSource::All, None, &out)
            .expect("run_scenarios must succeed");

        let lines = read_jsonl(&out);
        assert_eq!(lines.len(), 2, "both rows expected");
        let sources: Vec<&str> = lines
            .iter()
            .map(|l| l["source"].as_str().unwrap())
            .collect();
        assert!(sources.contains(&"mcp"), "mcp must be present");
        assert!(sources.contains(&"uds"), "uds must be present");
    }

    // -----------------------------------------------------------------------
    // Empty query_log
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_scenarios_empty_query_log() {
        let (dir, db_path) = make_snapshot_db().await;
        let out = dir.path().join("empty.jsonl");

        run_scenarios(&db_path, ScenarioSource::All, None, &out)
            .expect("run_scenarios must succeed with empty log");

        let content = std::fs::read_to_string(&out).expect("read output");
        assert_eq!(
            content.lines().filter(|l| !l.trim().is_empty()).count(),
            0,
            "empty query_log must produce 0 lines"
        );
    }

    // -----------------------------------------------------------------------
    // --limit applied (FR-08)
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_scenarios_limit_applied() {
        let (dir, db_path) = make_snapshot_db().await;
        let pool = open_write_pool(&db_path).await;
        for i in 0..10 {
            insert_query_log_row(
                &pool,
                &format!("sess-{i}"),
                &format!("query {i}"),
                "flexible",
                "mcp",
                None,
                None,
            )
            .await;
        }
        pool.close().await;

        let out = dir.path().join("limited.jsonl");
        run_scenarios(&db_path, ScenarioSource::All, Some(3), &out)
            .expect("run_scenarios with limit must succeed");

        let lines = read_jsonl(&out);
        assert_eq!(lines.len(), 3, "limit=3 must produce exactly 3 lines");
    }

    // -----------------------------------------------------------------------
    // expected field is null (FR-09)
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_scenarios_expected_field_is_null() {
        let (dir, db_path) = make_snapshot_db().await;
        let pool = open_write_pool(&db_path).await;
        insert_query_log_row(
            &pool,
            "sess-exp",
            "some query",
            "flexible",
            "mcp",
            Some("[1]"),
            Some("[0.9]"),
        )
        .await;
        pool.close().await;

        let out = dir.path().join("expected.jsonl");
        run_scenarios(&db_path, ScenarioSource::All, None, &out)
            .expect("run_scenarios must succeed");

        let lines = read_jsonl(&out);
        assert!(!lines.is_empty());
        for line in &lines {
            assert!(
                line["expected"].is_null(),
                "expected must be null for log-sourced scenarios, got: {:?}",
                line["expected"]
            );
        }
    }

    // -----------------------------------------------------------------------
    // Unique IDs (FR-09)
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_scenarios_unique_ids() {
        let (dir, db_path) = make_snapshot_db().await;
        let pool = open_write_pool(&db_path).await;
        for i in 0..5 {
            insert_query_log_row(
                &pool,
                &format!("sess-uid-{i}"),
                &format!("query uid {i}"),
                "flexible",
                "mcp",
                None,
                None,
            )
            .await;
        }
        pool.close().await;

        let out = dir.path().join("unique_ids.jsonl");
        run_scenarios(&db_path, ScenarioSource::All, None, &out)
            .expect("run_scenarios must succeed");

        let lines = read_jsonl(&out);
        assert_eq!(lines.len(), 5);

        let ids: Vec<&str> = lines.iter().map(|l| l["id"].as_str().unwrap()).collect();
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "all ids must be unique");
    }

    // -----------------------------------------------------------------------
    // Read-only enforcement — snapshot unchanged (R-01, R-02)
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_scenarios_does_not_write_to_snapshot() {
        use std::io::Read;

        let (dir, db_path) = make_snapshot_db().await;
        let pool = open_write_pool(&db_path).await;
        insert_query_log_row(
            &pool,
            "sess-ro",
            "readonly test",
            "flexible",
            "mcp",
            Some("[42]"),
            Some("[0.99]"),
        )
        .await;
        pool.close().await;

        // Compute SHA-256 of the snapshot before
        let snapshot_bytes_before = {
            let mut f = std::fs::File::open(&db_path).expect("open snapshot");
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).expect("read snapshot");
            buf
        };

        let out = dir.path().join("ro_test.jsonl");
        run_scenarios(&db_path, ScenarioSource::All, None, &out)
            .expect("run_scenarios must succeed");

        // Read snapshot after
        let snapshot_bytes_after = {
            let mut f = std::fs::File::open(&db_path).expect("open snapshot");
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).expect("read snapshot");
            buf
        };

        assert_eq!(
            snapshot_bytes_before, snapshot_bytes_after,
            "snapshot must not be modified by eval scenarios"
        );
    }

    // -----------------------------------------------------------------------
    // Null result_entry_ids → baseline is None
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_scenarios_null_entry_ids_produces_null_baseline() {
        let (dir, db_path) = make_snapshot_db().await;
        let pool = open_write_pool(&db_path).await;
        insert_query_log_row(
            &pool,
            "sess-null",
            "null baseline query",
            "flexible",
            "mcp",
            None,
            None,
        )
        .await;
        pool.close().await;

        let out = dir.path().join("null_baseline.jsonl");
        run_scenarios(&db_path, ScenarioSource::All, None, &out)
            .expect("run_scenarios must succeed");

        let lines = read_jsonl(&out);
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0]["baseline"].is_null(),
            "null result_entry_ids must produce null baseline"
        );
    }

    // -----------------------------------------------------------------------
    // Unicode query text
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_scenarios_unicode_query_text() {
        let (dir, db_path) = make_snapshot_db().await;
        let pool = open_write_pool(&db_path).await;
        let unicode_query = "こんにちは世界 — مرحبا بالعالم — 🦀";
        insert_query_log_row(
            &pool,
            "sess-uni",
            unicode_query,
            "flexible",
            "mcp",
            None,
            None,
        )
        .await;
        pool.close().await;

        let out = dir.path().join("unicode.jsonl");
        run_scenarios(&db_path, ScenarioSource::All, None, &out)
            .expect("run_scenarios must succeed with unicode");

        let lines = read_jsonl(&out);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0]["query"].as_str().unwrap(),
            unicode_query,
            "unicode query text must round-trip correctly"
        );
    }
}
