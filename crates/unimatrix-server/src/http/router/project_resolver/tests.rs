//! Unit tests for the unified slug-keyed `MultiProjectRouter` (`StoreResolver`).
//!
//! vnc-038 ADR-004 (#5083) — the served-project `ProjectKey::Default` is DELETED.
//! These drive the REAL shipped `MultiProjectRouter` / `ProjectEntry` /
//! `adapter_for` API (not a stub). The resolver is keyed by `ProjectKey::Slug`
//! ONLY: no `default` entry, no `Default` arm. Lead risks: R-07 (call-site audit),
//! R-09 (no cross-pollination, MANDATORY at N=2). The no-bypass funnel record
//! (`adapter_for` is the SOLE dispatch route) is asserted structurally +
//! behaviorally; the two-store HTTP dispatch correctness lives in
//! `tests/project_routing_integration.rs`.
//!
//! Store-identity idiom (pattern #4958): `Store` has no `PartialEq`, so "same
//! store" is asserted with `Arc::ptr_eq` and the error path with `.expect_err(..)`.

use std::sync::Arc;

use unimatrix_core::Store;

use super::{MultiProjectRouter, ProjectServerInput};
use crate::http::router::seam::{ProjectKey, ProjectSlug, RouteError, StoreResolver};
use crate::server::tests::make_server;

/// Build one `(slug, store, server)` triple over a fresh, isolated store.
///
/// Reuses the existing `make_server()` test helper (no isolated scaffolding):
/// each call opens its OWN temp store, so two triples are DISTINCT stores —
/// exactly the two-store setup the N=2 isolation proof needs. Returns the
/// `ProjectServerInput` the resolver constructor consumes PLUS an `Arc<Store>`
/// clone of the slug's own store, so the test can assert resolve-identity against
/// the intended store via `Arc::ptr_eq`.
async fn make_slug_input(slug: &str) -> (ProjectServerInput, Arc<Store>) {
    let server = make_server().await;
    // The server's own store — the handle the slug's adapter dispatches against
    // (OQ-PR-4 resolve/dispatch agreement). Cloning it lets the test assert the
    // resolved store IS this one.
    let store = Arc::clone(&server.store);
    let slug = ProjectSlug::try_from(slug).expect("valid slug");
    let input = ProjectServerInput {
        slug,
        store: Arc::clone(&store),
        server,
    };
    (input, store)
}

fn slug_key(s: &str) -> ProjectKey {
    ProjectKey::Slug(ProjectSlug::try_from(s).expect("valid slug"))
}

const TEST_MAX_BODY: usize = 1024 * 1024;

/// Non-empty `allowed_hosts` for fixtures (bug #774). NEVER pass `Vec::new()`:
/// rmcp treats an empty `allowed_hosts` as allow-all (fail-open), so baking an
/// empty vec into fixtures would encode the fail-open shape.
fn test_allowed_hosts() -> Vec<String> {
    vec!["localhost".to_string()]
}

// ===========================================================================
// §A. Per-slug resolution to the slug's OWN store (R-07 sc.1)
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_resolves_slug_to_its_store() {
    // Two distinct slugs over two DISTINCT stores. resolve_store(Slug("alpha"))
    // returns alpha's store; Slug("beta") returns beta's. Arc::ptr_eq against the
    // intended store proves no cross-wiring (pattern #4958).
    let (alpha_input, alpha_store) = make_slug_input("alpha").await;
    let (beta_input, beta_store) = make_slug_input("beta").await;

    let resolver = MultiProjectRouter::from_servers(
        vec![alpha_input, beta_input],
        TEST_MAX_BODY,
        vec![],
        test_allowed_hosts(),
    )
    .expect("build resolver");

    let resolved_alpha = resolver
        .resolve_store(&slug_key("alpha"))
        .expect("alpha resolves");
    let resolved_beta = resolver
        .resolve_store(&slug_key("beta"))
        .expect("beta resolves");

    assert!(
        Arc::ptr_eq(&resolved_alpha, &alpha_store),
        "Slug(alpha) must resolve to alpha's own store"
    );
    assert!(
        Arc::ptr_eq(&resolved_beta, &beta_store),
        "Slug(beta) must resolve to beta's own store"
    );
    // Cross-check: alpha must NOT resolve to beta's store and vice-versa.
    assert!(
        !Arc::ptr_eq(&resolved_alpha, &beta_store),
        "alpha must never resolve to beta's store (isolation)"
    );
    assert!(
        !Arc::ptr_eq(&resolved_beta, &alpha_store),
        "beta must never resolve to alpha's store (isolation)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_resolve_unregistered_slug_unknown_project() {
    // A valid-grammar but unregistered slug -> UnknownProject. NEVER a panic,
    // never a fall-through to another slug (R-09). There is NO default store to
    // leak (vnc-038 ADR-004).
    let (alpha_input, _alpha_store) = make_slug_input("alpha").await;

    let resolver = MultiProjectRouter::from_servers(
        vec![alpha_input],
        TEST_MAX_BODY,
        vec![],
        test_allowed_hosts(),
    )
    .expect("build resolver");

    let err = resolver
        .resolve_store(&slug_key("ghost"))
        .expect_err("unregistered slug must be UnknownProject");
    assert_eq!(err, RouteError::UnknownProject);

    // adapter_for for the ghost is None (caller 404s; no fixed-adapter fallback,
    // the #4974 guard).
    assert!(
        resolver.adapter_for(&slug_key("ghost")).is_none(),
        "unknown slug -> adapter_for None (404, never a fixed fallback)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_single_deployment_is_n1() {
    // RD-5: one registered slug resolves through the SAME slug-keyed map with no
    // special-case branch. N=1 is one map entry, not a distinct default path.
    let (only_input, only_store) = make_slug_input("only").await;

    let resolver = MultiProjectRouter::from_servers(
        vec![only_input],
        TEST_MAX_BODY,
        vec![],
        test_allowed_hosts(),
    )
    .expect("build resolver");

    let resolved = resolver
        .resolve_store(&slug_key("only"))
        .expect("only resolves");
    assert!(
        Arc::ptr_eq(&resolved, &only_store),
        "single deployment resolves through the slug-keyed path (N=1 is one entry)"
    );
    // Any other slug is still a hard UnknownProject — no implicit default.
    assert_eq!(
        resolver
            .resolve_store(&slug_key("other"))
            .expect_err("non-registered slug is unknown"),
        RouteError::UnknownProject
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_empty_resolver_no_servable_store() {
    // [[projects]] absent -> empty slug map -> NOTHING servable. Every slug is
    // UnknownProject; there is no default store (R-10 at the resolver layer).
    let resolver =
        MultiProjectRouter::from_servers(vec![], TEST_MAX_BODY, vec![], test_allowed_hosts())
            .expect("build empty resolver");

    assert_eq!(
        resolver
            .resolve_store(&slug_key("anyslug"))
            .expect_err("any slug is unknown with no projects"),
        RouteError::UnknownProject,
        "with no [[projects]], every slug -> UnknownProject (never a default store)"
    );
    assert!(
        resolver.adapter_for(&slug_key("anyslug")).is_none(),
        "empty resolver dispatches nothing"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_swaps_at_slugrouter_callsite() {
    // R-07 sc.2: MultiProjectRouter coerces to Arc<dyn StoreResolver> — the SOLE
    // resolver at the SlugRouter::new call site, no route-grammar / layer change.
    let (alpha_input, _alpha_store) = make_slug_input("alpha").await;

    let resolver = MultiProjectRouter::from_servers(
        vec![alpha_input],
        TEST_MAX_BODY,
        vec![],
        test_allowed_hosts(),
    )
    .expect("build resolver");

    let as_seam: Arc<dyn StoreResolver> = Arc::new(resolver);
    assert!(
        as_seam.resolve_store(&slug_key("alpha")).is_ok(),
        "the unified resolver routes Slug through the same trait call site"
    );
}

// ===========================================================================
// §B. N=2 cross-pollination proof (R-09 / AC-06 / C-11) — MANDATORY, NOT N=1
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_two_slugs_route_to_distinct_stores() {
    // Register A and B; assert each resolves to its OWN Arc<Store> (Arc::ptr_eq),
    // and never the other's. This is the load-bearing N=2 isolation proof — an
    // N=1 green would not catch a residual cross-wiring (#4974).
    let (a_input, a_store) = make_slug_input("a").await;
    let (b_input, b_store) = make_slug_input("b").await;

    let resolver = MultiProjectRouter::from_servers(
        vec![a_input, b_input],
        TEST_MAX_BODY,
        vec![],
        test_allowed_hosts(),
    )
    .expect("build resolver");

    let resolved_a = resolver.resolve_store(&slug_key("a")).expect("a resolves");
    let resolved_b = resolver.resolve_store(&slug_key("b")).expect("b resolves");
    assert!(
        Arc::ptr_eq(&resolved_a, &a_store),
        "a resolves to a's store"
    );
    assert!(
        Arc::ptr_eq(&resolved_b, &b_store),
        "b resolves to b's store"
    );
    assert!(
        !Arc::ptr_eq(&resolved_a, &b_store) && !Arc::ptr_eq(&resolved_b, &a_store),
        "neither slug leaks the other's store (N=2 isolation)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_resolve_dispatch_same_map() {
    // adapter_for(key) wraps EXACTLY the store resolve_store(key) returned (the
    // wraps_store debug-assert tie): resolve and dispatch read the SAME per-entry
    // map and cannot diverge. Also: each slug's adapter wraps ONLY its own store.
    let (a_input, a_store) = make_slug_input("a").await;
    let (b_input, b_store) = make_slug_input("b").await;

    let resolver = MultiProjectRouter::from_servers(
        vec![a_input, b_input],
        TEST_MAX_BODY,
        vec![],
        test_allowed_hosts(),
    )
    .expect("build resolver");

    let assert_agreement = |key: &ProjectKey, expected: &Arc<Store>| {
        let resolved = resolver.resolve_store(key).expect("resolves");
        assert!(
            Arc::ptr_eq(&resolved, expected),
            "resolve_store must return the intended store"
        );
        let adapter = resolver
            .adapter_for(key)
            .expect("a resolvable key yields an adapter");
        assert!(
            adapter.wraps_store(&resolved),
            "adapter_for(key) must wrap the SAME store resolve_store(key) returned \
             — no residual fixed-adapter bypass"
        );
    };
    assert_agreement(&slug_key("a"), &a_store);
    assert_agreement(&slug_key("b"), &b_store);

    // each slug's adapter wraps ONLY its own store, never another slug's — there
    // is no single fixed adapter serving all keys.
    let a_adapter = resolver.adapter_for(&slug_key("a")).expect("a adapter");
    assert!(
        !a_adapter.wraps_store(&b_store),
        "a's adapter must not wrap b's store (no fixed fallback)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_prefix_related_slugs_no_misresolution() {
    // N=2 prefix-collision edge: register `proj` and `project`; assert each
    // resolves to its OWN distinct store (no path-prefix mis-resolution).
    let (proj_input, proj_store) = make_slug_input("proj").await;
    let (project_input, project_store) = make_slug_input("project").await;

    let resolver = MultiProjectRouter::from_servers(
        vec![proj_input, project_input],
        TEST_MAX_BODY,
        vec![],
        test_allowed_hosts(),
    )
    .expect("build resolver");

    let resolved_proj = resolver
        .resolve_store(&slug_key("proj"))
        .expect("proj resolves");
    let resolved_project = resolver
        .resolve_store(&slug_key("project"))
        .expect("project resolves");
    assert!(
        Arc::ptr_eq(&resolved_proj, &proj_store),
        "`proj` resolves to its own store"
    );
    assert!(
        Arc::ptr_eq(&resolved_project, &project_store),
        "`project` resolves to its own store"
    );
    assert!(
        !Arc::ptr_eq(&resolved_proj, &project_store),
        "prefix-related slugs must not cross-resolve"
    );
}

// ===========================================================================
// §C. R-06 / FR-C7 — N clients : 1 slug share the SAME per-slug store
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_n_clients_one_slug_share_store() {
    // Two resolve calls for the same slug return clones of the SAME Arc<Store>
    // (Arc::ptr_eq) — N clients on one slug share state. No store re-open per call.
    let (alpha_input, alpha_store) = make_slug_input("alpha").await;

    let resolver = MultiProjectRouter::from_servers(
        vec![alpha_input],
        TEST_MAX_BODY,
        vec![],
        test_allowed_hosts(),
    )
    .expect("build resolver");

    let client_a = resolver
        .resolve_store(&slug_key("alpha"))
        .expect("client A");
    let client_b = resolver
        .resolve_store(&slug_key("alpha"))
        .expect("client B");
    assert!(
        Arc::ptr_eq(&client_a, &client_b),
        "two clients on one slug share the SAME Arc<Store>"
    );
    assert!(
        Arc::ptr_eq(&client_a, &alpha_store),
        "the shared store is the slug's own store"
    );
}

// ===========================================================================
// Constructor — defensive duplicate-slug re-check (fail loud, no panic)
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_from_servers_rejects_duplicate_slug() {
    // Duplicate slugs are already rejected at config-validate; from_servers
    // defensively re-checks and returns Err (loud), never panics / silently drops.
    let (dup_a, _a) = make_slug_input("dup").await;
    let (dup_b, _b) = make_slug_input("dup").await;

    let result = MultiProjectRouter::from_servers(
        vec![dup_a, dup_b],
        TEST_MAX_BODY,
        vec![],
        test_allowed_hosts(),
    );
    let err = result.expect_err("duplicate slug must fail loud");
    assert!(
        err.contains("duplicate"),
        "duplicate-slug error must name the failure, got: {err}"
    );
}
