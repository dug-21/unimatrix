//! `MultiProjectRouter` — the unified slug-keyed `StoreResolver`
//! (vnc-034 ADR-003/004/005; vnc-038 ADR-004 #5083).
//!
//! The SOLE `StoreResolver` for the cloud/container HTTP surface, injected at the
//! `SlugRouter::new` call site. vnc-038 ADR-004 DELETED the served-project
//! `ProjectKey::Default`, the `default` field, the default constructor params, and
//! the `Default` arms: this resolver is keyed by `ProjectKey::Slug` ONLY. Single
//! project is N=1 — one entry in the slug map, no special-case arm. Local
//! STDIO/UDS keeps its DIRECT path-hash store binding and NEVER enters this
//! resolver (ADR-006 #5087).
//!
//! ## Type-collision note (OQ-PR-2)
//! The design docs (ARCHITECTURE §7, BRIEF) call this resolver "ProjectRouter".
//! In code it is `MultiProjectRouter` to avoid shadowing the (now-removed) generic
//! HTTP `ProjectRouter<ReqBody>`. The funnel-elimination deleted that fixed
//! single-project dispatcher; per-key dispatch now lives INSIDE this resolver.
//!
//! ## Funnel-elimination (the load-bearing Wave-2 change)
//! Each registered slug's `ProjectEntry` carries BOTH the slug's `Arc<Store>` AND
//! its own `McpAdapter`. `resolve_store` is the store funnel (proves transport
//! identity, FR-X1/X3); `adapter_for` is the SOLE MCP dispatch route (OQ-PR-9).
//! The Wave-1 `let _store` discard and the parallel fixed-adapter dispatch are
//! gone. With two real stores, a residual fixed-adapter fallback would silently
//! serve the wrong store — the bug Wave 1 could not catch under N=1. Resolution
//! and dispatch read the SAME per-entry map, so they can never diverge
//! (`SlugRouter` asserts agreement via `McpAdapter::wraps_store`, OQ-PR-4).
//!
//! ## Isolation invariant (AC-W2-R3)
//! `resolve_store(Slug(a))` NEVER returns B's store (there is no default store);
//! an unknown slug is `UnknownProject`, never a fall-through. Each entry's store / vector
//! index / hash chain / analytics are the slug's OWN isolated resources (FR-C3),
//! built per-slug in the listener wiring (`build_project_entry`, main.rs).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use unimatrix_core::Store;

use super::McpAdapter;
use super::seam::{ProjectKey, ProjectSlug, RouteError, StoreResolver};
use crate::infra::session::SessionRegistry;
use crate::server::{PendingEntriesAnalysis, UnimatrixServer};
use crate::services::ServiceLayer;

/// One registered slug's runtime entry (vnc-034 Wave 2, FR-C3).
///
/// Built once at startup and held in the resolver's map. Both fields are the
/// slug's OWN isolated resources: the `store` (sole write capability, FR-X3) and
/// an `McpAdapter` over a `UnimatrixServer` built on that store + the slug's own
/// vector index / hash chain / analytics dir. No sharing across entries.
#[derive(Clone)]
pub(crate) struct ProjectEntry {
    /// Sole write capability for this slug (FR-X3). Held so `resolve_store` can
    /// hand back the per-slug `Arc<Store>` and the `SlugRouter` funnel can assert
    /// resolve/dispatch agreement against `adapter`.
    store: Arc<Store>,
    /// Per-slug MCP dispatcher (the SOLE dispatch route for this key).
    adapter: McpAdapter,
    /// Per-slug session registry (vnc-046 ADR-001). `Arc::clone`d off `server` in
    /// `from_server` BEFORE it moves into `McpAdapter::new`, so this handle and the
    /// slug's `UnimatrixServer.session_registry` are clones of ONE `Arc` —
    /// `registry_for` hands back the same instance the adapter's server reads
    /// (convergence-by-construction, R-03). The write side (observe) and read side
    /// (cycle-review) meet on one instance per slug.
    session_registry: Arc<SessionRegistry>,
    /// Per-slug pending-entries buffer (vnc-046 ADR-001). Same clone-before-move
    /// convergence as `session_registry`; paired with it on the purge gate.
    pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>,
    /// Per-slug config-driven service layer (vnc-046 ADR-001, P2). Cloned off
    /// `server` (a handful of `Arc::clone`s); `services_for` hands it back per
    /// request so cross-project knowledge reads cannot leak (SR-07).
    services: ServiceLayer,
}

impl std::fmt::Debug for ProjectEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectEntry")
            .field("adapter", &self.adapter)
            .finish_non_exhaustive()
    }
}

impl ProjectEntry {
    /// Build a `ProjectEntry` from an already-assembled per-slug `UnimatrixServer`.
    ///
    /// The caller (listener wiring) owns subsystem assembly — opening the slug's
    /// store, vector index, registry, etc. — and passes the resulting `server`
    /// plus the `store` handle it was built over. The `McpAdapter` is constructed
    /// here so callers never need to name the `pub(crate)` `McpAdapter` type.
    ///
    /// `store` MUST be the same `Arc<Store>` the `server` dispatches against —
    /// `McpAdapter::wraps_store` checks this in the funnel's `debug_assert!`
    /// (OQ-PR-4). Passing a mismatched handle would trip that assertion in debug
    /// builds.
    /// `allowed_hosts` (bug #774) is wired verbatim into rmcp's Host-header gate.
    /// It MUST be non-empty — an empty vec makes rmcp allow ALL hosts (fail-open,
    /// defeats CVE-2026-42559). Source it from `PublicUrl.sans` (always non-empty).
    pub(crate) fn from_server(
        store: Arc<Store>,
        server: UnimatrixServer,
        max_body_bytes: usize,
        allowed_origins: Vec<String>,
        allowed_hosts: Vec<String>,
    ) -> Self {
        // vnc-046 ADR-001/002 — CLONE-BEFORE-MOVE (ordering is load-bearing).
        // `McpAdapter::new` consumes `server`, so the per-slug handles MUST be
        // cloned off it FIRST. These are clones of the SAME `Arc`s the adapter's
        // `UnimatrixServer` reads (convergence-by-construction, R-03) — never
        // re-minted (`SessionRegistry::new()` here would break the whole feature).
        let session_registry = Arc::clone(&server.session_registry);
        let pending_entries_analysis = Arc::clone(&server.pending_entries_analysis);
        let services = server.service_layer().clone();
        let adapter = McpAdapter::new(server, max_body_bytes, allowed_origins, allowed_hosts);
        ProjectEntry {
            store,
            adapter,
            session_registry,
            pending_entries_analysis,
            services,
        }
    }
}

/// The Wave-2 `StoreResolver` (vnc-034 ADR-003). Maps each transport-derived
/// `ProjectKey` to its per-slug `ProjectEntry`; drop-in for `DefaultResolver` at
/// the `SlugRouter::new` call site.
///
/// Stateless after construction: a fixed map built once at boot, no runtime
/// mutation (register/delete restart the server — see registry-cli, Wave 3). The
/// map holds only the `Arc<Store>` + adapter handle; per-slug hot caches live
/// inside each slug's `UnimatrixServer`, not here.
pub struct MultiProjectRouter {
    /// slug -> per-slug entry. The SOLE map (vnc-038 ADR-004 #5083): there is no
    /// default entry and no default arm. Single project is N=1 — one entry in
    /// this same map, no special-case branch. An unregistered slug resolves to
    /// `UnknownProject`, never a fall-through (R-09).
    slugs: HashMap<ProjectSlug, ProjectEntry>,
}

impl std::fmt::Debug for MultiProjectRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiProjectRouter")
            .field("slug_count", &self.slugs.len())
            .finish()
    }
}

/// A per-slug runtime input for [`MultiProjectRouter::from_servers`] (vnc-034
/// Wave 2). Carries the slug, its OWN `Arc<Store>`, and the assembled per-slug
/// `UnimatrixServer` the listener wiring (`build_project_entry`) built over that
/// store + the slug's own vector index / hash chain / analytics. `store` MUST be
/// the handle `server` dispatches against (resolve/dispatch agreement, OQ-PR-4).
///
/// Public so the binary crate's listener wiring can construct it without naming
/// the `pub(crate)` `ProjectEntry`/`McpAdapter` types.
pub struct ProjectServerInput {
    /// The validated slug (route key).
    pub slug: ProjectSlug,
    /// The slug's own store (sole write capability, FR-X3).
    pub store: Arc<Store>,
    /// The assembled per-slug MCP server.
    pub server: UnimatrixServer,
    /// The slug's own vector dump directory (`{base_dir}/{slug}/vector`).
    ///
    /// Carried so the listener wiring can register this slug's `VectorIndex`
    /// (reachable via `server.vector_index()`) for the per-slug shutdown dump
    /// (#823). Without this, the per-slug HNSW index was an in-memory-only
    /// dropped local that was never persisted, silently degrading semantic
    /// search after a restart in multi-project HTTP mode.
    pub vector_dir: std::path::PathBuf,
}

impl std::fmt::Debug for ProjectServerInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectServerInput")
            .field("slug", &self.slug)
            .finish_non_exhaustive()
    }
}

impl MultiProjectRouter {
    /// Construct the unified resolver from the per-slug servers built by the
    /// listener wiring from the validated `[[projects]]` slugs (vnc-038 ADR-004).
    ///
    /// Each input's `McpAdapter` is constructed here, so the binary crate never
    /// names the `pub(crate)` adapter type.
    ///
    /// vnc-038 ADR-004 (#5083) removed the `default_store`/`default_server` params
    /// and the default entry: the resolver is keyed by `ProjectKey::Slug` ONLY.
    /// When `slug_servers` is empty the resolver holds NO entries — every slug
    /// resolves to `UnknownProject` and nothing is servable (boot emits the loud
    /// "register a project to begin", Component 7). Single project is N=1: one
    /// entry in `slugs`, no special-case branch.
    ///
    /// Duplicate slugs are already rejected at config-validate
    /// (`validate_projects_config`); this is a defensive re-check that fails loud
    /// rather than panicking. No `.unwrap()`.
    ///
    /// `allowed_hosts` (bug #774) is wired into every per-slug adapter's rmcp
    /// Host-header gate. It MUST be non-empty (empty = rmcp allow-all fail-open).
    /// Source it from `PublicUrl.sans` (structurally non-empty).
    pub fn from_servers(
        slug_servers: Vec<ProjectServerInput>,
        max_body_bytes: usize,
        allowed_origins: Vec<String>,
        allowed_hosts: Vec<String>,
    ) -> Result<Self, String> {
        let mut slugs = HashMap::with_capacity(slug_servers.len());
        for input in slug_servers {
            if slugs.contains_key(&input.slug) {
                return Err(format!("duplicate slug entry: {}", input.slug));
            }
            let entry = ProjectEntry::from_server(
                input.store,
                input.server,
                max_body_bytes,
                allowed_origins.clone(),
                allowed_hosts.clone(),
            );
            slugs.insert(input.slug, entry);
        }

        Ok(MultiProjectRouter { slugs })
    }
}

impl StoreResolver for MultiProjectRouter {
    /// THE store funnel. Total over `ProjectKey`; map lookup + `Arc::clone` only —
    /// no I/O, no `.unwrap()`, no panic.
    ///
    /// - `Slug(s)` → the per-slug store, or `UnknownProject` for an unregistered
    ///   slug. NEVER falls back to a default or another slug (vnc-038 ADR-004,
    ///   R-09) — there is no default store to leak.
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError> {
        match key {
            ProjectKey::Slug(s) => match self.slugs.get(s) {
                Some(entry) => Ok(Arc::clone(&entry.store)),
                None => Err(RouteError::UnknownProject),
            },
        }
    }

    /// THE SOLE MCP dispatch route. Selects the per-key adapter from the SAME map
    /// `resolve_store` reads, so resolution and dispatch can never diverge.
    /// `None` ONLY for a key that does not resolve (same domain as
    /// `UnknownProject`) — the `SlugRouter` 404s, never a fixed-adapter fallback
    /// (the #4974 guard; no trait default impl). vnc-038 ADR-004: no `Default` arm.
    #[allow(private_interfaces)]
    fn adapter_for(&self, key: &ProjectKey) -> Option<&McpAdapter> {
        match key {
            ProjectKey::Slug(s) => self.slugs.get(s).map(|e| &e.adapter),
        }
    }

    /// Resolve the per-slug session registry from the SAME map `resolve_store`
    /// reads (vnc-046 ADR-001). O(1) lookup + `Arc::clone`; `UnknownProject` for an
    /// unregistered slug. No `.unwrap()`, no panic, no I/O — matches the
    /// `resolve_store` discipline.
    fn registry_for(&self, key: &ProjectKey) -> Result<Arc<SessionRegistry>, RouteError> {
        match key {
            ProjectKey::Slug(s) => match self.slugs.get(s) {
                Some(entry) => Ok(Arc::clone(&entry.session_registry)),
                None => Err(RouteError::UnknownProject),
            },
        }
    }

    /// Resolve the per-slug pending-entries buffer from the same map (vnc-046
    /// ADR-001). O(1) lookup + `Arc::clone`; `UnknownProject` otherwise.
    fn pending_for(
        &self,
        key: &ProjectKey,
    ) -> Result<Arc<Mutex<PendingEntriesAnalysis>>, RouteError> {
        match key {
            ProjectKey::Slug(s) => match self.slugs.get(s) {
                Some(entry) => Ok(Arc::clone(&entry.pending_entries_analysis)),
                None => Err(RouteError::UnknownProject),
            },
        }
    }

    /// Resolve the per-slug service layer from the same map (vnc-046 ADR-001).
    /// `ServiceLayer::clone` is a handful of `Arc::clone`s; `UnknownProject`
    /// otherwise.
    fn services_for(&self, key: &ProjectKey) -> Result<ServiceLayer, RouteError> {
        match key {
            ProjectKey::Slug(s) => match self.slugs.get(s) {
                Some(entry) => Ok(entry.services.clone()),
                None => Err(RouteError::UnknownProject),
            },
        }
    }
}

#[cfg(test)]
mod tests;
