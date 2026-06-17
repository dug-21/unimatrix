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
//! vnc-038 ADR-004 (#5083) — the served-project `ProjectKey::Default` is DELETED:
//! one route grammar (`/v1/{slug}/...`), one slug-keyed resolver, no default store
//! and no default arm. Single project is N=1 through the same slug path; a no-slug
//! request is a loud `RouteError`, never a silent default (R-10). Local STDIO/UDS
//! keeps its DIRECT path-hash store binding and NEVER enters this resolver
//! (ADR-006 #5087). The `SlugRouter` layer is `PathRouter`'s per-request MCP edge,
//! so every MCP request flows `parse_project_key -> resolve_store -> dispatch`
//! through this seam.

use std::convert::Infallible;
use std::sync::Arc;

use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body::Body;
use http_body_util::combinators::BoxBody;
use unimatrix_core::Store;

use super::McpAdapter;
use super::observe::json_error_response;

/// Transport-derived project identity (ADR-003 C4 invariant 1; vnc-038 ADR-004).
///
/// Constructible ONLY from the transport — the URL path here. NEVER from a
/// request payload, so a client has no field with which to name another
/// project: mis-targeting is unrepresentable, not merely rejected (FR-X2).
///
/// vnc-038 ADR-004 (#5083) deleted the served-project `Default` variant: there
/// is no default store and no default route. The unified HTTP resolver handles
/// ONLY `Slug` — single project is N=1 through the same slug-keyed path, no
/// special case. A request with no valid slug is a loud `RouteError`, never a
/// silent default store (R-10). Local STDIO/UDS keeps its DIRECT path-hash store
/// binding and NEVER enters this resolver (ADR-006 #5087) — it is not a
/// `ProjectKey` at all.
///
/// Kept as a single-variant enum (not a bare newtype) so the `StoreResolver`
/// trait signature `resolve_store(&ProjectKey)` is unchanged and future keys can
/// be added additively (ADR-004).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectKey {
    /// Cloud/container multi-project slug (`/v1/{slug}/tools/...` MCP or
    /// `/v1/{slug}/observe`). The sole served-project key under the unified
    /// resolver (ADR-004); the resolver returns `UnknownProject` for any
    /// unregistered slug, never a fall-through.
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

    /// Per-key MCP dispatch selection (vnc-034 Wave 2 — funnel-elimination).
    ///
    /// THE SOLE MCP dispatch route. `SlugRouter::route_mcp` dispatches through
    /// this and nothing else — there is no fixed-adapter fallback. Returns
    /// `Some(adapter)` for any key the resolver can resolve, and `None` ONLY for
    /// a key that does not resolve (same domain as `resolve_store`'s
    /// `UnknownProject`); the caller then 404s, it does NOT fall back to a fixed
    /// adapter.
    ///
    /// **No default impl (deliberate).** A `{ None }` default would re-introduce
    /// the Wave-1 bypass: a resolver could resolve a store yet return `None`, and
    /// a caller's fixed-adapter fallback would dispatch it. Requiring every impl
    /// to provide `adapter_for` forces the resolved identity and the dispatch
    /// adapter to come from the SAME map (OQ-PR-9).
    ///
    /// `McpAdapter` is a deliberately-opaque dispatch handle (`pub(crate)`, no
    /// public constructor): external crates inject a resolver and drive HTTP, but
    /// never name or build an adapter (OQ-PR-3). The `private_interfaces` lint is
    /// allowed here because the type is intentionally crate-internal.
    #[allow(private_interfaces)]
    fn adapter_for(&self, key: &ProjectKey) -> Option<&McpAdapter>;
}

/// Store-resolution failure at the routing edge (ADR-003/004).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteError {
    /// The slug parsed but is not registered, OR the path carried no valid slug
    /// (vnc-038 ADR-004). NEVER falls back to a default store — there is no
    /// default store (R-07/R-09/R-10). 404 at the routing edge.
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

/// Parse a request path into a transport-derived `ProjectKey` (vnc-038 ADR-004).
///
/// ```text
/// /v1/{slug}/tools/...   -> ProjectKey::Slug(slug)   (MCP)
/// /v1/{slug}/observe     -> ProjectKey::Slug(slug)   (observe is a segment UNDER the slug, ADR-003)
/// (anything else)        -> Err(RouteError)          (loud; NEVER a default store, R-10)
/// ```
///
/// A single rule: the candidate slug is always the 2nd path segment after `v1`.
/// Both `/v1/{slug}/tools/...` and `/v1/{slug}/observe` carry the slug in segment
/// 2, so observe needs no special arm. The allowlist runs at this edge, BEFORE
/// any path use (R-03 / InvalidSlug).
///
/// vnc-038 ADR-004 (#5083) DELETED the `(v1, tools) -> Default` alias arm and the
/// `_ => Default` backward-compat fallback. `tools` in the slug position now
/// parses as a slug *candidate* (`/v1/tools/...` means "the project whose slug is
/// `tools`"); it is unregisterable because `tools` stays in `RESERVED_SLUGS`, so
/// the resolver returns `UnknownProject` — never a default. Any no-`/v1`-slug
/// path is a loud `UnknownProject` here, never a servable default (AC-01/R-10).
///
/// `/health` and top-level `/observe` never reach this function — `PathRouter`
/// splits `/health`; top-level `/observe` is removed (Component 6). Local
/// STDIO/UDS never calls this function (ADR-006).
pub(crate) fn parse_project_key(path: &str) -> Result<ProjectKey, RouteError> {
    let trimmed = path.trim_start_matches('/');
    let mut segs = trimmed.split('/');
    match (segs.next(), segs.next()) {
        // `/v1/{slug}/...` — a candidate slug in the 2nd segment (covers both
        // MCP `/v1/{slug}/tools/...` and observe `/v1/{slug}/observe`). The
        // allowlist runs at this edge, BEFORE any path use (R-03).
        (Some("v1"), Some(maybe_slug)) => {
            let slug = ProjectSlug::try_from(maybe_slug)?;
            Ok(ProjectKey::Slug(slug))
        }
        // ANYTHING ELSE — loud. No `(v1, tools) -> Default` alias, no
        // `_ => Default` fallback. A no-slug path never resolves a servable
        // project (AC-01 / R-10).
        _ => Err(RouteError::UnknownProject),
    }
}

// ---------------------------------------------------------------------------
// SlugRouter — the single-funnel call site (the C4 seam layer)
// ---------------------------------------------------------------------------

/// Tower-style MCP layer between `PathRouter` and the per-key `McpAdapter`
/// (ADR-003).
///
/// Per request it (1) parses the path into a transport-derived `ProjectKey`,
/// (2) calls `resolve_store(&key)` on the injected `StoreResolver` — THE single
/// funnel that proves identity — and (3) dispatches through the per-key adapter
/// `adapter_for(&key)` returns. The resolver is held as `Arc<dyn StoreResolver>`
/// so Wave 2 swaps `DefaultResolver` for `MultiProjectRouter` at the
/// `SlugRouter::new` call site with NO change to this layer or the route grammar
/// (R-01 scenario 2).
///
/// **Funnel-elimination (vnc-034 Wave 2):** the Wave-1 `let _store` discard and
/// the parallel fixed-adapter dispatch are GONE. The resolved key drives BOTH the
/// store funnel AND adapter selection; `adapter_for` is the SOLE dispatch route.
/// There is no residual fixed `ProjectRouter` that a request could still reach,
/// so with two real stores a slug request can never silently serve the wrong
/// store (the bug Wave 1 could not catch under N=1). Per-slug hot-path routing
/// lives INSIDE the seam (`resolve_store` / `adapter_for`), not a new edge
/// (ADR-003, SR-07).
pub struct SlugRouter {
    /// Injected store-resolution + dispatch funnel. Wave 1 = `DefaultResolver`;
    /// Wave 2 = `MultiProjectRouter`. Same trait, same call site.
    resolver: Arc<dyn StoreResolver>,
}

impl std::fmt::Debug for SlugRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlugRouter")
            .field("resolver", &"Arc<dyn StoreResolver>")
            .finish()
    }
}

impl Clone for SlugRouter {
    fn clone(&self) -> Self {
        SlugRouter {
            resolver: Arc::clone(&self.resolver),
        }
    }
}

impl SlugRouter {
    /// Build a `SlugRouter` over an injected resolver. The Wave 1 <-> Wave 2 swap
    /// happens here: pass a `DefaultResolver` (Wave 1) or a `MultiProjectRouter`
    /// (Wave 2) as `resolver` — nothing else in this layer changes. The resolver
    /// now owns per-key dispatch (`adapter_for`), so there is no separate
    /// fixed-adapter argument.
    pub fn new(resolver: Arc<dyn StoreResolver>) -> Self {
        SlugRouter { resolver }
    }

    /// Route an MCP request through the single resolution funnel.
    ///
    /// Only MCP-bound paths reach here (`PathRouter` already split off `/health`
    /// and `/observe`). Parse -> `resolve_store` (the funnel) -> `adapter_for`
    /// (the SOLE dispatch route) -> dispatch. A parse rejection or an unknown
    /// project becomes a JSON error response — never a panic, never a path join,
    /// never the default store on `UnknownProject`, never a fixed-adapter
    /// fallback (R-01 sc.3 / R-03 / funnel-elimination record).
    pub async fn route_mcp<ReqBody>(
        &mut self,
        request: Request<ReqBody>,
    ) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible>
    where
        ReqBody: Body + Send + 'static,
        ReqBody::Data: Send + 'static,
        ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let key = match parse_project_key(request.uri().path()) {
            Ok(k) => k,
            Err(RouteError::InvalidSlug(_)) => {
                return Ok(json_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid project slug",
                ));
            }
            // vnc-038 ADR-004: `parse_project_key` now yields `UnknownProject`
            // for any no-slug path (the `_ => Default` fallback was deleted), so
            // this arm is reachable — a no-slug request 404s, never a default.
            Err(RouteError::UnknownProject) => {
                return Ok(json_error_response(
                    StatusCode::NOT_FOUND,
                    "unknown project",
                ));
            }
        };

        // THE single funnel — the resolved handle is USED, not discarded. The
        // `Arc<Store>` is the sole write capability (FR-X3); resolving it proves
        // transport-derived identity before any dispatch (FR-X5). Wave-2: the
        // discard (`let _store`) is GONE.
        let store = match self.resolver.resolve_store(&key) {
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

        // THE SOLE dispatch route — the per-key adapter the resolver owns. No
        // fixed-adapter fallback: a key that resolved a store but no adapter is
        // treated as `UnknownProject` (fail closed; unreachable when
        // `resolve_store` succeeded — both read the same per-entry map).
        let mut adapter = match self.resolver.adapter_for(&key) {
            Some(adapter) => adapter.clone(),
            None => {
                return Ok(json_error_response(
                    StatusCode::NOT_FOUND,
                    "unknown project",
                ));
            }
        };

        // resolve/dispatch agreement: the dispatched adapter wraps the SAME store
        // `resolve_store` returned, so resolution and dispatch can never diverge
        // (OQ-PR-4). `store` is consumed here, proving the funnel ran (FR-X5).
        debug_assert!(
            adapter.wraps_store(&store),
            "adapter_for returned an adapter over a different store than resolve_store"
        );
        let _ = &store;

        adapter.handle(request).await
    }
}

// ---------------------------------------------------------------------------
// Route-grammar unit tests (vnc-038 Component 5 — ADR-004, AC-01 / R-07 / R-10)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod grammar_tests {
    use super::{ProjectKey, ProjectSlug, RouteError, parse_project_key};

    /// `/v1/{slug}/tools/...` → `Slug(slug)` (the MCP path).
    #[test]
    fn test_parse_v1_slug_returns_slug() {
        let key = parse_project_key("/v1/alpha/tools/call").expect("valid slug path parses");
        assert_eq!(
            key,
            ProjectKey::Slug(ProjectSlug::try_from("alpha").expect("valid")),
            "/v1/alpha/tools/... must resolve the slug from segment 2"
        );
    }

    /// `/v1/{slug}/observe` → `Slug(slug)` — observe is a segment UNDER the slug
    /// (ADR-003), so the slug is still segment 2; no special arm needed.
    #[test]
    fn test_parse_v1_slug_observe_returns_slug() {
        let key = parse_project_key("/v1/alpha/observe").expect("observe path parses");
        assert_eq!(
            key,
            ProjectKey::Slug(ProjectSlug::try_from("alpha").expect("valid")),
            "/v1/alpha/observe must carry the slug in segment 2 (ADR-003)"
        );
    }

    /// `/v1/tools/...` no longer yields `Default` (the alias arm is DELETED).
    /// `tools` now parses as a slug *candidate*; it is unregisterable (reserved),
    /// so it 404s at the resolver — never a default store.
    #[test]
    fn test_parse_v1_tools_no_longer_default() {
        let key = parse_project_key("/v1/tools/call").expect("tools parses as slug candidate");
        assert_eq!(
            key,
            ProjectKey::Slug(ProjectSlug::try_from("tools").expect("valid charset")),
            "/v1/tools/... must parse `tools` as a slug candidate, NEVER a Default alias"
        );
    }

    /// Any no-slug path is a loud `RouteError`, NEVER `Ok(Default)`. The
    /// `_ => Ok(Default)` backward-compat fallback is DELETED (AC-01 / R-10).
    #[test]
    fn test_parse_unmatched_is_loud_error() {
        for path in ["/", "/v1", "/v2/alpha/tools", "/foo/bar", "/health", ""] {
            let err = parse_project_key(path)
                .expect_err("a no-/v1-slug path must be a loud error, never Default");
            assert_eq!(
                err,
                RouteError::UnknownProject,
                "no-slug path {path:?} must be UnknownProject, never a servable default"
            );
        }
    }

    /// `/v1` with no second segment → loud error (no slug to resolve).
    #[test]
    fn test_parse_v1_only_no_slug_is_error() {
        let err = parse_project_key("/v1").expect_err("/v1 alone has no slug");
        assert_eq!(err, RouteError::UnknownProject);
    }

    /// Invalid slugs are rejected at the parse edge (allowlist) BEFORE any
    /// filesystem use — uppercase, leading hyphen, traversal, underscore.
    ///
    /// NOTE: a *trailing* hyphen is NOT invalid: the spec charset is
    /// `^[a-z0-9][a-z0-9-]{0,62}$` (SPECIFICATION.md "Slug"), which only
    /// constrains the FIRST char to `[a-z0-9]` and permits `-` in every
    /// subsequent position — so `trail-` is a valid slug (asserted positively
    /// in `test_parse_trailing_hyphen_slug_accepted`). Path separators / `..`
    /// cannot pass the charset, so traversal is rejected here regardless.
    #[test]
    fn test_parse_invalid_slug_rejected_at_edge() {
        for bad in [
            "/v1/UPPER/tools",
            "/v1/-lead/tools",
            "/v1/under_score/tools",
            "/v1/..",
        ] {
            let err = parse_project_key(bad).expect_err("invalid slug must be rejected at edge");
            assert!(
                matches!(err, RouteError::InvalidSlug(_)),
                "{bad:?} must be InvalidSlug (allowlist) before any path use, got {err:?}"
            );
        }
    }

    /// A trailing hyphen is ACCEPTED by the spec charset
    /// `^[a-z0-9][a-z0-9-]{0,62}$` (only the first char is restricted to
    /// `[a-z0-9]`; `-` is allowed in any later position). Documents the
    /// grammar so the "rejected at edge" case above is not misread as
    /// forbidding trailing hyphens.
    #[test]
    fn test_parse_trailing_hyphen_slug_accepted() {
        let key = parse_project_key("/v1/trail-/tools").expect("trailing-hyphen slug is valid");
        assert_eq!(
            key,
            ProjectKey::Slug(ProjectSlug::try_from("trail-").expect("valid per charset")),
            "a trailing hyphen is permitted by ^[a-z0-9][a-z0-9-]{{0,62}}$"
        );
    }

    /// Prefix-related slugs parse to DISTINCT slugs (no path-prefix mis-parse):
    /// `/v1/proj/...` and `/v1/project/...` are different keys.
    #[test]
    fn test_parse_prefix_related_slugs_distinct() {
        let proj = parse_project_key("/v1/proj/tools").expect("proj parses");
        let project = parse_project_key("/v1/project/tools").expect("project parses");
        assert_ne!(
            proj, project,
            "prefix-related slugs must parse to distinct keys (no prefix mis-resolution)"
        );
    }
}
