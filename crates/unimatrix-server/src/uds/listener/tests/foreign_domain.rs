//! bugfix-917 (#917): DA3 claim-floor — a foreign `source_domain` through the
//! REAL ingest+storage path. Child of `listener.rs::tests` — shares its dispatch
//! helpers (`make_store`, `make_registry`, `make_dispatch_deps`, `make_services`,
//! `make_pending`, `UDS_CAPABILITIES`) per CLAUDE.md (extend, never scaffold).
//!
//! Architectural framing (ADR-004 vnc-013, #4308): `source_domain` is NOT on the
//! wire (`ImplantEvent` has no such field) and NOT persisted (observations has no
//! such column). It is DERIVED at DB-read time from `DomainPackRegistry`. So
//! "a foreign source_domain through ingest+storage" operationalizes as: a
//! foreign-pack `event_type` is stored VERBATIM at the real `dispatch_request`
//! boundary, and the production read path (`SqlObservationSource`) resolves it to
//! the foreign domain via a registry built FROM CONFIG.
//!
//! The registry is built through the real config chain — a parsed
//! `[[observation.domain_packs]]` TOML fixture → `UnimatrixConfig` →
//! `DomainPackRegistry::new` — to prove "domain-agnostic by config alone", not
//! merely the resolution mechanism.

use super::*;

use crate::infra::config::UnimatrixConfig;
use crate::services::observation::SqlObservationSource;
use unimatrix_observe::ObservationSource;
use unimatrix_observe::domain::{DomainPack, DomainPackRegistry};

/// A foreign `[[observation.domain_packs]]` stanza declaring the `sre` domain and
/// the event types it claims. Parsed through the SAME config path production uses.
const SRE_PACK_TOML: &str = r#"
[[observation.domain_packs]]
source_domain = "sre"
event_types = ["alert_fired", "incident_opened"]
categories = ["runbook"]
"#;

/// Build a `DomainPackRegistry` via the real config chain: parse a
/// `[[observation.domain_packs]]` TOML fixture into `UnimatrixConfig`, map each
/// `DomainPackConfig` to a `DomainPack`, then `DomainPackRegistry::new`.
///
/// The map step is the same 4-field copy as production's `domain_pack_from_config`
/// (main.rs), inlined here because that helper lives in the BINARY crate and is
/// unreachable from this lib test module. The TOML parse and `new` — the two
/// load-bearing "by config alone" steps — are the exact production code paths.
fn registry_from_toml(toml_str: &str) -> DomainPackRegistry {
    let config: UnimatrixConfig =
        toml::from_str(toml_str).expect("domain_packs TOML fixture must parse");
    let packs: Vec<DomainPack> = config
        .observation
        .domain_packs
        .iter()
        .map(|cfg| DomainPack {
            source_domain: cfg.source_domain.clone(),
            event_types: cfg.event_types.clone(),
            categories: cfg.categories.clone(),
            rules: vec![],
        })
        .collect();
    DomainPackRegistry::new(packs).expect("registry construction from config must succeed")
}

/// `load_feature_observations` joins observations to sessions on
/// `sessions.feature_cycle` (observation.rs). The listener ingest path does NOT
/// populate the `sessions` table, so the read-back requires this row or it returns
/// empty (N4, mandatory).
async fn insert_session_row(store: &Store, session_id: &str, feature_cycle: &str) {
    let now = unix_now_secs() as i64;
    sqlx::query(
        "INSERT INTO sessions (session_id, feature_cycle, started_at, status) \
         VALUES (?1, ?2, ?3, 0)",
    )
    .bind(session_id)
    .bind(feature_cycle)
    .bind(now)
    .execute(store.write_pool_server())
    .await
    .expect("insert sessions row");
}

/// GH#565 deadline-poll: the observation write is a fire-and-forget
/// `spawn_blocking`, so there is no fixed sleep. Wait until a row with the raw
/// `hook` appears, then assert EXACTLY one — stored, not dropped, not duplicated.
async fn poll_exactly_one_row(store: &Store, session_id: &str, expected_hook: &str) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM observations WHERE session_id = ?1 AND hook = ?2",
        )
        .bind(session_id)
        .bind(expected_hook)
        .fetch_one(store.read_pool_test())
        .await
        .expect("observations count query");
        if count >= 1 {
            assert_eq!(
                count, 1,
                "expected exactly 1 observations row for {session_id}/{expected_hook}, got {count}"
            );
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for spawn_blocking observation write of hook={expected_hook}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// CASE 1 — a foreign-pack event (`alert_fired`, claimed by the configured `sre`
/// pack, NOT by the builtin claude-code pack) is stored RAW at the real ingest
/// boundary, and the production read path resolves it to `source_domain = "sre"`.
///
/// `alert_fired` is deliberately outside the builtin pack's event_types
/// (PreToolUse/PostToolUse/SubagentStart/SubagentStop), so resolution is
/// deterministic (EC-07: shared event_types resolve non-deterministically).
// multi_thread: the read path (`load_feature_observations`) bridges async→sync via
// `tokio::task::block_in_place`, which panics on the single-threaded runtime.
#[tokio::test(flavor = "multi_thread")]
async fn test_foreign_pack_event_stored_raw_and_read_resolves_to_sre() {
    let store = make_store().await;
    let embed = make_embed_service();
    let registry = make_registry();
    let session_id = "sess-917-sre";
    registry.register_session(session_id, None, None);
    let (vs, es, adapt) = make_dispatch_deps(&store);

    let event = ImplantEvent {
        event_type: "alert_fired".to_string(),
        session_id: session_id.to_string(),
        timestamp: unix_now_secs(),
        payload: serde_json::json!({ "alert": "cpu_high" }),
        topic_signal: None,
        provider: None,
        cycle_stamp: None,
    };

    let response = dispatch_request(
        HookRequest::RecordEvent { event },
        &store,
        &embed,
        &vs,
        &es,
        &adapt,
        "0.1.0",
        &registry,
        &make_pending(),
        &make_services(&store, &embed, &vs, &es, &adapt),
        crate::uds::UDS_CAPABILITIES,
    )
    .await;

    // Not rejected at the protocol boundary — ingest is domain-neutral.
    assert!(
        matches!(response, HookResponse::Ack),
        "foreign-pack event must be accepted, got {response:?}"
    );

    // Stored verbatim: raw event_type preserved as `hook`, exactly one row.
    poll_exactly_one_row(&store, session_id, "alert_fired").await;

    // Read path: build the registry FROM the parsed TOML config, then load via the
    // exact production parse (SqlObservationSource::load_feature_observations).
    let feature_cycle = "bugfix-917-sre";
    insert_session_row(&store, session_id, feature_cycle).await;

    let obs_registry = Arc::new(registry_from_toml(SRE_PACK_TOML));
    let source = SqlObservationSource::new(Arc::clone(&store), obs_registry);
    let records = source
        .load_feature_observations(feature_cycle)
        .expect("load_feature_observations must succeed");

    assert_eq!(
        records.len(),
        1,
        "expected exactly 1 read-back record, got {}",
        records.len()
    );
    assert_eq!(
        records[0].event_type, "alert_fired",
        "raw event_type must survive the read path unchanged"
    );
    assert_eq!(
        records[0].source_domain, "sre",
        "foreign-pack event must resolve to the configured domain, not the claude-code fallback"
    );
}

/// CASE 2 (N3) — an `event_type` claimed by NO pack (neither builtin nor the
/// configured `sre` pack) is still stored RAW at the real ingest boundary, and the
/// read path falls back to `source_domain = "claude-code"`.
///
/// This closes "not-dropped / not-rejected for unknown types" at the exact boundary
/// where the AC-05/AC-11 bypass gap lived (in-memory record / direct-INSERT).
#[tokio::test(flavor = "multi_thread")]
async fn test_unknown_event_type_stored_raw_and_read_falls_back_to_claude_code() {
    let store = make_store().await;
    let embed = make_embed_service();
    let registry = make_registry();
    let session_id = "sess-917-unknown";
    registry.register_session(session_id, None, None);
    let (vs, es, adapt) = make_dispatch_deps(&store);

    let unknown_event_type = "totally_unknown_xyz917";
    let event = ImplantEvent {
        event_type: unknown_event_type.to_string(),
        session_id: session_id.to_string(),
        timestamp: unix_now_secs(),
        payload: serde_json::json!({ "note": "claimed by no pack" }),
        topic_signal: None,
        provider: None,
        cycle_stamp: None,
    };

    let response = dispatch_request(
        HookRequest::RecordEvent { event },
        &store,
        &embed,
        &vs,
        &es,
        &adapt,
        "0.1.0",
        &registry,
        &make_pending(),
        &make_services(&store, &embed, &vs, &es, &adapt),
        crate::uds::UDS_CAPABILITIES,
    )
    .await;

    assert!(
        matches!(response, HookResponse::Ack),
        "unknown event_type must be accepted, got {response:?}"
    );

    // Stored verbatim, not dropped, at the real ingest boundary.
    poll_exactly_one_row(&store, session_id, unknown_event_type).await;

    // Same foreign-pack registry: proves that even with `sre` configured, an event
    // claimed by NO pack falls back to claude-code (DEFAULT_HOOK_SOURCE_DOMAIN).
    let feature_cycle = "bugfix-917-unknown";
    insert_session_row(&store, session_id, feature_cycle).await;

    let obs_registry = Arc::new(registry_from_toml(SRE_PACK_TOML));
    let source = SqlObservationSource::new(Arc::clone(&store), obs_registry);
    let records = source
        .load_feature_observations(feature_cycle)
        .expect("load_feature_observations must succeed");

    assert_eq!(
        records.len(),
        1,
        "expected exactly 1 read-back record, got {}",
        records.len()
    );
    assert_eq!(
        records[0].event_type, unknown_event_type,
        "raw unknown event_type must survive the read path unchanged"
    );
    assert_eq!(
        records[0].source_domain, "claude-code",
        "event claimed by no pack must fall back to claude-code"
    );
}
