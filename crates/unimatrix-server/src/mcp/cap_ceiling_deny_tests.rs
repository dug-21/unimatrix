//! vnc-050 C4b (R-02, AC-08, AC-25): the token-mode ceiling denies over real
//! HTTP. Every assertion drives `tools/call`; `require_cap` is never called
//! directly.

use serde_json::{Value, json};

use super::http_tests::{Fixture, boot_with, call, err_code, ok, open, scalar, seed_entry};
use crate::error::{ERROR_CAPABILITY_DENIED, ERROR_INVALID_PARAMS};
use crate::infra::registry::{Capability, TrustLevel};

const READ: [u8; 32] = [0x21; 32];
const RW: [u8; 32] = [0x22; 32];
const READ_LABEL: &str = "cc-read";
const RW_LABEL: &str = "cc-rw";
const WRITER: &str = "writer-agent";
const READER: &str = "reader-agent";

fn denied(v: &Value) -> bool {
    err_code(v) == Some(ERROR_CAPABILITY_DENIED.0 as i64)
}

async fn fixture(dir: &std::path::Path, strip: bool) -> Fixture {
    let f = boot_with(
        dir,
        &[(READ_LABEL, READ, "read"), (RW_LABEL, RW, "read-write")],
        strip,
    )
    .await;
    // make_server is permissive: unknown agents auto-enroll with Write.
    f.server.registry.resolve_or_enroll(WRITER).expect("writer");
    // Omitted agent_id resolves (and auto-enrolls) `anonymous` before any
    // capability check — pre-existing identity behaviour, not a write-tool effect.
    f.server
        .registry
        .resolve_or_enroll("anonymous")
        .expect("anonymous");
    f.server
        .registry
        .enroll_agent(
            "system",
            READER,
            TrustLevel::Restricted,
            vec![Capability::Read, Capability::Search],
        )
        .expect("enroll reader");
    f
}

/// Stable fingerprint of every table a write tool can touch.
async fn fingerprint(f: &Fixture) -> Vec<i64> {
    let mut v = Vec::new();
    for sql in [
        "SELECT COUNT(*) FROM entries",
        "SELECT COUNT(*) FROM entries WHERE status != 0",
        "SELECT COALESCE(SUM(LENGTH(content) + LENGTH(title)), 0) FROM entries",
        "SELECT COUNT(*) FROM entry_tags",
        "SELECT COUNT(*) FROM graph_edges",
        "SELECT COUNT(*) FROM cycle_events",
        "SELECT COUNT(*) FROM agent_registry",
    ] {
        v.push(scalar(&f.server, sql).await);
    }
    v
}

/// The write-tool batch (Write and Admin gated) for one agent_id variant.
fn write_calls(a: u64, b: u64, agent: Option<&str>) -> Vec<(&'static str, Value)> {
    let mut calls = vec![
        (
            "context_store",
            json!({"content": "cc store", "topic": "cc", "category": "pattern"}),
        ),
        (
            "context_correct",
            json!({"original_id": a, "content": "cc corrected"}),
        ),
        ("context_deprecate", json!({"id": b})),
        ("context_quarantine", json!({"id": b})),
        (
            "context_edge",
            json!({"mode": "add", "source_id": a, "edge_type": "Supports", "target_id": b}),
        ),
        (
            "context_tag",
            json!({"id": a, "action": "add", "tag": "cc-tag"}),
        ),
        (
            "context_enroll",
            json!({"target_agent_id": "cc-new", "trust_level": "restricted", "capabilities": ["read"]}),
        ),
        (
            "context_cycle",
            json!({"type": "start", "topic": "cc-cycle"}),
        ),
    ];
    if let Some(id) = agent {
        for (_, args) in &mut calls {
            args["agent_id"] = json!(id);
        }
    }
    calls
}

#[tokio::test(flavor = "multi_thread")]
async fn read_token_write_tool_agent_id_table() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f = fixture(dir.path(), false).await;
    let a = seed_entry(&f.server, "cc-a").await;
    let b = seed_entry(&f.server, "cc-b").await;
    let sid = open(f.addr, &READ).await;
    let before = fingerprint(&f).await;

    let token_id = format!("token:{READ_LABEL}");
    for agent in [None, Some("human"), Some(WRITER), Some(token_id.as_str())] {
        for (name, args) in write_calls(a, b, agent) {
            let v = call(f.addr, &READ, &sid, name, args).await;
            if agent.is_some_and(|x| x.starts_with("token:")) {
                assert_eq!(
                    err_code(&v),
                    Some(ERROR_INVALID_PARAMS.0 as i64),
                    "{name} {agent:?}: {v}"
                );
            } else {
                assert!(denied(&v), "{name} {agent:?} must be denied: {v}");
            }
        }
    }
    assert_eq!(
        fingerprint(&f).await,
        before,
        "read token must not mutate the DB"
    );
    f.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn read_token_read_tools_succeed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f = fixture(dir.path(), false).await;
    let a = seed_entry(&f.server, "cc-read-ok").await;
    let sid = open(f.addr, &READ).await;
    for (name, args) in [
        ("context_lookup", json!({"id": a})),
        ("context_get", json!({"id": a})),
        (
            "context_lookup",
            json!({"topic": "cc-read-ok", "agent_id": "human"}),
        ),
    ] {
        let v = call(f.addr, &READ, &sid, name, args).await;
        assert!(ok(&v), "{name} must be served for a read token: {v}");
    }
    f.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn rw_token_write_tools_succeed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f = fixture(dir.path(), false).await;
    let a = seed_entry(&f.server, "cc-a").await;
    let b = seed_entry(&f.server, "cc-b").await;
    let sid = open(f.addr, &RW).await;
    let before = fingerprint(&f).await;
    // Write-gated tools that do not need the embedding model.
    for (name, args) in write_calls(a, b, Some(WRITER)) {
        if matches!(
            name,
            "context_store" | "context_correct" | "context_quarantine" | "context_enroll"
        ) {
            continue;
        }
        let v = call(f.addr, &RW, &sid, name, args).await;
        assert!(
            ok(&v),
            "{name} must succeed under rw token + Write holder: {v}"
        );
    }
    assert_ne!(fingerprint(&f).await, before, "rw companion must mutate");
    f.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn rw_token_human_admin_tool_denied() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f = fixture(dir.path(), false).await;
    let b = seed_entry(&f.server, "cc-b").await;
    let sid = open(f.addr, &RW).await;
    let before = fingerprint(&f).await;
    // The only `Capability::Admin` sites in mcp/tools.rs (re-grepped at delivery).
    for (name, args) in [
        ("context_quarantine", json!({"id": b, "agent_id": "human"})),
        (
            "context_enroll",
            json!({"target_agent_id": "cc-new", "trust_level": "restricted",
                   "capabilities": ["read"], "agent_id": "human"}),
        ),
    ] {
        let v = call(f.addr, &RW, &sid, name, args).await;
        assert!(denied(&v), "{name}: rw ceiling excludes Admin: {v}");
    }
    assert_eq!(fingerprint(&f).await, before);
    f.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn r20_rw_token_human_non_admin_write_succeeds_with_label() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f = fixture(dir.path(), false).await;
    let a = seed_entry(&f.server, "cc-r20").await;
    let sid = open(f.addr, &RW).await;
    let v = call(
        f.addr,
        &RW,
        &sid,
        "context_tag",
        json!({"id": a, "action": "add", "tag": "r20", "agent_id": "human"}),
    )
    .await;
    assert!(
        ok(&v),
        "rw + human non-Admin write succeeds (accepted R-20): {v}"
    );
    // Tag audit is fire-and-forget: poll briefly.
    let mut n = 0;
    for _ in 0..50 {
        n = scalar(
            &f.server,
            "SELECT COUNT(*) FROM audit_log WHERE agent_id = 'human' AND operation LIKE '%tag%'",
        )
        .await;
        if n > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(n >= 1, "tool audit carries agent 'human'");
    f.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn narrow_only_intersection() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f = fixture(dir.path(), false).await;
    let a = seed_entry(&f.server, "cc-narrow").await;
    let sid = open(f.addr, &RW).await;
    let before = fingerprint(&f).await;
    let v = call(
        f.addr,
        &RW,
        &sid,
        "context_tag",
        json!({"id": a, "action": "add", "tag": "x", "agent_id": READER}),
    )
    .await;
    assert!(
        denied(&v),
        "registry lacks Write: rw token cannot widen: {v}"
    );
    assert_eq!(fingerprint(&f).await, before);
    f.shutdown.cancel();
}

/// Parts without a `BearerPrincipal`: the router's guard fails closed (500)
/// before any MCP dispatch, so the tool-level empty ceiling (unit-tested in
/// `ceiling_from_empty_when_parts_without_principal`) is defence in depth.
#[tokio::test(flavor = "multi_thread")]
async fn empty_ceiling_denies_read_and_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f = fixture(dir.path(), true).await;
    let before = fingerprint(&f).await;
    let init = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                   "clientInfo": {"name": "cc", "version": "0"}}
    });
    for token in [&READ, &RW, &super::http_tests::LEGACY] {
        let (head, _) = super::http_tests::post(f.addr, token, None, &init).await;
        assert!(
            !head.starts_with("HTTP/1.1 2"),
            "missing principal must fail closed: {head}"
        );
    }
    assert_eq!(fingerprint(&f).await, before);
    f.shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn r22_rw_omitted_agent_id_resolves_anonymous_no_token_row() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f = fixture(dir.path(), false).await;
    let a = seed_entry(&f.server, "cc-r22").await;
    let sid = open(f.addr, &RW).await;
    let _ = call(f.addr, &RW, &sid, "context_get", json!({"id": a})).await;
    let n = scalar(
        &f.server,
        "SELECT COUNT(*) FROM agent_registry WHERE agent_id LIKE 'token:%'",
    )
    .await;
    assert_eq!(n, 0, "no token: identity reaches the agents table");
    f.shutdown.cancel();
}

#[test]
fn require_cap_single_check_point() {
    let src = include_str!("tools.rs");
    assert!(!src.contains("require_cap(&ctx.agent_id"), "old signature");
    assert!(
        !src.contains("has_capability("),
        "direct registry check (R-02.5)"
    );
    assert!(
        !src.contains("require_capability("),
        "direct registry check (R-02.5)"
    );
    assert_eq!(
        src.matches("self.require_cap(&ctx, Capability::").count(),
        14,
        "one check per tool; update this count with any new tool"
    );
    assert_eq!(
        src.matches("require_cap(&ctx, Capability::Admin)").count(),
        2,
        "Admin sites changed: add them to rw_token_human_admin_tool_denied"
    );
}
