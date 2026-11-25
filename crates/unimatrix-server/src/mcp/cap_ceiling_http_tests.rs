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

const LEGACY: [u8; 32] = [0x5A; 32];
const SCOPED: [u8; 32] = [0x11; 32];
const LABEL: &str = "step0-ci-read";

/// Boot the real auth layer + PathRouter + listener (plain HTTP) for slug `alpha`.
async fn boot(dir: &std::path::Path) -> (SocketAddr, CancellationToken) {
    write_table(dir, &[(LABEL, SCOPED, "read", &["alpha"], false)]);
    let table = Arc::new(TokenTable::open(dir));
    let validator = TokenTableValidator::new(Some(LEGACY), table);
    let auth = StaticTokenAuthLayer::with_validator(Arc::new(validator));

    let server = make_server().await;
    let deps = make_server().await;
    let input = ProjectServerInput {
        slug: ProjectSlug::try_from("alpha").expect("valid"),
        store: Arc::clone(&server.store),
        server,
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
    let service = tower::Layer::layer(&auth, PathRouter::new(resolver, observe_ctx));
    let config = HttpConfig {
        enabled: true,
        content_port: 0,
        bind_address: "127.0.0.1".to_string(),
        max_concurrent_sessions: 16,
        max_request_body_bytes: 1_048_576,
        connection_timeout_secs: 30,
        allowed_origins: Vec::new(),
    };
    let shutdown = CancellationToken::new();
    let (_h, addr) = start_http_listener(&config, None, service, shutdown.clone())
        .await
        .expect("listener binds");
    (addr, shutdown)
}

/// One HTTP/1.1 POST over a fresh TCP connection; returns (head, body) text.
async fn post(
    addr: SocketAddr,
    token: &[u8; 32],
    session: Option<&str>,
    body: &serde_json::Value,
) -> (String, String) {
    let body = body.to_string();
    let mut req = format!(
        "POST /v1/alpha/mcp HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\
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

fn session_id(head: &str) -> String {
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
