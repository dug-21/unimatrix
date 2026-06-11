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
//! Wave-1 degenerate-but-genuine seam (NFR-09): this resolver, the trait, and
//! `SlugRouter` are defined and unit-tested in Wave 1 but only WIRED into the listener
//! (`main.rs`) in Sub-wave 3. Until that wiring lands `DefaultResolver` has no
//! production caller, so this module allows `dead_code` (mirroring the seam) —
//! present-but-unwired, not abandoned. The allow is removed when Sub-wave 3 constructs
//! `DefaultResolver` at the listener.
#![allow(dead_code)]

use std::sync::Arc;

use unimatrix_core::Store;

use super::seam::{ProjectKey, RouteError, StoreResolver};

/// Wave-1 `StoreResolver`: one store, served THROUGH the funnel (ADR-003, FR-X5).
///
/// Holds exactly one `Arc<Store>` — the sole write capability threaded from the
/// routing edge (C4 invariant 2 / FR-X3). No per-slug map, no I/O, no locking: the
/// whole point is that Wave 1 is the degenerate-but-genuine case of the trait. Wave 2
/// substitutes a `ProjectRouter` at the same call site with no interface re-cut.
#[derive(Debug, Clone)]
pub struct DefaultResolver {
    /// The one Wave-1 store. Local mode: the ADR-004 path-hash store. Cloud
    /// single-project mode: the one project store. Identical resolver, identical path.
    store: Arc<Store>,
}

impl DefaultResolver {
    /// Construct the Wave-1 resolver over the single configured store.
    ///
    /// Called once during listener wiring (Sub-wave 3) after `open_store_with_retry`
    /// yields the `Arc<Store>` — in BOTH deployment modes, with only the store's
    /// provenance differing (R-04). This component does not open the store; it wraps
    /// the already-opened handle behind the trait.
    pub fn new(store: Arc<Store>) -> Self {
        DefaultResolver { store }
    }
}

impl StoreResolver for DefaultResolver {
    /// Resolve a transport-derived `ProjectKey` to its store handle (THE single funnel).
    ///
    /// - `ProjectKey::Default` -> `Ok` of an `Arc` clone of the one store. Repeated
    ///   calls return clones of the SAME underlying store — not a re-opened handle per
    ///   request (AC-W1-X1).
    /// - `ProjectKey::Slug(_)` -> `Err(RouteError::UnknownProject)`. In Wave 1 slug
    ///   routes are INERT: never a panic, and NEVER a silent fall-through to the
    ///   default store (R-01 sc.3). Wave 2 swaps in `ProjectRouter`, which resolves
    ///   `Slug(_)` to its per-slug store.
    ///
    /// Total over `ProjectKey`. No `.unwrap()`, no panic, no I/O.
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError> {
        match key {
            ProjectKey::Default => Ok(Arc::clone(&self.store)),
            ProjectKey::Slug(_) => Err(RouteError::UnknownProject),
        }
    }
}
