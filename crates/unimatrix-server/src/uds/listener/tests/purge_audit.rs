//! vnc-025 (#670) purge-audit component tests: `transcript_session_purged`
//! emission at the session-close and stale-sweep purge points.
//! Test plan: product/features/vnc-025/test-plan/purge-audit.md.
//! (The cycle_review trigger is owned by the cycle-review-purge component —
//! emission mechanics in `mcp/tools.rs` tests.)

use super::transcript::{Deps, buffer_contents, capture_tracing, dispatch_delta};
use super::*;

const PURGE_OP: &str = "transcript_session_purged";

/// One fully hydrated purge audit row.
pub(crate) struct PurgeRow {
    pub(crate) session_id: String,
    pub(crate) agent_id: String,
    pub(crate) target_ids: String,
    pub(crate) outcome: i64,
    pub(crate) detail: String,
    /// All TEXT columns concatenated — for sentinel-absence assertions.
    pub(crate) all_text: String,
}

async fn purge_rows(store: &Store) -> Vec<PurgeRow> {
    use sqlx::Row as _;
    sqlx::query(
        "SELECT session_id, agent_id, target_ids, outcome, detail, \
         session_id || ' ' || agent_id || ' ' || operation || ' ' || target_ids || ' ' || \
         detail || ' ' || credential_type || ' ' || capability_used || ' ' || \
         agent_attribution || ' ' || metadata AS all_text \
         FROM audit_log WHERE operation = ?1 ORDER BY event_id",
    )
    .bind(PURGE_OP)
    .fetch_all(store.read_pool_test())
    .await
    .expect("query purge audit rows")
    .into_iter()
    .map(|r| PurgeRow {
        session_id: r.get("session_id"),
        agent_id: r.get("agent_id"),
        target_ids: r.get("target_ids"),
        outcome: r.get("outcome"),
        detail: r.get("detail"),
        all_text: r.get("all_text"),
    })
    .collect()
}

/// Poll until `n` purge rows have landed (fire-and-forget emission is async).
async fn wait_for_purge_rows(store: &Store, n: usize) -> Vec<PurgeRow> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let rows = purge_rows(store).await;
        if rows.len() >= n {
            return rows;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {n} purge audit rows, have {}",
            rows.len()
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

async fn dispatch_close(deps: &Deps, registry: &SessionRegistry, session_id: &str) {
    let resp = deps
        .dispatch(
            HookRequest::SessionClose {
                session_id: session_id.to_string(),
                outcome: Some("success".to_string()),
                duration_secs: 1,
            },
            registry,
        )
        .await;
    assert!(matches!(resp, HookResponse::Ack), "got {resp:?}");
}

fn assert_pinned_shape(row: &PurgeRow, session_id: &str, bytes: u64, trigger: &str) {
    assert_eq!(row.session_id, session_id);
    assert_eq!(row.agent_id, "server");
    assert_eq!(row.target_ids, "[]", "target_ids must be empty");
    assert_eq!(row.outcome, Outcome::Success as u8 as i64);
    assert_eq!(row.detail, format!("bytes={bytes} trigger={trigger}"));
}

// =========================================================================
// §1 Emission at each purge point — AC-08, R-08.1
// =========================================================================

/// AC-08: close path emits one row with the pinned shape, trigger=session_close.
#[tokio::test]
async fn test_session_close_purge_emits_audit_row() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("sess-close-1", None, None);
    dispatch_delta(&deps, &registry, "sess-close-1", 0, "hello transcript").await; // 16 bytes

    dispatch_close(&deps, &registry, "sess-close-1").await;

    let rows = wait_for_purge_rows(&deps.store, 1).await;
    assert_eq!(rows.len(), 1);
    assert_pinned_shape(&rows[0], "sess-close-1", 16, "session_close");
}

/// AC-08: stale sweep emits one row per non-empty purged buffer,
/// trigger=stale_sweep (session has injection history → regular SweepResult).
#[tokio::test]
async fn test_sweep_purge_emits_audit_rows() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("stale-a", None, None);
    registry.record_injection("stale-a", &[(1, 0.9)]);
    dispatch_delta(&deps, &registry, "stale-a", 0, "stale").await; // 5 bytes
    registry.backdate_session_for_test("stale-a");

    // Closing an unrelated (empty-buffer) session runs the sweep step.
    registry.register_session("closer", None, None);
    dispatch_close(&deps, &registry, "closer").await;

    let rows = wait_for_purge_rows(&deps.store, 1).await;
    assert_eq!(rows.len(), 1, "exactly one non-empty buffer was swept");
    assert_pinned_shape(&rows[0], "stale-a", 5, "stale_sweep");
}

/// R-08.1 (MANDATORY named case): a silently-evicted session (empty
/// injection_history, no SweepResult) with a non-empty buffer still gets an
/// audit row.
#[tokio::test]
async fn test_silently_evicted_session_gets_audit_row() {
    let deps = Deps::new().await;
    let registry = make_registry();
    // No injection history — sweep evicts silently (FR-09.4).
    registry.register_session("stale-silent", None, None);
    dispatch_delta(&deps, &registry, "stale-silent", 0, "evicted").await; // 7 bytes
    registry.backdate_session_for_test("stale-silent");

    registry.register_session("closer", None, None);
    dispatch_close(&deps, &registry, "closer").await;

    assert!(
        registry.get_state("stale-silent").is_none(),
        "silently-evicted session must be gone"
    );
    let rows = wait_for_purge_rows(&deps.store, 1).await;
    assert_pinned_shape(&rows[0], "stale-silent", 7, "stale_sweep");
}

/// R-07.4: zero-byte purges emit nothing at either UDS purge point.
#[tokio::test]
async fn test_empty_buffer_purge_emits_nothing() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("empty-stale", None, None);
    registry.backdate_session_for_test("empty-stale");
    registry.register_session("empty-close", None, None);

    // Close sweeps the stale session AND drains the closing one — both empty.
    dispatch_close(&deps, &registry, "empty-close").await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    assert!(
        purge_rows(&deps.store).await.is_empty(),
        "zero-byte purges must emit no audit row"
    );
}

// =========================================================================
// §2 Emission mechanics + failure independence — R-07
// =========================================================================

/// FR-14/R-07.1/.2: with the audit store unavailable, the purge stands, the
/// emission fails with exactly one content-free warn, and there is no retry.
#[tokio::test]
async fn test_purge_completes_when_audit_store_unavailable() {
    const SENTINEL: &str = "SENTINEL-purge-store-down";
    let (writer, _guard) = capture_tracing();

    // Audit handle over a store whose write pool is closed.
    let dead_store = make_store().await;
    dead_store.write_pool_server().close().await;
    let dead_audit = Arc::new(crate::infra::audit::AuditLog::new(Arc::clone(&dead_store)));

    let registry = make_registry();
    registry.register_session("sess-down", None, None);
    registry.apply_transcript_delta("sess-down", 0, SENTINEL.as_bytes());

    let (_output, purge) = registry
        .drain_and_signal_session("sess-down", "success")
        .expect("session drains");
    let record = purge.expect("non-empty buffer yields a purge record");

    // Purge already stands before emission is attempted.
    assert!(registry.get_state("sess-down").is_none());

    emit_purge_audits(&dead_audit, vec![record], "session_close");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let captured = String::from_utf8_lossy(&writer.0.lock().unwrap()).to_string();
    assert_eq!(
        captured
            .matches("transcript purge audit write failed")
            .count(),
        1,
        "exactly one warn, no retry loop"
    );
    assert!(
        !captured.contains(SENTINEL),
        "audit-failure warn must be content-free"
    );
}

/// R-07.3 (#2266 precedent): a sweep burst of 20+ non-empty buffers — every
/// row eventually lands; the write pool does not starve.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_sweep_burst_all_audits_land() {
    const N: usize = 22;
    let deps = Deps::new().await;
    let registry = make_registry();
    for i in 0..N {
        let sid = format!("burst-{i}");
        registry.register_session(&sid, None, None);
        registry.apply_transcript_delta(&sid, 0, b"0123456789");
        registry.backdate_session_for_test(&sid);
    }
    registry.register_session("closer", None, None);
    dispatch_close(&deps, &registry, "closer").await;

    let rows = wait_for_purge_rows(&deps.store, N).await;
    assert_eq!(rows.len(), N);
    let mut ids: Vec<&str> = rows.iter().map(|r| r.session_id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), N, "one row per swept session, no duplicates");
    for row in &rows {
        assert_eq!(row.detail, "bytes=10 trigger=stale_sweep");
    }
}

/// FR-14 fire-and-forget: emission returns promptly even while the audit
/// store's write path is blocked by a held write transaction.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_purge_never_blocks_on_audit_latency() {
    let deps = Deps::new().await;
    let audit = Arc::new(crate::infra::audit::AuditLog::new(Arc::clone(&deps.store)));

    let registry = make_registry();
    registry.register_session("sess-slow", None, None);
    registry.apply_transcript_delta("sess-slow", 0, b"slow sink bytes");
    let (_output, purge) = registry
        .drain_and_signal_session("sess-slow", "success")
        .expect("session drains");
    let record = purge.expect("purge record");

    // Hold the write lock so the spawned audit write must wait.
    let mut txn = deps.store.write_pool_server().begin().await.unwrap();
    sqlx::query("INSERT INTO observations (session_id, ts_millis, hook) VALUES ('lock', 0, 'x')")
        .execute(&mut *txn)
        .await
        .unwrap();

    let start = std::time::Instant::now();
    emit_purge_audits(&audit, vec![record], "session_close");
    assert!(
        start.elapsed() < std::time::Duration::from_millis(200),
        "emission must be fire-and-forget, took {:?}",
        start.elapsed()
    );

    // Release the lock; the deferred write lands.
    txn.rollback().await.unwrap();
    let rows = wait_for_purge_rows(&deps.store, 1).await;
    assert_eq!(rows[0].detail, "bytes=15 trigger=session_close");
}

// =========================================================================
// §3 Content-free audit — R-05.3, AC-12 arm
// =========================================================================

/// R-05.3: purge a sentinel-bearing buffer — EVERY column of the audit row
/// (including detail) is sentinel-free.
#[tokio::test]
async fn test_purge_audit_row_sentinel_free() {
    const SENTINEL: &str = "SENTINEL-vnc025-audit-row-leak";
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("sess-row-sent", None, None);
    dispatch_delta(&deps, &registry, "sess-row-sent", 0, SENTINEL).await;

    dispatch_close(&deps, &registry, "sess-row-sent").await;

    let rows = wait_for_purge_rows(&deps.store, 1).await;
    assert_pinned_shape(
        &rows[0],
        "sess-row-sent",
        SENTINEL.len() as u64,
        "session_close",
    );
    assert!(
        !rows[0].all_text.contains(SENTINEL),
        "audit row carries transcript content: {}",
        rows[0].all_text
    );
}

/// §6 dispatch-wiring: a session with a non-empty buffer closing while a stale
/// non-empty session is pending sweep — both rows land with their own trigger
/// (close/sweep flows complete end-to-end through the updated call sites).
#[tokio::test]
async fn test_close_and_sweep_in_one_pass_emit_both_triggers() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("stale-b", None, None);
    dispatch_delta(&deps, &registry, "stale-b", 0, "abc").await; // 3 bytes
    registry.backdate_session_for_test("stale-b");

    registry.register_session("closing-b", None, None);
    dispatch_delta(&deps, &registry, "closing-b", 0, "abcd").await; // 4 bytes
    dispatch_close(&deps, &registry, "closing-b").await;

    let rows = wait_for_purge_rows(&deps.store, 2).await;
    let details: Vec<&str> = rows.iter().map(|r| r.detail.as_str()).collect();
    assert!(
        details.contains(&"bytes=3 trigger=stale_sweep"),
        "{details:?}"
    );
    assert!(
        details.contains(&"bytes=4 trigger=session_close"),
        "{details:?}"
    );
}
