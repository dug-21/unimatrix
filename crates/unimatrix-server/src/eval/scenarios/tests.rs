//! Tests for eval scenarios extraction (nan-007).

#[cfg(test)]
mod tests {
    use std::path::Path;

    use sqlx::sqlite::SqliteConnectOptions;
    use tempfile::TempDir;
    use unimatrix_store::pool_config::PoolConfig;

    use super::super::output::run_scenarios;
    use super::super::types::ScenarioSource;

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
              similarity_scores, retrieval_mode, source, phase) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(session_id)
        .bind(query_text)
        .bind(0_i64)
        .bind(0_i64)
        .bind(entry_ids_json)
        .bind(scores_json)
        .bind(retrieval_mode)
        .bind(source)
        .bind(Option::<String>::None) // col-028: phase=NULL for test helper rows (IR-03)
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
            assert!(
                line["id"].as_str().is_some_and(|s| s.starts_with("qlog-")),
                "id must be qlog-N, got: {:?}",
                line["id"]
            );
            assert!(line["query"].as_str().is_some(), "query must be a string");
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
            assert!(line["source"].as_str().is_some(), "source must be a string");
            assert!(line["expected"].is_null(), "expected must be null");
            assert!(line["baseline"].is_object(), "baseline must be an object");
            let entry_ids = &line["baseline"]["entry_ids"];
            let scores = &line["baseline"]["scores"];
            assert!(entry_ids.is_array(), "baseline.entry_ids must be array");
            assert!(scores.is_array(), "baseline.scores must be array");
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

        let snapshot_bytes_before = {
            let mut f = std::fs::File::open(&db_path).expect("open snapshot");
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).expect("read snapshot");
            buf
        };

        let out = dir.path().join("ro_test.jsonl");
        run_scenarios(&db_path, ScenarioSource::All, None, &out)
            .expect("run_scenarios must succeed");

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
