//! Unit tests for the Wave-2 `MultiProjectRouter` (`StoreResolver` impl).
//!
//! vnc-034 Wave 2 — project-router test plan §A/§B/§C. These drive the REAL
//! shipped `MultiProjectRouter` / `ProjectEntry` / `adapter_for` API (not a stub).
//! Lead risks: R-01 (Critical — additive swap, single funnel), R-04 (seam parity),
//! R-06 (N clients : 1 slug). The no-bypass funnel record (`adapter_for` is the SOLE
//! dispatch route) is asserted here structurally + behaviorally; the two-store HTTP
//! dispatch correctness lives in `tests/project_routing_integration.rs`.
//!
//! Store-identity idiom (pattern #4958): `Store` has no `PartialEq`, so "same store"
//! is asserted with `Arc::ptr_eq` and the error path with `.expect_err(..)`.

use std::sync::Arc;

use unimatrix_core::Store;

use super::{MultiProjectRouter, ProjectServerInput};
use crate::http::router::McpAdapter;
use crate::http::router::seam::{ProjectKey, ProjectSlug, RouteError, StoreResolver};
use crate::server::tests::make_server;

/// Build one `(slug, store, server)` triple over a fresh, isolated store.
///
/// Reuses the existing `make_server()` test helper (no isolated scaffolding):
/// each call opens its OWN temp store, so two triples are DISTINCT stores —
/// exactly the two-store setup AC-W2-R3 isolation needs. Returns the
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

/// Build a default `(server, store)` pair over a fresh store.
async fn make_default() -> (crate::server::UnimatrixServer, Arc<Store>) {
    let server = make_server().await;
    let store = Arc::clone(&server.store);
    (server, store)
}

const TEST_MAX_BODY: usize = 1024 * 1024;

// ===========================================================================
// §A. R-01 (Critical) — additive swap, single funnel, routing inside the seam
// ===========================================================================

// ---- AC-W2-R1 — per-slug resolution to the slug's OWN store ----

#[tokio::test(flavor = "multi_thread")]
async fn test_resolves_slug_to_its_store() {
    // Two distinct slugs over two DISTINCT stores. resolve_store(Slug("alpha"))
    // returns alpha's store; Slug("beta") returns beta's. Arc::ptr_eq against the
    // intended store proves no cross-wiring (pattern #4958).
    let (default_server, default_store) = make_default().await;
    let (alpha_input, alpha_store) = make_slug_input("alpha").await;
    let (beta_input, beta_store) = make_slug_input("beta").await;

    let resolver = MultiProjectRouter::from_servers(
        default_store,
        default_server,
        vec![alpha_input, beta_input],
        TEST_MAX_BODY,
        vec![],
    )
    .expect("build resolver");

    let alpha_key = ProjectKey::Slug(ProjectSlug::try_from("alpha").expect("valid"));
    let beta_key = ProjectKey::Slug(ProjectSlug::try_from("beta").expect("valid"));

    let resolved_alpha = resolver.resolve_store(&alpha_key).expect("alpha resolves");
    let resolved_beta = resolver.resolve_store(&beta_key).expect("beta resolves");

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
async fn test_unknown_slug_returns_unknown_project() {
    // A slug not in the map -> UnknownProject. NEVER a panic, never a fall-through
    // to the default or another slug (R-01 sc.3, AC-W2-R3).
    let (default_server, default_store) = make_default().await;
    let default_store_handle = Arc::clone(&default_store);
    let (alpha_input, _alpha_store) = make_slug_input("alpha").await;

    let resolver = MultiProjectRouter::from_servers(
        default_store,
        default_server,
        vec![alpha_input],
        TEST_MAX_BODY,
        vec![],
    )
    .expect("build resolver");

    let ghost = ProjectKey::Slug(ProjectSlug::try_from("ghost").expect("valid"));
    let err = resolver
        .resolve_store(&ghost)
        .expect_err("unregistered slug must be UnknownProject");
    assert_eq!(err, RouteError::UnknownProject);

    // It must NOT have fallen back to the default store.
    let resolved_default = resolver
        .resolve_store(&ProjectKey::Default)
        .expect("default resolves");
    assert!(
        Arc::ptr_eq(&resolved_default, &default_store_handle),
        "Default still resolves to the default store — the ghost did not leak it"
    );
    // adapter_for for the ghost is None (caller 404s; no fixed-adapter fallback).
    assert!(
        resolver.adapter_for(&ghost).is_none(),
        "unknown slug -> adapter_for None (404, never a fixed fallback)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_default_key_returns_default_store() {
    // resolve_store(Default) returns the one default store unchanged (R-04,
    // backward-compat for /v1/tools/...).
    let (default_server, default_store) = make_default().await;
    let default_handle = Arc::clone(&default_store);
    let (alpha_input, _alpha_store) = make_slug_input("alpha").await;

    let resolver = MultiProjectRouter::from_servers(
        default_store,
        default_server,
        vec![alpha_input],
        TEST_MAX_BODY,
        vec![],
    )
    .expect("build resolver");

    let resolved = resolver
        .resolve_store(&ProjectKey::Default)
        .expect("Default resolves");
    assert!(
        Arc::ptr_eq(&resolved, &default_handle),
        "Default must resolve to the injected default store (Arc identity)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_swaps_at_slugrouter_callsite() {
    // R-01 sc.2: MultiProjectRouter coerces to Arc<dyn StoreResolver> — the SOLE
    // swap point at the SlugRouter::new call site, no route-grammar / layer change.
    // Construction + coercion IS the assertion (mirrors the Wave-1 swap test).
    let (default_server, default_store) = make_default().await;
    let (alpha_input, _alpha_store) = make_slug_input("alpha").await;

    let resolver = MultiProjectRouter::from_servers(
        default_store,
        default_server,
        vec![alpha_input],
        TEST_MAX_BODY,
        vec![],
    )
    .expect("build resolver");

    // The drop-in: it is usable exactly where DefaultResolver was — as a trait
    // object behind the seam. If MultiProjectRouter stopped implementing the
    // trait, this would fail to compile.
    let as_seam: Arc<dyn StoreResolver> = Arc::new(resolver);
    assert!(
        as_seam.resolve_store(&ProjectKey::Default).is_ok(),
        "the swapped-in resolver routes Default through the same trait call site"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_no_residual_fixed_adapter_path() {
    // THE no-bypass funnel test (Wave-1 honesty record). Wave-1 route_mcp discarded
    // the resolved store (`let _store`) and dispatched through a parallel FIXED
    // adapter holding the single store. Wave 2 eliminates that: dispatch goes ONLY
    // through adapter_for(key), which reads the SAME per-entry map resolve_store
    // reads. Structural + behavioral assertion:
    //   (1) the adapter adapter_for(key) returns wraps EXACTLY the store
    //       resolve_store(key) returns — they cannot diverge (no leftover fixed
    //       adapter over a different store), proven via McpAdapter::wraps_store;
    //   (2) each distinct slug gets its OWN adapter (no shared/fixed fallback);
    //   (3) an unknown key yields None (404), never a fixed-adapter fallback.
    let (default_server, default_store) = make_default().await;
    let default_handle = Arc::clone(&default_store);
    let (alpha_input, alpha_store) = make_slug_input("alpha").await;
    let (beta_input, beta_store) = make_slug_input("beta").await;

    let resolver = MultiProjectRouter::from_servers(
        default_store,
        default_server,
        vec![alpha_input, beta_input],
        TEST_MAX_BODY,
        vec![],
    )
    .expect("build resolver");

    let alpha_key = ProjectKey::Slug(ProjectSlug::try_from("alpha").expect("valid"));
    let beta_key = ProjectKey::Slug(ProjectSlug::try_from("beta").expect("valid"));

    // (1) resolve/dispatch agreement per key: the adapter wraps the SAME store the
    //     funnel resolved — this is the structural proof there is no residual
    //     fixed adapter over a different store that a request could still reach.
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
    assert_agreement(&ProjectKey::Default, &default_handle);
    assert_agreement(&alpha_key, &alpha_store);
    assert_agreement(&beta_key, &beta_store);

    // (2) each slug's adapter wraps ONLY its own store, never another slug's or the
    //     default — i.e. there is no single fixed adapter serving all keys.
    let alpha_adapter = resolver.adapter_for(&alpha_key).expect("alpha adapter");
    assert!(
        !alpha_adapter.wraps_store(&beta_store),
        "alpha's adapter must not wrap beta's store"
    );
    assert!(
        !alpha_adapter.wraps_store(&default_handle),
        "alpha's adapter must not wrap the default store (no fixed fallback)"
    );

    // (3) an unknown key resolves no adapter (404), never a fixed fallback.
    let ghost = ProjectKey::Slug(ProjectSlug::try_from("ghost").expect("valid"));
    assert!(
        resolver.resolve_store(&ghost).is_err(),
        "unknown slug -> resolve error"
    );
    assert!(
        resolver.adapter_for(&ghost).is_none(),
        "unknown slug -> adapter_for None; the caller 404s, never a fixed adapter"
    );
}

// ===========================================================================
// §B. R-04 — seam parity (slug ⟂ default, no fall-through either direction)
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_default_path_unchanged_with_projects() {
    // AC-W2-R2 / AC-CT-C4: with slugs registered, Default still resolves to the
    // SAME default store with the SAME Arc semantics as a no-projects resolver —
    // byte-identical default path, no I/O per call (no re-open).
    let (default_server, default_store) = make_default().await;
    let default_handle = Arc::clone(&default_store);
    let (alpha_input, _a) = make_slug_input("alpha").await;
    let (beta_input, _b) = make_slug_input("beta").await;

    let resolver = MultiProjectRouter::from_servers(
        default_store,
        default_server,
        vec![alpha_input, beta_input],
        TEST_MAX_BODY,
        vec![],
    )
    .expect("build resolver");

    let first = resolver
        .resolve_store(&ProjectKey::Default)
        .expect("default first");
    let second = resolver
        .resolve_store(&ProjectKey::Default)
        .expect("default second");
    assert!(
        Arc::ptr_eq(&first, &default_handle) && Arc::ptr_eq(&second, &default_handle),
        "repeated Default resolutions clone the SAME default store — no re-open"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_no_projects_default_byte_identical() {
    // AC-W2-R2: [[projects]] absent (empty slug_servers) -> resolver holds only the
    // default. /v1/tools/... (Default) resolves to the one store; any Slug -> 404.
    // Same observable behavior as the Wave-1 DefaultResolver (no re-init).
    let (default_server, default_store) = make_default().await;
    let default_handle = Arc::clone(&default_store);

    let resolver = MultiProjectRouter::from_servers(
        default_store,
        default_server,
        vec![],
        TEST_MAX_BODY,
        vec![],
    )
    .expect("build resolver");

    let resolved = resolver
        .resolve_store(&ProjectKey::Default)
        .expect("default resolves with no projects");
    assert!(
        Arc::ptr_eq(&resolved, &default_handle),
        "no-projects Default resolves to the one store, byte-identical to Wave 1"
    );

    let any_slug = ProjectKey::Slug(ProjectSlug::try_from("anyslug").expect("valid"));
    assert_eq!(
        resolver
            .resolve_store(&any_slug)
            .expect_err("any slug is unknown with no projects"),
        RouteError::UnknownProject,
        "with no [[projects]], every slug -> UnknownProject (never the default)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_slug_never_leaks_into_default_resolution() {
    // Disjoint resolution: resolve_store(Default) ignores the slug map entirely;
    // resolve_store(Slug(_)) never consults the default store. (A2: path-hash /
    // default mode and slug mode never cross.)
    let (default_server, default_store) = make_default().await;
    let default_handle = Arc::clone(&default_store);
    let (alpha_input, alpha_store) = make_slug_input("alpha").await;

    let resolver = MultiProjectRouter::from_servers(
        default_store,
        default_server,
        vec![alpha_input],
        TEST_MAX_BODY,
        vec![],
    )
    .expect("build resolver");

    let resolved_default = resolver
        .resolve_store(&ProjectKey::Default)
        .expect("default resolves");
    let alpha_key = ProjectKey::Slug(ProjectSlug::try_from("alpha").expect("valid"));
    let resolved_alpha = resolver.resolve_store(&alpha_key).expect("alpha resolves");

    assert!(
        Arc::ptr_eq(&resolved_default, &default_handle),
        "Default resolves ONLY to the default store"
    );
    assert!(
        Arc::ptr_eq(&resolved_alpha, &alpha_store),
        "Slug(alpha) resolves ONLY to alpha's store"
    );
    assert!(
        !Arc::ptr_eq(&resolved_default, &alpha_store),
        "the default resolution must never be a slug's store"
    );
    assert!(
        !Arc::ptr_eq(&resolved_alpha, &default_handle),
        "a slug resolution must never be the default store"
    );
}

// ===========================================================================
// §C. R-06 / FR-C7 — N clients : 1 slug share the SAME per-slug store
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_n_clients_one_slug_shared_store() {
    // Two resolve calls for the same slug return clones of the SAME Arc<Store>
    // (Arc::ptr_eq) — N clients on one slug share state (per-session_id attribution
    // is asserted at HTTP integration). No store re-open per call.
    let (default_server, default_store) = make_default().await;
    let (alpha_input, alpha_store) = make_slug_input("alpha").await;

    let resolver = MultiProjectRouter::from_servers(
        default_store,
        default_server,
        vec![alpha_input],
        TEST_MAX_BODY,
        vec![],
    )
    .expect("build resolver");

    let alpha_key = ProjectKey::Slug(ProjectSlug::try_from("alpha").expect("valid"));
    let client_a = resolver.resolve_store(&alpha_key).expect("client A");
    let client_b = resolver.resolve_store(&alpha_key).expect("client B");
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
    let (default_server, default_store) = make_default().await;
    let (dup_a, _a) = make_slug_input("dup").await;
    let (dup_b, _b) = make_slug_input("dup").await;

    let result = MultiProjectRouter::from_servers(
        default_store,
        default_server,
        vec![dup_a, dup_b],
        TEST_MAX_BODY,
        vec![],
    );
    let err = result.expect_err("duplicate slug must fail loud");
    assert!(
        err.contains("duplicate"),
        "duplicate-slug error must name the failure, got: {err}"
    );
}
