//! `MultiProjectRouter` — the Wave-2 `StoreResolver` (vnc-034 ADR-003/004/005).
//!
//! The Wave-2 drop-in for `DefaultResolver` at the SAME `SlugRouter::new` call
//! site (ADR-003, R-01 sc.2): Wave 1 injects `DefaultResolver`, Wave 2 injects
//! `MultiProjectRouter`. No change to `SlugRouter`, `parse_project_key`,
//! `ProjectKey`, `ProjectSlug`, or `RouteError`.
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
//! `resolve_store(Slug(a))` NEVER returns B's or the default store; an unknown
//! slug is `UnknownProject`, never a fall-through. Each entry's store / vector
//! index / hash chain / analytics are the slug's OWN isolated resources (FR-C3),
//! built per-slug in the listener wiring (`build_project_entry`, main.rs).

use std::collections::HashMap;
use std::sync::Arc;

use unimatrix_core::Store;

use super::McpAdapter;
use super::seam::{ProjectKey, ProjectSlug, RouteError, StoreResolver};
use crate::server::UnimatrixServer;

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
    pub(crate) fn from_server(
        store: Arc<Store>,
        server: UnimatrixServer,
        max_body_bytes: usize,
        allowed_origins: Vec<String>,
    ) -> Self {
        let adapter = McpAdapter::new(server, max_body_bytes, allowed_origins);
        ProjectEntry { store, adapter }
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
    /// The `/v1/tools/...` default-alias entry (AC-W2-R2). `Some` in single+multi
    /// mode; `None` only if a deployment disables the default alias (not the
    /// Wave-2 default).
    default: Option<ProjectEntry>,
    /// slug -> per-slug entry. Empty when `[[projects]]` is absent (backward-compat:
    /// behaves byte-identically to `DefaultResolver` for `/v1/tools/...`).
    slugs: HashMap<ProjectSlug, ProjectEntry>,
}

impl std::fmt::Debug for MultiProjectRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiProjectRouter")
            .field("has_default", &self.default.is_some())
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
}

impl std::fmt::Debug for ProjectServerInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectServerInput")
            .field("slug", &self.slug)
            .finish_non_exhaustive()
    }
}

impl MultiProjectRouter {
    /// Construct the resolver from the default server + the per-slug servers built
    /// by the listener wiring from the validated `[[projects]]` slugs.
    ///
    /// Each input's `McpAdapter` is constructed here, so the binary crate never
    /// names the `pub(crate)` adapter type. `default_store` MUST be the handle
    /// `default_server` dispatches against (OQ-PR-4).
    ///
    /// Duplicate slugs are already rejected at config-validate
    /// (`validate_projects_config`); this is a defensive re-check that fails loud
    /// (a `ConfigError`-style message) rather than panicking. No `.unwrap()`.
    ///
    /// When `slug_servers` is empty (`[[projects]]` absent) the resolver holds
    /// only the default ⇒ `/v1/tools/...` is byte-identical to Wave-1 and any
    /// `/v1/{slug}/...` → `UnknownProject` (AC-W2-R2 / AC-CT-C4). (The single-
    /// project deployment uses `DefaultResolver` directly; this constructor is the
    /// multi-project path.)
    pub fn from_servers(
        default_store: Arc<Store>,
        default_server: UnimatrixServer,
        slug_servers: Vec<ProjectServerInput>,
        max_body_bytes: usize,
        allowed_origins: Vec<String>,
    ) -> Result<Self, String> {
        let default = ProjectEntry::from_server(
            default_store,
            default_server,
            max_body_bytes,
            allowed_origins.clone(),
        );

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
            );
            slugs.insert(input.slug, entry);
        }

        Ok(MultiProjectRouter {
            default: Some(default),
            slugs,
        })
    }
}

impl StoreResolver for MultiProjectRouter {
    /// THE store funnel. Total over `ProjectKey`; map lookup + `Arc::clone` only —
    /// no I/O, no `.unwrap()`, no panic.
    ///
    /// - `Default` → the default entry's store (or `UnknownProject` if the alias
    ///   is disabled).
    /// - `Slug(s)` → the per-slug store, or `UnknownProject` for an unregistered
    ///   slug. NEVER falls back to the default or another slug (R-01 sc.3,
    ///   AC-W2-R3) — identical no-fallthrough contract as `DefaultResolver`.
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError> {
        match key {
            ProjectKey::Default => match &self.default {
                Some(entry) => Ok(Arc::clone(&entry.store)),
                None => Err(RouteError::UnknownProject),
            },
            ProjectKey::Slug(s) => match self.slugs.get(s) {
                Some(entry) => Ok(Arc::clone(&entry.store)),
                None => Err(RouteError::UnknownProject),
            },
        }
    }

    /// THE SOLE MCP dispatch route. Selects the per-key adapter from the SAME map
    /// `resolve_store` reads, so resolution and dispatch can never diverge.
    /// `None` ONLY for a key that does not resolve (same domain as
    /// `UnknownProject`) — the `SlugRouter` 404s, never a fixed-adapter fallback.
    #[allow(private_interfaces)]
    fn adapter_for(&self, key: &ProjectKey) -> Option<&McpAdapter> {
        match key {
            ProjectKey::Default => self.default.as_ref().map(|e| &e.adapter),
            ProjectKey::Slug(s) => self.slugs.get(s).map(|e| &e.adapter),
        }
    }
}

#[cfg(test)]
mod tests;
