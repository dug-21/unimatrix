//! C4 isolation seam (vnc-034 ADR-003/004/005) — `ProjectKey` / `ProjectSlug` /
//! `StoreResolver` / `RouteError` + the `SlugRouter` layer.
//!
//! Wave-1 MINIMAL: route grammar + trait + `SlugRouter` layer + the
//! `ProjectSlug` allowlist parse edge. The slug RESOLVER logic
//! (slug -> per-slug `Arc<Store>`) and `ProjectRouter`-as-`StoreResolver` are
//! Wave 2 — NOT implemented here. The Wave 1 <-> Wave 2 boundary IS the
//! `StoreResolver` trait: Wave 2 swaps the injected resolver at the SAME
//! `SlugRouter::new` call site, with no change to `SlugRouter`,
//! `parse_project_key`, `ProjectKey`, or `ProjectSlug`.
//!
//! Documented-but-degenerate-seam note (NFR-09): the seam types and the
//! `SlugRouter` layer are defined and unit-tested in Wave 1 but only WIRED into
//! the listener (`main.rs`) in Sub-wave 3. Until that wiring lands they have no
//! production caller, so this module allows `dead_code` — the items are
//! intentionally present-but-unwired, not abandoned. The allow is removed when
//! Sub-wave 3 inserts `SlugRouter` between `PathRouter` and `McpAdapter`.
#![allow(dead_code)]

use std::convert::Infallible;
use std::sync::Arc;

use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body::Body;
use http_body_util::combinators::BoxBody;
use unimatrix_core::Store;

use super::ProjectRouter;
use super::observe::json_error_response;

/// Transport-derived project identity (ADR-003 C4 invariant 1).
///
/// Constructible ONLY from the transport — the URL path here, or the daemon
/// path-hash for the local UDS install. NEVER from a request payload, so a
/// client has no field with which to name another project: mis-targeting is
/// unrepresentable, not merely rejected (FR-X2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectKey {
    /// Slug-free: the local path-hash store, or the cloud single-project alias
    /// (`/v1/tools/...`, ADR-005). The only key exercised in Wave 1.
    Default,
    /// Cloud multi-project slug (`/v1/{slug}/tools/...`). The route shape exists
    /// in Wave 1 but the Wave-1 resolver returns `UnknownProject` for it;
    /// Wave 2 lights it up additively.
    Slug(ProjectSlug),
}

/// Slug allowlist newtype (ADR-004 / SR-09 / R-03 — fix-before-merge security).
///
/// `TryFrom<&str>` enforces `^[a-z0-9][a-z0-9-]{0,62}$` at the parse edge,
/// BEFORE any filesystem use. Because `../`, encoded separators (`%2f`, `%2e`),
/// absolute paths, `.`/`/`/`\`, whitespace, and uppercase cannot pass the
/// charset, a slug-derived path CANNOT escape `/data/.unimatrix/{slug}/` —
/// escape is structurally impossible, not runtime-rejected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectSlug(String);

impl ProjectSlug {
    /// Borrow the validated slug string. Only constructible via `TryFrom`, so a
    /// `ProjectSlug` value always satisfies the allowlist.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProjectSlug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<&str> for ProjectSlug {
    type Error = RouteError;

    /// Allowlist parse edge: `^[a-z0-9][a-z0-9-]{0,62}$`.
    ///
    /// Lowercase `a-z0-9` plus hyphen; must start alphanumeric; 1..=63 chars.
    /// Forbidden-by-construction (cannot pass the charset): `.`, `/`, `\`, `%`,
    /// whitespace, uppercase, and ANY path separator or percent-encoding thereof
    /// (`../`, `%2f`, `%2e`, absolute paths). Validation lives HERE, before any
    /// path join (R-03). No panic, no `.unwrap()`.
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        // Length: 1..=63. `len()` is byte length; the ASCII-only charset check
        // below guarantees bytes == chars, so this bound is exact.
        if s.is_empty() || s.len() > 63 {
            return Err(RouteError::InvalidSlug(s.to_owned()));
        }

        let mut chars = s.chars();
        // First char must be alphanumeric lowercase (no leading hyphen).
        match chars.next() {
            Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
            _ => return Err(RouteError::InvalidSlug(s.to_owned())),
        }
        // Remaining chars: lowercase alnum or hyphen only.
        for c in chars {
            let ok = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
            if !ok {
                return Err(RouteError::InvalidSlug(s.to_owned()));
            }
        }

        Ok(ProjectSlug(s.to_owned()))
    }
}

/// THE single store-resolution funnel (ADR-003 C4, FR-X1).
///
/// Every read/write in the process resolves a store through this one method.
/// The Wave 1 <-> Wave 2 boundary is this trait: Wave 1 injects a default
/// resolver; Wave 2 injects a slug-aware `ProjectRouter` at the same call site
/// with no interface re-cut.
pub trait StoreResolver: Send + Sync + 'static {
    /// Resolve a transport-derived `ProjectKey` to its store handle. The
    /// returned `Arc<Store>` is the sole write capability threaded from the
    /// routing edge (invariant 2).
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>;
}

/// Store-resolution failure at the routing edge (ADR-003/004).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteError {
    /// The slug parsed but is not registered. Wave 1: ANY `Slug(_)` (the
    /// resolver is inert until Wave 2). Wave 2: an unknown slug. NEVER falls
    /// back to the default store (R-01 scenario 3).
    UnknownProject,
    /// The candidate slug failed the allowlist at the parse edge (R-03). Carries
    /// the rejected input for diagnostics only — never used to build a path.
    InvalidSlug(String),
}

impl std::fmt::Display for RouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteError::UnknownProject => f.write_str("unknown project"),
            RouteError::InvalidSlug(_) => f.write_str("invalid project slug"),
        }
    }
}

impl std::error::Error for RouteError {}

/// Parse a request path into a transport-derived `ProjectKey` (ADR-005, LOCKED).
///
/// ```text
/// /v1/tools/...          -> ProjectKey::Default       (default alias)
/// /v1/{slug}/tools/...   -> ProjectKey::Slug(slug)    (Wave 2 additive; Wave 1 resolver inert)
/// (anything else)        -> ProjectKey::Default       (backward-compat for current MCP paths)
/// ```
///
/// `/v1/tools/...` is matched BEFORE the slug arm, so the reserved literal
/// `tools` in the slug position never becomes a slug. Other reserved words
/// (`health`, `observe`, `v1`) reaching the slug arm pass the charset but, in
/// Wave 1, resolve to `UnknownProject`; refusing to REGISTER reserved slugs is a
/// Wave-2 CLI concern (documented seam constraint, not built here). `/health`
/// and `/observe` never reach this function — `PathRouter` splits them off.
pub(crate) fn parse_project_key(path: &str) -> Result<ProjectKey, RouteError> {
    let trimmed = path.trim_start_matches('/');
    let mut segs = trimmed.split('/');
    match (segs.next(), segs.next()) {
        // `/v1/tools/...` — the default alias. Matched first so `tools` in the
        // slug position is never treated as a slug.
        (Some("v1"), Some("tools")) => Ok(ProjectKey::Default),
        // `/v1/{slug}/...` — a candidate slug in the 2nd segment. The allowlist
        // runs at this edge, BEFORE any path use (R-03).
        (Some("v1"), Some(maybe_slug)) => {
            let slug = ProjectSlug::try_from(maybe_slug)?;
            Ok(ProjectKey::Slug(slug))
        }
        // Non-/v1 MCP paths keep current behavior — default route (backward-compat).
        _ => Ok(ProjectKey::Default),
    }
}

// ---------------------------------------------------------------------------
// SlugRouter — the single-funnel call site (the C4 seam layer)
// ---------------------------------------------------------------------------

/// Tower-style MCP layer between `PathRouter` and `McpAdapter` (ADR-003).
///
/// Per request it (1) parses the path into a transport-derived `ProjectKey`,
/// (2) calls `resolve_store(&key)` on the injected `StoreResolver` — the single
/// funnel — and (3) threads the resolved store into MCP dispatch. The resolver
/// is held as `Arc<dyn StoreResolver>` so Wave 2 swaps `DefaultResolver` for
/// `ProjectRouter` at the `SlugRouter::new` call site with NO change to this
/// layer or the route grammar (R-01 scenario 2).
///
/// **Per-slug hot-path routing lives INSIDE `resolve_store` (Wave 2), not in a
/// new edge here (ADR-003, SR-07).** Wave 1 keeps this layer thin: parse ->
/// resolve -> dispatch. The source-assertion gate (R-01) sees ONE funnel.
pub struct SlugRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    /// Injected store-resolution funnel. Wave 1 = `DefaultResolver`; Wave 2 =
    /// `ProjectRouter`. Same trait, same call site.
    resolver: Arc<dyn StoreResolver>,
    /// Existing MCP dispatch (holds the `McpAdapter` over the same store).
    project_router: ProjectRouter<ReqBody>,
}

impl<ReqBody> std::fmt::Debug for SlugRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlugRouter")
            .field("resolver", &"Arc<dyn StoreResolver>")
            .field("project_router", &self.project_router)
            .finish()
    }
}

impl<ReqBody> Clone for SlugRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn clone(&self) -> Self {
        SlugRouter {
            resolver: Arc::clone(&self.resolver),
            project_router: self.project_router.clone(),
        }
    }
}

impl<ReqBody> SlugRouter<ReqBody>
where
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send + 'static,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    /// Build a `SlugRouter` over an injected resolver and the existing
    /// `ProjectRouter`. The Wave 1 <-> Wave 2 swap happens here: pass a
    /// `DefaultResolver` (Wave 1) or a `ProjectRouter` resolver (Wave 2) as
    /// `resolver` — nothing else in this layer changes.
    pub fn new(resolver: Arc<dyn StoreResolver>, project_router: ProjectRouter<ReqBody>) -> Self {
        SlugRouter {
            resolver,
            project_router,
        }
    }

    /// Route an MCP request through the single resolution funnel.
    ///
    /// Only MCP-bound paths reach here (`PathRouter` already split off `/health`
    /// and `/observe`). Parse -> `resolve_store` -> dispatch. A parse rejection
    /// or an unknown project becomes a JSON error response — never a panic,
    /// never a path join, never the default store on `UnknownProject`
    /// (R-01 sc.3 / R-03).
    pub async fn route_mcp(
        &mut self,
        request: Request<ReqBody>,
    ) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
        let key = match parse_project_key(request.uri().path()) {
            Ok(k) => k,
            Err(RouteError::InvalidSlug(_)) => {
                return Ok(json_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid project slug",
                ));
            }
            // `parse_project_key` only ever yields `InvalidSlug`; this arm keeps
            // the match total without inventing behavior for an unreachable case.
            Err(RouteError::UnknownProject) => {
                return Ok(json_error_response(
                    StatusCode::NOT_FOUND,
                    "unknown project",
                ));
            }
        };

        // THE single funnel. The resolved `Arc<Store>` is the sole write
        // capability (FR-X3). Wave 1: `resolve_store(Default)` returns the one
        // store the `McpAdapter` already holds, so the seam is genuinely
        // EXERCISED (A4 / FR-X5) — served THROUGH the funnel, not around it.
        // Wave-2 per-slug `McpAdapter` selection lives INSIDE `resolve_store`,
        // NOT as a new edge here.
        let _store: Arc<Store> = match self.resolver.resolve_store(&key) {
            Ok(store) => store,
            Err(RouteError::UnknownProject) => {
                return Ok(json_error_response(
                    StatusCode::NOT_FOUND,
                    "unknown project",
                ));
            }
            Err(RouteError::InvalidSlug(_)) => {
                return Ok(json_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid project slug",
                ));
            }
        };

        self.project_router.route_mcp(request).await
    }
}
