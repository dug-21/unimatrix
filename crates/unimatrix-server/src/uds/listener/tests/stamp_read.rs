//! vnc-030 (#699) listener cycle_stamp 3-site read + topic_source taxonomy +
//! close-path inversion flip + enrich FeatureSource guard (ADR-004 / ADR-005).
//!
//! Three record sites, each asserted INDEPENDENTLY (#3486 lesson — field-exists on
//! the struct is insufficient): Site A (rework candidate), Site B (single
//! RecordEvent), Site C (batch RecordEvents). The single shared `apply_stamp_to_row`
//! helper is exercised by all three. Split per the 500-line rule; shares the parent
//! module's `use super::*` helpers (`make_store`, `make_dispatch_deps`,
//! `dispatch_request`, `make_cycle_event`, `make_registry`, `make_pending`,
//! `make_services`, `make_embed_service`).
//!
//! Test plan: product/features/vnc-030/test-plan/listener-stamp-read.md +
//! seam-and-roundtrip.md §2 (3-site round-trip, R-01).
//!
//! OQ-A resolution (confirmed here): there is NO dedicated row-level 'vote' write.
//! 'vote' rows are produced ONLY via the enrich decision-tree branch where an
//! unstamped event with no extraction fills from a registry feature whose source is
//! `Inferred(Voted)` (eager-set, #198). Session-level majority vote resolves
//! `sessions.feature_cycle`, never observation rows. FR-21 one-source-per-write-site
//! holds: 'vote' has exactly one write site (the enrich Inferred(Voted) arm).

use super::*;
use unimatrix_engine::wire::{CycleStampPayload, ImplantEvent};

// -- Fixtures --------------------------------------------------------------

/// A stamped observation-producing event (PostToolUse) carrying a cycle_stamp.
fn make_stamped_event(
    event_type: &str,
    session_id: &str,
    topic: &str,
    phase: Option<&str>,
    topic_signal: Option<String>,
) -> ImplantEvent {
    ImplantEvent {
        event_type: event_type.to_string(),
        session_id: session_id.to_string(),
        timestamp: unix_now_secs(),
        payload: serde_json::json!({"tool_name": "Bash", "tool_input": {"cmd": "ls"}}),
        topic_signal,
        provider: None,
        cycle_stamp: Some(CycleStampPayload {
            topic: topic.to_string(),
            phase: phase.map(|p| p.to_string()),
        }),
    }
}

/// Read `(topic_signal, phase, topic_source)` for the rows of a session, newest first
/// is irrelevant — tests assert on a single expected row or count.
async fn query_attribution(
    store: &Store,
    session_id: &str,
) -> Vec<(Option<String>, Option<String>, Option<String>)> {
    use sqlx::Row as _;
    let rows = sqlx::query(
        "SELECT topic_signal, phase, topic_source \
         FROM observations WHERE session_id = ?1 ORDER BY ts_millis, rowid",
    )
    .bind(session_id)
    .fetch_all(store.read_pool_test())
    .await
    .expect("query attribution");
    rows.into_iter()
        .map(|row| {
            (
                row.get::<Option<String>, _>(0),
                row.get::<Option<String>, _>(1),
                row.get::<Option<String>, _>(2),
            )
        })
        .collect()
}

/// Settle any fire-and-forget spawn_blocking write before reading rows.
async fn settle() {
    tokio::task::yield_now().await;
    std::thread::sleep(std::time::Duration::from_millis(50));
}

// Boilerplate dispatch wrapper to keep the per-test arrange blocks short.
macro_rules! dispatch {
    ($req:expr, $store:expr, $registry:expr) => {{
        let embed = make_embed_service();
        let (vs, es, adapt) = make_dispatch_deps($store);
        let services = make_services($store, &embed, &vs, &es, &adapt);
        dispatch_request(
            $req,
            $store,
            &embed,
            &vs,
            &es,
            &adapt,
            "0.1.0",
            $registry,
            &make_pending(),
            &services,
            crate::uds::UDS_CAPABILITIES,
        )
        .await
    }};
}

// -- 1. Three-site round-trip (R-01 — CRITICAL) ----------------------------

/// Site B — single general RecordEvent (~listener.rs:951). Stamped event lands a
/// declared row; apply_stamp sets Declared; tally NOT grown; enrich skipped.
#[tokio::test]
async fn stamp_read_site_b_records_declared() {
    let store = make_store().await;
    let registry = make_registry();
    // Session registered (as after cycle_start) so apply_stamp can affirm Declared.
    registry.register_session("sess-b", None, None);
    // Contradicting extracted signal present — the stamp must still win and the
    // tally must not grow (client strips it; server guard is belt-and-suspenders).
    let event = make_stamped_event(
        "PostToolUse",
        "sess-b",
        "vnc-030",
        Some("delivery"),
        Some("wrong-feature".to_string()),
    );

    let resp = dispatch!(HookRequest::RecordEvent { event }, &store, &registry);
    settle().await;

    assert!(matches!(resp, HookResponse::Ack));
    let rows = query_attribution(&store, "sess-b").await;
    assert_eq!(rows.len(), 1, "exactly one row");
    assert_eq!(
        rows[0].0,
        Some("vnc-030".to_string()),
        "topic_signal = stamp.topic"
    );
    assert_eq!(
        rows[0].1,
        Some("delivery".to_string()),
        "phase = stamp.phase"
    );
    assert_eq!(
        rows[0].2,
        Some("declared".to_string()),
        "topic_source = declared"
    );

    // apply_stamp set the registry to Declared.
    let state = registry
        .get_state("sess-b")
        .expect("session present after apply_stamp");
    assert_eq!(state.feature, Some("vnc-030".to_string()));
    assert_eq!(state.feature_source, FeatureSource::Declared);
    // Tally NOT grown by the stamped event (vote self-scopes to the residue class).
    assert!(
        state.topic_signals.is_empty(),
        "stamped event must NOT feed the vote tally (R-05)"
    );
}

/// Site A — rework candidate (`post_tool_use_rework_candidate`, ~listener.rs:786).
/// Asserted INDEPENDENTLY, not assumed from Site B.
#[tokio::test]
async fn stamp_read_site_a_records_declared() {
    let store = make_store().await;
    let registry = make_registry();
    registry.register_session("sess-a", None, None);
    let event = make_stamped_event(
        "post_tool_use_rework_candidate",
        "sess-a",
        "vnc-030",
        Some("delivery"),
        None,
    );

    let resp = dispatch!(HookRequest::RecordEvent { event }, &store, &registry);
    settle().await;

    assert!(matches!(resp, HookResponse::Ack));
    let rows = query_attribution(&store, "sess-a").await;
    assert_eq!(rows.len(), 1, "exactly one row");
    assert_eq!(rows[0].0, Some("vnc-030".to_string()));
    assert_eq!(
        rows[0].2,
        Some("declared".to_string()),
        "site A topic_source = declared"
    );

    let state = registry.get_state("sess-a").expect("session present");
    assert_eq!(state.feature_source, FeatureSource::Declared);
    assert!(
        state.topic_signals.is_empty(),
        "site A: tally not grown (R-05)"
    );
}

/// Site C — batch RecordEvents (~listener.rs:1108). A batch of N stamped events →
/// N declared rows (catches the batch-site-forgotten case, R-06).
#[tokio::test]
async fn stamp_read_batch_n_declared_rows() {
    let store = make_store().await;
    let registry = make_registry();
    registry.register_session("sess-c", None, None);
    let events: Vec<ImplantEvent> = (0..3)
        .map(|_| make_stamped_event("PostToolUse", "sess-c", "vnc-030", Some("delivery"), None))
        .collect();

    let resp = dispatch!(HookRequest::RecordEvents { events }, &store, &registry);
    settle().await;

    assert!(matches!(resp, HookResponse::Ack));
    let rows = query_attribution(&store, "sess-c").await;
    assert_eq!(rows.len(), 3, "N stamped events → N rows (R-06)");
    for r in &rows {
        assert_eq!(r.0, Some("vnc-030".to_string()));
        assert_eq!(
            r.2,
            Some("declared".to_string()),
            "every batch row declared"
        );
    }
    assert!(
        registry
            .get_state("sess-c")
            .unwrap()
            .topic_signals
            .is_empty(),
        "batch: stamped events do not feed the tally (R-05)"
    );
}

/// Negative — an unstamped Rust-hook-shaped frame through a record site takes the
/// legacy chain, NOT declared. Here: no registry feature, no extraction → NULL.
#[tokio::test]
async fn unstamped_frame_legacy_chain_not_declared() {
    let store = make_store().await;
    let registry = make_registry();
    let event = make_cycle_event(
        "PostToolUse",
        "sess-unstamped",
        serde_json::json!({"tool_name": "Bash"}),
        None,
    );

    let _ = dispatch!(HookRequest::RecordEvent { event }, &store, &registry);
    settle().await;

    let rows = query_attribution(&store, "sess-unstamped").await;
    assert_eq!(rows.len(), 1);
    assert_ne!(
        rows[0].2,
        Some("declared".to_string()),
        "unstamped frame must NOT be declared"
    );
    assert_eq!(
        rows[0].2, None,
        "no feature, no extraction → topic_source NULL"
    );
}

// -- 2. Stamp attribution regardless of registry state (FR-14) -------------

/// A stamped event against an EMPTY/post-restart registry still lands declared —
/// the stamp, not the registry, is the source of truth post-restart (apply_stamp
/// covers the no-post-restart-cycle_start case).
#[tokio::test]
async fn stamped_event_declared_even_with_empty_registry() {
    let store = make_store().await;
    let registry = make_registry(); // empty: no register_session call

    let event = make_stamped_event("PostToolUse", "sess-empty", "vnc-030", None, None);
    let _ = dispatch!(HookRequest::RecordEvent { event }, &store, &registry);
    settle().await;

    let rows = query_attribution(&store, "sess-empty").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, Some("vnc-030".to_string()));
    assert_eq!(rows[0].2, Some("declared".to_string()));
    // apply_stamp no-ops on the absent session (row still gets declared from the stamp).
    assert!(
        registry.get_state("sess-empty").is_none(),
        "apply_stamp does not pre-register an absent session"
    );
}

// -- 3. topic_source per write site (R-12, FR-21 — one source per value) ----

/// 'extracted' — unstamped event arriving with a topic_signal, registry not Declared.
#[tokio::test]
async fn topic_source_extracted_from_unstamped_with_signal() {
    let store = make_store().await;
    let registry = make_registry();
    let event = make_cycle_event(
        "PostToolUse",
        "sess-ext",
        serde_json::json!({"tool_name": "Bash"}),
        Some("col-099".to_string()),
    );

    let _ = dispatch!(HookRequest::RecordEvent { event }, &store, &registry);
    settle().await;

    let rows = query_attribution(&store, "sess-ext").await;
    assert_eq!(rows[0].0, Some("col-099".to_string()));
    assert_eq!(rows[0].2, Some("extracted".to_string()));
}

/// 'registry-fill' — extracted None, filled from an Inferred(Registered) feature.
#[tokio::test]
async fn topic_source_registry_fill_from_enrich() {
    let store = make_store().await;
    let registry = make_registry();
    // register_session → Inferred(Registered) with a feature.
    registry.register_session("sess-fill", None, Some("col-100".to_string()));
    let event = make_cycle_event(
        "PostToolUse",
        "sess-fill",
        serde_json::json!({"tool_name": "Bash"}),
        None, // no extracted signal → registry fill
    );

    let _ = dispatch!(HookRequest::RecordEvent { event }, &store, &registry);
    settle().await;

    let rows = query_attribution(&store, "sess-fill").await;
    assert_eq!(rows[0].0, Some("col-100".to_string()));
    assert_eq!(rows[0].2, Some("registry-fill".to_string()));
}

/// 'vote' (OQ-A) — extracted None, filled from an Inferred(Voted) feature. This is
/// the ONLY row-level 'vote' write site (the enrich Inferred(Voted) arm).
#[tokio::test]
async fn topic_source_vote_from_inferred_voted_fill() {
    let store = make_store().await;
    let registry = make_registry();
    registry.register_session("sess-vote", None, None);
    // Eager attribution (#198) sets Inferred(Voted).
    assert!(registry.set_feature_if_absent("sess-vote", "col-101"));
    assert_eq!(
        registry.get_state("sess-vote").unwrap().feature_source,
        FeatureSource::Inferred(InferredOrigin::Voted)
    );

    let event = make_cycle_event(
        "PostToolUse",
        "sess-vote",
        serde_json::json!({"tool_name": "Bash"}),
        None, // no extraction → fills from the Voted registry feature
    );
    let _ = dispatch!(HookRequest::RecordEvent { event }, &store, &registry);
    settle().await;

    let rows = query_attribution(&store, "sess-vote").await;
    assert_eq!(rows[0].0, Some("col-101".to_string()));
    assert_eq!(
        rows[0].2,
        Some("vote".to_string()),
        "OQ-A: vote via Inferred(Voted) fill"
    );
}

/// 'declared' — declared registry feature overrides a contradicting extraction
/// (the unstamped-window #588 remedy; FR-14 unstamped path).
#[tokio::test]
async fn topic_source_declared_overrides_extraction_unstamped() {
    let store = make_store().await;
    let registry = make_registry();
    registry.register_session("sess-decl", None, None);
    registry.set_feature_force("sess-decl", "vnc-030"); // → Declared

    let event = make_cycle_event(
        "PostToolUse",
        "sess-decl",
        serde_json::json!({"tool_name": "Bash"}),
        Some("wrong-feature".to_string()), // contradicting extraction
    );
    let _ = dispatch!(HookRequest::RecordEvent { event }, &store, &registry);
    settle().await;

    let rows = query_attribution(&store, "sess-decl").await;
    assert_eq!(
        rows[0].0,
        Some("vnc-030".to_string()),
        "declared registry beats extraction"
    );
    assert_eq!(rows[0].2, Some("declared".to_string()));
}

/// NULL — no stamp, no extraction, no registry feature → topic_source IS NULL.
#[tokio::test]
async fn topic_source_null_when_unattributed() {
    let store = make_store().await;
    let registry = make_registry();
    let event = make_cycle_event(
        "PostToolUse",
        "sess-null",
        serde_json::json!({"tool_name": "Bash"}),
        None,
    );
    let _ = dispatch!(HookRequest::RecordEvent { event }, &store, &registry);
    settle().await;

    let rows = query_attribution(&store, "sess-null").await;
    assert_eq!(rows[0].2, None, "unattributed → topic_source NULL");
}

// -- 4. Security — topic content binds parameterized (CRITICAL) ------------

/// A stamped topic containing SQL metacharacters lands as a LITERAL column value
/// via the parameterized ?10 bind — never string interpolation.
#[tokio::test]
async fn topic_with_sql_metachars_binds_parameterized() {
    let store = make_store().await;
    let registry = make_registry();
    let evil = "vnc'; DROP TABLE observations;--";
    let event = make_stamped_event("PostToolUse", "sess-sql", evil, None, None);

    let _ = dispatch!(HookRequest::RecordEvent { event }, &store, &registry);
    settle().await;

    // The observations table still exists and holds the literal topic.
    let rows = query_attribution(&store, "sess-sql").await;
    assert_eq!(rows.len(), 1, "table intact; one row written");
    assert_eq!(
        rows[0].0,
        Some(evil.to_string()),
        "topic stored as a literal value"
    );
    assert_eq!(rows[0].2, Some("declared".to_string()));
}

// -- 5. enrich decision tree, one case per branch (R-04) -------------------

#[test]
fn enrich_declared_registry_beats_contradicting_extraction() {
    let registry = make_registry();
    registry.register_session("s", None, None);
    registry.set_feature_force("s", "vnc-030");
    let (sig, src) = enrich_topic_signal_with_source(Some("other".to_string()), "s", &registry);
    assert_eq!(sig, Some("vnc-030".to_string()));
    assert_eq!(src, Some("declared".to_string()));
}

#[test]
fn enrich_extraction_wins_against_inferred_registry() {
    let registry = make_registry();
    registry.register_session("s", None, Some("col-1".to_string())); // Inferred(Registered)
    let (sig, src) = enrich_topic_signal_with_source(Some("col-2".to_string()), "s", &registry);
    assert_eq!(sig, Some("col-2".to_string()));
    assert_eq!(src, Some("extracted".to_string()));
}

#[test]
fn enrich_registry_fill_when_no_extraction() {
    let registry = make_registry();
    registry.register_session("s", None, Some("col-1".to_string()));
    let (sig, src) = enrich_topic_signal_with_source(None, "s", &registry);
    assert_eq!(sig, Some("col-1".to_string()));
    assert_eq!(src, Some("registry-fill".to_string()));
}

#[test]
fn enrich_vote_when_inferred_voted_no_extraction() {
    let registry = make_registry();
    registry.register_session("s", None, None);
    registry.set_feature_if_absent("s", "col-1"); // Inferred(Voted)
    let (sig, src) = enrich_topic_signal_with_source(None, "s", &registry);
    assert_eq!(sig, Some("col-1".to_string()));
    assert_eq!(src, Some("vote".to_string()));
}

#[test]
fn enrich_declared_null_fill_when_no_extraction() {
    let registry = make_registry();
    registry.register_session("s", None, None);
    registry.set_feature_force("s", "vnc-030"); // Declared
    let (sig, src) = enrich_topic_signal_with_source(None, "s", &registry);
    assert_eq!(sig, Some("vnc-030".to_string()));
    assert_eq!(src, Some("declared".to_string()), "declared NULL-fill");
}

#[test]
fn enrich_null_when_nothing_attributes() {
    let registry = make_registry();
    let (sig, src) = enrich_topic_signal_with_source(None, "unknown", &registry);
    assert_eq!(sig, None);
    assert_eq!(src, None);
}

// -- 6. Close-path inversion flip (R-04, FR-17) ----------------------------
//
// The flip lives in `process_session_close`'s `final_feature_cycle` computation:
// a Declared-and-present feature short-circuits the vote + content fallback. The
// per-branch decision (declared vs vote) is asserted at the unit level here against
// the registry state the close path snapshots; the end-to-end SESSIONS-row write is
// a Stage 3c integration concern (fire-and-forget spawn against the SESSIONS table).

/// A Declared session whose registry feature contradicts the accumulated vote:
/// the close path's gate (`matches!(feature_source, Declared) && feature.is_some()`)
/// must select the declared feature, NOT the vote winner.
#[test]
fn close_declared_beats_contradicting_vote_gate() {
    let registry = make_registry();
    registry.register_session("s-close", None, None);
    // Accumulate a contradicting majority vote.
    registry.record_topic_signal("s-close", "vote-winner".to_string(), unix_now_secs());
    registry.record_topic_signal("s-close", "vote-winner".to_string(), unix_now_secs());
    // Declare a different feature.
    registry.set_feature_force("s-close", "declared-feature");

    let state = registry.get_state("s-close").unwrap();
    // Mirror the close-path gate (listener.rs process_session_close).
    let declared_wins =
        matches!(state.feature_source, FeatureSource::Declared) && state.feature.is_some();
    assert!(
        declared_wins,
        "declared-and-present must short-circuit the vote"
    );
    assert_eq!(
        state.feature,
        Some("declared-feature".to_string()),
        "declared feature wins over the contradicting vote (FR-17)"
    );
}

/// An Inferred session does NOT trip the gate → the existing vote → content →
/// registry path runs (today's order, floor preserved — R-09).
#[test]
fn close_inferred_session_uses_vote_path_gate() {
    let registry = make_registry();
    registry.register_session("s-inf", None, Some("registered".to_string())); // Inferred(Registered)
    let state = registry.get_state("s-inf").unwrap();
    let declared_wins =
        matches!(state.feature_source, FeatureSource::Declared) && state.feature.is_some();
    assert!(
        !declared_wins,
        "Inferred session must fall through to the vote path"
    );
}
