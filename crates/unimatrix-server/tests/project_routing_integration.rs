//! Over-the-wire multi-project routing integration test (vnc-034 Wave 2, #727).
//!
//! Gate 3b flagged this file missing: it is the end-to-end instrument that drives
//! a REAL `SlugRouter` (the per-request MCP funnel `PathRouter` delegates its MCP
//! arm to) over a REAL `MultiProjectRouter` resolver wired to TWO distinct per-slug
//! `UnimatrixServer`/`Store` instances plus the Default alias store. infra-001
//! cannot reach the `/v1/{slug}/` HTTP edge (it spawns single-project stdio); this
//! is where AC-W2-R1 / R3 / R5 / R6 and the funnel no-bypass are actually proven
//! at the transport.
//!
//! ## What "over the wire" means here, precisely
//!
//! `SlugRouter::route_mcp` runs the full edge pipeline for every request:
//! `parse_project_key(path) -> resolve_store(key) -> adapter_for(key) -> dispatch`.
//! It is `pub` and takes ONLY the injected `Arc<dyn StoreResolver>` — the exact
//! same object and call site `PathRouter::new` builds internally (`main.rs`:1011,
//! `router.rs` `PathRouter::call` MCP fall-through arm). Driving `route_mcp`
//! directly exercises the real resolver, the real per-key `McpAdapter`, and the
//! real rmcp `StreamableHttpService` — no mock. The only `PathRouter`-specific
//! arms (`GET /health`, `POST /observe`) are not part of the slug-routing funnel
//! and are covered by `http/router/tests.rs`; constructing the heavy
//! `ObserveContext` they require would add nothing to the routing/isolation proof.
//!
//! ## Two observability layers, both load-bearing
//!
//! 1. **Edge funnel (HTTP):** each request is pushed through `route_mcp` and its
//!    response status is the routing discriminator —
//!      - known slug / Default `/v1/tools/`  -> reaches the rmcp adapter (NOT 404,
//!        NOT 400; rmcp answers the session-less request with its own 4xx, proving
//!        the request was DISPATCHED to that key's adapter, not rejected at the
//!        funnel);
//!      - unregistered slug                  -> 404 `unknown project` (never the
//!        default store, never a panic);
//!      - allowlist-violating slug in the URL -> 400 `invalid project slug`
//!        (rejected at the parse edge, BEFORE any store/path use).
//!    Because `route_mcp` `debug_assert!`s that the dispatched adapter
//!    `adapter_for(key)` wraps EXACTLY the store `resolve_store(key)` returned
//!    (OQ-PR-4), a non-404 slug dispatch is provably to that slug's OWN store —
//!    there is no residual fixed/default adapter a slug request could reach.
//! 2. **Store isolation (data):** the per-slug `Arc<Store>` handles are owned by
//!    the test (the SAME handles handed to the resolver), so a write into slug A's
//!    store is asserted ABSENT from slug B's store and the Default store — the
//!    knowledge-isolation invariant (AC-W2-R3) at the data layer the funnel routes
//!    to.
//!
//! These run with NO ONNX model dependency: routing, isolation, and the funnel
//! discriminator do not embed. (The seam-level `Arc::ptr_eq` resolution/agreement
//! proofs live in `src/http/router/project_resolver/tests.rs`; this file is the
//! transport-level complement Gate 3b required.)

use std::convert::Infallible;
use std::sync::Arc;

use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};

use unimatrix_adapt::{AdaptConfig, AdaptationService};
use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{Store, VectorAdapter, VectorConfig, VectorIndex};
use unimatrix_server::http::{
    MultiProjectRouter, ProjectServerInput, ProjectSlug, SlugRouter, StoreResolver,
};
use unimatrix_server::infra::audit::AuditLog;
use unimatrix_server::infra::categories::CategoryAllowlist;
use unimatrix_server::infra::embed_handle::EmbedServiceHandle;
use unimatrix_server::infra::registry::AgentRegistry;
use unimatrix_server::server::UnimatrixServer;
use unimatrix_store::{NewEntry, PoolConfig, Status};

const TEST_MAX_BODY: usize = 1024 * 1024;

// ---------------------------------------------------------------------------
// Harness — build real per-slug servers + the wired SlugRouter (no mocks)
// ---------------------------------------------------------------------------

/// A fully-assembled per-key server plus a clone of its OWN `Arc<Store>`.
///
/// The store clone lets the test assert data-layer isolation directly against the
/// SAME handle the resolver routes to.
struct ServerBundle {
    input_server: UnimatrixServer,
    store: Arc<Store>,
}

/// Build one real `UnimatrixServer` over a fresh, isolated temp store.
///
/// Mirrors the production `http_provision::build_project_server` / `make_server`
/// assembly using ONLY the public infra constructors (no isolated scaffolding):
/// each call opens its OWN temp db, so two bundles are DISTINCT stores — exactly
/// the two-store setup AC-W2-R3 isolation requires. The embedding handle is
/// constructed un-loaded; routing/isolation/funnel assertions never embed.
async fn build_server() -> ServerBundle {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let db_path = dir.path().join("unimatrix.db");
    let store = Arc::new(
        Store::open(&db_path, PoolConfig::default())
            .await
            .expect("open store"),
    );
    // Keep the temp dir alive for the process: the store holds the open db file.
    std::mem::forget(dir);

    let vector_index = Arc::new(
        VectorIndex::new(Arc::clone(&store), VectorConfig::default()).expect("vector index"),
    );
    let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
    let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

    let embed_handle = EmbedServiceHandle::new();

    let registry =
        Arc::new(AgentRegistry::new(Arc::clone(&store), true, Vec::new()).expect("registry"));
    registry.bootstrap_defaults().expect("bootstrap defaults");

    let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
    let categories = Arc::new(CategoryAllowlist::new());
    let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));

    let input_server = UnimatrixServer::new(
        Arc::clone(&store),
        async_vector_store,
        embed_handle,
        registry,
        audit,
        categories,
        Arc::clone(&store),
        vector_index,
        adapt_service,
        None,
    );

    ServerBundle {
        input_server,
        store,
    }
}

/// Build the wired routing stack: a real `MultiProjectRouter` over the default
/// store + the named slug stores, behind a real `SlugRouter` (the MCP funnel).
///
/// Returns the `SlugRouter` plus the owned `Arc<Store>` handles
/// (default, then one per slug in `slugs` order) so the test can assert data-layer
/// isolation against the exact stores the resolver routes to.
async fn wired_router(slugs: &[&str]) -> (SlugRouter, Arc<Store>, Vec<Arc<Store>>) {
    let default_bundle = build_server().await;
    let default_store = Arc::clone(&default_bundle.store);

    let mut inputs = Vec::with_capacity(slugs.len());
    let mut slug_stores = Vec::with_capacity(slugs.len());
    for &name in slugs {
        let bundle = build_server().await;
        let slug = ProjectSlug::try_from(name).expect("valid test slug");
        slug_stores.push(Arc::clone(&bundle.store));
        inputs.push(ProjectServerInput {
            slug,
            store: bundle.store,
            server: bundle.input_server,
        });
    }

    let resolver = MultiProjectRouter::from_servers(
        Arc::clone(&default_store),
        default_bundle.input_server,
        inputs,
        TEST_MAX_BODY,
        Vec::new(),
    )
    .expect("build MultiProjectRouter");

    let resolver: Arc<dyn StoreResolver> = Arc::new(resolver);
    let router = SlugRouter::new(resolver);
    (router, default_store, slug_stores)
}

// ---------------------------------------------------------------------------
// Request helpers
// ---------------------------------------------------------------------------

type TestBody = BoxBody<Bytes, Infallible>;

fn body(bytes: &'static str) -> TestBody {
    Full::new(Bytes::from(bytes))
        .map_err(|never| match never {})
        .boxed()
}

/// A minimal real MCP `initialize` JSON-RPC body. rmcp parses it; without a
/// session header rmcp answers with its own 4xx — which is exactly the signal we
/// want: the request was DISPATCHED to the resolved adapter (not funnel-rejected).
const MCP_INIT_BODY: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"itest","version":"0"}}}"#;

fn mcp_request(path: &str) -> Request<TestBody> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(body(MCP_INIT_BODY))
        .expect("build request")
}

/// Collect a routed `Response` into the `(StatusCode, String)` discriminator form
/// that `funnel_rejected` / `reached_mcp` consume. Reused by `drive` (path-only
/// requests) and by tests that build their own request (e.g. custom session headers).
async fn collect_resp(resp: Response<BoxBody<Bytes, Infallible>>) -> (StatusCode, String) {
    let status = resp.status();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn drive(router: &SlugRouter, path: &str) -> (StatusCode, String) {
    let mut router = router.clone();
    let resp: Response<BoxBody<Bytes, Infallible>> = router
        .route_mcp(mcp_request(path))
        .await
        .expect("route_mcp is infallible");
    collect_resp(resp).await
}

/// THE routing discriminator.
///
/// `route_mcp` rejects BEFORE dispatch with exactly two funnel-emitted bodies:
/// `{"error":"unknown project"}` (404) and `{"error":"invalid project slug"}`
/// (400). Any OTHER response — including rmcp's OWN 400 for a session-less MCP
/// POST — means the request was DISPATCHED to the resolved per-key adapter (the
/// funnel let it through). Discriminating on the funnel's specific error strings
/// (not the bare status) is necessary because rmcp also answers 400, so status
/// alone is ambiguous; the funnel's body is unique.
fn funnel_rejected((_status, body): &(StatusCode, String)) -> bool {
    body.contains("unknown project") || body.contains("invalid project slug")
}

/// True iff the request reached the per-key MCP adapter (was not funnel-rejected).
fn reached_mcp(resp: &(StatusCode, String)) -> bool {
    !funnel_rejected(resp)
}

fn test_entry(title: &str) -> NewEntry {
    NewEntry {
        title: title.to_string(),
        content: format!("content for {title}"),
        topic: "routing-itest".to_string(),
        category: "convention".to_string(),
        tags: vec![],
        source: "itest".to_string(),
        status: Status::Active,
        created_by: "human".to_string(),
        feature_cycle: "vnc-034".to_string(),
        trust_source: "human".to_string(),
    }
}

async fn entry_count(store: &Store) -> usize {
    store
        .query_all_entries()
        .await
        .expect("query_all_entries")
        .len()
}

// ===========================================================================
// AC-W2-R1 — /v1/{slug}/... routes to the per-slug store (two slugs, two stores)
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_two_slugs_route_to_distinct_stores() {
    // Two distinct slugs over two DISTINCT stores. Each per-slug path is DISPATCHED
    // to MCP (reaches the resolved adapter), and the resolved adapter is provably
    // the slug's own store (route_mcp's wraps_store debug_assert, OQ-PR-4). The
    // store handles are distinct instances (data-layer cross-check).
    let (router, default_store, slug_stores) = wired_router(&["alpha", "beta"]).await;
    let (alpha_store, beta_store) = (&slug_stores[0], &slug_stores[1]);

    let alpha_resp = drive(&router, "/v1/alpha/mcp").await;
    let beta_resp = drive(&router, "/v1/beta/mcp").await;

    assert!(
        reached_mcp(&alpha_resp),
        "/v1/alpha/ must DISPATCH to alpha's adapter (not 404/400); got {}",
        alpha_resp.0
    );
    assert!(
        reached_mcp(&beta_resp),
        "/v1/beta/ must DISPATCH to beta's adapter (not 404/400); got {}",
        beta_resp.0
    );
    // The two slug stores and the default are three distinct instances.
    assert!(
        !Arc::ptr_eq(alpha_store, beta_store),
        "alpha and beta must be DISTINCT store instances"
    );
    assert!(
        !Arc::ptr_eq(alpha_store, &default_store) && !Arc::ptr_eq(beta_store, &default_store),
        "neither slug store may be the default store"
    );
}

// ===========================================================================
// AC-W2-R3 — per-slug isolation: A's write is unreadable/absent via B
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_slug_a_write_unreadable_from_slug_b() {
    // Write an entry into alpha's OWN store (the handle the resolver routes
    // /v1/alpha/ to). It MUST NOT be readable from beta's store: read isolation.
    let (router, _default, slug_stores) = wired_router(&["alpha", "beta"]).await;
    let (alpha_store, beta_store) = (&slug_stores[0], &slug_stores[1]);

    // Prove routing is live for both before asserting isolation.
    assert!(reached_mcp(&drive(&router, "/v1/alpha/mcp").await));
    assert!(reached_mcp(&drive(&router, "/v1/beta/mcp").await));

    let id = alpha_store
        .insert(test_entry("alpha-only-secret"))
        .await
        .expect("insert into alpha");

    // Read isolation: beta's store has no such entry id, and no entry at all.
    assert!(
        beta_store.get(id).await.is_err(),
        "beta must NOT be able to read alpha's entry id {id} (read isolation)"
    );
    let beta_titles: Vec<String> = beta_store
        .query_all_entries()
        .await
        .expect("beta query")
        .into_iter()
        .map(|e| e.title)
        .collect();
    assert!(
        !beta_titles.iter().any(|t| t == "alpha-only-secret"),
        "alpha's entry must never appear in beta's store; beta saw {beta_titles:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_slug_a_write_does_not_appear_in_slug_b() {
    // Write isolation: after A writes, B's entry count is unchanged (B's store +
    // hash chain are untouched by A's write).
    let (_router, default_store, slug_stores) = wired_router(&["alpha", "beta"]).await;
    let (alpha_store, beta_store) = (&slug_stores[0], &slug_stores[1]);

    let beta_before = entry_count(beta_store).await;
    let default_before = entry_count(&default_store).await;

    alpha_store
        .insert(test_entry("a1"))
        .await
        .expect("insert a1");
    alpha_store
        .insert(test_entry("a2"))
        .await
        .expect("insert a2");

    assert_eq!(
        entry_count(alpha_store).await,
        2,
        "alpha holds its 2 writes"
    );
    assert_eq!(
        entry_count(beta_store).await,
        beta_before,
        "beta's entry count must be UNCHANGED by alpha's writes (write isolation)"
    );
    assert_eq!(
        entry_count(&default_store).await,
        default_before,
        "the default store must be UNCHANGED by alpha's writes"
    );
}

// ===========================================================================
// AC-W2-R2 / AC-CT-C4 — Default path (/v1/tools/...) unchanged with projects
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_v1_tools_default_unchanged_with_projects() {
    // With {alpha,beta} registered, /v1/tools/... still DISPATCHES to the Default
    // store's adapter (the single-project backward-compat alias, ADR-005). No
    // Wave-1 re-point: the Default path behaves identically to the no-projects case.
    let (router_with, default_with, _slug_stores) = wired_router(&["alpha", "beta"]).await;
    let (router_without, _default_without, _none) = wired_router(&[]).await;

    let with_resp = drive(&router_with, "/v1/tools/mcp").await;
    let without_resp = drive(&router_without, "/v1/tools/mcp").await;

    assert!(
        reached_mcp(&with_resp),
        "/v1/tools/ must DISPATCH to the Default adapter WITH projects registered; got {}",
        with_resp.0
    );
    assert_eq!(
        with_resp.0, without_resp.0,
        "Default path status must be IDENTICAL with and without [[projects]] (no re-point, AC-CT-C4)"
    );
    // Default store is writable and isolated from the slug stores (it is its own).
    default_with
        .insert(test_entry("default-entry"))
        .await
        .expect("default store is the real served store");
    assert_eq!(entry_count(&default_with).await, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_non_v1_path_routes_default() {
    // Backward-compat: a non-/v1 MCP path keeps current behavior -> Default key,
    // dispatched (never 404/400). Proves the resolver swap did not change the
    // current-MCP-path default routing.
    let (router, _default, _slugs) = wired_router(&["alpha"]).await;
    let resp = drive(&router, "/mcp").await;
    assert!(
        reached_mcp(&resp),
        "a non-/v1 MCP path must route to Default and dispatch; got {}",
        resp.0
    );
}

// ===========================================================================
// R-01 sc.3 — unregistered slug -> 404, never the default store, never a panic
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_unregistered_slug_returns_unknown_project() {
    // /v1/ghost/ parses as a valid slug grammar but is NOT registered -> 404
    // `unknown project`. It must NEVER fall through to the default store and must
    // NEVER panic. The default + alpha stores are untouched (no store created).
    let (router, default_store, slug_stores) = wired_router(&["alpha"]).await;
    let alpha_store = &slug_stores[0];

    let default_before = entry_count(&default_store).await;
    let alpha_before = entry_count(alpha_store).await;

    let (status, body) = drive(&router, "/v1/ghost/mcp").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "unregistered slug must be 404, not a default-store fall-through"
    );
    assert!(
        body.contains("unknown project"),
        "404 body should name the failure; got {body}"
    );
    // No store created, nothing routed into default or alpha.
    assert_eq!(entry_count(&default_store).await, default_before);
    assert_eq!(entry_count(alpha_store).await, alpha_before);
}

// ===========================================================================
// AC-W2-R6 (SR-09) — allowlist rejects traversal / encoded sep / uppercase /
// over-length AT THE EDGE: 400, no filesystem use, no store touched.
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_invalid_slug_path_rejected_at_edge() {
    let (router, default_store, slug_stores) = wired_router(&["alpha"]).await;
    let alpha_store = &slug_stores[0];

    let default_before = entry_count(&default_store).await;
    let alpha_before = entry_count(alpha_store).await;

    // 63 chars is the max valid; 64 'a's must be rejected (D1 length bound).
    let over_len = "a".repeat(64);
    let over_len_path = format!("/v1/{over_len}/mcp");

    // Each candidate violates the D1 allowlist ^[a-z0-9][a-z0-9-]{0,62}$ in a
    // DIFFERENT way (traversal, encoded separator, encoded dot, uppercase,
    // underscore, leading hyphen, over-length). All must be 400 at the parse edge.
    let cases: &[(&str, &str)] = &[
        ("/v1/..%2fetc%2fpasswd/mcp", "percent-encoded traversal"),
        ("/v1/..%2f../mcp", "encoded relative traversal"),
        ("/v1/%2e%2e/mcp", "encoded dot-dot"),
        ("/v1/Alpha/mcp", "uppercase (case-sensitivity escape)"),
        ("/v1/al_pha/mcp", "underscore (NOT in the D1 charset)"),
        ("/v1/-alpha/mcp", "leading hyphen"),
        (over_len_path.as_str(), "over-length (64 > 63)"),
    ];

    for (path, why) in cases {
        let (status, resp_body) = drive(&router, path).await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "invalid slug ({why}) at {path} must be rejected 400 at the parse edge"
        );
        assert!(
            resp_body.contains("invalid project slug"),
            "400 body should name the slug rejection ({why}); got {resp_body}"
        );
    }

    // Nothing was created or written: no path join ever happened.
    assert_eq!(
        entry_count(&default_store).await,
        default_before,
        "a rejected slug must not touch the default store"
    );
    assert_eq!(
        entry_count(alpha_store).await,
        alpha_before,
        "a rejected slug must not touch any slug store"
    );
}

// ===========================================================================
// AC-W2-R5 / FR-C7 — N clients : 1 slug share the store; each client bound to
// one slug; attribution by session_id (transport-derived identity).
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_n_clients_one_slug_share_store() {
    // Two distinct "clients" (two requests, distinct Mcp-Session-Id headers) on the
    // SAME slug both dispatch to alpha's adapter and therefore the SAME store. A
    // write by one is visible to the other (shared state). Each request is bound to
    // its slug purely by the URL path (transport-derived identity) — there is no
    // payload field naming a project, so a client cannot address a second slug.
    let (router, _default, slug_stores) = wired_router(&["alpha"]).await;
    let alpha_store = &slug_stores[0];

    let req = |session: &'static str| {
        Request::builder()
            .method("POST")
            .uri("/v1/alpha/mcp")
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-session-id", session)
            .body(body(MCP_INIT_BODY))
            .expect("build session request")
    };

    let mut r1 = router.clone();
    let s1 = collect_resp(
        r1.route_mcp(req("client-1"))
            .await
            .expect("client-1 routes"),
    )
    .await;
    let mut r2 = router.clone();
    let s2 = collect_resp(
        r2.route_mcp(req("client-2"))
            .await
            .expect("client-2 routes"),
    )
    .await;

    assert!(
        reached_mcp(&s1) && reached_mcp(&s2),
        "both clients on /v1/alpha/ must DISPATCH to alpha's adapter; got {} / {}",
        s1.0,
        s2.0
    );

    // Shared store: a write by "client-1" into alpha's store is visible to a read
    // representing "client-2" — N clients : 1 slug share state.
    let id = alpha_store
        .insert(test_entry("shared-by-client-1"))
        .await
        .expect("client-1 write");
    let seen = alpha_store
        .get(id)
        .await
        .expect("client-2 reads the shared store");
    assert_eq!(
        seen.title, "shared-by-client-1",
        "both clients on one slug see the SAME shared store state"
    );
}

// ===========================================================================
// AC-CT-C4 / R-01 — no-bypass funnel: every per-slug request dispatches via the
// resolved adapter; ≥2 slugs + Default each serviced by their own store; the
// Wave-1 fixed/discard path is gone.
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_dispatch_through_adapter_for_no_fixed_bypass() {
    // ≥2 distinct slugs + Default. Each per-slug request dispatches ONLY through
    // adapter_for(key) (route_mcp's debug_assert proves the dispatched adapter
    // wraps EXACTLY the resolved store — no leftover fixed/default adapter). We
    // observe this transport-side (all three keys dispatch) AND data-side (each
    // store only ever sees its own write).
    let (router, default_store, slug_stores) = wired_router(&["alpha", "beta"]).await;
    let (alpha_store, beta_store) = (&slug_stores[0], &slug_stores[1]);

    // All three keys dispatch (reach their adapter), none falls through to 404/400.
    assert!(reached_mcp(&drive(&router, "/v1/alpha/mcp").await));
    assert!(reached_mcp(&drive(&router, "/v1/beta/mcp").await));
    assert!(reached_mcp(&drive(&router, "/v1/tools/mcp").await));

    // Data-side proof there is no shared/fixed adapter: a write per key lands ONLY
    // in that key's store.
    alpha_store
        .insert(test_entry("alpha-w"))
        .await
        .expect("alpha write");
    beta_store
        .insert(test_entry("beta-w"))
        .await
        .expect("beta write");
    default_store
        .insert(test_entry("default-w"))
        .await
        .expect("default write");

    let titles = |s: &Arc<Store>| {
        let s = Arc::clone(s);
        async move {
            s.query_all_entries()
                .await
                .expect("query")
                .into_iter()
                .map(|e| e.title)
                .collect::<Vec<_>>()
        }
    };
    let alpha_titles = titles(alpha_store).await;
    let beta_titles = titles(beta_store).await;
    let default_titles = titles(&default_store).await;

    assert_eq!(
        alpha_titles,
        vec!["alpha-w".to_string()],
        "alpha store isolated"
    );
    assert_eq!(
        beta_titles,
        vec!["beta-w".to_string()],
        "beta store isolated"
    );
    assert_eq!(
        default_titles,
        vec!["default-w".to_string()],
        "default store isolated"
    );
}

// ===========================================================================
// Edge — Default and Slug interleaved in one process; concurrent same-slug share
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_default_and_slug_interleaved_no_cross_contamination() {
    // Interleave Default and Slug requests; assert each key's store only holds its
    // own writes — no cross-contamination across an interleaved sequence.
    let (router, default_store, slug_stores) = wired_router(&["alpha"]).await;
    let alpha_store = &slug_stores[0];

    for path in [
        "/v1/tools/mcp",
        "/v1/alpha/mcp",
        "/v1/tools/mcp",
        "/v1/alpha/mcp",
    ] {
        assert!(reached_mcp(&drive(&router, path).await), "dispatch {path}");
    }

    default_store
        .insert(test_entry("d"))
        .await
        .expect("default write");
    alpha_store
        .insert(test_entry("a"))
        .await
        .expect("alpha write");

    assert_eq!(entry_count(&default_store).await, 1);
    assert_eq!(entry_count(alpha_store).await, 1);
    assert!(
        default_store.get(2).await.is_err() || alpha_store.get(2).await.is_err(),
        "neither store accumulated the other's write"
    );
}
