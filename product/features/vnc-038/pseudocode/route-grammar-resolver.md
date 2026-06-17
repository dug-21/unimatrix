# Component 5 — Route Grammar + Unified Resolver (Rust)

**Files:** `crates/unimatrix-server/src/http/router/seam.rs`, `http/router/project_resolver.rs`
**ADR:** ADR-004 (#5083), ADR-003 (#5082), ADR-006 (#5087) · **AC:** AC-01, AC-06, AC-09 · **Risk:** R-07, R-09, R-13

## Purpose

Collapse the route grammar to a single `/v1/{slug}/...` → `Slug` rule (no default), and make `MultiProjectRouter` the sole slug-keyed `StoreResolver` with no `Default` arm. After this component a request with no valid slug is a loud `RouteError`, never a silent default store — the integrity hole is closed by deletion. Local STDIO/UDS never enters this surface (ADR-006).

## A. seam.rs — `ProjectKey` and `parse_project_key`

### `ProjectKey` (MODIFY — remove `Default` for served-project routing)

```
// BEFORE: enum ProjectKey { Default, Slug(ProjectSlug) }
// AFTER:
enum ProjectKey { Slug(ProjectSlug) }
```

Consequence (call-site audit per R-07 / #2398): every `ProjectKey::Default` reference must be removed or reconciled. Enumerate before deleting:
- `parse_project_key` arms (below)
- `MultiProjectRouter::resolve_store` / `adapter_for` `Default` arms (Section B)
- boot `resolve_store(&ProjectKey::Default)` (Component 6/7)
- any test referencing `Default`

> NOTE: a single-variant enum is intentional (ADR-004: single = N=1, no special case). Keep it an enum (not a bare newtype) so the `StoreResolver` trait signature `resolve_store(&ProjectKey)` is unchanged and future keys can be added additively.

### `parse_project_key(path)` (MODIFY — one rule, loud on no-slug)

```
fn parse_project_key(path: &str) -> Result<ProjectKey, RouteError>:
    trimmed = path.trim_start_matches('/')
    segs    = trimmed.split('/')
    match (segs.next(), segs.next()):
        // /v1/{slug}/...  -> candidate slug in the 2nd segment.
        // This covers BOTH /v1/{slug}/tools/... (MCP) and /v1/{slug}/observe (ADR-003):
        // observe is a SEGMENT UNDER the slug, so the slug is still segment 2.
        (Some("v1"), Some(maybe_slug)) =>
            slug = ProjectSlug::try_from(maybe_slug)?        // allowlist at the edge (InvalidSlug)
            Ok(ProjectKey::Slug(slug))
        // ANYTHING ELSE -> loud. No (v1, tools)->Default arm. No _ => Default fallback.
        _ => Err(RouteError::UnknownProject)                 // AC-01: no-slug never resolves a servable project
    // DELETED: (Some("v1"), Some("tools")) => Default     (the default alias)
    // DELETED: _ => Ok(Default)                            (the backward-compat fallback)
```

> `tools` as a 2nd segment now parses as a *slug candidate* (`/v1/tools/...` means "the project whose slug is `tools`"). It is unregisterable because `tools` stays in `RESERVED_SLUGS` (Component 9), so it resolves to `UnknownProject` at the resolver — never a default. `/health` and `/observe` top-level never reach this fn (`PathRouter` splits `/health`; top-level `/observe` is REMOVED — Component 6).

### `SlugRouter::route_mcp` (MINIMAL change — match becomes total over one variant)

```
// route_mcp logic is UNCHANGED in shape: parse -> resolve_store -> adapter_for -> dispatch.
// The parse-error arms already map InvalidSlug->400 and UnknownProject->404; both now
// reachable from parse_project_key (UnknownProject is no longer "unreachable").
// Remove the comment that calls the UnknownProject arm unreachable.
```

## B. project_resolver.rs — `MultiProjectRouter` loses the default

### `MultiProjectRouter` (MODIFY — drop `default` field)

```
// BEFORE: struct MultiProjectRouter { default: Option<ProjectEntry>, slugs: HashMap<ProjectSlug, ProjectEntry> }
// AFTER:
struct MultiProjectRouter { slugs: HashMap<ProjectSlug, ProjectEntry> }
struct ProjectEntry { store: Arc<Store>, adapter: McpAdapter }      // UNCHANGED
// Debug impl: drop has_default field; keep slug_count.
```

### `from_servers` (MODIFY — drop default params; build from slugs only)

```
// BEFORE: from_servers(default_store, default_server, slug_servers, max_body, allowed_origins)
// AFTER:
fn from_servers(slug_servers: Vec<ProjectServerInput>, max_body_bytes, allowed_origins)
    -> Result<Self, String>:
    slugs = HashMap::with_capacity(slug_servers.len())
    for input in slug_servers:
        if slugs.contains_key(&input.slug): return Err("duplicate slug entry: {slug}")  // defensive, no panic
        entry = ProjectEntry::from_server(input.store, input.server, max_body_bytes, allowed_origins.clone())
        slugs.insert(input.slug, entry)
    Ok(MultiProjectRouter { slugs })
    // NO default entry constructed; NO default_store/default_server params.
```

### `resolve_store` / `adapter_for` (MODIFY — slug-only, no Default arm, no fallthrough)

```
impl StoreResolver for MultiProjectRouter:
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>:
        match key:
            ProjectKey::Slug(s) => match self.slugs.get(s):
                Some(entry) => Ok(Arc::clone(&entry.store))
                None        => Err(RouteError::UnknownProject)    // NEVER another slug, NEVER a default (R-09)
        // DELETED: ProjectKey::Default arm

    fn adapter_for(&self, key: &ProjectKey) -> Option<&McpAdapter>:
        match key:
            ProjectKey::Slug(s) => self.slugs.get(s).map(|e| &e.adapter)
        // DELETED: ProjectKey::Default arm. No trait default impl (the #4974 guard stays).
```

> Both methods read the SAME `slugs` map (resolve/dispatch agreement — `SlugRouter`'s `debug_assert!(wraps_store)` is preserved). N=1 is one map entry with no special branch (ADR-004).

## C. `DefaultResolver` (DELETE — ADR-004)

```
- Remove the DefaultResolver type and its with_adapter constructor entirely (search: DefaultResolver).
- It is referenced only in the boot swap (Component 7) and tests; reconcile all (call-site audit, R-07 sc.2).
- HARD BOUNDARY (ADR-006 / R-13): DefaultResolver/Default-arm deletions are HTTP-cloud/container ONLY.
  They MUST NOT reach the local STDIO (main.rs:1158) / UDS (main.rs:859) boot paths — local opens its
  path-hash store directly and never touched DefaultResolver/parse_project_key/ProjectKey::Default.
```

## Data Flow

- IN: request path (MCP `/v1/{slug}/...` or observe `/v1/{slug}/observe`).
- `parse_project_key` → `ProjectKey::Slug(slug)` or `RouteError`.
- `resolve_store(&Slug)` → `Arc<Store>` for the slug, or `UnknownProject`.
- OUT: per-slug store handle; no default store reachable on any path.

## Error Handling

- `InvalidSlug(raw)` (allowlist fail) → 400 in `route_mcp`; raw used for diagnostics only, never a path join.
- `UnknownProject` (unregistered slug, or no-slug path) → 404; loud; never a default store (R-07/R-09/R-10).

## Key Test Scenarios (hints)

1. AC-01 (R-07 sc.1): `parse_project_key("/v1/tools/x")` and `parse_project_key("/v1")` no longer yield a servable `Default`; assert `Slug("tools")` (then `UnknownProject` at resolver, reserved) and `UnknownProject` respectively.
2. Resolver: `resolve_store(Slug(unregistered))` → `UnknownProject`, never another slug's store (R-09).
3. N=1 (R-07 sc.3): one registered slug resolves through the slug-keyed map with no special-case branch.
4. N=2 isolation (R-09, MANDATORY — NOT N=1): register A and B; a counting resolver asserts each request consults the resolver once with the transport-derived key; a write bound to B leaves A untouched, for MCP. (Observe N=2 in Component 6.)
5. Prefix-related slugs (`proj` vs `project`) → no path-prefix mis-resolution (edge).
6. Call-site audit: no residual `ProjectKey::Default`, `DefaultResolver`, or `MultiProjectRouter.default` reference compiles (R-07 sc.2).
