//! Unit tests for `query_log.rs` — new observation-sourced query functions.
//!
//! Tests cover:
//!   - `query_phase_freq_observations` (Query A)
//!   - `query_phase_outcome_map` (Query B)
//!   - `count_phase_session_pairs`
//!   - `MILLIS_PER_DAY` constant value assertion
//!   - Write-path contract (ADR-005)
//!
//! Extracted to a separate file to keep `query_log.rs` under the 500-line limit.

use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::Row;

use crate::db::SqlxStore;
use crate::query_log::{MILLIS_PER_DAY, PhaseFreqRow};
use crate::test_helpers::{TestEntry, open_test_store};

// -- Test helpers --

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Insert an observations row directly for test control.
/// Uses a raw write to include the `phase` column (not exposed on SqlxStore::insert_observation).
async fn insert_observation(
    store: &SqlxStore,
    session_id: &str,
    phase: Option<&str>,
    hook: &str,
    tool: &str,
    input: Option<&str>,
    ts_millis: i64,
) {
    sqlx::query(
        "INSERT INTO observations
             (session_id, ts_millis, hook, tool, input, phase)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(session_id)
    .bind(ts_millis)
    .bind(hook)
    .bind(tool)
    .bind(input)
    .bind(phase)
    .execute(&store.write_pool)
    .await
    .expect("insert_observation");
}

/// Insert a sessions row with optional feature_cycle.
async fn insert_session(store: &SqlxStore, session_id: &str, feature_cycle: Option<&str>) {
    sqlx::query(
        "INSERT INTO sessions
             (session_id, feature_cycle, started_at, status)
         VALUES (?1, ?2, 0, 0)",
    )
    .bind(session_id)
    .bind(feature_cycle)
    .execute(&store.write_pool)
    .await
    .expect("insert_session");
}

/// Insert a cycle_events row.
async fn insert_cycle_event(
    store: &SqlxStore,
    cycle_id: &str,
    phase: &str,
    event_type: &str,
    outcome: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO cycle_events
             (cycle_id, seq, event_type, phase, outcome, timestamp)
         VALUES (?1, 1, ?2, ?3, ?4, 0)",
    )
    .bind(cycle_id)
    .bind(event_type)
    .bind(phase)
    .bind(outcome)
    .execute(&store.write_pool)
    .await
    .expect("insert_cycle_event");
}

/// Insert an entry and return its assigned id.
async fn insert_entry(store: &SqlxStore, category: &str) -> u64 {
    store
        .insert(TestEntry::new("test-topic", category).build())
        .await
        .expect("insert entry")
}

// -- Constant value assertion --

/// T-SQ-07: MILLIS_PER_DAY constant value (R-05).
#[test]
fn test_millis_per_day_constant_value() {
    assert_eq!(
        MILLIS_PER_DAY, 86_400_000i64,
        "MILLIS_PER_DAY must equal 86_400 * 1_000"
    );
}

// -- Write-path contract (AC-SV-01 / R-01) --

/// test_observation_input_json_extract_returns_id_for_hook_path
///
/// Validates ADR-005: hook-listener write path produces plain JSON object string
/// (no double-encoding). json_extract(input, '$.id') must return 42, not NULL.
#[tokio::test]
async fn test_observation_input_json_extract_returns_id_for_hook_path() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let ts = now_millis() - 1_000;
    // Store input exactly as the hook listener produces it: plain JSON object string.
    insert_observation(
        &store,
        "sess-adr005",
        Some("delivery"),
        "PreToolUse",
        "context_get",
        Some(r#"{"id": 42}"#),
        ts,
    )
    .await;

    let row =
        sqlx::query("SELECT json_extract(input, '$.id') FROM observations WHERE session_id = ?1")
            .bind("sess-adr005")
            .fetch_one(store.read_pool())
            .await
            .expect("raw json_extract query");

    // Must not be NULL
    let raw: Option<i64> = row.try_get(0).expect("column 0");
    assert!(
        raw.is_some(),
        "json_extract(input, '$.id') must not be NULL (ADR-005)"
    );
    assert_eq!(raw.unwrap(), 42i64, "json_extract must return integer 42");
}

// -- AC-01 / AC-13a: Rebuild source is observations, not query_log --

/// test_query_phase_freq_observations_returns_rows_when_observations_populated
#[tokio::test]
async fn test_query_phase_freq_observations_returns_rows_when_observations_populated() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let entry_id = insert_entry(&store, "decision").await;
    let ts = now_millis() - 1_000;
    let input = format!(r#"{{"id": {entry_id}}}"#);

    for _ in 0..5 {
        insert_observation(
            &store,
            "sess-ac01",
            Some("delivery"),
            "PreToolUse",
            "context_get",
            Some(&input),
            ts,
        )
        .await;
    }

    let rows = store
        .query_phase_freq_observations(30)
        .await
        .expect("query_phase_freq_observations");

    assert!(
        !rows.is_empty(),
        "must return rows when observations populated"
    );
    let row = &rows[0];
    assert_eq!(row.phase, "delivery");
    assert_eq!(row.category, "decision");
    assert_eq!(row.entry_id, entry_id);
    assert_eq!(row.freq, 5i64);
}

/// test_query_phase_freq_observations_returns_empty_when_observations_empty
#[tokio::test]
async fn test_query_phase_freq_observations_returns_empty_when_observations_empty() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    // query_log may have rows, but observations is empty — must return empty Vec.
    let rows = store
        .query_phase_freq_observations(30)
        .await
        .expect("query_phase_freq_observations");

    assert!(rows.is_empty(), "empty observations must return empty Vec");
}

// -- AC-02 / AC-13f: Tool name filter — four variants --

/// test_query_phase_freq_observations_includes_all_four_tool_variants
#[tokio::test]
async fn test_query_phase_freq_observations_includes_all_four_tool_variants() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let entry_id = insert_entry(&store, "decision").await;
    let ts = now_millis() - 1_000;
    let input = format!(r#"{{"id": {entry_id}}}"#);

    for tool in &[
        "context_get",
        "mcp__unimatrix__context_get",
        "context_lookup",
        "mcp__unimatrix__context_lookup",
    ] {
        insert_observation(
            &store,
            &format!("sess-{tool}"),
            Some("delivery"),
            "PreToolUse",
            tool,
            Some(&input),
            ts,
        )
        .await;
    }

    let rows = store
        .query_phase_freq_observations(30)
        .await
        .expect("query_phase_freq_observations");

    let total_freq: i64 = rows.iter().map(|r| r.freq).sum();
    assert_eq!(total_freq, 4i64, "all 4 tool variants must be counted");
}

/// test_query_phase_freq_observations_excludes_context_search_tool
#[tokio::test]
async fn test_query_phase_freq_observations_excludes_context_search_tool() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let entry_id = insert_entry(&store, "decision").await;
    let ts = now_millis() - 1_000;
    let input = format!(r#"{{"id": {entry_id}}}"#);

    insert_observation(
        &store,
        "sess-search",
        Some("delivery"),
        "PreToolUse",
        "context_search",
        Some(&input),
        ts,
    )
    .await;

    let rows = store
        .query_phase_freq_observations(30)
        .await
        .expect("query_phase_freq_observations");

    assert!(rows.is_empty(), "context_search tool must be excluded");
}

// -- AC-02 / R-10: hook column filter (not hook_event) --

/// test_query_phase_freq_observations_filters_pretooluse_only
///
/// Also validates ADR-007: column name is `hook`, not `hook_event`.
/// If the wrong column name were used, this query would produce a runtime SQL error.
#[tokio::test]
async fn test_query_phase_freq_observations_filters_pretooluse_only() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let entry_id = insert_entry(&store, "decision").await;
    let ts = now_millis() - 1_000;
    let input = format!(r#"{{"id": {entry_id}}}"#);

    // PreToolUse: counted
    insert_observation(
        &store,
        "sess-hook",
        Some("delivery"),
        "PreToolUse",
        "context_get",
        Some(&input),
        ts,
    )
    .await;

    // PostToolUse: must be excluded
    insert_observation(
        &store,
        "sess-hook",
        Some("delivery"),
        "PostToolUse",
        "context_get",
        Some(&input),
        ts,
    )
    .await;

    let rows = store
        .query_phase_freq_observations(30)
        .await
        .expect("query_phase_freq_observations");

    assert_eq!(rows.len(), 1, "only PreToolUse row must be counted");
    assert_eq!(rows[0].freq, 1i64, "PostToolUse must not increase freq");
}

// -- AC-03: CAST and string-form IDs --

/// test_query_phase_freq_observations_cast_handles_string_form_id
#[tokio::test]
async fn test_query_phase_freq_observations_cast_handles_string_form_id() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let entry_id = insert_entry(&store, "pattern").await;
    let ts = now_millis() - 1_000;
    // String-form ID — CAST must convert "N" → integer N
    let input = format!(r#"{{"id": "{entry_id}"}}"#);

    insert_observation(
        &store,
        "sess-cast",
        Some("delivery"),
        "PreToolUse",
        "context_get",
        Some(&input),
        ts,
    )
    .await;

    let rows = store
        .query_phase_freq_observations(30)
        .await
        .expect("query_phase_freq_observations");

    assert_eq!(rows.len(), 1, "string-form id must produce a row");
    assert_eq!(
        rows[0].entry_id, entry_id,
        "CAST must convert string id to integer"
    );
}

/// test_query_phase_freq_observations_excludes_null_id_observations
#[tokio::test]
async fn test_query_phase_freq_observations_excludes_null_id_observations() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let ts = now_millis() - 1_000;
    // input has no $.id field — json_extract returns NULL, which is filtered out
    insert_observation(
        &store,
        "sess-nullid",
        Some("delivery"),
        "PreToolUse",
        "context_lookup",
        Some(r#"{"filter": "topic"}"#),
        ts,
    )
    .await;

    let rows = store
        .query_phase_freq_observations(30)
        .await
        .expect("query_phase_freq_observations");

    assert!(rows.is_empty(), "observation without $.id must be excluded");
}

// -- AC-07 / R-05: ts_millis lookback boundary --

/// test_query_phase_freq_observations_respects_ts_millis_boundary
#[tokio::test]
async fn test_query_phase_freq_observations_respects_ts_millis_boundary() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let entry_id = insert_entry(&store, "decision").await;
    let input = format!(r#"{{"id": {entry_id}}}"#);

    let now = now_millis();
    let cutoff = now - MILLIS_PER_DAY; // 1-day window

    // Inside window: cutoff + 500ms
    insert_observation(
        &store,
        "sess-inside",
        Some("delivery"),
        "PreToolUse",
        "context_get",
        Some(&input),
        cutoff + 500,
    )
    .await;

    // Outside window: cutoff - 500ms
    insert_observation(
        &store,
        "sess-outside",
        Some("delivery"),
        "PreToolUse",
        "context_get",
        Some(&input),
        cutoff - 500,
    )
    .await;

    let rows = store
        .query_phase_freq_observations(1)
        .await
        .expect("query_phase_freq_observations");

    assert_eq!(
        rows.len(),
        1,
        "only inside-window observation must be returned"
    );
    assert_eq!(rows[0].freq, 1i64);
}

/// test_query_phase_freq_observations_lookback_30_days_arithmetic
///
/// Validates that 30-day multiplication uses MILLIS_PER_DAY (ms), not seconds.
#[tokio::test]
async fn test_query_phase_freq_observations_lookback_30_days_arithmetic() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let entry_id = insert_entry(&store, "decision").await;
    let input = format!(r#"{{"id": {entry_id}}}"#);

    let now = now_millis();
    // Inside 30-day window by 5 seconds
    let ts = now - 30 * MILLIS_PER_DAY + 5_000;

    insert_observation(
        &store,
        "sess-30d",
        Some("delivery"),
        "PreToolUse",
        "context_get",
        Some(&input),
        ts,
    )
    .await;

    let rows = store
        .query_phase_freq_observations(30)
        .await
        .expect("query_phase_freq_observations");

    assert!(
        !rows.is_empty(),
        "30-day window arithmetic must include observations at boundary"
    );
}

// -- AC-15 / R-08: Query B — NULL feature_cycle degradation --

/// test_query_phase_outcome_map_excludes_null_feature_cycle_sessions
#[tokio::test]
async fn test_query_phase_outcome_map_excludes_null_feature_cycle_sessions() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    // Session with NULL feature_cycle
    insert_session(&store, "sess-nullfc", None).await;
    // cycle_event pointing to a cycle_id that has no sessions with matching feature_cycle
    insert_cycle_event(
        &store,
        "sess-nullfc",
        "delivery",
        "cycle_phase_end",
        Some("pass"),
    )
    .await;

    let rows = store
        .query_phase_outcome_map()
        .await
        .expect("query_phase_outcome_map");

    assert!(
        rows.is_empty(),
        "NULL feature_cycle sessions must be excluded from Query B"
    );
}

/// test_query_phase_outcome_map_returns_rows_for_non_null_sessions
#[tokio::test]
async fn test_query_phase_outcome_map_returns_rows_for_non_null_sessions() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    // Session with feature_cycle = "crt-050"; cycle_id must match
    insert_session(&store, "sess-crt050", Some("crt-050")).await;
    insert_cycle_event(
        &store,
        "crt-050",
        "delivery",
        "cycle_phase_end",
        Some("PASS"),
    )
    .await;

    let rows = store
        .query_phase_outcome_map()
        .await
        .expect("query_phase_outcome_map");

    assert_eq!(rows.len(), 1, "non-null feature_cycle row must be returned");
    assert_eq!(rows[0].phase, "delivery");
    assert_eq!(rows[0].feature_cycle, "crt-050");
    assert_eq!(rows[0].outcome, "PASS");
}

/// test_query_phase_outcome_map_excludes_non_phase_end_events
#[tokio::test]
async fn test_query_phase_outcome_map_excludes_non_phase_end_events() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    insert_session(&store, "sess-other", Some("crt-050")).await;
    // event_type = 'cycle_start', not 'cycle_phase_end'
    insert_cycle_event(&store, "crt-050", "delivery", "cycle_start", Some("pass")).await;

    let rows = store
        .query_phase_outcome_map()
        .await
        .expect("query_phase_outcome_map");

    assert!(
        rows.is_empty(),
        "non-cycle_phase_end events must be excluded"
    );
}

/// test_query_phase_outcome_map_excludes_null_outcome
#[tokio::test]
async fn test_query_phase_outcome_map_excludes_null_outcome() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    insert_session(&store, "sess-nullout", Some("crt-050")).await;
    // outcome = NULL
    insert_cycle_event(&store, "crt-050", "delivery", "cycle_phase_end", None).await;

    let rows = store
        .query_phase_outcome_map()
        .await
        .expect("query_phase_outcome_map");

    assert!(rows.is_empty(), "NULL outcome events must be excluded");
}

// -- count_phase_session_pairs --

/// test_count_phase_session_pairs_returns_correct_count
#[tokio::test]
async fn test_count_phase_session_pairs_returns_correct_count() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let entry_id = insert_entry(&store, "decision").await;
    let ts = now_millis() - 1_000;
    let input = format!(r#"{{"id": {entry_id}}}"#);

    // 2 sessions, same phase "delivery" → 2 distinct (phase, session_id) pairs
    insert_observation(
        &store,
        "sess-a",
        Some("delivery"),
        "PreToolUse",
        "context_get",
        Some(&input),
        ts,
    )
    .await;
    insert_observation(
        &store,
        "sess-b",
        Some("delivery"),
        "PreToolUse",
        "context_get",
        Some(&input),
        ts,
    )
    .await;
    // Same session twice — still 2 distinct pairs
    insert_observation(
        &store,
        "sess-a",
        Some("delivery"),
        "PreToolUse",
        "context_get",
        Some(&input),
        ts,
    )
    .await;

    let count = store
        .count_phase_session_pairs(30)
        .await
        .expect("count_phase_session_pairs");

    assert_eq!(count, 2i64, "distinct (phase, session_id) pairs must be 2");
}

/// test_count_phase_session_pairs_returns_zero_when_empty
#[tokio::test]
async fn test_count_phase_session_pairs_returns_zero_when_empty() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let count = store
        .count_phase_session_pairs(30)
        .await
        .expect("count_phase_session_pairs");

    assert_eq!(count, 0i64);
}

/// test_count_phase_session_pairs_excludes_outside_window
#[tokio::test]
async fn test_count_phase_session_pairs_excludes_outside_window() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let entry_id = insert_entry(&store, "decision").await;
    let input = format!(r#"{{"id": {entry_id}}}"#);

    // ts_millis = 0 — far outside any window
    insert_observation(
        &store,
        "sess-old",
        Some("delivery"),
        "PreToolUse",
        "context_get",
        Some(&input),
        0,
    )
    .await;

    let count = store
        .count_phase_session_pairs(30)
        .await
        .expect("count_phase_session_pairs");

    assert_eq!(count, 0i64, "outside-window rows must not be counted");
}

// -- Compile-time type assertions --

/// Verify PhaseFreqRow field types match sqlx 0.8 mapping requirements.
#[allow(dead_code)]
fn assert_phase_freq_row_field_types(r: PhaseFreqRow) {
    let _phase: String = r.phase;
    let _category: String = r.category;
    let _entry_id: u64 = r.entry_id;
    let _freq: i64 = r.freq; // must be i64, NOT u64 (R-13)
}
