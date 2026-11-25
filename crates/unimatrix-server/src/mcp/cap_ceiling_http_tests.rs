//! R-02 step 0 (vnc-050): over real HTTP in a stateful rmcp session, the
//! `BearerPrincipal` inserted by the auth layer is visible in the tool's
//! `RequestContext` via `Parts.extensions`.
//!
//! rmcp upgrade guard — do not remove.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use super::{PROBE, ProbeRecord};
use crate::http::auth::{StaticTokenAuthLayer, TokenTableValidator};
use crate::http::principal::{BearerPrincipal, TokenMode};
use crate::http::router::{
    MultiProjectRouter, ObserveContext, PathRouter, ProjectServerInput, ProjectSlug, StoreResolver,
};
use crate::http::start_http_listener;
use crate::http::token_table::TokenTable;
use crate::http::token_table::tests::write_table;
use crate::infra::config::HttpConfig;
use crate::server::tests::make_server;

pub(super) const LEGACY: [u8; 32] = [0x5A; 32];
const SCOPED: [u8; 32] = [0x11; 32];
const LABEL: &str = "step0-ci-read";

/// Booted listener + the in-process server handle (shared Arcs with the router).
pub(super) struct Fixture {
    pub(super) addr: SocketAddr,
    pub(super) shutdown: CancellationToken,
    pub(super) server: crate::server::UnimatrixServer,
}

fn strip_principal(
    mut r: http::Request<hyper::body::Incoming>,
) -> http::Request<hyper::body::Incoming> {
    r.extensions_mut().remove::<BearerPrincipal>();
    r
}

/// Boot the real auth layer + PathRouter + listener (plain HTTP) for slug `alpha`.
///
/// `scoped`: `(label, token, mode)` rows bound to `alpha`. `strip`: insert a
/// test-only layer between auth and router that removes the `BearerPrincipal`
/// (Parts reach the tool without a principal — empty-ceiling path).
pub(super) async fn boot_with(
    dir: &std::path::Path,
    scoped: &[(&str, [u8; 32], &str)],
    strip: bool,
) -> Fixture {
    let rows: Vec<_> = scoped
        .iter()
        .map(|(l, t, m)| (*l, *t, *m, &["alpha"][..], false))
        .collect();
    write_table(dir, &rows);
    let table = Arc::new(TokenTable::open(dir));
    let validator = TokenTableValidator::new(Some(LEGACY), table);
    let auth = StaticTokenAuthLayer::with_validator(Arc::new(validator));

    let server = make_server().await;
    let deps = make_server().await;
    let input = ProjectServerInput {
        slug: ProjectSlug::try_from("alpha").expect("valid"),
        store: Arc::clone(&server.store),
        server: server.clone(),
        vector_dir: std::path::PathBuf::from("alpha/vector"),
    };
    let router = MultiProjectRouter::from_servers(
        vec![input],
        1024 * 1024,
        vec![],
        vec!["localhost".to_string()],
    )
    .expect("build resolver");
    let resolver: Arc<dyn StoreResolver> = Arc::new(router);
    let observe_ctx = ObserveContext {
        resolver: Arc::clone(&resolver),
        embed_service: Arc::clone(&deps.embed_service),
        server_version: "test".to_string(),
    };
    let config = HttpConfig {
        enabled: true,
        content_port: 0,
        bind_address: "127.0.0.1".to_string(),
        max_concurrent_sessions: 16,
        max_request_body_bytes: 1_048_576,
        connection_timeout_secs: 30,
        allowed_origins: Vec::new(),
        legacy_token_enabled: true,
    };
    let shutdown = CancellationToken::new();
    let path_router = PathRouter::new(resolver, observe_ctx);
    let addr = if strip {
        let inner = tower::Layer::layer(
            &tower::util::MapRequestLayer::new(strip_principal as fn(_) -> _),
            path_router,
        );
        let service = tower::Layer::layer(&auth, inner);
        start_http_listener(&config, None, service, shutdown.clone())
            .await
            .expect("listener binds")
            .1
    } else {
        let service = tower::Layer::layer(&auth, path_router);
        start_http_listener(&config, None, service, shutdown.clone())
            .await
            .expect("listener binds")
            .1
    };
    Fixture {
        addr,
        shutdown,
        server,
    }
}

async fn boot(dir: &std::path::Path) -> (SocketAddr, CancellationToken) {
    let f = boot_with(dir, &[(LABEL, SCOPED, "read")], false).await;
    (f.addr, f.shutdown)
}

/// One HTTP/1.1 POST to `/v1/alpha/mcp`; returns (head, body) text.
pub(super) async fn post(
    addr: SocketAddr,
    token: &[u8; 32],
    session: Option<&str>,
    body: &serde_json::Value,
) -> (String, String) {
    post_at(addr, "alpha", token, session, body).await
}

/// One HTTP/1.1 POST to `/v1/{slug}/mcp` over a fresh TCP connection.
pub(super) async fn post_at(
    addr: SocketAddr,
    slug: &str,
    token: &[u8; 32],
    session: Option<&str>,
    body: &serde_json::Value,
) -> (String, String) {
    let body = body.to_string();
    let mut req = format!(
        "POST /v1/{slug}/mcp HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\
         Authorization: Bearer {}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\n",
        hex::encode(token),
        body.len()
    );
    if let Some(s) = session {
        req.push_str(&format!(
            "mcp-session-id: {s}\r\nmcp-protocol-version: 2025-06-18\r\n"
        ));
    }
    req.push_str("\r\n");
    req.push_str(&body);
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    stream.write_all(req.as_bytes()).await.expect("write");
    let mut buf = Vec::new();
    // SSE request streams close after the response; bound the read anyway.
    let _ = tokio::time::timeout(Duration::from_secs(15), stream.read_to_end(&mut buf)).await;
    let text = String::from_utf8_lossy(&buf).to_string();
    let (head, rest) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    (head.to_string(), rest.to_string())
}

pub(super) fn session_id(head: &str) -> String {
    head.lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case("mcp-session-id")
                .then(|| v.trim().to_string())
        })
        .unwrap_or_else(|| panic!("initialize must return mcp-session-id; head: {head}"))
}

/// initialize -> initialized -> tools/call context_search in one session.
async fn run_session(addr: SocketAddr, token: &[u8; 32]) {
    let init = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "step0", "version": "0"}
        }
    });
    let (head, body) = post(addr, token, None, &init).await;
    assert!(
        head.starts_with("HTTP/1.1 200"),
        "initialize: {head}\n{body}"
    );
    let sid = session_id(&head);
    let note = serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    let (head, _) = post(addr, token, Some(&sid), &note).await;
    assert!(head.starts_with("HTTP/1.1 202"), "initialized: {head}");
    let call = serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "context_search", "arguments": {"query": "step0"}}
    });
    let (head, body) = post(addr, token, Some(&sid), &call).await;
    assert!(
        head.starts_with("HTTP/1.1 200"),
        "tools/call: {head}\n{body}"
    );
}

/// initialize + notifications/initialized on `alpha`; returns the session id.
pub(super) async fn open(addr: SocketAddr, token: &[u8; 32]) -> String {
    open_at(addr, "alpha", token).await
}

/// initialize + notifications/initialized on `slug`; returns the session id.
pub(super) async fn open_at(addr: SocketAddr, slug: &str, token: &[u8; 32]) -> String {
    let init = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "cap-ceiling", "version": "0"}
        }
    });
    let (head, body) = post_at(addr, slug, token, None, &init).await;
    assert!(
        head.starts_with("HTTP/1.1 200"),
        "initialize: {head}\n{body}"
    );
    let sid = session_id(&head);
    let note = serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    let (head, _) = post_at(addr, slug, token, Some(&sid), &note).await;
    assert!(head.starts_with("HTTP/1.1 202"), "initialized: {head}");
    sid
}

/// `tools/call` on `alpha` in session `sid`; returns the JSON-RPC response.
pub(super) async fn call(
    addr: SocketAddr,
    token: &[u8; 32],
    sid: &str,
    name: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    call_at(addr, "alpha", token, sid, name, args).await
}

/// `tools/call` on `slug` in session `sid`; returns the JSON-RPC response.
pub(super) async fn call_at(
    addr: SocketAddr,
    slug: &str,
    token: &[u8; 32],
    sid: &str,
    name: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let req = serde_json::json!({
        "jsonrpc": "2.0", "id": 7, "method": "tools/call",
        "params": {"name": name, "arguments": args}
    });
    let (head, body) = post_at(addr, slug, token, Some(sid), &req).await;
    assert!(head.starts_with("HTTP/1.1 200"), "{name}: {head}\n{body}");
    body.lines()
        .filter_map(|l| l.trim().strip_prefix("data:"))
        .filter_map(|d| serde_json::from_str::<serde_json::Value>(d.trim()).ok())
        .find(|v| v.get("id") == Some(&serde_json::json!(7)))
        .unwrap_or_else(|| panic!("{name}: no JSON-RPC response in body: {body}"))
}

/// JSON-RPC error code, if the call failed at the protocol level.
pub(super) fn err_code(v: &serde_json::Value) -> Option<i64> {
    v.get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_i64())
}

/// Call succeeded (no JSON-RPC error and no tool-level `isError`).
pub(super) fn ok(v: &serde_json::Value) -> bool {
    v.get("error").is_none()
        && v.get("result")
            .and_then(|r| r.get("isError"))
            .and_then(|e| e.as_bool())
            != Some(true)
}

pub(super) async fn seed_entry(server: &crate::server::UnimatrixServer, topic: &str) -> u64 {
    let entry = unimatrix_core::NewEntry {
        title: format!("seed {topic}"),
        content: format!("seed content for {topic}"),
        topic: topic.to_string(),
        category: "pattern".to_string(),
        tags: vec![],
        source: String::new(),
        status: unimatrix_core::Status::Active,
        created_by: "test".to_string(),
        feature_cycle: String::new(),
        trust_source: "agent".to_string(),
    };
    server.store.insert(entry).await.expect("seed insert")
}

pub(super) async fn scalar(server: &crate::server::UnimatrixServer, sql: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(server.store.read_pool_test())
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn probe_snapshot() -> Vec<ProbeRecord> {
    PROBE.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn principal_visible_in_tool_request_context_over_http() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (addr, shutdown) = boot(dir.path()).await;

    // Scoped read token: the tool sees Principal(Scoped{LABEL, Read}) => ceiling [Read, Search].
    run_session(addr, &SCOPED).await;
    let seen = probe_snapshot().into_iter().any(|r| {
        matches!(&r, ProbeRecord::Principal(BearerPrincipal::Scoped { label, mode, .. })
            if label == LABEL && *mode == TokenMode::Read)
    });
    assert!(
        seen,
        "BearerPrincipal::Scoped must reach the tool RequestContext via Parts.extensions; \
         probe: {:?}",
        probe_snapshot()
    );

    // Legacy token: Principal(Legacy) => ceiling None.
    let before = probe_snapshot()
        .iter()
        .filter(|r| **r == ProbeRecord::Principal(BearerPrincipal::Legacy))
        .count();
    run_session(addr, &LEGACY).await;
    let after = probe_snapshot()
        .iter()
        .filter(|r| **r == ProbeRecord::Principal(BearerPrincipal::Legacy))
        .count();
    assert!(
        after > before,
        "legacy principal must reach the tool context"
    );

    shutdown.cancel();
}
