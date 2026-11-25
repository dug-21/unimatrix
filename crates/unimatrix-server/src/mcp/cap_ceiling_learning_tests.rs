//! vnc-050 C4b (R-04, ADR-004, AC-15, AC-26): read-token calls are served but
//! record no learning signal. Driven over real HTTP; background recording is
//! drained with the `UsageService::flush` test seam.

use serde_json::json;

use super::http_tests::{Fixture, LEGACY, boot_with, call, ok, open, scalar, seed_entry};

const READ: [u8; 32] = [0x31; 32];
const RW: [u8; 32] = [0x32; 32];
const SID: &str = "cc-learn-session";

async fn fixture(dir: &std::path::Path) -> Fixture {
    let f = boot_with(
        dir,
        &[("cl-read", READ, "read"), ("cl-rw", RW, "read-write")],
        false,
    )
    .await;
    f.server
        .session_registry
        .register_session(SID, None, Some("vnc-050".to_string()));
    f
}

async fn learning_snapshot(f: &Fixture) -> Vec<i64> {
    let mut v = Vec::new();
    for sql in [
        "SELECT COALESCE(SUM(access_count), 0) FROM entries",
        "SELECT COALESCE(SUM(helpful_count + unhelpful_count), 0) FROM entries",
        "SELECT COUNT(*) FROM co_access",
        "SELECT COUNT(*) FROM feature_entries",
        "SELECT COUNT(*) FROM query_log",
        "SELECT COUNT(*) FROM signal_queue",
        "SELECT COUNT(*) FROM observations",
    ] {
        v.push(scalar(&f.server, sql).await);
    }
    v
}

fn confirmed(f: &Fixture) -> usize {
    f.server
        .session_registry
        .get_state(SID)
        .map(|s| s.confirmed_entries.len())
        .unwrap_or(0)
}

/// search / lookup / get / briefing with helpful, feature, agent `human`.
async fn read_batch(f: &Fixture, token: &[u8; 32], a: u64, b: u64) {
    let sid = open(f.addr, token).await;
    let common = |mut v: serde_json::Value| {
        v["agent_id"] = json!("human");
        v["session_id"] = json!(SID);
        v["helpful"] = json!(true);
        v["feature"] = json!("vnc-050");
        v
    };
    for (name, args, must_ok) in [
        ("context_lookup", common(json!({"id": a})), true),
        ("context_lookup", common(json!({"topic": "cl"})), true),
        ("context_get", common(json!({"id": b})), true),
        // search/briefing need the embedding model (not loaded in tests):
        // exercised for gate placement, outcome not asserted.
        ("context_search", common(json!({"query": "cl"})), false),
        ("context_briefing", common(json!({"task": "cl"})), false),
    ] {
        let v = call(f.addr, token, &sid, name, args).await;
        if must_ok {
            assert!(ok(&v), "{name} must be served: {v}");
        }
    }
    f.server.services.usage.flush().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn read_token_learning_tables_unchanged() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f = fixture(dir.path()).await;
    let a = seed_entry(&f.server, "cl").await;
    let b = seed_entry(&f.server, "cl").await;
    let before = learning_snapshot(&f).await;
    let audit_before = scalar(&f.server, "SELECT COUNT(*) FROM audit_log").await;

    read_batch(&f, &READ, a, b).await;

    assert_eq!(
        learning_snapshot(&f).await,
        before,
        "no learning sink written"
    );
    assert_eq!(f.server.services.usage.enqueued(), 0, "gate before enqueue");
    assert_eq!(confirmed(&f), 0, "session confirmed_entries untouched");
    let audit_after = scalar(&f.server, "SELECT COUNT(*) FROM audit_log").await;
    assert!(audit_after > audit_before, "tool audit rows still written");
    f.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn rw_token_learning_tables_change() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f = fixture(dir.path()).await;
    let a = seed_entry(&f.server, "cl").await;
    let b = seed_entry(&f.server, "cl").await;
    let before = learning_snapshot(&f).await;

    read_batch(&f, &RW, a, b).await;

    assert!(f.server.services.usage.enqueued() > 0, "rw records usage");
    assert_ne!(learning_snapshot(&f).await, before, "gate is not vacuous");
    assert!(confirmed(&f) > 0, "rw records confirmed_entries");
    f.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn legacy_token_learning_recorded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f = fixture(dir.path()).await;
    let a = seed_entry(&f.server, "cl").await;
    let b = seed_entry(&f.server, "cl").await;
    let before = learning_snapshot(&f).await;

    read_batch(&f, &LEGACY, a, b).await;

    assert!(f.server.services.usage.enqueued() > 0);
    assert_ne!(learning_snapshot(&f).await, before);
    f.shutdown.cancel();
}

/// Source test (R-04.6): every MCP `UsageContext` literal derives
/// `learning_allowed` from the ceiling; no `Default` can yield `true`.
#[test]
fn usage_context_inventory() {
    let tools = include_str!("tools.rs");
    let gate = "learning_allowed: cap_ceiling::records_learning(ctx.cap_ceiling.as_ref())";
    let sites: Vec<_> = tools.match_indices("UsageContext {").collect();
    assert_eq!(
        sites.len(),
        5,
        "update inventory for new UsageContext sites"
    );
    for (i, _) in sites {
        let lit = &tools[i..];
        let end = lit.find('}').expect("literal end");
        assert!(lit[..end].contains(gate), "site at byte {i} lacks gate");
    }
    let usage = include_str!("../services/usage.rs");
    assert!(!usage.contains("impl Default for UsageContext"));
    let def = usage.find("pub(crate) struct UsageContext").expect("def");
    assert!(!usage[def.saturating_sub(200)..def].contains("Default"));
    assert!(
        tools.contains("if cap_ceiling::records_learning(ctx.cap_ceiling.as_ref()) {"),
        "query_log insert gated"
    );
}
