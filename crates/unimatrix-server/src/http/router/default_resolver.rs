//! `DefaultResolver` — the Wave-1 `StoreResolver` impl (vnc-034 ADR-003/005).
//!
//! THE single Wave-1 resolver behind the `StoreResolver` seam. It returns the one
//! configured `Arc<Store>` for `ProjectKey::Default` and `RouteError::UnknownProject`
//! for ANY `ProjectKey::Slug(_)` — slug routes parse (the route SHAPE exists) but the
//! resolver is INERT until Wave 2 swaps in a slug-aware `ProjectRouter` at the SAME
//! `SlugRouter::new` call site (ADR-003, R-01 sc.2).
//!
//! Local / cloud parity (R-04 / NFR-10 / AC-W1-X2 — load-bearing): the SAME
//! `DefaultResolver` is constructed in both deployment modes; only the injected
//! `store`'s provenance differs. The local UDS daemon injects its path-hash store
//! (ADR-004 #80); the cloud single-project install injects the one project store.
//! Both resolve `ProjectKey::Default` through the IDENTICAL `resolve_store` — there is
//! no cloud-only branch — so the common local install exercises the very seam the
//! cloud isolation depends on. The path-hash assumption lives ONLY in how the local
//! store is *opened* upstream; it never enters `resolve_store` and never leaks into
//! cloud mode, and the cloud slug never leaks into the local path (R-04 sc.2).
//!
//! Wave-1 degenerate-but-genuine seam (NFR-09): this resolver is the genuine, but
//! degenerate, Wave-1 case of the trait — one store, no slug map. It is constructed
//! at the listener (`main.rs`) and injected into `SlugRouter`, so it is the live
//! per-request funnel for `ProjectKey::Default`. Wave 2 substitutes a `ProjectRouter`
//! at the same call site with no interface re-cut.

use std::sync::Arc;

use unimatrix_core::Store;

use super::McpAdapter;
use super::seam::{ProjectKey, RouteError, StoreResolver};
use crate::server::UnimatrixServer;

/// Wave-1 `StoreResolver`: one store, served THROUGH the funnel (ADR-003, FR-X5).
///
/// Holds exactly one `Arc<Store>` — the sole write capability threaded from the
/// routing edge (C4 invariant 2 / FR-X3) — and (vnc-034 Wave 2) the single
/// `McpAdapter` it dispatches through, so `adapter_for(&Default)` is the SOLE
/// dispatch route. No per-slug map, no I/O, no locking: the whole point is that
/// the single-project deployment is the degenerate-but-genuine case of the trait.
/// Wave 2 substitutes a `MultiProjectRouter` at the same call site with no
/// interface re-cut.
#[derive(Debug, Clone)]
pub struct DefaultResolver {
    /// The one store. Local mode: the ADR-004 path-hash store. Cloud
    /// single-project mode: the one project store. Identical resolver, identical path.
    store: Arc<Store>,
    /// The single MCP adapter for `Default` dispatch (vnc-034 Wave 2 funnel-
    /// elimination). `None` for store-only test resolvers that never dispatch;
    /// the production listener wiring builds it via [`DefaultResolver::with_adapter`]
    /// so `adapter_for(&Default)` returns `Some` and the discard path is removed.
    adapter: Option<McpAdapter>,
}

impl DefaultResolver {
    /// Construct a store-only resolver (no dispatch adapter).
    ///
    /// For tests / callers that exercise only `resolve_store`. `adapter_for`
    /// returns `None`, so this MUST NOT be used as the production MCP dispatch
    /// resolver — the listener wiring uses [`DefaultResolver::with_adapter`].
    pub fn new(store: Arc<Store>) -> Self {
        DefaultResolver {
            store,
            adapter: None,
        }
    }

    /// Construct the production single-project resolver over the store AND the
    /// default `McpAdapter` (vnc-034 Wave 2).
    ///
    /// Called once during listener wiring after the store + `UnimatrixServer` are
    /// assembled. Building the adapter here lets `adapter_for(&Default)` be the
    /// SOLE dispatch route — single-project deployments take the SAME one path as
    /// Wave 1, just SELECTED via the funnel instead of via a discarded-store fixed
    /// path (AC-CT-C4: byte-identical observable behavior, no client re-init).
    /// `store` MUST be the handle `server` dispatches against (OQ-PR-4).
    pub fn with_adapter(
        store: Arc<Store>,
        server: UnimatrixServer,
        max_body_bytes: usize,
        allowed_origins: Vec<String>,
    ) -> Self {
        let adapter = McpAdapter::new(server, max_body_bytes, allowed_origins);
        DefaultResolver {
            store,
            adapter: Some(adapter),
        }
    }
}

impl StoreResolver for DefaultResolver {
    /// Resolve a transport-derived `ProjectKey` to its store handle (THE single funnel).
    ///
    /// - `ProjectKey::Default` -> `Ok` of an `Arc` clone of the one store. Repeated
    ///   calls return clones of the SAME underlying store — not a re-opened handle per
    ///   request (AC-W1-X1).
    /// - `ProjectKey::Slug(_)` -> `Err(RouteError::UnknownProject)`. Slug routes are
    ///   INERT under the single-project resolver: never a panic, and NEVER a silent
    ///   fall-through to the default store (R-01 sc.3). Wave 2 swaps in
    ///   `MultiProjectRouter`, which resolves `Slug(_)` to its per-slug store.
    ///
    /// Total over `ProjectKey`. No `.unwrap()`, no panic, no I/O.
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError> {
        match key {
            ProjectKey::Default => Ok(Arc::clone(&self.store)),
            ProjectKey::Slug(_) => Err(RouteError::UnknownProject),
        }
    }

    /// THE SOLE dispatch route for the single-project resolver (vnc-034 Wave 2).
    ///
    /// - `Default` -> the one adapter (when built via [`DefaultResolver::with_adapter`]);
    ///   `None` for store-only resolvers (tests).
    /// - `Slug(_)` -> `None` (unknown -> 404; never a fixed-adapter fallback).
    #[allow(private_interfaces)]
    fn adapter_for(&self, key: &ProjectKey) -> Option<&McpAdapter> {
        match key {
            ProjectKey::Default => self.adapter.as_ref(),
            ProjectKey::Slug(_) => None,
        }
    }
}
