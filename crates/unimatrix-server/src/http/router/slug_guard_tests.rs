//! vnc-050 C3 slug guard + C6 observe gate — real HTTP boundary tests.
//!
//! Boots the real auth layer (`TokenTableValidator`) + `PathRouter` +
//! listener over two slugs `alpha`/`beta`. Every rejection has a success
//! companion.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use unimatrix_core::Store;

use super::super::handlers::{DISPATCH_SPY, SESSION_WRITE_DENIED_MESSAGE};
use super::super::project_resolver::RESOLVE_SPY;
use super::*;
use crate::http::auth::{StaticTokenAuthLayer, TokenTableValidator};
use crate::http::router::{MultiProjectRouter, ObserveContext, PathRouter, ProjectServerInput};
use crate::http::start_http_listener;
use crate::http::token_table::TokenTable;
use crate::http::token_table::tests::write_table;
use crate::infra::config::HttpConfig;
use crate::server::tests::make_server;

const LEGACY: [u8; 32] = [0x6B; 32];
const READ_A: [u8; 32] = [0x21; 32];
const RW_A: [u8; 32] = [0x22; 32];
const L_READ: &str = "sg-read-alpha";
const L_RW: &str = "sg-rw-alpha";

/// Test-only layer: strips `BearerPrincipal` after auth (OVERVIEW seam
/// `StripPrincipalLayer`; hosted here because `http/auth*` is owned by
/// another agent this wave).
#[derive(Clone)]
struct StripPrincipal<S>(S);

impl<S, B> tower::Service<Request<B>> for StripPrincipal<S>
where
    S: tower::Service<Request<B>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;
    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.0.poll_ready(cx)
    }
    fn call(&mut self, mut req: Request<B>) -> Self::Future {
        req.extensions_mut().remove::<BearerPrincipal>();
        self.0.call(req)
    }
}

/// Sink that always "fails": drops every event and counts calls.
struct FailingSink(Mutex<usize>);
impl AccessAudit for FailingSink {
    fn record(&self, _slug: &ProjectSlug, _event: AuditEvent) {
        *self.0.lock().unwrap() += 1;
    }
}

struct Harness {
    addr: SocketAddr,
    alpha: Arc<Store>,
    beta: Arc<Store>,
    _shutdown: CancellationToken,
    _dir: tempfile::TempDir,
}

#[derive(Clone, Copy)]
enum Variant {
    Normal,
    Strip,
    FailingSink,
}

async fn boot(variant: Variant) -> Harness {
    let dir = tempfile::TempDir::new().expect("tmp");
    write_table(
        dir.path(),
        &[
            (L_READ, READ_A, "read", &["alpha"], false),
            (L_RW, RW_A, "read-write", &["alpha"], false),
        ],
    );
    let table = Arc::new(TokenTable::open(dir.path()));
    let auth = StaticTokenAuthLayer::with_validator(Arc::new(TokenTableValidator::new(
        Some(LEGACY),
        table,
    )));
    let mut inputs = Vec::new();
    let mut stores = Vec::new();
    for slug in ["alpha", "beta"] {
        let server = make_server().await;
        stores.push(Arc::clone(&server.store));
        inputs.push(ProjectServerInput {
            slug: ProjectSlug::try_from(slug).expect("valid"),
            store: Arc::clone(&server.store),
            server,
            vector_dir: std::path::PathBuf::from(format!("{slug}/vector")),
        });
    }
    let deps = make_server().await;
    let router = MultiProjectRouter::from_servers(
        inputs,
        1024 * 1024,
        vec![],
        vec!["localhost".to_string()],
    )
    .expect("resolver");
    let resolver: Arc<dyn StoreResolver> = Arc::new(router);
    let observe_ctx = ObserveContext {
        resolver: Arc::clone(&resolver),
        embed_service: Arc::clone(&deps.embed_service),
        server_version: "test".to_string(),
    };
    let mut path_router = PathRouter::new(resolver, observe_ctx);
    if matches!(variant, Variant::FailingSink) {
        path_router.access_audit = Arc::new(FailingSink(Mutex::new(0)));
    }
    let config = HttpConfig {
        enabled: true,
        content_port: 0,
        bind_address: "127.0.0.1".to_string(),
        max_concurrent_sessions: 16,
        max_request_body_bytes: 1_048_576,
        connection_timeout_secs: 30,
        allowed_origins: Vec::new(),
        ..HttpConfig::default()
    };
    let shutdown = CancellationToken::new();
    let addr = if matches!(variant, Variant::Strip) {
        let svc = tower::Layer::layer(&auth, StripPrincipal(path_router));
        start_http_listener(&config, None, svc, shutdown.clone()).await
    } else {
        let svc = tower::Layer::layer(&auth, path_router);
        start_http_listener(&config, None, svc, shutdown.clone()).await
    }
    .expect("listener")
    .1;
    let beta = stores.pop().expect("beta");
    let alpha = stores.pop().expect("alpha");
    Harness {
        addr,
        alpha,
        beta,
        _shutdown: shutdown,
        _dir: dir,
    }
}

/// One HTTP/1.1 request; returns (status, body).
async fn send(
    addr: SocketAddr,
    method: &str,
    path: &str,
    token: &[u8; 32],
    body: &str,
) -> (u16, String) {
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\
         Authorization: Bearer {}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\n\r\n{body}",
        hex::encode(token),
        body.len()
    );
    let mut s = tokio::net::TcpStream::connect(addr).await.expect("connect");
    s.write_all(req.as_bytes()).await.expect("write");
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(10), s.read_to_end(&mut buf)).await;
    let text = String::from_utf8_lossy(&buf).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let body = text.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
    (status, body.to_string())
}

const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"sg","version":"0"}}}"#;
const PING: &str = r#"{"type":"Ping"}"#;
const DENIED_BODY: &str = r#"{"error":"token not permitted for project"}"#;

fn denied(status: u16, body: &str) -> bool {
    status == 403 && body.trim() == DENIED_BODY
}

async fn count(store: &Store, sql: &str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(store.read_pool_test())
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
}

/// Poll `http_access` rows whose metadata contains `needle` until `want`.
async fn access_rows(store: &Store, needle: &str, want: i64) -> Vec<(String, String, i64)> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let rows: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT agent_id, metadata, outcome FROM audit_log \
             WHERE operation = 'http_access' AND metadata LIKE ?1",
        )
        .bind(format!("%{needle}%"))
        .fetch_all(store.read_pool_test())
        .await
        .expect("audit read");
        if rows.len() as i64 >= want || tokio::time::Instant::now() > deadline {
            return rows;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn spy_has(spy: &Mutex<Vec<String>>, needle: &str) -> bool {
    spy.lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .any(|k| k.contains(needle))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn route_matrix_slug_binding() {
    let h = boot(Variant::Normal).await;
    let routes: [(&str, &str, &str); 4] = [
        ("POST", "mcp", INIT),
        ("GET", "mcp", ""),
        ("DELETE", "mcp", ""),
        ("POST", "observe", PING),
    ];
    for (method, tail, body) in routes {
        for (name, tok) in [("legacy", &LEGACY), ("read", &READ_A), ("rw", &RW_A)] {
            // Target B: scoped → 403 generic; legacy passes the guard.
            let (st, b) = send(h.addr, method, &format!("/v1/beta/{tail}"), tok, body).await;
            if name == "legacy" {
                assert!(
                    !denied(st, &b) && st != 500,
                    "{method} {tail} legacy@B: {st} {b}"
                );
            } else {
                assert!(denied(st, &b), "{method} {tail} {name}@B: {st} {b}");
            }
            // Companion: A passes the guard for all.
            let (st, b) = send(h.addr, method, &format!("/v1/alpha/{tail}"), tok, body).await;
            assert!(
                !denied(st, &b) && st != 500,
                "{method} {tail} {name}@A: {st} {b}"
            );
            if tail == "observe" && name == "read" {
                assert_eq!(st, 403, "read@A observe → SessionWrite 403");
                assert!(b.contains(SESSION_WRITE_DENIED_MESSAGE), "{b}");
            } else if (method, tail) == ("POST", "mcp") || tail == "observe" {
                assert_eq!(st, 200, "{method} {tail} {name}@A: {b}");
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn path_variants_never_200() {
    let h = boot(Variant::Normal).await;
    for path in [
        "/v1/sgpv/",
        "/v1/sgpv",
        "/v1/%42/mcp",
        "/v1//sgpv/mcp",
        "/v1/tools/call",
        "/v1/beta/mcp",
        "/v1/Beta/mcp",
        "/v1/BETA/mcp",
    ] {
        let (st, b) = send(h.addr, "POST", path, &RW_A, INIT).await;
        assert!(
            matches!(st, 400 | 403 | 404),
            "{path}: must be 403/400/404, got {st} {b}"
        );
    }
    assert!(
        !spy_has(&RESOLVE_SPY, "\"sgpv\""),
        "denied sgpv reached resolve_store"
    );
    assert!(
        !spy_has(&RESOLVE_SPY, "\"tools\""),
        "denied tools reached resolve_store"
    );
    // Companion: rw@A on A is 200.
    let (st, _) = send(h.addr, "POST", "/v1/alpha/mcp", &RW_A, INIT).await;
    assert_eq!(st, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn guard_403_body_identical_existing_vs_missing_slug() {
    let h = boot(Variant::Normal).await;
    let b = send(h.addr, "POST", "/v1/beta/mcp", &RW_A, INIT).await;
    let z = send(h.addr, "POST", "/v1/zeta/mcp", &RW_A, INIT).await;
    assert_eq!(b, z, "existing vs unprovisioned must be indistinguishable");
    assert!(denied(b.0, &b.1));
    // Companion: legacy distinguishes (404 for zeta) — guard does not alter it.
    let (st, _) = send(h.addr, "POST", "/v1/zeta/mcp", &LEGACY, INIT).await;
    assert_eq!(st, 404);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn guard_runs_before_resolve_store() {
    let h = boot(Variant::Normal).await;
    // Unique unprovisioned slug so parallel tests cannot pollute the spy.
    let (st, b) = send(h.addr, "POST", "/v1/sg-unique-q/mcp", &READ_A, INIT).await;
    assert!(denied(st, &b), "{st} {b}");
    assert!(!spy_has(&RESOLVE_SPY, "sg-unique-q"));
    // Companion: legacy on the same slug passes the guard and reaches the funnel.
    let (st, _) = send(h.addr, "POST", "/v1/sg-unique-q/mcp", &LEGACY, INIT).await;
    assert_eq!(st, 404);
    assert!(spy_has(&RESOLVE_SPY, "sg-unique-q"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_principal_returns_500() {
    let h = boot(Variant::Strip).await;
    let (st, _) = send(h.addr, "POST", "/v1/alpha/mcp", &LEGACY, INIT).await;
    assert_eq!(st, 500, "missing principal fails closed");
    // Companion: /health is exempt and still served.
    let (st, _) = send(h.addr, "GET", "/health", &LEGACY, "").await;
    assert_eq!(st, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_access_row_per_request() {
    let h = boot(Variant::Normal).await;
    send(h.addr, "POST", "/v1/alpha/mcp", &LEGACY, INIT).await;
    send(h.addr, "POST", "/v1/alpha/mcp", &RW_A, INIT).await;
    send(h.addr, "POST", "/v1/alpha/observe", &RW_A, PING).await;
    let legacy = access_rows(&h.alpha, "\"token_label\":\"legacy\"", 1).await;
    assert_eq!(legacy.len(), 1, "{legacy:?}");
    assert_eq!(legacy[0].0, "http-bearer");
    let rw = access_rows(&h.alpha, L_RW, 2).await;
    assert_eq!(rw.len(), 2, "{rw:?}");
    assert!(
        rw.iter()
            .all(|r| r.0 == format!("token:{L_RW}") && r.2 == 0)
    );
    assert!(rw.iter().any(|r| r.1.contains("\"route\":\"observe\"")));
    assert!(
        rw.iter()
            .any(|r| r.1.contains("\"route\":\"mcp\"") && r.1.contains("POST"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn denial_logged_to_bound_slug_only() {
    let h = boot(Variant::Normal).await;
    let before = count(&h.beta, "SELECT COUNT(*) FROM audit_log").await;
    let (st, b) = send(h.addr, "POST", "/v1/beta/mcp", &RW_A, INIT).await;
    assert!(denied(st, &b));
    let rows = access_rows(&h.alpha, L_RW, 1).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].2, 1, "outcome Denied");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        count(&h.beta, "SELECT COUNT(*) FROM audit_log").await,
        before
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audit_sink_failure_does_not_fail_request() {
    let h = boot(Variant::FailingSink).await;
    let (st, _) = send(h.addr, "POST", "/v1/alpha/mcp", &RW_A, INIT).await;
    assert_eq!(st, 200);
    let (st, b) = send(h.addr, "POST", "/v1/beta/mcp", &RW_A, INIT).await;
    assert!(denied(st, &b), "denial still enforced with failing sink");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audit_rows_sentinel_free() {
    let h = boot(Variant::Normal).await;
    send(h.addr, "POST", "/v1/alpha/mcp", &RW_A, INIT).await;
    send(h.addr, "POST", "/v1/beta/mcp", &READ_A, INIT).await;
    access_rows(&h.alpha, L_READ, 1).await;
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT agent_id, detail, metadata FROM audit_log")
            .fetch_all(h.alpha.read_pool_test())
            .await
            .expect("read");
    let secrets = [hex::encode(RW_A), hex::encode(READ_A), hex::encode(LEGACY)];
    for r in &rows {
        let all = format!("{} {} {}", r.0, r.1, r.2).to_lowercase();
        assert!(!all.contains("bearer "), "{all}");
        for s in &secrets {
            assert!(!all.contains(s), "secret leaked: {all}");
        }
    }
    assert!(!rows.is_empty());
}

#[path = "observe_gate_tests.rs"]
mod observe_gate;
