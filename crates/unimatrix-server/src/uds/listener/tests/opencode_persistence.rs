//! vnc-049 C5 — ingest attribution persistence, authoritative non-proxy assertions.
//!
//! Child of `listener.rs::tests` — shares its dispatch helpers (`make_store`,
//! `make_registry`, `make_embed_service`, `make_dispatch_deps`, `make_services`,
//! `make_pending`, `dispatch_request`, `unix_now_secs`, `UDS_CAPABILITIES`) per
//! CLAUDE.md (extend, never scaffold). Mirrors `foreign_domain.rs`: drive the REAL
//! `dispatch_request` → `insert_observation` path, then SELECT the stored row.
//!
//! These tests SELECT the raw persisted `source_domain`/`model_id` columns (the C5
//! write side), NOT the read-path projection (C6). Distinctness and attribution are
//! asserted on the QUERIED row, never on the in-memory `ImplantEvent` (ACCEPTANCE-MAP
//! forbids `event.model_id.is_some()` / "--model was passed" as discharge).
//!
//! Lesson #5670: `test_local_vs_cloud_model_distinct_on_queried_row` IS the required
//! client→listener→DB crossing test carrying `model_id` end to end through production
//! `insert_observation`, not a test-only INSERT helper.

use super::*;
use sqlx::Row;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build an opencode PreToolUse ImplantEvent carrying the given provider + model_id.
/// PreToolUse is a reachable event whose row is stored via the observations write path.
fn opencode_event(
    session_id: &str,
    provider: Option<&str>,
    model_id: Option<&str>,
) -> ImplantEvent {
    ImplantEvent {
        event_type: "PreToolUse".to_string(),
        session_id: session_id.to_string(),
        timestamp: unix_now_secs(),
        payload: serde_json::json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } }),
        topic_signal: None,
        provider: provider.map(|s| s.to_string()),
        model_id: model_id.map(|s| s.to_string()),
        cycle_stamp: None,
    }
}

/// Drive one RecordEvent through the REAL assembled dispatch path.
async fn dispatch_event(store: &Arc<Store>, registry: &SessionRegistry, event: ImplantEvent) {
    let embed = make_embed_service();
    let (vs, es, adapt) = make_dispatch_deps(store);
    let response = dispatch_request(
        HookRequest::RecordEvent { event },
        store,
        &embed,
        &es,
        "0.1.0",
        registry,
        &make_pending(),
        &make_services(store, &embed, &vs, &es, &adapt),
        crate::uds::UDS_CAPABILITIES,
    )
    .await;
    assert!(
        matches!(response, HookResponse::Ack),
        "event must be accepted, got {response:?}"
    );
}

/// Deadline-poll for the fire-and-forget observation write, then return the stored
/// `(source_domain, model_id)` for the row. Asserts exactly one row for the session.
async fn poll_stored_attribution(
    store: &Arc<Store>,
    session_id: &str,
) -> (Option<String>, Option<String>) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM observations WHERE session_id = ?1")
                .bind(session_id)
                .fetch_one(store.read_pool_test())
                .await
                .expect("observations count query");
        if count >= 1 {
            assert_eq!(
                count, 1,
                "expected exactly 1 observations row for {session_id}, got {count}"
            );
            let row = sqlx::query(
                "SELECT source_domain, model_id FROM observations WHERE session_id = ?1",
            )
            .bind(session_id)
            .fetch_one(store.read_pool_test())
            .await
            .expect("select attribution");
            return (
                row.try_get::<Option<String>, _>(0).unwrap(),
                row.try_get::<Option<String>, _>(1).unwrap(),
            );
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for observation write of session {session_id}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

fn register(registry: &SessionRegistry, session_id: &str) {
    registry.register_session(session_id, None, None);
}

// ---------------------------------------------------------------------------
// AC-03 (R-02) — stored source_domain is "opencode" + mandatory negative
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_opencode_event_stored_source_domain_is_opencode() {
    let store = make_store().await;
    let registry = make_registry();
    let session_id = "sess-049-ac03";
    register(&registry, session_id);

    dispatch_event(
        &store,
        &registry,
        opencode_event(session_id, Some("opencode"), None),
    )
    .await;
    let (source_domain, _model_id) = poll_stored_attribution(&store, session_id).await;

    // AC-03 positive: the stored row carries source_domain = "opencode".
    assert_eq!(
        source_domain.as_deref(),
        Some("opencode"),
        "opencode event must persist source_domain=opencode at ingest (ADR-001)"
    );
    // AC-03 MANDATORY negative: the stored value is NOT the claude-code default.
    assert_ne!(
        source_domain.as_deref(),
        Some("claude-code"),
        "opencode row must never read back as claude-code (R-02.2)"
    );
}

// ---------------------------------------------------------------------------
// R-05 — opencode-only stamp: non-opencode providers stay NULL at write
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_source_domain_stamp_fires_only_for_opencode() {
    let store = make_store().await;
    let registry = make_registry();

    // Per-provider write matrix (#5427): only opencode stamps; the rest stay NULL and
    // fall through to today's read-derived resolution (T-SEC-12/13 unchanged).
    for (idx, provider) in ["claude-code", "gemini-cli", "codex-cli"]
        .iter()
        .enumerate()
    {
        let session_id = format!("sess-049-r05-{idx}");
        register(&registry, &session_id);
        dispatch_event(
            &store,
            &registry,
            opencode_event(&session_id, Some(provider), None),
        )
        .await;
        let (source_domain, _) = poll_stored_attribution(&store, &session_id).await;
        assert!(
            source_domain.is_none(),
            "provider {provider} must leave source_domain NULL at write (read-derived fallback, R-05)"
        );
    }

    // A missing provider also stays NULL (never stamped).
    let session_id = "sess-049-r05-none";
    register(&registry, session_id);
    dispatch_event(&store, &registry, opencode_event(session_id, None, None)).await;
    let (source_domain, _) = poll_stored_attribution(&store, session_id).await;
    assert!(
        source_domain.is_none(),
        "absent provider must leave source_domain NULL"
    );
}

// ---------------------------------------------------------------------------
// AC-06 GATING (R-01) — local-vs-cloud model_id distinctness on the QUERIED row
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_local_vs_cloud_model_distinct_on_queried_row() {
    let store = make_store().await;
    let registry = make_registry();

    let local_session = "sess-049-local";
    let cloud_session = "sess-049-cloud";
    register(&registry, local_session);
    register(&registry, cloud_session);

    // LOCAL model event (ollama/qwen3-coder) and a separately-ingested CLOUD model event.
    dispatch_event(
        &store,
        &registry,
        opencode_event(local_session, Some("opencode"), Some("ollama/qwen3-coder")),
    )
    .await;
    dispatch_event(
        &store,
        &registry,
        opencode_event(
            cloud_session,
            Some("opencode"),
            Some("anthropic/claude-3.5-sonnet"),
        ),
    )
    .await;

    let (local_domain, local_model) = poll_stored_attribution(&store, local_session).await;
    let (cloud_domain, cloud_model) = poll_stored_attribution(&store, cloud_session).await;

    // Both are opencode rows (provider persisted via source_domain).
    assert_eq!(local_domain.as_deref(), Some("opencode"));
    assert_eq!(cloud_domain.as_deref(), Some("opencode"));

    // Distinctness asserted on the QUERIED rows, NOT the in-memory ImplantEvent.
    assert_eq!(local_model.as_deref(), Some("ollama/qwen3-coder"));
    assert_eq!(cloud_model.as_deref(), Some("anthropic/claude-3.5-sonnet"));
    assert_ne!(
        local_model, cloud_model,
        "stored local and cloud model_id must be distinguishable on query (AC-06 GATING)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_two_local_models_mutually_distinguishable() {
    let store = make_store().await;
    let registry = make_registry();

    let s1 = "sess-049-qwen";
    let s2 = "sess-049-llama";
    register(&registry, s1);
    register(&registry, s2);

    dispatch_event(
        &store,
        &registry,
        opencode_event(s1, Some("opencode"), Some("ollama/qwen3-coder")),
    )
    .await;
    dispatch_event(
        &store,
        &registry,
        opencode_event(s2, Some("opencode"), Some("ollama/llama3")),
    )
    .await;

    let (_, m1) = poll_stored_attribution(&store, s1).await;
    let (_, m2) = poll_stored_attribution(&store, s2).await;

    assert_eq!(m1.as_deref(), Some("ollama/qwen3-coder"));
    assert_eq!(m2.as_deref(), Some("ollama/llama3"));
    assert_ne!(
        m1, m2,
        "two local models must be mutually distinct on query (R-01.3)"
    );
}

/// Anti-tautology negative (MANDATORY, #4177/#4876): proves the distinctness test is
/// sensitive to a Wire→INSERT drop of `model_id`. This test asserts the QUERIED
/// `model_id` EQUALS the wire value — it FAILS if the `?12` bind at `insert_observation`
/// / `insert_observations_batch` is removed (stored NULL) or homogenized. The distinctness
/// test above is therefore not vacuous: drop the bind and both this test and the
/// distinctness assertion (NULL == NULL) go red.
#[tokio::test(flavor = "multi_thread")]
async fn test_opencode_model_id_survives_wire_to_insert() {
    let store = make_store().await;
    let registry = make_registry();
    let session_id = "sess-049-carrier";
    register(&registry, session_id);

    dispatch_event(
        &store,
        &registry,
        opencode_event(session_id, Some("opencode"), Some("ollama/qwen3-coder")),
    )
    .await;

    let (_, model_id) = poll_stored_attribution(&store, session_id).await;
    assert_eq!(
        model_id.as_deref(),
        Some("ollama/qwen3-coder"),
        "model_id must survive wire→INSERT unchanged; NULL/homogenized here means the ?12 bind was dropped"
    );
}

// ---------------------------------------------------------------------------
// R-15 — untrusted model_id charset rejected before the parameterized bind
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_persist_rejects_model_id_violating_format() {
    let store = make_store().await;
    let registry = make_registry();

    // Illegal chars (uppercase, semicolon, space) and over-length all drop to NULL + warn.
    let bad_session = "sess-049-badmodel";
    register(&registry, bad_session);
    dispatch_event(
        &store,
        &registry,
        opencode_event(bad_session, Some("opencode"), Some("Bad;DROP TABLE")),
    )
    .await;
    let (_, model_id) = poll_stored_attribution(&store, bad_session).await;
    assert!(
        model_id.is_none(),
        "an invalid model_id must be dropped to NULL, never stored raw (R-15)"
    );

    let long_session = "sess-049-longmodel";
    register(&registry, long_session);
    let overlong = "a".repeat(200); // > MODEL_ID_MAX_LEN (128)
    dispatch_event(
        &store,
        &registry,
        opencode_event(long_session, Some("opencode"), Some(&overlong)),
    )
    .await;
    let (_, model_id) = poll_stored_attribution(&store, long_session).await;
    assert!(
        model_id.is_none(),
        "over-length model_id must be dropped to NULL (R-15)"
    );
}

// ---------------------------------------------------------------------------
// ADR-001 fail-loud — the pinned derivation never silently defaults to claude-code
// ---------------------------------------------------------------------------

/// The stamp derivation is fail-loud by construction: for an opencode event it resolves
/// to Some("opencode") (never None, never "claude-code"); for every other provider it
/// returns None (→ read-derived), never a silent claude-code default at the write site.
#[test]
fn test_derive_source_domain_never_silently_defaults_to_claude_code() {
    let opencode = opencode_event("s", Some("opencode"), None);
    let derived = derive_source_domain(&opencode);
    assert_eq!(derived.as_deref(), Some("opencode"));
    assert_ne!(
        derived.as_deref(),
        Some("claude-code"),
        "opencode stamp must never resolve to the claude-code default (ADR-001 fail-loud)"
    );

    for provider in ["claude-code", "gemini-cli", "codex-cli"] {
        let ev = opencode_event("s", Some(provider), None);
        assert!(
            derive_source_domain(&ev).is_none(),
            "non-opencode provider {provider} must not be stamped at the write site (R-05)"
        );
    }
    assert!(
        derive_source_domain(&opencode_event("s", None, None)).is_none(),
        "absent provider must not be stamped"
    );
}
