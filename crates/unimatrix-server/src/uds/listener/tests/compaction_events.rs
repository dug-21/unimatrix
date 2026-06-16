//! crt-054 (#752) Surface A component tests: the durable `compaction_events`
//! writer at `handle_compact_payload` (Component 6) and the `store_ops`
//! failure-counter wrapper (Component 8).
//!
//! Test plans:
//! - product/features/crt-054/test-plan/compaction-events-writer.md
//! - product/features/crt-054/test-plan/compaction-insert-helper.md
//!
//! Layer: integration — every test drives the real `handle_compact_payload`
//! seam through `dispatch_compact`, against a real test `Store` whose
//! `compaction_events` table is created by the crt-054 migration (Component 7).
//! The deadlock-under-contention drive (AC-04) and the full pre/post-boundary
//! gate classification are Stage 3c / crt-055 — out of scope here.
//!
//! Child of `listener.rs::tests` — reuses the `transcript.rs` dispatch harness
//! (`Deps`, `dispatch_delta`, `dispatch_compact`) rather than re-scaffolding.

use super::transcript::{Deps, dispatch_compact, dispatch_delta};
use super::*;

use unimatrix_store::counters::{COMPACTION_EVENTS_INSERT_FAILED, read_counter};

/// One `compaction_events` row, content-free by construction.
struct EventRow {
    session_id: String,
    compacted_at: i64,
    high_water: i64,
}

/// Read all `compaction_events` rows for a session, ordered by insertion (id).
async fn event_rows(store: &Store, session_id: &str) -> Vec<EventRow> {
    use sqlx::Row as _;
    sqlx::query(
        "SELECT session_id, compacted_at, high_water FROM compaction_events \
         WHERE session_id = ?1 ORDER BY id",
    )
    .bind(session_id)
    .fetch_all(store.read_pool_test())
    .await
    .expect("query compaction_events rows")
    .into_iter()
    .map(|r| EventRow {
        session_id: r.get("session_id"),
        compacted_at: r.get("compacted_at"),
        high_water: r.get("high_water"),
    })
    .collect()
}

/// Total row count in `compaction_events` (used for the failure-path no-row assertion).
async fn total_event_rows(store: &Store) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM compaction_events")
        .fetch_one(store.read_pool_test())
        .await
        .expect("count compaction_events rows")
}

/// Send `bytes` as a transcript delta so the session's buffer accumulates a
/// non-trivial `high_water` (the merged contiguous byte length).
async fn seed_transcript(deps: &Deps, registry: &SessionRegistry, session_id: &str, bytes: &str) {
    dispatch_delta(deps, registry, session_id, 0, bytes).await;
}

// ---------------------------------------------------------------------------
// One-row contract (AC-02, R-11/R-13/R-14) — Component 6
// ---------------------------------------------------------------------------

/// AC-02: one compaction writes exactly one row keyed by the compacting session.
#[tokio::test]
async fn test_compaction_writes_one_row() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("s-one-row", None, None);
    seed_transcript(&deps, &registry, "s-one-row", "hello transcript world").await;

    dispatch_compact(&deps, &registry, "s-one-row").await;

    let rows = event_rows(&deps.store, "s-one-row").await;
    assert_eq!(rows.len(), 1, "exactly one compaction_events row must land");
    assert_eq!(rows[0].session_id, "s-one-row");
}

/// AC-02 / R-13: `high_water` equals the buffer's `high_water()` at compaction
/// (non-default — the fixture sends real bytes).
#[tokio::test]
async fn test_high_water_equals_buffer_high_water() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("s-hw", None, None);
    let payload = "the quick brown fox jumps over the lazy dog";
    seed_transcript(&deps, &registry, "s-hw", payload).await;

    // Capture the live buffer high_water BEFORE compaction (the value the writer reads).
    let expected_hw = registry
        .get_state("s-hw")
        .map(|s| lock_buffer(&s.transcript).high_water())
        .expect("session registered");
    assert!(
        expected_hw > 0,
        "fixture must produce a non-trivial high_water"
    );

    dispatch_compact(&deps, &registry, "s-hw").await;

    let rows = event_rows(&deps.store, "s-hw").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].high_water, expected_hw as i64,
        "row high_water must equal the buffer high_water captured at compaction"
    );
}

/// AC-02 / R-11 (AC-16 seconds-producer half): `compacted_at` is Unix SECONDS
/// (server wall clock, `.as_secs()`), within tolerance of `now` — NOT millis.
/// A millis value would be ~1000x too large and fail the upper bound.
#[tokio::test]
async fn test_compacted_at_is_seconds_within_tolerance() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("s-secs", None, None);
    seed_transcript(
        &deps,
        &registry,
        "s-secs",
        "transcript bytes for seconds test",
    )
    .await;

    let before = unix_now_secs() as i64;
    dispatch_compact(&deps, &registry, "s-secs").await;
    let after = unix_now_secs() as i64;

    let rows = event_rows(&deps.store, "s-secs").await;
    assert_eq!(rows.len(), 1);
    let ts = rows[0].compacted_at;
    assert!(
        ts >= before && ts <= after,
        "compacted_at must be Unix SECONDS within [{before}, {after}], got {ts} \
         (a millis value would be ~1000x too large)"
    );
}

/// AC-02 / R-14: a second compaction on the same session adds a SECOND distinct
/// row with a monotonic (>=) `compacted_at`. Insert-only — no UPDATE/DELETE.
#[tokio::test]
async fn test_second_compaction_adds_monotonic_row() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("s-multi", None, None);
    seed_transcript(
        &deps,
        &registry,
        "s-multi",
        "first window of transcript bytes",
    )
    .await;

    dispatch_compact(&deps, &registry, "s-multi").await;
    // Add more bytes, then compact again.
    dispatch_delta(&deps, &registry, "s-multi", 31, " plus a second window").await;
    dispatch_compact(&deps, &registry, "s-multi").await;

    let rows = event_rows(&deps.store, "s-multi").await;
    assert_eq!(
        rows.len(),
        2,
        "two compactions must produce two rows (insert-only)"
    );
    assert!(
        rows[1].compacted_at >= rows[0].compacted_at,
        "second compacted_at must be monotonic (>=) the first"
    );
}

// ---------------------------------------------------------------------------
// Undeclared-session row (AC-03) — declaration independence (ADR-004)
// ---------------------------------------------------------------------------

/// AC-03: a session with NO declared feature_cycle still gets a session-keyed row.
/// Surface A is declaration-independent — written at the handler regardless.
#[tokio::test]
async fn test_compaction_row_written_for_undeclared_session() {
    let deps = Deps::new().await;
    let registry = make_registry();
    // register_session(_, None, None) — no feature declared.
    registry.register_session("s-undeclared", None, None);
    seed_transcript(&deps, &registry, "s-undeclared", "undeclared session bytes").await;

    dispatch_compact(&deps, &registry, "s-undeclared").await;

    let rows = event_rows(&deps.store, "s-undeclared").await;
    assert_eq!(
        rows.len(),
        1,
        "undeclared session must still get a session-keyed compaction_events row"
    );
}

/// AC-03 (schema): `compaction_events` carries no `feature_cycle` and no
/// content/payload column — only id/session_id/compacted_at/high_water.
#[tokio::test]
async fn test_compaction_events_no_feature_cycle_or_content_column() {
    use sqlx::Row as _;
    let deps = Deps::new().await;
    let cols: Vec<String> = sqlx::query("PRAGMA table_info(compaction_events)")
        .fetch_all(deps.store.read_pool_test())
        .await
        .expect("pragma table_info")
        .into_iter()
        .map(|r| r.get::<String, _>("name"))
        .collect();

    assert!(
        !cols.iter().any(|c| c == "feature_cycle"),
        "compaction_events must NOT carry a feature_cycle column (attributed at review)"
    );
    for forbidden in ["content", "payload", "bytes", "transcript", "text"] {
        assert!(
            !cols.iter().any(|c| c == forbidden),
            "compaction_events must NOT carry a content-bearing column ({forbidden}); got {cols:?}"
        );
    }
    assert_eq!(
        cols,
        vec!["id", "session_id", "compacted_at", "high_water"],
        "compaction_events columns must be exactly id/session_id/compacted_at/high_water"
    );
}

/// AC-03 / absent session: `high_water` defaults to 0 and the row is still written.
#[tokio::test]
async fn test_compaction_row_for_absent_session_high_water_zero() {
    let deps = Deps::new().await;
    let registry = make_registry();
    // No register_session — session_state is absent at the handler; high_water → 0.
    dispatch_compact(&deps, &registry, "s-absent").await;

    let rows = event_rows(&deps.store, "s-absent").await;
    assert_eq!(
        rows.len(),
        1,
        "row must be written even for an absent session"
    );
    assert_eq!(
        rows[0].high_water, 0,
        "absent session → high_water defaults to 0"
    );
}

// ---------------------------------------------------------------------------
// Failure path (AC-04a, R-15) — Component 8 wrapper, named counter MANDATORY
// ---------------------------------------------------------------------------

/// AC-04a / R-15 (MANDATORY): force the INSERT to fail (drop the table) and assert
/// the named counter `compaction_events_insert_failed` increments by EXACTLY 1.
/// A log-only posture does NOT satisfy this — the durable counter is load-bearing.
/// Also asserts the compaction ACK still completes (non-blocking, no panic).
#[tokio::test]
async fn test_insert_failure_increments_named_counter() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("s-fail", None, None);
    seed_transcript(&deps, &registry, "s-fail", "bytes that will not persist").await;

    let before = read_counter(
        deps.store.write_pool_server(),
        COMPACTION_EVENTS_INSERT_FAILED,
    )
    .await
    .expect("read counter before");

    // Force the store-level INSERT to fail: remove the target table.
    sqlx::query("DROP TABLE compaction_events")
        .execute(deps.store.write_pool_server())
        .await
        .expect("drop compaction_events for fault injection");

    // The compaction ACK must still complete (non-blocking, no panic).
    let resp = dispatch_compact(&deps, &registry, "s-fail").await;
    assert!(
        matches!(resp, HookResponse::BriefingContent { .. }),
        "compaction ACK must complete despite INSERT failure, got {resp:?}"
    );

    let after = read_counter(
        deps.store.write_pool_server(),
        COMPACTION_EVENTS_INSERT_FAILED,
    )
    .await
    .expect("read counter after");
    assert_eq!(
        after,
        before + 1,
        "named counter compaction_events_insert_failed must increment by exactly 1 on INSERT failure"
    );
}

/// AC-04a / R-15 scenario 2: the failure counter is content-free — its name is a
/// fixed literal carrying no session_id/bytes. (The value is a pure count.)
#[tokio::test]
async fn test_insert_failure_counter_is_content_free() {
    assert_eq!(
        COMPACTION_EVENTS_INSERT_FAILED, "compaction_events_insert_failed",
        "the counter name must be a fixed content-free literal — no ids/bytes interpolated"
    );
}

/// AC-04a / R-15: on forced INSERT failure no row lands and the handler does not
/// panic — the seam-level non-blocking guarantee (writer test 9).
#[tokio::test]
async fn test_insert_failure_non_blocking_no_row() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("s-norow", None, None);
    seed_transcript(&deps, &registry, "s-norow", "more non-persisting bytes").await;

    // Fault injection: make the INSERT fail by replacing the table with an
    // incompatible shape so the parameterized INSERT errors at execute time.
    sqlx::query("DROP TABLE compaction_events")
        .execute(deps.store.write_pool_server())
        .await
        .expect("drop table");
    sqlx::query("CREATE TABLE compaction_events (id INTEGER PRIMARY KEY, wrong_col TEXT NOT NULL)")
        .execute(deps.store.write_pool_server())
        .await
        .expect("recreate incompatible table");

    let resp = dispatch_compact(&deps, &registry, "s-norow").await;
    assert!(
        matches!(resp, HookResponse::BriefingContent { .. }),
        "ACK must complete despite INSERT failure"
    );

    // No row landed for this event (the INSERT errored on the wrong schema).
    let count = total_event_rows(&deps.store).await;
    assert_eq!(
        count, 0,
        "no compaction_events row may land when the INSERT fails"
    );
}
