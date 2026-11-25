//! vnc-050 C5 (AC-01, R-15): the legacy token keeps full read + write on every
//! slug over real HTTP, and a corrupt `tokens.json` never affects it. Uses the
//! production `StaticTokenAuthLayer::with_token_table` constructor.

use std::sync::Arc;

use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::http_tests::{LEGACY, call_at, ok, open_at, seed_entry};
use crate::http::auth::StaticTokenAuthLayer;
use crate::http::router::{
    MultiProjectRouter, ObserveContext, PathRouter, ProjectServerInput, ProjectSlug, StoreResolver,
};
use crate::http::start_http_listener;
use crate::infra::config::HttpConfig;
use crate::server::tests::make_server;

const WRITER: &str = "legacy-writer";

/// Boot auth (reading `{dir}/tokens.json`) + router for `alpha` and `beta`.
/// Returns the seeded entry id per slug (same order as `SLUGS`).
async fn boot_two(dir: &std::path::Path) -> (std::net::SocketAddr, CancellationToken, Vec<u64>) {
    let auth = StaticTokenAuthLayer::with_token_table(LEGACY, dir, true);
    let mut inputs = Vec::new();
    let mut seeded = Vec::new();
    for slug in ["alpha", "beta"] {
        let server = make_server().await;
        server.registry.resolve_or_enroll(WRITER).expect("enroll");
        seeded.push(seed_entry(&server, "legacy").await);
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
    let service = tower::Layer::layer(&auth, PathRouter::new(resolver, observe_ctx));
    let addr = start_http_listener(&config, None, service, shutdown.clone())
        .await
        .expect("listener binds")
        .1;
    (addr, shutdown, seeded)
}

/// Legacy read (lookup) + write (deprecate) succeed on both slugs. Embedding-free
/// tools: the test embed service is never ready.
async fn assert_legacy_rw_both(addr: std::net::SocketAddr, seeded: &[u64]) {
    for (slug, id) in ["alpha", "beta"].into_iter().zip(seeded) {
        let sid = open_at(addr, slug, &LEGACY).await;
        let read = call_at(
            addr,
            slug,
            &LEGACY,
            &sid,
            "context_lookup",
            json!({"topic": "legacy"}),
        )
        .await;
        assert!(ok(&read), "{slug} legacy read: {read}");
        let write = call_at(
            addr,
            slug,
            &LEGACY,
            &sid,
            "context_deprecate",
            json!({"id": id, "agent_id": WRITER}),
        )
        .await;
        assert!(ok(&write), "{slug} legacy write: {write}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn legacy_read_write_two_slugs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (addr, shutdown, seeded) = boot_two(dir.path()).await;
    assert_legacy_rw_both(addr, &seeded).await;
    shutdown.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn legacy_unaffected_by_corrupt_tokens_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("tokens.json"), b"{not json").expect("corrupt");
    let (addr, shutdown, seeded) = boot_two(dir.path()).await;
    assert_legacy_rw_both(addr, &seeded).await;
    shutdown.cancel();
}
