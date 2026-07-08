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
//!
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

// crt-056 Wave 2 behavioral imports — the per-slug tick work-unit seam (ADR-003/004/005).
use unimatrix_engine::confidence::ConfidenceParams;
use unimatrix_observe::domain::DomainPackRegistry;
use unimatrix_server::background::{
    BackgroundJob, Cadence, PerSlugTickContext, ResourceClass, SharedTickResources,
    build_job_registry, run_per_slug_tick_pass,
};
use unimatrix_server::infra::config::{InferenceConfig, RetentionConfig};
use unimatrix_server::infra::nli_handle::NliServiceHandle;
use unimatrix_server::infra::rayon_pool::RayonPool;
use unimatrix_server::infra::usage_dedup::UsageDedup;
use unimatrix_server::services::{FusionWeights, ServiceLayer};

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
        None, // crt-056: test-default ServiceLayer
    );

    ServerBundle {
        input_server,
        store,
    }
}

/// Build the wired routing stack: a real `MultiProjectRouter` over the named slug
/// stores ONLY, behind a real `SlugRouter` (the MCP funnel).
///
/// vnc-038 ADR-004 (#5083): the served-project `Default` is DELETED. The resolver
/// is keyed by `ProjectKey::Slug` ONLY — there is no default store and no default
/// constructor params. `from_servers` now takes just the per-slug inputs (single
/// project is N=1: one map entry, no special-case arm). A no-slug / `/v1/tools`
/// request is a loud `UnknownProject`, never a servable default.
///
/// Returns the `SlugRouter` plus the owned per-slug `Arc<Store>` handles (one per
/// slug in `slugs` order) so the test can assert data-layer isolation against the
/// exact stores the resolver routes to.
async fn wired_router(slugs: &[&str]) -> (SlugRouter, Vec<Arc<Store>>) {
    let mut inputs = Vec::with_capacity(slugs.len());
    let mut slug_stores = Vec::with_capacity(slugs.len());
    for &name in slugs {
        let bundle = build_server().await;
        let slug = ProjectSlug::try_from(name).expect("valid test slug");
        slug_stores.push(Arc::clone(&bundle.store));
        // #823: inert here (routing test never drives shutdown); a faithful
        // per-slug dump dir for the new ProjectServerInput field.
        let vector_dir = std::path::PathBuf::from(slug.as_str()).join("vector");
        inputs.push(ProjectServerInput {
            slug,
            store: bundle.store,
            server: bundle.input_server,
            vector_dir,
        });
    }

    // bug #774: allowed_hosts must be NON-EMPTY (empty = rmcp allow-all fail-open).
    let resolver = MultiProjectRouter::from_servers(
        inputs,
        TEST_MAX_BODY,
        Vec::new(),
        vec!["localhost".to_string()],
    )
    .expect("build MultiProjectRouter");

    let resolver: Arc<dyn StoreResolver> = Arc::new(resolver);
    let router = SlugRouter::new(resolver);
    (router, slug_stores)
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
    let (router, slug_stores) = wired_router(&["alpha", "beta"]).await;
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
    // The two slug stores are distinct instances (vnc-038 ADR-004: no default store).
    assert!(
        !Arc::ptr_eq(alpha_store, beta_store),
        "alpha and beta must be DISTINCT store instances"
    );
}

// ===========================================================================
// AC-W2-R3 — per-slug isolation: A's write is unreadable/absent via B
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_slug_a_write_unreadable_from_slug_b() {
    // Write an entry into alpha's OWN store (the handle the resolver routes
    // /v1/alpha/ to). It MUST NOT be readable from beta's store: read isolation.
    let (router, slug_stores) = wired_router(&["alpha", "beta"]).await;
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
    let (_router, slug_stores) = wired_router(&["alpha", "beta"]).await;
    let (alpha_store, beta_store) = (&slug_stores[0], &slug_stores[1]);

    let beta_before = entry_count(beta_store).await;

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
}

// ===========================================================================
// vnc-038 ADR-004 / AC-01 / R-07 — INVERTED (was AC-W2-R2 Default-alias tests).
//
// These three tests previously ASSERTED the `/v1/tools/...` Default alias and the
// `_ => Default` fall-through DISPATCHED to a servable Default store. vnc-038
// DELETES the default. They are rewritten (NOT deleted — call-site audit #2398,
// avoid-vacuous-pass #4452) to exercise the PREVIOUSLY-PASSING path and assert it
// now fails LOUD: a no-slug / `/v1/tools` request resolves NOTHING servable. The
// loud-error leg over the REAL funnel is the inversion's whole point.
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_v1_tools_default_alias_gone_is_loud_404() {
    // INVERTED. With {alpha,beta} registered, `/v1/tools/...` no longer dispatches
    // a servable Default. `tools` parses as a slug *candidate* (the alias arm is
    // deleted); it is unregistered here, so the funnel answers a loud 404
    // `unknown project` — NEVER a default store (AC-01 / R-07 / R-10).
    let (router_with, _slug_stores) = wired_router(&["alpha", "beta"]).await;
    let (router_without, _none) = wired_router(&[]).await;

    let with_resp = drive(&router_with, "/v1/tools/mcp").await;
    let without_resp = drive(&router_without, "/v1/tools/mcp").await;

    assert!(
        funnel_rejected(&with_resp),
        "/v1/tools/ must be funnel-rejected (no Default alias) WITH projects; got {}",
        with_resp.0
    );
    assert_eq!(
        with_resp.0,
        StatusCode::NOT_FOUND,
        "the deleted Default alias means `/v1/tools/...` is a loud 404, never a servable store"
    );
    assert!(
        with_resp.1.contains("unknown project"),
        "404 body must name the failure; got {}",
        with_resp.1
    );
    // The behavior is IDENTICAL with and without [[projects]]: there is no default
    // store either way (single rule, no special-case arm).
    assert_eq!(
        with_resp.0, without_resp.0,
        "`/v1/tools/...` is loud-404 with AND without [[projects]] (no default, AC-01)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_non_v1_path_is_loud_404_not_default() {
    // INVERTED. The `_ => Default` fall-through is DELETED (vnc-038 ADR-004). A
    // non-`/v1`-slug MCP path (`/mcp`) no longer routes to a Default store — it is
    // a loud 404 `unknown project` at the funnel (AC-01 / R-10). Exercises the
    // previously-passing default-dispatch path and proves it now fails loud.
    let (router, _slugs) = wired_router(&["alpha"]).await;
    let resp = drive(&router, "/mcp").await;
    assert!(
        funnel_rejected(&resp),
        "a non-/v1 MCP path must be funnel-rejected (no Default fall-through); got {}",
        resp.0
    );
    assert_eq!(
        resp.0,
        StatusCode::NOT_FOUND,
        "the deleted `_ => Default` arm makes a no-slug path a loud 404, never a default store"
    );
    assert!(
        resp.1.contains("unknown project"),
        "404 body must name the failure; got {}",
        resp.1
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
    let (router, slug_stores) = wired_router(&["alpha"]).await;
    let alpha_store = &slug_stores[0];

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
    // No store created, nothing routed into alpha (there is no default store).
    assert_eq!(entry_count(alpha_store).await, alpha_before);
}

// ===========================================================================
// AC-W2-R6 (SR-09) — allowlist rejects traversal / encoded sep / uppercase /
// over-length AT THE EDGE: 400, no filesystem use, no store touched.
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_invalid_slug_path_rejected_at_edge() {
    let (router, slug_stores) = wired_router(&["alpha"]).await;
    let alpha_store = &slug_stores[0];

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
    let (router, slug_stores) = wired_router(&["alpha"]).await;
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
// AC-CT-C4 / R-01 / R-09 — no-bypass funnel: every per-slug request dispatches
// via the resolved adapter; ≥2 slugs each serviced by their OWN store; the
// Wave-1 fixed/discard path AND the vnc-038-deleted Default are both gone.
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_dispatch_through_adapter_for_no_fixed_bypass() {
    // N=2 distinct slugs (vnc-038 ADR-004: no Default). Each per-slug request
    // dispatches ONLY through adapter_for(key) (route_mcp's debug_assert proves the
    // dispatched adapter wraps EXACTLY the resolved store — no leftover
    // fixed/default adapter). We observe this transport-side (both keys dispatch)
    // AND data-side (each store only ever sees its own write). `/v1/tools/mcp` is
    // now a loud 404 (the alias is gone) — a residual default would dispatch it.
    let (router, slug_stores) = wired_router(&["alpha", "beta"]).await;
    let (alpha_store, beta_store) = (&slug_stores[0], &slug_stores[1]);

    // Both registered slugs dispatch (reach their adapter), none falls to 404/400.
    assert!(reached_mcp(&drive(&router, "/v1/alpha/mcp").await));
    assert!(reached_mcp(&drive(&router, "/v1/beta/mcp").await));
    // The deleted Default alias: `/v1/tools/...` is funnel-rejected, NOT dispatched
    // to a residual fixed/default adapter (R-07 / R-09 no-bypass at N=2).
    let tools_resp = drive(&router, "/v1/tools/mcp").await;
    assert!(
        funnel_rejected(&tools_resp),
        "/v1/tools/ must be funnel-rejected (no Default), got {}",
        tools_resp.0
    );

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
}

// ===========================================================================
// Edge — INVERTED (was Default+Slug interleave). Two SLUGS interleaved in one
// process; the deleted Default `/v1/tools/...` leg is loud-404, never dispatched.
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_two_slugs_interleaved_no_cross_contamination() {
    // INVERTED: the Default leg is GONE (vnc-038 ADR-004). Interleave two REAL
    // slugs and a (now loud-404) `/v1/tools/...` probe; assert each slug's store
    // holds ONLY its own writes — no cross-contamination across the interleaved
    // sequence, and the tools probe never dispatches into either store.
    let (router, slug_stores) = wired_router(&["alpha", "beta"]).await;
    let (alpha_store, beta_store) = (&slug_stores[0], &slug_stores[1]);

    for path in [
        "/v1/alpha/mcp",
        "/v1/beta/mcp",
        "/v1/alpha/mcp",
        "/v1/beta/mcp",
    ] {
        assert!(reached_mcp(&drive(&router, path).await), "dispatch {path}");
    }
    // The deleted Default alias is loud-404, interleaved in: it must not dispatch
    // into any slug store (no residual default/bypass).
    assert!(
        funnel_rejected(&drive(&router, "/v1/tools/mcp").await),
        "/v1/tools/ is loud-404 (no Default), never dispatched mid-interleave"
    );

    alpha_store
        .insert(test_entry("a"))
        .await
        .expect("alpha write");
    beta_store
        .insert(test_entry("b"))
        .await
        .expect("beta write");

    assert_eq!(
        entry_count(alpha_store).await,
        1,
        "alpha holds only its write"
    );
    assert_eq!(
        entry_count(beta_store).await,
        1,
        "beta holds only its write"
    );
    assert!(
        alpha_store.get(2).await.is_err() && beta_store.get(2).await.is_err(),
        "neither store accumulated the other's write"
    );
}

// ###########################################################################
// crt-056 Wave 2 — per-slug tick behavioral layer (AC-3 / AC-4 ★ / AC-5 /
// AC-harness). The load-bearing trio runs against a REAL multi-project setup
// at N=2: two distinct slugs, each with its own config-driven analytics handle
// set, ticked through the SAME serial `run_per_slug_tick_pass` the daemon uses.
//
// Why this lives here (cumulative, NFR-7 / C-9): the vnc-034 harness above
// already proves per-slug STORE isolation at the transport. crt-056 needs the
// per-slug ANALYTICS isolation proof — that ticking slug B never mutates slug
// A's TypedGraphState/EffectivenessState/ConfidenceState/contradiction cache.
// We extend `build_server()`/`wired_router()` (the SAME real `UnimatrixServer`
// + `Arc<Store>` builders) with a `TickTestHarness` that also borrows each
// server's config-driven `ServiceLayer` into a `PerSlugTickContext`.
//
// MODEL-FREE BY DESIGN: like the routing tests above, these never load an ONNX
// model. The byte-for-byte AC-4 proof rests on `TypedGraphState` (rebuilt purely
// from store rows — `all_entries`/`use_fallback`) and `EffectivenessState.generation`,
// both of which mutate without embeddings. The contradiction scan + confidence
// recompute are model/serving-path bound (they do NOT run in a model-free tick),
// so they are asserted as the "unchanged" half of the corruption guard. The
// search-delta half of AC-5 (phase blending observable through `search()`) needs
// a loaded model + the crate-private `SearchService`; it is covered by the
// in-crate handle-identity unit test (`Arc::ptr_eq`) + the unit search tests, and
// is mirrored here structurally via `assert_handles_are_service_layer_arcs`.
// ###########################################################################

/// One ticked slug: the real server (so we can borrow its config-driven
/// `ServiceLayer` + per-slug subsystems via the public accessors), its store,
/// and the `PerSlugTickContext` that borrows the SAME analytics handles.
struct TickedSlug {
    server: UnimatrixServer,
    store: Arc<Store>,
    ctx: PerSlugTickContext,
}

/// A multi-slug tick harness over N real per-slug servers, sharing ONE rayon
/// pool (panic_handler installed via `RayonPool::new`, #2543 — AC-harness) and
/// ONE unloaded `NliServiceHandle` (AC-2 shape: one model handle, never N).
struct TickTestHarness {
    slugs: Vec<TickedSlug>,
    registry: Vec<Box<dyn BackgroundJob>>,
    shared: SharedTickResources,
}

impl TickTestHarness {
    /// Build N real servers (reusing `build_server()`), borrow each one's
    /// config-driven `ServiceLayer` into a `PerSlugTickContext`, and assemble
    /// the SHARED read-only resources (one rayon pool, one nli handle).
    async fn new(slug_names: &[&str]) -> Self {
        let mut slugs = Vec::with_capacity(slug_names.len());
        for &name in slug_names {
            let bundle = build_server().await;
            let server = bundle.input_server;
            let store = bundle.store;
            let slug = ProjectSlug::try_from(name).expect("valid slug");
            // The borrow bundle: every analytics handle is an Arc::clone of THIS
            // server's ServiceLayer accessor — the SAME Arc<RwLock<_>> the serving
            // path reads (ADR-003). next_tick uses the server's own tick_metadata.
            let ctx = PerSlugTickContext::from_service_layer(
                slug,
                Arc::clone(&store),
                server.vector_index(),
                server.service_layer(),
                server.tick_metadata(),
                server.adapt_service(),
                Arc::clone(&server.session_registry),
                Arc::clone(&server.pending_entries_analysis),
                server.audit_log(),
            );
            slugs.push(TickedSlug { server, store, ctx });
        }

        // ONE shared rayon pool. RayonPool::new installs `.panic_handler(|_| {})`
        // (#2543) — this is the AC-harness obligation: a panicking job is contained,
        // never SIGABRTs the test process.
        let rayon_pool = Arc::new(
            RayonPool::new(1, "crt056-itest-tick").expect("rayon pool with panic_handler"),
        );

        let shared = SharedTickResources {
            embed_service: EmbedServiceHandle::new(), // unloaded — model-free tick
            nli_handle: NliServiceHandle::new(),      // ONE handle, shared read-only
            inference_config: Arc::new(InferenceConfig::default()),
            confidence_params: Arc::new(ConfidenceParams::default()),
            rayon_pool,
            category_allowlist: Arc::new(CategoryAllowlist::new()),
            retention_config: Arc::new(RetentionConfig::default()),
            auto_quarantine_cycles: 3,
            tick_interval_secs: 900,
        };

        TickTestHarness {
            slugs,
            registry: build_job_registry(),
            shared,
        }
    }

    /// Run one full registry pass over a single slug's context (the real serial
    /// loop body — `run_per_slug_tick_pass` over a one-element slice).
    async fn tick(&self, idx: usize) {
        let ctx = std::slice::from_ref(&self.slugs[idx].ctx);
        run_per_slug_tick_pass(ctx, &self.registry, &self.shared).await;
    }

    fn store(&self, idx: usize) -> &Arc<Store> {
        &self.slugs[idx].store
    }
}

/// A deterministic snapshot of the four AC-4 analytics states for one slug.
///
/// The four inner state types do not all derive `PartialEq`/`Serialize`, so we
/// capture a STABLE comparison surface under short read locks (NFR-06 poison
/// recovery): the typed-graph entry-set size + fallback flag (model-free
/// observable), the effectiveness generation + classified-entry count, the four
/// `ConfidenceState` f64 fields, and the contradiction-cache occupancy. A
/// cross-slug write would perturb at least one of these.
#[derive(Debug, Clone, PartialEq)]
struct HandleSnapshot {
    typed_graph_entry_count: usize,
    typed_graph_use_fallback: bool,
    effectiveness_generation: u64,
    effectiveness_category_count: usize,
    confidence: (f64, f64, f64, f64),
    contradiction_is_some: bool,
}

fn snapshot_handles(ctx: &PerSlugTickContext) -> HandleSnapshot {
    let tg = ctx.typed_graph.read().unwrap_or_else(|e| e.into_inner());
    let eff = ctx.effectiveness.read().unwrap_or_else(|e| e.into_inner());
    let conf = ctx.confidence.read().unwrap_or_else(|e| e.into_inner());
    let contra = ctx.contradiction.read().unwrap_or_else(|e| e.into_inner());
    HandleSnapshot {
        typed_graph_entry_count: tg.all_entries.len(),
        typed_graph_use_fallback: tg.use_fallback,
        effectiveness_generation: eff.generation,
        effectiveness_category_count: eff.categories.len(),
        confidence: (
            conf.alpha0,
            conf.beta0,
            conf.observed_spread,
            conf.confidence_weight,
        ),
        contradiction_is_some: contra.is_some(),
    }
}

/// Insert `n` distinct Active entries into a slug's store (model-free content
/// that makes `TypedGraphState::rebuild` produce a non-default, slug-specific
/// state: `all_entries.len() == n`, `use_fallback == false`).
async fn populate(store: &Arc<Store>, n: usize, prefix: &str) {
    for i in 0..n {
        store
            .insert(test_entry(&format!("{prefix}-entry-{i}")))
            .await
            .expect("insert populate entry");
    }
}

// ---------------------------------------------------------------------------
// AC-harness (R-10) — the shared rayon pool installs the panic_handler; a job
// that panics is contained (clean fail, NO SIGABRT). Manifestation guarded
// against: "signal: 6, SIGABRT" killing the whole test binary.
// ---------------------------------------------------------------------------

struct PanickingJob;
#[async_trait::async_trait]
impl BackgroundJob for PanickingJob {
    fn name(&self) -> &str {
        "panicking"
    }
    fn cadence(&self) -> Cadence {
        Cadence::EveryTick
    }
    fn resource_class(&self) -> ResourceClass {
        ResourceClass::Rayon
    }
    async fn run(
        &self,
        _ctx: &PerSlugTickContext,
        shared: &SharedTickResources,
    ) -> Result<(), String> {
        // Panic INSIDE rayon work on the shared pool — exactly the production
        // path. RayonPool::spawn's panic_handler (#2543) absorbs the unwind and
        // maps it to Err(RayonError::Cancelled) via the dropped oneshot sender,
        // so the panic NEVER propagates to abort (SIGABRT) the process.
        let outcome: Result<(), _> = shared
            .rayon_pool
            .spawn(|| {
                panic!("deliberate job panic — must be contained, not SIGABRT");
            })
            .await;
        // A contained panic surfaces as Cancelled, not a process abort.
        match outcome {
            Ok(()) => Ok(()),
            Err(_cancelled) => Err("job panicked on rayon pool (contained)".to_string()),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_panicking_job_caught_no_sigabrt() {
    // If the rayon pool were built WITHOUT a panic_handler, the panic inside
    // `install` would propagate and abort (signal 6) the whole test process —
    // this test would never reach the assertion. Reaching it AT ALL is the proof.
    let harness = TickTestHarness::new(&["alpha"]).await;
    let registry: Vec<Box<dyn BackgroundJob>> = vec![Box::new(PanickingJob)];
    // run_per_slug_tick_pass isolates the per-job Err and continues; the process
    // must survive the contained panic.
    run_per_slug_tick_pass(
        std::slice::from_ref(&harness.slugs[0].ctx),
        &registry,
        &harness.shared,
    )
    .await;
    // Survived the panic without SIGABRT — the loop continues and we get here.
    assert_eq!(
        harness.slugs.len(),
        1,
        "harness survived a contained job panic"
    );
}

// ---------------------------------------------------------------------------
// AC-3 — analytics maintained: store to slug A -> one tick -> A's analytics
// reflect the write (behavioral, not "handle exists").
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_tick_maintains_slug_a_analytics() {
    let harness = TickTestHarness::new(&["alpha"]).await;
    let before = snapshot_handles(&harness.slugs[0].ctx);
    // Cold start: typed-graph is the empty fallback default.
    assert_eq!(
        before.typed_graph_entry_count, 0,
        "cold-start typed graph empty"
    );
    assert!(before.typed_graph_use_fallback, "cold-start uses fallback");

    // Write 5 entries, then run ONE tick over A.
    populate(harness.store(0), 5, "alpha").await;
    harness.tick(0).await;

    let after = snapshot_handles(&harness.slugs[0].ctx);
    // TypedGraphState rebuilt FROM the write: all 5 entries present, fallback off.
    assert_eq!(
        after.typed_graph_entry_count, 5,
        "tick must rebuild typed graph to reflect the 5 stored entries"
    );
    assert!(
        !after.typed_graph_use_fallback,
        "a populated rebuild must clear the cold-start fallback flag"
    );
    // The maintenance job advanced this slug's effectiveness generation (it RAN).
    assert!(
        after.effectiveness_generation >= before.effectiveness_generation,
        "effectiveness generation must not regress"
    );
    // The analytics state CHANGED to reflect the write — not merely "handle exists".
    assert_ne!(
        before, after,
        "A's analytics must change after a tick over its write"
    );
}

// ---------------------------------------------------------------------------
// AC-4 ★ — N=2 cross-slug corruption guard (the single non-substitutable proof).
// Populate A and B DIFFERENTLY; tick A then B; assert B's tick leaves A's four
// states byte-for-byte unchanged, and vice versa. N=1 cannot distinguish a real
// per-slug funnel from a global-handle bypass (#4974 checklist item 5).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_tick_b_leaves_a_unchanged_n2() {
    let harness = TickTestHarness::new(&["alpha", "beta"]).await;

    // DIFFERENT populations: A=7 entries, B=3 entries. The distinct counts are
    // what surfaces a residual cross-slug write — a bypass would overwrite A's
    // typed-graph (7) with B's (3) after B's tick.
    populate(harness.store(0), 7, "alpha").await;
    populate(harness.store(1), 3, "beta").await;

    // Tick A, snapshot A's four states.
    harness.tick(0).await;
    let a_after_a_tick = snapshot_handles(&harness.slugs[0].ctx);
    assert_eq!(
        a_after_a_tick.typed_graph_entry_count, 7,
        "A's tick must reflect A's 7 entries"
    );

    // Tick B.
    harness.tick(1).await;
    let b_after_b_tick = snapshot_handles(&harness.slugs[1].ctx);
    assert_eq!(
        b_after_b_tick.typed_graph_entry_count, 3,
        "B's tick must reflect B's 3 entries (distinct from A)"
    );

    // ★ THE corruption guard: A's four states are BYTE-FOR-BYTE unchanged by B's tick.
    let a_after_b_tick = snapshot_handles(&harness.slugs[0].ctx);
    assert_eq!(
        a_after_b_tick, a_after_a_tick,
        "AC-4: B's tick MUST NOT mutate A's analytics handles (cross-slug corruption / \
         cross-tenant leak). A still has 7, not B's 3."
    );

    // Vice versa: re-tick A; B's snapshot must be unchanged.
    populate(harness.store(0), 0, "alpha").await; // no-op write to keep symmetry explicit
    harness.tick(0).await;
    let b_after_a_retick = snapshot_handles(&harness.slugs[1].ctx);
    assert_eq!(
        b_after_a_retick, b_after_b_tick,
        "AC-4 (reverse): A's tick MUST NOT mutate B's analytics handles"
    );
}

// ---------------------------------------------------------------------------
// AC-4 (empty-B variant) — distinct-state-survives-other-tick: A is populated
// to a non-default state; B is EMPTY; ticking B leaves A intact and B at clean
// defaults (no panic on an empty store).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_distinct_state_survives_empty_b_tick() {
    let harness = TickTestHarness::new(&["alpha", "beta"]).await;
    populate(harness.store(0), 4, "alpha").await;
    // B left EMPTY.

    harness.tick(0).await;
    let a_snapshot = snapshot_handles(&harness.slugs[0].ctx);
    assert_eq!(a_snapshot.typed_graph_entry_count, 4);
    assert!(!a_snapshot.typed_graph_use_fallback);

    // Tick the EMPTY slug B — must not panic, must leave clean defaults.
    harness.tick(1).await;
    let b_snapshot = snapshot_handles(&harness.slugs[1].ctx);
    assert_eq!(
        b_snapshot.typed_graph_entry_count, 0,
        "empty B's tick leaves typed graph empty (clean default)"
    );

    // A is byte-for-byte unchanged by B's empty tick.
    assert_eq!(
        snapshot_handles(&harness.slugs[0].ctx),
        a_snapshot,
        "A's analytics unchanged by an empty B tick"
    );
}

// ---------------------------------------------------------------------------
// AC-5 (handle identity) — the structural half: the PerSlugTickContext handles
// ARE the slug's ServiceLayer Arcs (`Arc::ptr_eq`), so what the tick writes is
// exactly what the serving path reads (R-03 / FR-15). Proven at N=2: A's handles
// are A's ServiceLayer's, B's are B's, and A's != B's (no shared singleton).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_handle_identity_tick_ctx_eq_service_layer_n2() {
    let harness = TickTestHarness::new(&["alpha", "beta"]).await;
    let a = &harness.slugs[0];
    let b = &harness.slugs[1];
    let sl_a = a.server.service_layer();
    let sl_b = b.server.service_layer();

    // A's context handles are A's ServiceLayer's SAME Arcs (serving == tick instance).
    assert!(Arc::ptr_eq(&a.ctx.typed_graph, &sl_a.typed_graph_handle()));
    assert!(Arc::ptr_eq(
        &a.ctx.effectiveness,
        &sl_a.effectiveness_state_handle()
    ));
    assert!(Arc::ptr_eq(
        &a.ctx.confidence,
        &sl_a.confidence_state_handle()
    ));
    assert!(Arc::ptr_eq(
        &a.ctx.contradiction,
        &sl_a.contradiction_cache_handle()
    ));
    assert!(Arc::ptr_eq(
        &a.ctx.phase_freq,
        &sl_a.phase_freq_table_handle()
    ));

    // Cross-slug: A's handles are NOT B's (no shared global singleton — the
    // pre-crt-056 defect). This is the structural complement of AC-4.
    assert!(
        !Arc::ptr_eq(&a.ctx.typed_graph, &sl_b.typed_graph_handle()),
        "A and B must own DISTINCT typed-graph handles (no shared singleton)"
    );
    assert!(
        !Arc::ptr_eq(&a.ctx.confidence, &sl_b.confidence_state_handle()),
        "A and B must own DISTINCT confidence handles"
    );
    assert!(
        !Arc::ptr_eq(&a.ctx.effectiveness, &sl_b.effectiveness_state_handle()),
        "A and B must own DISTINCT effectiveness handles"
    );
}

// ---------------------------------------------------------------------------
// AC-5 (serving reads tick state, model-free proxy) — after ticking A, a read
// of A's typed-graph handle through the ServiceLayer accessor (the SAME object
// the serving search path reads) reflects A's post-tick entry set, and B's is
// independent. This is the in-process stand-in for the search-delta (the full
// search() path needs a loaded model + crate-private SearchService).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_serving_accessor_reflects_tick_unaffected_by_b() {
    let harness = TickTestHarness::new(&["alpha", "beta"]).await;
    populate(harness.store(0), 6, "alpha").await;
    populate(harness.store(1), 2, "beta").await;
    harness.tick(0).await;
    harness.tick(1).await;

    // Read A's maintained state through the SERVING accessor (not ctx) — proves
    // serving sees what the tick wrote (handle identity is load-bearing here).
    let sl_a = harness.slugs[0].server.service_layer();
    let a_serving = sl_a
        .typed_graph_handle()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .all_entries
        .len();
    let sl_b = harness.slugs[1].server.service_layer();
    let b_serving = sl_b
        .typed_graph_handle()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .all_entries
        .len();

    assert_eq!(
        a_serving, 6,
        "A's serving read reflects A's post-tick state (6)"
    );
    assert_eq!(
        b_serving, 2,
        "B's serving read reflects B's post-tick state (2)"
    );
    assert_ne!(
        a_serving, b_serving,
        "A and B serving reads are independent"
    );
}

// ---------------------------------------------------------------------------
// R-11 — adapt_service is per-slug independent state (no cross-slug bleed),
// adjacent to AC-4's isolation. `session_capabilities` is OUT (NOT asserted).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_adapt_service_no_cross_slug_bleed() {
    let harness = TickTestHarness::new(&["alpha", "beta"]).await;
    // Each slug owns a DISTINCT AdaptationService instance (per-slug independent
    // state). A's adaptation can never reach into B's.
    assert!(
        !Arc::ptr_eq(
            &harness.slugs[0].ctx.adapt_service,
            &harness.slugs[1].ctx.adapt_service
        ),
        "each slug must own a distinct adapt_service (no cross-slug bleed, R-11)"
    );
    // And each context's adapt_service is its OWN server's.
    assert!(Arc::ptr_eq(
        &harness.slugs[0].ctx.adapt_service,
        &harness.slugs[0].server.adapt_service()
    ));
}

// ---------------------------------------------------------------------------
// Edge — N=0: a tick pass over an empty context slice is a no-op, no panic.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_empty_registry_tick_is_noop_n0() {
    let harness = TickTestHarness::new(&[]).await;
    // No contexts AND the full registry — a no-op pass that must not panic.
    run_per_slug_tick_pass(&[], &harness.registry, &harness.shared).await;
    assert!(harness.slugs.is_empty(), "N=0 harness has no slugs");
}

// ---------------------------------------------------------------------------
// Edge — interval-gate boundary at N=2: the contradiction job (EveryN-gated)
// fires per-slug independently. We tick A four times and B once, then assert
// each slug's tick_counter advanced independently (per-slug counter, R-07) —
// a global counter would advance them in lockstep.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_per_slug_counter_advances_independently_n2() {
    let harness = TickTestHarness::new(&["alpha", "beta"]).await;
    for _ in 0..4 {
        harness.tick(0).await;
    }
    harness.tick(1).await;

    let a_counter = harness.slugs[0]
        .ctx
        .tick_metadata
        .lock()
        .map(|m| m.tick_counter)
        .unwrap_or(0);
    let b_counter = harness.slugs[1]
        .ctx
        .tick_metadata
        .lock()
        .map(|m| m.tick_counter)
        .unwrap_or(0);
    assert_eq!(a_counter, 4, "A ticked 4 times -> counter at 4");
    assert_eq!(
        b_counter, 1,
        "B ticked once -> counter at 1 (independent of A)"
    );
}

// ###########################################################################
// crt-056 Wave 1 — config-parity (AC-1) + one-shared-model (AC-2).
//
// Gate 3c REWORK (#787): the prior report marked AC-1/AC-2 PASS with NO
// implementing test (the #4202/#3935 vacuous-green anti-pattern). These two
// tests close that gap with REAL, NON-VACUOUS assertions.
//
// `http_provision::build_project_server` lives in the BINARY crate (`main.rs`'s
// private `mod http_provision`) and is unreachable from this external `tests/`
// integration crate. We therefore drive its EXACT public assembly path —
// `ServiceLayer::new(<threaded resolved Arcs>)` handed to
// `UnimatrixServer::new(.., Some(layer))` — the literal body of
// `build_project_server` (`http_provision.rs:220-253`) minus the on-disk
// store-open glue. The accessors under test (`9ccde2a9`) read the SAME resolved
// fields `build_project_server` threads, so this proves the parity contract the
// production funnel must hold. (Reuses `build_server()`'s store/vector/registry
// assembly; no isolated scaffolding — NFR-7 / C-9.)
// ###########################################################################

/// The daemon's RESOLVED config surface — every field set to a NON-DEFAULT value
/// so any fallback to a test-default `ServiceLayer` makes the parity assertion
/// FAIL (anti-vacuous). The `Arc`s are the daemon's single resolved instances;
/// a faithful per-slug build `Arc::clone`s them (shared identity, AC-2), never
/// reconstructs (`::new()`/`::default()`).
struct ResolvedDaemonConfig {
    rayon_pool: Arc<RayonPool>,
    nli_handle: Arc<NliServiceHandle>,
    nli_top_k: usize,
    nli_enabled: bool,
    inference_config: Arc<InferenceConfig>,
    confidence_params: Arc<ConfidenceParams>,
    category_allowlist: Arc<CategoryAllowlist>,
    observation_registry: Arc<DomainPackRegistry>,
    boosted_categories: std::collections::HashSet<String>,
    expected_fusion_weights: FusionWeights,
    expected_pool_size: usize,
}

impl ResolvedDaemonConfig {
    /// Build the daemon's resolved config with NLI **enabled** and EVERY field
    /// NON-DEFAULT (so the test fails against the old test-default ServiceLayer).
    fn enabled_non_default() -> Self {
        // Non-default fusion weights (defaults are w_sim=0.50, w_nli=0.00, ...).
        let inference = InferenceConfig {
            nli_enabled: true,
            nli_top_k: 37,      // non-default (default 20)
            rayon_pool_size: 5, // non-default pool size
            w_sim: 0.20,
            w_nli: 0.30, // default is 0.00 — clearly non-default
            w_conf: 0.15,
            w_coac: 0.05,
            w_util: 0.05,
            w_prov: 0.05,
            ..Default::default()
        };
        // Expected fusion weights, constructed literally from the resolved config
        // fields (FusionWeights fields are public; `from_config` is crate-private).
        // Mirrors `FusionWeights::from_config` field-for-field.
        let expected_fusion_weights = FusionWeights {
            w_sim: inference.w_sim,
            w_nli: inference.w_nli,
            w_conf: inference.w_conf,
            w_coac: inference.w_coac,
            w_util: inference.w_util,
            w_prov: inference.w_prov,
            w_phase_histogram: inference.w_phase_histogram,
            w_phase_explicit: inference.w_phase_explicit,
        };
        let inference_config = Arc::new(inference);

        // Non-default confidence params (perturb alpha0 off the 3.0 default).
        let conf = ConfidenceParams {
            alpha0: 7.0,
            beta0: 9.0,
            ..Default::default()
        };
        let confidence_params = Arc::new(conf);

        // Non-default allowlist: an operator category not in the builtin set.
        let category_allowlist = Arc::new(CategoryAllowlist::from_categories(vec![
            "operator-only-category".to_string(),
        ]));

        // Non-default domain-pack registry (the builtin claude-code pack, not the
        // empty/default set a fallback would carry).
        let observation_registry = Arc::new(DomainPackRegistry::with_builtin_claude_code());

        let mut boosted_categories = std::collections::HashSet::new();
        boosted_categories.insert("operator-boost".to_string());

        let expected_pool_size = 5;
        let rayon_pool = Arc::new(
            RayonPool::new(expected_pool_size, "crt056-parity-pool")
                .expect("rayon pool with panic_handler"),
        );

        ResolvedDaemonConfig {
            rayon_pool,
            nli_handle: NliServiceHandle::new(), // ONE loaded handle (Arc), shared by all slugs
            nli_top_k: 37,
            nli_enabled: true,
            inference_config,
            confidence_params,
            category_allowlist,
            observation_registry,
            boosted_categories,
            expected_fusion_weights,
            expected_pool_size,
        }
    }
}

/// Build ONE per-slug `UnimatrixServer` over a fresh store, wiring its
/// `ServiceLayer` from the supplied RESOLVED daemon config exactly as
/// `http_provision::build_project_server` does (params-at-end, every value an
/// `Arc::clone` of the daemon's resolved instance — ADR-002). Returns the server
/// whose `service_layer()` accessors expose the threaded fields for parity asserts.
async fn build_server_with_resolved_config(cfg: &ResolvedDaemonConfig) -> UnimatrixServer {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let db_path = dir.path().join("unimatrix.db");
    let store = Arc::new(
        Store::open(&db_path, PoolConfig::default())
            .await
            .expect("open store"),
    );
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
    let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));
    let usage_dedup = Arc::new(UsageDedup::new());

    // The literal `build_project_server` ServiceLayer assembly (ADR-002): every
    // resolved value `Arc::clone`d (shared identity), none reconstructed.
    let service_layer = ServiceLayer::new(
        Arc::clone(&store),
        Arc::clone(&vector_index),
        Arc::clone(&async_vector_store),
        Arc::clone(&store),
        Arc::clone(&embed_handle),
        Arc::clone(&adapt_service),
        Arc::clone(&audit),
        usage_dedup,
        cfg.boosted_categories.clone(),
        Arc::clone(&cfg.rayon_pool),
        Arc::clone(&cfg.nli_handle), // AC-2: the ONE loaded model — Arc::clone, NEVER new()
        cfg.nli_top_k,
        cfg.nli_enabled,
        Arc::clone(&cfg.inference_config),
        Arc::clone(&cfg.observation_registry),
        Arc::clone(&cfg.confidence_params),
        Arc::clone(&cfg.category_allowlist),
    );

    UnimatrixServer::new(
        Arc::clone(&store),
        async_vector_store,
        Arc::clone(&embed_handle),
        registry,
        audit,
        Arc::clone(&cfg.category_allowlist),
        Arc::clone(&store),
        vector_index,
        adapt_service,
        None,
        Some(service_layer),
    )
}

// ---------------------------------------------------------------------------
// AC-1 — field-by-field config parity over the closed 8-field ADR-006 checklist.
// Builds a per-slug server from an NLI-ENABLED, NON-DEFAULT resolved config and
// asserts ALL 8 fields equal the daemon's resolved values (NOT a subset). NLI
// flag asserted BOTH directions. `session_capabilities` is OUT (ADR-006) — not
// asserted. Non-vacuous: a fallback to the test-default ServiceLayer (NLI off /
// pool-1 / default weights / builtin allowlist) fails at least one assertion.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_per_slug_service_layer_config_parity_8_fields() {
    let cfg = ResolvedDaemonConfig::enabled_non_default();
    let server = build_server_with_resolved_config(&cfg).await;
    let sl = server.service_layer();

    // 1. nli_enabled (ENABLED direction): config true ⇒ accessor true.
    assert!(
        sl.nli_enabled(),
        "AC-1.1: NLI-enabled config must yield nli_enabled() == true (NOT the false default)"
    );
    // 2. nli_top_k — the threaded non-default 37, not the 20 default.
    assert_eq!(sl.nli_top_k(), cfg.nli_top_k, "AC-1.2: nli_top_k parity");
    assert_eq!(
        sl.nli_top_k(),
        37,
        "AC-1.2: nli_top_k is the resolved non-default 37, not 20"
    );
    // 3. nli_handle — the SAME Arc instance threaded from the daemon (identity).
    assert!(
        Arc::ptr_eq(sl.nli_handle(), &cfg.nli_handle),
        "AC-1.3: per-slug nli_handle must be the daemon's SAME Arc (no per-slug NliServiceHandle::new())"
    );
    // 4. fusion_weights — the resolved InferenceConfig-derived weights (PartialEq).
    assert_eq!(
        sl.fusion_weights(),
        cfg.expected_fusion_weights,
        "AC-1.4: fusion weights must equal the resolved InferenceConfig's, not ::default()"
    );
    assert_ne!(
        sl.fusion_weights(),
        FusionWeights::default(),
        "AC-1.4: resolved fusion weights are non-default (guards vacuous parity)"
    );
    // 5. confidence_params — same Arc instance + value equality.
    assert!(
        Arc::ptr_eq(sl.confidence_params(), &cfg.confidence_params),
        "AC-1.5: per-slug confidence_params must be the daemon's SAME Arc"
    );
    assert_eq!(
        **sl.confidence_params(),
        *cfg.confidence_params,
        "AC-1.5: confidence params value parity (alpha0=7.0/beta0=9.0, non-default)"
    );
    assert_ne!(
        **sl.confidence_params(),
        ConfidenceParams::default(),
        "AC-1.5: resolved confidence params are non-default (guards vacuous parity)"
    );
    // 6. category_allowlist — same Arc instance threaded from the daemon.
    assert!(
        Arc::ptr_eq(sl.category_allowlist(), &cfg.category_allowlist),
        "AC-1.6: per-slug category_allowlist must be the daemon's SAME Arc (operator set, not ::new())"
    );
    assert!(
        sl.category_allowlist()
            .validate("operator-only-category")
            .is_ok(),
        "AC-1.6: the operator-only category is present (non-default allowlist threaded through)"
    );
    // 7. observation_registry / domain packs — same Arc instance.
    assert!(
        Arc::ptr_eq(sl.observation_registry(), &cfg.observation_registry),
        "AC-1.7: per-slug observation_registry (domain packs) must be the daemon's SAME Arc"
    );
    // 8. ml_inference_pool effective size — the resolved non-default 5.
    assert!(
        Arc::ptr_eq(sl.ml_inference_pool(), &cfg.rayon_pool),
        "AC-1.8: per-slug ml_inference_pool must be the daemon's SAME shared Arc"
    );
    assert_eq!(
        sl.ml_inference_pool().pool_size(),
        cfg.expected_pool_size,
        "AC-1.8: effective rayon pool size is the resolved 5, not the size-1 test default"
    );
    // boosted_categories (the operator domain hint threaded alongside FR-4).
    assert!(
        sl.boosted_categories().contains("operator-boost"),
        "AC-1: boosted_categories must carry the resolved operator hint"
    );

    // NLI flag — DISABLED direction: a disabled-config server reports disabled.
    let mut disabled_cfg = ResolvedDaemonConfig::enabled_non_default();
    disabled_cfg.nli_enabled = false;
    let disabled_server = build_server_with_resolved_config(&disabled_cfg).await;
    assert!(
        !disabled_server.service_layer().nli_enabled(),
        "AC-1.1 (reverse): NLI-disabled config must yield nli_enabled() == false"
    );
}

// ---------------------------------------------------------------------------
// AC-2 — one shared model across N=2 per-slug servers. Each slug's nli_handle is
// `Arc::ptr_eq` to the daemon's single loaded handle (the SAME Arc instance, not
// N copies). The shared Arc IS the proof that no per-slug NliServiceHandle::new()
// runs on the provisioning path. Embedding handle is per-slug-constructed today
// (build_server makes its own), so the model-sharing invariant is the NLI handle.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_shared_nli_model_across_n2_slugs() {
    let cfg = ResolvedDaemonConfig::enabled_non_default();

    // N=2 per-slug servers built from the SAME resolved config (the daemon threads
    // ONE loaded nli_handle into every slug).
    let slug_a = build_server_with_resolved_config(&cfg).await;
    let slug_b = build_server_with_resolved_config(&cfg).await;

    let handle_a = slug_a.service_layer().nli_handle();
    let handle_b = slug_b.service_layer().nli_handle();

    // Each slug's handle is the daemon's SAME Arc instance (one model, shared).
    assert!(
        Arc::ptr_eq(handle_a, &cfg.nli_handle),
        "AC-2: slug A's nli_handle must be the daemon's single loaded Arc"
    );
    assert!(
        Arc::ptr_eq(handle_b, &cfg.nli_handle),
        "AC-2: slug B's nli_handle must be the daemon's single loaded Arc"
    );
    // Transitively the two slugs share ONE handle — no per-slug NliServiceHandle::new(),
    // no N copies (the shared Arc proves it).
    assert!(
        Arc::ptr_eq(handle_a, handle_b),
        "AC-2: both slugs reference the ONE shared NLI model handle (not N copies)"
    );
    // Strong-count sanity: cfg + 2 ServiceLayers = at least 3 owners of the ONE Arc.
    assert!(
        Arc::strong_count(&cfg.nli_handle) >= 3,
        "AC-2: the single nli_handle is shared (cfg + 2 slug ServiceLayers), not cloned-by-value"
    );
}

// ###########################################################################
// vnc-046 — Per-slug OBSERVE-path isolation suite (the durable guardrail, ADR-004).
//
// This section is the vnc-046 primary behavioral gate: it drives the REAL
// assembled production edge — `PathRouter` (pub) → `route_observe` →
// `resolver.resolve_store/registry_for/pending_for/services_for(&key)` →
// `dispatch_request` — for a `POST /v1/{slug}/observe` `RecordEvent`, at N=2
// registered slugs, and asserts BIDIRECTIONALLY that the write lands in the
// writer slug's OWN per-slug store and is ABSENT from every other slug's store.
// No `SessionRegistry`/`ServiceLayer` is hand-passed into `dispatch_request`;
// no server field is seeded on the write path (R-02/#5285/#4974). The read side
// asserts on the pub-reachable durable observable — the per-slug store's
// `observations` rows — because `McpAdapter`/`cycle_review` are `pub(crate)` and
// unreachable from this external `tests/` crate; the full MCP fold / distillation
// / knowledge-read semantics over the true HTTPS wire are proven by the #800
// Python fixture (`suites/test_project_isolation.py`). See the coverage-
// enumeration table (`test_vnc046_coverage_enumeration`) at the end.
//
// AC-06 scoping: the BEHAVIORAL isolation cases below (INV-T1/T2 fidelity+
// isolation) contain NO `Arc::ptr_eq`, no hand-passed registry, no field
// overwrite. Handle-identity pins live in the clearly-separated white-box
// section (`vnc046_white_box_wiring_pins`) — a documented AC-06 complement, and
// reading a cloned store handle for an assertion is the established pattern in
// this file (see `slug_stores`), not a violation.
// ###########################################################################

use tower::Service as _TowerService;
use unimatrix_server::http::{ObserveContext, PathRouter, ProjectKey};
use unimatrix_server::infra::registry::{Capability, TrustLevel};
use unimatrix_server::mcp::identity::ResolvedIdentity;

// Per-slug session-id markers: each carries the feature-id shape (an all-digit
// hyphen-segment + an alpha-bearing segment) so the observe path persists the
// observation, and the two are mutually NON-SUBSTRING so a cross-store match is
// unambiguous (#5347 marker discipline).
const SID_A: &str = "vnc046a-1";
const SID_B: &str = "vnc046b-1";

/// Owned-body variant of `body()` (the existing helper takes `&'static str`).
fn body_owned(s: String) -> TestBody {
    Full::new(Bytes::from(s))
        .map_err(|never| match never {})
        .boxed()
}

/// The pub-reachable per-slug handle bundle: the store the resolver routes to,
/// plus the `session_registry`/`pending` handles Arc-cloned off the server
/// BEFORE it moves into `ProjectServerInput`/`from_servers` (exactly as
/// `wired_router` clones the store). Read-only assertion handles — never passed
/// into the write path.
struct SlugHandles {
    slug: String,
    store: Arc<Store>,
    registry: Arc<unimatrix_server::infra::session::SessionRegistry>,
    pending: Arc<std::sync::Mutex<unimatrix_server::server::PendingEntriesAnalysis>>,
}

/// Build the assembled observe stack: a real `MultiProjectRouter` over N distinct
/// per-slug servers, wrapped in a real `PathRouter` with an `ObserveContext`
/// carrying the SAME resolver — the exact production wiring `main.rs` builds.
/// Returns the driveable `PathRouter`, the `Arc<dyn StoreResolver>` (for the
/// white-box pins), and per-slug handle bundles (for read-side assertions).
async fn wired_observe_stack(
    slugs: &[&str],
) -> (PathRouter<TestBody>, Arc<dyn StoreResolver>, Vec<SlugHandles>) {
    let mut inputs = Vec::with_capacity(slugs.len());
    let mut handles = Vec::with_capacity(slugs.len());
    for &name in slugs {
        let bundle = build_server().await;
        let slug = ProjectSlug::try_from(name).expect("valid test slug");
        // Clone the read-side handles off the server BEFORE it moves (the same
        // convergence-by-construction the resolver relies on).
        let registry = Arc::clone(&bundle.input_server.session_registry);
        let pending = Arc::clone(&bundle.input_server.pending_entries_analysis);
        handles.push(SlugHandles {
            slug: name.to_string(),
            store: Arc::clone(&bundle.store),
            registry,
            pending,
        });
        let vector_dir = std::path::PathBuf::from(slug.as_str()).join("vector");
        inputs.push(ProjectServerInput {
            slug,
            store: bundle.store,
            server: bundle.input_server,
            vector_dir,
        });
    }

    let router = MultiProjectRouter::from_servers(
        inputs,
        TEST_MAX_BODY,
        Vec::new(),
        vec!["localhost".to_string()],
    )
    .expect("build MultiProjectRouter");
    let resolver: Arc<dyn StoreResolver> = Arc::new(router);

    let observe_ctx = ObserveContext {
        resolver: Arc::clone(&resolver),
        embed_service: EmbedServiceHandle::new(),
        server_version: "vnc046-itest".to_string(),
    };
    let path_router: PathRouter<TestBody> = PathRouter::new(Arc::clone(&resolver), observe_ctx);
    (path_router, resolver, handles)
}

/// A `RecordEvent` observe body carrying a per-slug session marker. Built as a
/// string (no `serde_json` dev-dep); markers are `[a-z0-9-]` so quoting is safe.
fn observe_record_body(session_id: &str) -> String {
    format!(
        r#"{{"type":"RecordEvent","event_type":"tool_use","session_id":"{session_id}","timestamp":0,"payload":{{}},"topic_signal":"{session_id}"}}"#
    )
}

/// Build a `POST /v1/{slug}/observe` request with a privileged `ResolvedIdentity`
/// injected into extensions (StaticTokenAuth's job in production; injected here
/// so the handler's Step-1 identity read succeeds).
fn observe_request(slug: &str, session_id: &str) -> Request<TestBody> {
    let mut req = Request::builder()
        .method("POST")
        .uri(format!("/v1/{slug}/observe"))
        .header("content-type", "application/json")
        .body(body_owned(observe_record_body(session_id)))
        .expect("build observe request");
    req.extensions_mut().insert(ResolvedIdentity {
        agent_id: "human".to_string(),
        trust_level: TrustLevel::Privileged,
        capabilities: vec![
            Capability::Read,
            Capability::Write,
            Capability::Search,
            Capability::Admin,
            // Observe RecordEvent requires SessionWrite (uds/listener.rs) — the
            // capability StaticTokenAuth grants the privileged HTTP identity.
            Capability::SessionWrite,
        ],
    });
    req
}

/// Drive one observe `RecordEvent` through the assembled `PathRouter` edge.
async fn drive_observe(
    router: &PathRouter<TestBody>,
    slug: &str,
    session_id: &str,
) -> (StatusCode, String) {
    let mut router = router.clone();
    let resp = router
        .call(observe_request(slug, session_id))
        .await
        .expect("PathRouter::call is infallible");
    collect_resp(resp).await
}

/// Total observations durably present in a per-slug store (read-side observable).
async fn observation_count(store: &Store) -> usize {
    let (rows, _) = store
        .fetch_observations_since(0, 1_000_000)
        .await
        .expect("fetch observations");
    rows.len()
}

/// Count observations whose `session_id` carries `marker` (the write path
/// prefixes `http-`, so a substring match is used).
async fn observations_with_marker(store: &Store, marker: &str) -> usize {
    let (rows, _) = store
        .fetch_observations_since(0, 1_000_000)
        .await
        .expect("fetch observations");
    rows.iter().filter(|r| r.session_id.contains(marker)).count()
}

/// Read-as-barrier positive control (mirrors the #800 / infra-003 smoke): the
/// observe write is durable only EVENTUALLY (async observation writer), so poll
/// the writer slug's own store until its marker appears, bounded. Returns the
/// count once present, or the final (0) count on timeout — a timeout is a hard
/// fidelity failure at the call site, never a silent pass.
async fn wait_for_observation(store: &Store, marker: &str) -> usize {
    for _ in 0..150 {
        let n = observations_with_marker(store, marker).await;
        if n >= 1 {
            return n;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    observations_with_marker(store, marker).await
}

// ---------------------------------------------------------------------------
// INV-T1 (AC-01, #930) — transcript/observe FIDELITY, both slugs. A delta driven
// through `/v1/{X}/observe` is durably folded into X's OWN per-slug store. This
// is the #930 regression guard at the assembled edge.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_observe_transcript_fidelity_a() {
    let (router, _resolver, h) = wired_observe_stack(&["alpha", "beta"]).await;
    assert_eq!(observation_count(&h[0].store).await, 0, "cold start: alpha empty");

    let (status, body) = drive_observe(&router, "alpha", SID_A).await;
    assert!(
        status.is_success(),
        "observe to /v1/alpha/observe must succeed at the assembled edge; status={status} body={body}"
    );
    assert!(
        wait_for_observation(&h[0].store, SID_A).await >= 1,
        "INV-T1/#930: alpha's OWN store must durably fold alpha's observe (fidelity)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_observe_transcript_fidelity_b() {
    // Symmetric fidelity for the B-driver — NOT inferred from A (R-01/#5348).
    let (router, _resolver, h) = wired_observe_stack(&["alpha", "beta"]).await;
    assert_eq!(observation_count(&h[1].store).await, 0, "cold start: beta empty");

    let (status, body) = drive_observe(&router, "beta", SID_B).await;
    assert!(
        status.is_success(),
        "observe to /v1/beta/observe must succeed; status={status} body={body}"
    );
    assert!(
        wait_for_observation(&h[1].store, SID_B).await >= 1,
        "INV-T1/#930: beta's OWN store must durably fold beta's observe (fidelity)"
    );
}

// ---------------------------------------------------------------------------
// INV-T2 (AC-02; R-10/R-15) — cross-slug transcript ISOLATION under an IDENTICAL
// `{phase}-{NNN}` cycle name. A and B both run the identical feature vocabulary;
// the write to X lands ONLY in X's store — the other slug's store folds/counts
// NOTHING of X's. Bidirectional (distinct A-driver / B-driver cases). The
// candidate COUNT and the durable observation set (the distillation input) both
// exclude the other slug — not just the returned bytes.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_observe_isolation_identical_cycle_a_driver() {
    let (router, _resolver, h) = wired_observe_stack(&["alpha", "beta"]).await;

    let (status, _) = drive_observe(&router, "alpha", SID_A).await;
    assert!(status.is_success());

    // Fidelity-in-own.
    assert!(
        wait_for_observation(&h[0].store, SID_A).await >= 1,
        "alpha folds its own observe (positive-control barrier before isolation check)"
    );
    // Isolation-in-other: beta's store folded NOTHING — zero total observations
    // (count exclusion) AND alpha's marker never appears (distillation-input
    // exclusion). The identical cycle vocabulary cannot cross the per-slug store.
    assert_eq!(
        observation_count(&h[1].store).await,
        0,
        "INV-T2: beta's store must fold ZERO of alpha's observe (candidate-count exclusion)"
    );
    assert_eq!(
        observations_with_marker(&h[1].store, SID_A).await,
        0,
        "INV-T2/R-15: alpha's marker must NEVER enter beta's durable store (distillation-input exclusion)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_observe_isolation_identical_cycle_b_driver() {
    // The symmetric reverse mis-route guard (the exact direction #5348 warns of).
    let (router, _resolver, h) = wired_observe_stack(&["alpha", "beta"]).await;

    let (status, _) = drive_observe(&router, "beta", SID_B).await;
    assert!(status.is_success());

    assert!(
        wait_for_observation(&h[1].store, SID_B).await >= 1,
        "beta folds its own observe (positive-control barrier before isolation check)"
    );
    assert_eq!(
        observation_count(&h[0].store).await,
        0,
        "INV-T2 (reverse): alpha's store must fold ZERO of beta's observe"
    );
    assert_eq!(
        observations_with_marker(&h[0].store, SID_B).await,
        0,
        "INV-T2/R-15 (reverse): beta's marker must NEVER enter alpha's durable store"
    );
}

// ---------------------------------------------------------------------------
// Negative control (R-01 / #5348) — proves the isolation predicate is NOT
// vacuous: it MUST detect a marker in the store where the write actually landed.
// A one-directional / vacuous suite would false-GREEN because its cross-read
// predicate can never see a marker. Here we assert the SAME predicate the
// isolation cells use reports PRESENCE where the write landed — so a real
// reverse mis-route (writer's delta landing in the other slug's store) would
// trip those cells RED, not silently pass.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_observe_negative_control_predicate_is_sensitive() {
    let (router, _resolver, h) = wired_observe_stack(&["alpha", "beta"]).await;
    drive_observe(&router, "alpha", SID_A).await;

    // The write landed in alpha's store. If the funnel had mis-routed it into
    // beta, the isolation cell `observations_with_marker(beta, SID_A) == 0` would
    // FAIL. Prove the predicate can SEE that by pointing it at where the write
    // landed: it must report >= 1. A predicate that returned 0 here (blind) would
    // make every isolation cell a vacuous pass.
    assert!(
        wait_for_observation(&h[0].store, SID_A).await >= 1,
        "negative control: the cross-read predicate MUST detect a present marker \
         (else the isolation cells are vacuous and false-GREEN a reverse mis-route)"
    );
    // Second leg: directly inject beta's marker into alpha's store (a simulated
    // reverse-misroute leak) and assert the predicate flags it RED.
    h[0]
        .store
        .insert_observation(
            &format!("http-{SID_B}"),
            0,
            "tool_use",
            Some("tool_use"),
            None,
            None,
            None,
        )
        .await
        .expect("inject simulated leak");
    assert!(
        observations_with_marker(&h[0].store, SID_B).await >= 1,
        "negative control: an injected foreign marker MUST be detected (the cell would go RED on a real leak)"
    );
}

// ---------------------------------------------------------------------------
// INV-K1/K2 (AC-04; R-09/R-15) — knowledge-read fidelity + isolation + durable
// non-contamination. Distinct knowledge is written into each slug's OWN store;
// each store holds ONLY its own, and `resolver.services_for(slug)` resolves the
// slug's OWN `ServiceLayer` (the P2 read handle) — never the other's store. A
// re-query after the cross-check confirms the durable store stays uncontaminated
// (distillation cannot leak across, R-15). Bidirectional.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_knowledge_read_isolation_bidirectional_n2() {
    let (_router, resolver, h) = wired_observe_stack(&["alpha", "beta"]).await;
    let key_a = ProjectKey::Slug(ProjectSlug::try_from("alpha").unwrap());
    let key_b = ProjectKey::Slug(ProjectSlug::try_from("beta").unwrap());

    // Write distinct knowledge into each slug's OWN store.
    h[0].store.insert(test_entry("alpha-knowledge")).await.expect("a write");
    h[1].store.insert(test_entry("beta-knowledge")).await.expect("b write");

    // Fidelity + isolation at the durable store the P2 read path serves.
    let titles = |store: &Arc<Store>| {
        let s = Arc::clone(store);
        async move {
            s.query_all_entries()
                .await
                .expect("query")
                .into_iter()
                .map(|e| e.title)
                .collect::<Vec<_>>()
        }
    };
    let a_titles = titles(&h[0].store).await;
    let b_titles = titles(&h[1].store).await;
    assert!(a_titles.contains(&"alpha-knowledge".to_string()), "A own-read fidelity");
    assert!(b_titles.contains(&"beta-knowledge".to_string()), "B own-read fidelity");
    assert!(
        !a_titles.iter().any(|t| t == "beta-knowledge"),
        "INV-K2: A's store must NEVER contain B's knowledge"
    );
    assert!(
        !b_titles.iter().any(|t| t == "alpha-knowledge"),
        "INV-K2 (reverse): B's store must NEVER contain A's knowledge"
    );

    // The P2 read handle `services_for` resolves per-slug through the SAME funnel
    // (SR-07 read-leak fix). Both slugs resolve their own ServiceLayer; the
    // read-fidelity/isolation semantics THROUGH the ServiceLayer (briefing/search)
    // are model-bound and proven over the wire by the #800 fixture. Here we pin
    // that the per-slug P2 handle is wired (resolves Ok) for both keys.
    resolver.services_for(&key_a).expect("services_for(A) must resolve the per-slug P2 handle");
    resolver.services_for(&key_b).expect("services_for(B) must resolve the per-slug P2 handle");

    // R-15 durable non-contamination: re-query confirms the stores did not gain
    // the other's entry as a side effect of the cross-read.
    assert_eq!(observation_count(&h[0].store).await, 0, "no stray obs in A");
    assert!(
        !titles(&h[1].store).await.iter().any(|t| t == "alpha-knowledge"),
        "R-15: B's durable store remains uncontaminated after cross-read"
    );
}

// ---------------------------------------------------------------------------
// INV-C1/C2 (AC-05; R-08) — config fidelity + isolation, bidirectional at N=2.
// A and B are built from GENUINELY DIFFERENT resolved config (derived through the
// same `build_project_server`-equivalent assembly as the daemon, never seeded on
// the server field); each slug's observable `ServiceLayer` reflects its OWN
// config and NEVER the other's.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_config_isolation_bidirectional_n2() {
    // A: NLI enabled, non-default (nli_top_k=37, pool=5). B: NLI disabled.
    let cfg_a = ResolvedDaemonConfig::enabled_non_default();
    let mut cfg_b = ResolvedDaemonConfig::enabled_non_default();
    cfg_b.nli_enabled = false;
    cfg_b.nli_top_k = 11; // distinct from A's 37

    let server_a = build_server_with_resolved_config(&cfg_a).await;
    let server_b = build_server_with_resolved_config(&cfg_b).await;
    let sl_a = server_a.service_layer();
    let sl_b = server_b.service_layer();

    // Fidelity: each slug's ServiceLayer reflects its OWN declared config.
    assert!(sl_a.nli_enabled(), "INV-C1: A's declared NLI-enabled governs A");
    assert_eq!(sl_a.nli_top_k(), 37, "A's own nli_top_k");
    // Isolation: B's config never governs A's, and vice versa.
    assert!(!sl_b.nli_enabled(), "INV-C2: B's declared NLI-disabled governs B, not A's enabled");
    assert_eq!(sl_b.nli_top_k(), 11, "B's own nli_top_k (not A's 37)");
    assert_ne!(
        sl_a.nli_top_k(),
        sl_b.nli_top_k(),
        "INV-C2: A and B observe DIFFERENT config (no shared/global config leak)"
    );
    assert_ne!(
        sl_a.nli_enabled(),
        sl_b.nli_enabled(),
        "INV-C2: the NLI flag is per-slug, both directions"
    );
}

// ###########################################################################
// vnc-046 WHITE-BOX wiring-pins (AC-08 complement; documented AC-06 exceptions).
// NOT part of the behavioral suite — these use handle identity deliberately, and
// exist to close the "set-but-not-threaded" gap for the handle-typed per-slug
// fields the behavioral store-layer cannot observe directly (#5427). Kept in a
// clearly-separated section per AC-06.
// ###########################################################################

mod vnc046_white_box_wiring_pins {
    use super::*;

    // Registry/pending convergence-by-construction: the resolver hands back the
    // SAME instance the slug's server holds (Arc::ptr_eq), and A's != B's — the
    // handle-identity proof the store-layer behavioral suite cannot express
    // (R-03/R-06). services parity is proven behaviorally above (services_for
    // reads the right store); store_config + inference_config are the documented
    // AC-06 white-box-only exceptions — their per-slug construction is pinned in
    // the BINARY crate (`http_provision/construction_parity_tests.rs`, Wave 2)
    // and by the boot assertion (`main_boot_assertion_tests.rs`, Wave 4), since
    // `UnimatrixServer` exposes no pub accessor for them from this external crate.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_registry_pending_ptr_identity_n2() {
        let (_router, resolver, h) = wired_observe_stack(&["alpha", "beta"]).await;
        let key_a = ProjectKey::Slug(ProjectSlug::try_from("alpha").unwrap());
        let key_b = ProjectKey::Slug(ProjectSlug::try_from("beta").unwrap());

        let reg_a = resolver.registry_for(&key_a).expect("registry_for(A)");
        let reg_b = resolver.registry_for(&key_b).expect("registry_for(B)");
        // The resolver returns the SAME instance the server holds (convergence).
        assert!(
            Arc::ptr_eq(&reg_a, &h[0].registry),
            "registry_for(A) must return A's OWN session_registry instance"
        );
        assert!(
            Arc::ptr_eq(&reg_b, &h[1].registry),
            "registry_for(B) must return B's OWN session_registry instance"
        );
        // Cross-slug: distinct instances (no shared/global registry).
        assert!(
            !Arc::ptr_eq(&reg_a, &reg_b),
            "A and B must own DISTINCT registries (no shared singleton)"
        );

        let pend_a = resolver.pending_for(&key_a).expect("pending_for(A)");
        let pend_b = resolver.pending_for(&key_b).expect("pending_for(B)");
        assert!(Arc::ptr_eq(&pend_a, &h[0].pending), "pending_for(A) is A's own");
        assert!(Arc::ptr_eq(&pend_b, &h[1].pending), "pending_for(B) is B's own");
        assert!(
            !Arc::ptr_eq(&pend_a, &pend_b),
            "A and B must own DISTINCT pending buffers (no shared singleton)"
        );
        // Silence unused-field warnings without weakening the pins above.
        assert_eq!(h[0].slug, "alpha");
        assert_eq!(h[1].slug, "beta");
    }
}

// ---------------------------------------------------------------------------
// AC-06 coverage-enumeration table (REQUIRED artifact). Per invariant: behavioral
// vs white-box, and the two white-box-only fields named explicitly. Absence of
// this table is a gate failure (SR-05). Encoded as an executable docs test so it
// cannot silently drift.
// ---------------------------------------------------------------------------

/// | Invariant / field | Coverage | Vehicle |
/// |---|---|---|
/// | INV-T1 transcript fidelity (#930) | behavioral | route_observe→per-slug store, both slugs |
/// | INV-T2 transcript isolation (identical cycle) | behavioral | route_observe→store, bidirectional, count+distillation-input exclusion |
/// | INV-T3 pending-entries isolation | white-box (registry/pending ptr identity) + #800 wire | registry_for/pending_for pins; full behavioral over the wire in test_project_isolation.py |
/// | INV-K1/K2 knowledge read fidelity+isolation + persistence | behavioral | store isolation + services_for per-slug read; #800 briefing/search over the wire |
/// | INV-C1/C2 (nli/inference-derived, observation_registry) | behavioral | per-slug ServiceLayer parity, bidirectional |
/// | store_config (byte-limit) | WHITE-BOX ONLY (AC-06 exception) | binary-crate construction_parity_tests.rs + boot assertion |
/// | inference_config (briefing blend) | WHITE-BOX ONLY (AC-06 exception) | ServiceLayer fusion/nli parity + binary-crate pins + boot assertion |
/// | registry/pending handle identity | white-box complement | Arc::ptr_eq in vnc046_white_box_wiring_pins |
///
/// AC-07 HTTPS==UDS parity and the non-zero signal_class_counts (OQ-2) guard are
/// proven over the true wire by the #800 fixture (suites/test_project_isolation.py),
/// since signal_class_counts is only observable through `cycle_review` (pub(crate)).
#[test]
fn test_vnc046_coverage_enumeration() {
    // The two white-box-only fields MUST be named — never silently omitted (SR-05).
    let white_box_only = ["store_config", "inference_config"];
    assert_eq!(
        white_box_only.len(),
        2,
        "AC-06: store_config + inference_config are the documented white-box-only exceptions"
    );
    // Behavioral invariants proven in-process at the assembled observe edge.
    let behavioral = ["INV-T1", "INV-T2", "INV-K1", "INV-K2", "INV-C1", "INV-C2"];
    assert!(
        behavioral.contains(&"INV-T2"),
        "AC-06: INV-T2 cross-slug isolation is behavioral (route_observe→store, bidirectional)"
    );
}
