# Component: ProjectRouter — the Wave-2 `StoreResolver` impl

> Source file: `crates/unimatrix-server/src/http/router.rs` (extends the existing
> `ProjectRouter`; per ADR-003/IMPLEMENTATION-BRIEF the Wave-2 resolver IS `ProjectRouter`).
> To stay under the 500-line/file limit, the Wave-2 resolver logic lands in a NEW submodule
> `crates/unimatrix-server/src/http/router/project_resolver.rs` (mirrors how
> `default_resolver.rs` and `seam.rs` were extracted in Wave 1), re-exported from `router.rs`.
> Requirements: FR-C1, FR-C2, FR-C3, FR-C7, FR-X1, FR-X3, FR-X4, FR-X5; AC-W2-R1/R2/R3,
> AC-CT-C4. LOCKED: D1 grammar (reused), additive seam swap (no Wave-1 re-init).

## Purpose

Populate the merged `StoreResolver` seam with slug routing. `resolve_store(Slug(s))`
maps `s` → the per-slug `Arc<Store>`; `resolve_store(Default)` returns the optional
single default store (the `/v1/tools/...` alias, AC-W2-R2). It is a **drop-in swap at
the same `SlugRouter::new` call site** (ADR-003): Wave 1 injected `DefaultResolver`,
Wave 2 injects `ProjectRouter`. No change to `SlugRouter`, `parse_project_key`,
`ProjectKey`, `ProjectSlug`, or `RouteError`.

## Type-collision resolution (load-bearing design decision — flagged)

Two distinct things are both spelled "ProjectRouter" in the codebase + docs:

| Name | Where | Role |
|------|-------|------|
| `ProjectRouter<ReqBody>` (HTTP) | `router.rs:336`, existing | tower MCP dispatcher; holds `default_server: McpAdapter`; `route_mcp` dispatches. NOT a `StoreResolver`. |
| `ProjectRouter` (resolver) | ARCHITECTURE §7, BRIEF data structures | the Wave-2 `StoreResolver`: `{ default: Option<Arc<Store>>, slugs: HashMap<ProjectSlug, ProjectEntry> }`. |

The Wave-1 `SlugRouter::route_mcp` (seam.rs:255) does two things: (a) `resolve_store`
to prove identity / get the `Arc<Store>` (today **discarded into `let _store`**), then (b)
dispatch via the HTTP `ProjectRouter`'s single fixed `default_server`. With one store that
is harmless, but it means the resolved handle is ceremonial: **AC-W1-X1 proved the seam's
SHAPE, not the funnel** (seam-funnel honesty record, BRIEF). For **per-slug isolation
(AC-W2-R3)** the slug request must dispatch through the **per-slug `McpAdapter`**, so the
resolved identity must select the adapter — and the discard path must be **eliminated**,
not merely supplemented. **Wave 2 is where the single-funnel invariant becomes genuinely
load-bearing.** There must be NO residual fixed-adapter fallback that can still bypass
per-slug dispatch: `adapter_for(&key)` is the SOLE dispatch route after this change.

**Decision (recommended, commits the pseudocode):** the per-slug `McpAdapter` map lives
INSIDE the resolver (ADR-003: "per-slug hot-path routing lives INSIDE the seam method,
not in a new edge"). Concretely:

- `ProjectEntry` carries BOTH the slug's `Arc<Store>` AND its `McpAdapter`.
- The resolver `ProjectRouter` implements `StoreResolver::resolve_store` (the funnel,
  returns the `Arc<Store>` — proves identity, satisfies FR-X1/X3 and the source-grade
  single-funnel assertion).
- The resolver `ProjectRouter` ALSO exposes `adapter_for(&ProjectKey)` which selects the
  per-key `McpAdapter`. `SlugRouter::route_mcp` is updated MINIMALLY to dispatch through
  the resolver's per-key adapter. The Wave-1 line `self.project_router.route_mcp(request)`
  is **REMOVED** — the discard path (`let _store` + fixed-adapter dispatch) goes away
  entirely; the resolver's `adapter_for` is the SOLE dispatch route (see "SlugRouter
  touch" below). The DEFAULT key now ALSO routes through `adapter_for(&Default)` → the
  default entry's adapter, so there is no second dispatch path even for `/v1/tools/...`.

This keeps a SINGLE funnel and a SINGLE edge (no new layer), satisfying R-01 sc.4
("per-slug hot-path routing resides inside the seam method, not a new edge") AND the
seam-funnel honesty record (no residual fixed-adapter fallback). The alternative (resolver
returns only `Arc<Store>`, HTTP `ProjectRouter` grows a slug→adapter map fed separately)
splits identity from dispatch across two types and risks exactly the bypass Wave 2 must
close — **rejected**. Flagged for synthesizer/human confirmation (see "Open questions
OQ-PR-1").

> NOTE for the implementer: the Wave-1 `default_resolver.rs` keeps `StoreResolver` as a
> store-only trait. To let `SlugRouter` dispatch per-key — and to make `adapter_for` the
> SOLE dispatch route — extend the trait with a SECOND method, `adapter_for`, that returns
> the per-key adapter for any key the resolver can resolve. `DefaultResolver` (Wave 1's
> single-store impl) implements it to return its one adapter for `Default` (so the old
> fixed-adapter dispatch becomes an `adapter_for` result, NOT a separate bypass path). See
> "StoreResolver extension" below. The Wave-1 store-resolution contract is unchanged; the
> only addition is that dispatch now flows through the trait, eliminating the discard.

## New types

```rust
/// One registered slug's runtime entry. Built once at startup (from [[projects]]) and
/// held in the resolver's map. Both fields are the slug's OWN isolated resources
/// (FR-C3): store, and an McpAdapter over a UnimatrixServer built on that store + the
/// slug's own vector index / hash chain / analytics dir. No sharing across entries.
struct ProjectEntry {
    store: Arc<Store>,     // sole write capability for this slug (FR-X3)
    adapter: McpAdapter,   // per-slug MCP dispatcher (existing McpAdapter type, router.rs:425)
}

/// The Wave-2 StoreResolver. Drop-in for DefaultResolver at the SlugRouter call site.
pub struct ProjectRouter {                       // NOTE: NOT the generic HTTP ProjectRouter<ReqBody>
    /// The /v1/tools/... default alias store (AC-W2-R2). Some in single+multi mode;
    /// None only if the deployment chooses to disable the default alias (not Wave-2 default).
    default: Option<ProjectEntry>,
    /// slug -> per-slug entry. Empty when [[projects]] absent (backward-compat).
    slugs: HashMap<ProjectSlug, ProjectEntry>,
}
```
> Naming: to avoid the collision with the generic `ProjectRouter<ReqBody>`, the resolver
> type SHOULD be named distinctly in code, e.g. `SlugStoreResolver` or `MultiProjectRouter`,
> while the BRIEF/ARCHITECTURE call it "ProjectRouter". The pseudocode uses `ProjectRouter`
> for fidelity to the design docs; the implementer MUST pick a name that does not shadow
> the existing `ProjectRouter<ReqBody>` and note it. Flagged OQ-PR-2.

## StoreResolver extension (minimal, keeps DefaultResolver unchanged)

```rust
// In seam.rs, extend the trait with a per-key dispatch method. This becomes the SOLE
// dispatch route — there is NO `None`-means-"use-the-caller's-fixed-adapter" escape
// hatch (that was the Wave-1 discard/bypass the funnel record forbids). adapter_for
// returns Some(adapter) for EVERY key the resolver can resolve, and None ONLY for a key
// that does not resolve (mirrors resolve_store's Err(UnknownProject)) — the caller then
// 404s, it does NOT fall back to a fixed adapter.
pub trait StoreResolver: Send + Sync + 'static {
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>;

    /// Per-key MCP dispatch selection (Wave 2). MUST be implemented by every resolver —
    /// NO default impl. Returns Some(adapter) for any resolvable key, None only when the
    /// key is unknown (same domain as resolve_store's UnknownProject). The SlugRouter
    /// dispatches through this and nothing else: there is no fixed-adapter fallback.
    fn adapter_for(&self, key: &ProjectKey) -> Option<&McpAdapter>;
}
```
> **No default impl (deliberate).** Giving `adapter_for` a `{ None }` default would
> reintroduce the bypass: a resolver could resolve a store yet return `None`, and the
> caller's fixed-adapter fallback would dispatch it — the exact ceremonial-funnel hole
> Wave 1 had. Requiring every impl to provide `adapter_for` forces the resolved identity
> and the dispatch adapter to come from the SAME map. `DefaultResolver` is updated to hold
> its single adapter and return it for `Default` (see below) — a small, contained Wave-1
> touch that is still additive (no client re-init).
>
> If exposing `McpAdapter` (`pub(crate)`) on the trait is undesirable, the method returns
> an opaque dispatch handle or is `pub(crate)`. Flagged OQ-PR-3. The intent is fixed: the
> resolver, not a new edge, owns per-slug dispatch selection, and it is the only path.

### `DefaultResolver` Wave-1 touch (to remove the bypass — minimal, additive)

`DefaultResolver` currently holds only the store. To make `adapter_for` the sole dispatch
route, it gains the default `McpAdapter` and returns it for `Default`:

```
struct DefaultResolver { store: Arc<Store>, adapter: McpAdapter }   // + adapter (Wave 2)

impl StoreResolver for DefaultResolver:
    fn resolve_store(&self, key) -> Result<Arc<Store>, RouteError>:
        match key:
            ProjectKey::Default  => Ok(Arc::clone(&self.store))     # UNCHANGED Wave-1 behavior
            ProjectKey::Slug(_)  => Err(RouteError::UnknownProject) # UNCHANGED
    fn adapter_for(&self, key) -> Option<&McpAdapter>:
        match key:
            ProjectKey::Default  => Some(&self.adapter)             # the one adapter, via the trait
            ProjectKey::Slug(_)  => None                            # unknown -> 404 (no fallback)
```
This is still additive (AC-CT-C4): the single-project deployment behaves byte-identically
— `/v1/tools/...` dispatches through the same adapter, just SELECTED via the funnel instead
of via the discarded-store fixed path. The `let _store` discard and the
`self.project_router.route_mcp` call are both removed. Flagged OQ-PR-8 (confirm the
`DefaultResolver` constructor call site in main.rs threads the default adapter).

## New / modified functions

### `ProjectRouter::from_registry` (NEW — constructor, called in main.rs wiring)

```
fn from_registry(
    default_store: Arc<Store>,                 // the boot single store (today's `store`)
    default_adapter: McpAdapter,               // the default McpAdapter (today's ProjectRouter inner)
    slug_entries: Vec<(ProjectSlug, ProjectEntry)>,  // built by the listener wiring, see below
) -> Result<Self, ServerError>:

    slugs = HashMap::new()
    for (slug, entry) in slug_entries:
        # duplicate slugs already rejected at config-validate; defensive re-check, no panic
        if slugs.contains_key(&slug):
            return Err(ServerError::Config(format!("duplicate slug entry: {slug}")))
        slugs.insert(slug, entry)

    Ok(ProjectRouter {
        default: Some(ProjectEntry { store: default_store, adapter: default_adapter }),
        slugs,
    })
```

### `impl StoreResolver for ProjectRouter`

```
fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>:
    match key:
        ProjectKey::Default =>
            match &self.default:
                Some(entry) => Ok(Arc::clone(&entry.store))
                None        => Err(RouteError::UnknownProject)   # default alias disabled
        ProjectKey::Slug(s) =>
            match self.slugs.get(s):
                Some(entry) => Ok(Arc::clone(&entry.store))
                None        => Err(RouteError::UnknownProject)   # unregistered slug
    # NEVER falls back: Slug(unknown) -> UnknownProject, never default; never another slug.
    # Identical no-fallthrough contract as DefaultResolver (R-01 sc.3, AC-W2-R3).
    # Total over ProjectKey. No .unwrap(), no panic, no I/O (map lookup + Arc::clone only).

fn adapter_for(&self, key: &ProjectKey) -> Option<&McpAdapter>:
    match key:
        ProjectKey::Default => self.default.as_ref().map(|e| &e.adapter)
        ProjectKey::Slug(s) => self.slugs.get(s).map(|e| &e.adapter)
```

### `SlugRouter::route_mcp` touch (eliminate the discard path — seam.rs:255)

Today (Wave 1) the method resolves the store into **`let _store` (DISCARDED)** and then
dispatches via the fixed HTTP `ProjectRouter`'s single `default_server`:
```
let _store = self.resolver.resolve_store(&key)?;   // ceremonial — discarded
self.project_router.route_mcp(request).await       // fixed-adapter dispatch (bypasses identity)
```
Wave 2 — **remove BOTH the discard and the fixed-adapter dispatch.** The resolved key
drives BOTH the store funnel AND adapter selection; `adapter_for` is the SOLE dispatch
route. There is NO `self.project_router.route_mcp` fallback left:
```
let store = match self.resolver.resolve_store(&key):     # the funnel — NO LONGER discarded
    Ok(s)  => s,
    Err(UnknownProject) => return 404 json,              # unchanged
    Err(InvalidSlug(_)) => return 400 json,              # unchanged
;
let adapter = match self.resolver.adapter_for(&key):     # SOLE dispatch route
    Some(a) => a,
    None    => return 404 json,    # key resolved no adapter == UnknownProject; NEVER a
                                   # fixed-adapter fallback. (Unreachable when resolve_store
                                   # succeeded — both read the same map — but fail closed.)
;
debug_assert!(adapter wraps `store`);   # resolve/dispatch agreement (OQ-PR-4)
adapter.clone().handle(request).await   # per-key dispatch (Default AND Slug, one path)
```
- **The `let _store` discard is GONE** and **`self.project_router.route_mcp` is GONE from
  this path.** The fixed HTTP `ProjectRouter<ReqBody>` is no longer the dispatcher behind
  the seam — it remains only as whatever non-seam HTTP wiring still legitimately needs it
  (if nothing does, it can be dropped from the `SlugRouter`; flagged OQ-PR-9). This is the
  single-funnel invariant becoming load-bearing (BRIEF seam-funnel record): with two real
  stores, a residual fixed-adapter fallback would silently serve the wrong store — the bug
  Wave 1 could not catch because N=1.
- `DefaultResolver` now returns its adapter via `adapter_for(&Default)`, so single-project
  deployments take the SAME one path — byte-identical behavior, no second dispatch route
  (AC-CT-C4 holds; the difference is structural, not observable).
- `McpAdapter::handle` is the existing dispatch entry (router.rs:466). No new dispatch
  machinery (R-01 sc.4: inside the seam).
- The resolved `store` binding is USED (proves the funnel ran, FR-X5) and the
  `debug_assert!` ties it to the dispatched adapter so resolution and dispatch can never
  diverge — flagged OQ-PR-4 (mirrors the McpAdapter R-01 extension-copy assertion idiom).

### Per-slug entry construction (listener-wiring helper, in main.rs / http_provision)

Building a `ProjectEntry` for a slug means building a per-slug `UnimatrixServer` over the
slug's store + vector index + subsystems, then an `McpAdapter` around it. This reuses the
SAME subsystem-assembly the default server uses (main.rs ~L490–940). To avoid duplicating
500 lines, extract a helper:

```
async fn build_project_entry(
    base_dir: &Path,                  // /data/.unimatrix
    slug: &ProjectSlug,
    cfg: &UnimatrixConfig,            // shared knobs (max_body_bytes, allowed_origins, etc.)
    shared: &SharedSubsystems,        // embed handle, adapt service, registries that ARE process-wide
) -> Result<ProjectEntry, ServerError>:

    data_dir = per_slug_data_dir(base_dir, slug)        # SINGLE path-join site (AC-W2-R6)
    paths    = project_paths_for(data_dir)              # db_path = data_dir/unimatrix.db, vector_dir, ...
    store    = open_store_with_retry(&paths.db_path).await?   # opens (must already exist; see below)
    vindex   = open_or_build_vector_index(&paths.vector_dir)?
    server   = UnimatrixServer::new(store.clone(), vindex, shared..., cfg-derived snapshots)
    adapter  = McpAdapter::new(server, cfg.http.max_request_body_bytes, cfg.http.allowed_origins.clone())
    Ok(ProjectEntry { store, adapter })
```
- `per_slug_data_dir(base, slug)` is the SHARED helper (see OVERVIEW + registry-cli);
  the ONLY place a slug becomes a path. Because `slug: &ProjectSlug` is already
  allowlist-validated (D1), the join cannot escape `/data/.unimatrix/{slug}/` (AC-W2-R6).
- **The store must already exist** (register creates it; ProjectRouter does NOT
  auto-create — C5: never client-/router-auto-created). If `paths.db_path` is missing,
  surface a loud `ServerError::Config("slug '{slug}' in [[projects]] is not registered;
  run `project register {slug}`")` — do NOT create it here. Flagged OQ-PR-5.
- Which subsystems are per-slug vs process-wide (embed model, adapt service) is an
  isolation question: the STORE, vector index, hash chain, analytics MUST be per-slug
  (FR-C3); the embedding MODEL handle MAY be shared (read-only, 87 MB — sharing avoids
  N× model memory, consistent with ADR-003 "1× model memory"). Flagged OQ-PR-6 — the
  implementer must confirm exactly which `UnimatrixServer` fields are per-slug. The
  isolation invariant (no cross-slug read/write of KNOWLEDGE) is satisfied as long as
  `store`, `entry_store`, `vector_store`/`vector_index`, audit, and analytics are
  per-slug; the embedding model is stateless inference and safe to share.

### main.rs wiring swap (the single seam swap site, ~L898)

```
# Build per-slug entries from the validated [[projects]] slugs:
let mut slug_entries = Vec::new();
for slug in validated_project_slugs:                  # from projects-config.md
    let entry = build_project_entry(&paths.base_dir, &slug, &config, &shared).await?;
    slug_entries.push((slug, entry));

# Build the default entry from today's store + the default McpAdapter:
let default_adapter = McpAdapter::new(server.clone(), max_body, allowed_origins.clone());

# THE swap — same call site, ProjectRouter instead of DefaultResolver. The default adapter
# is now OWNED by the resolver (it is the Default key's adapter_for result), NOT held as a
# separate fixed dispatcher behind the seam:
let resolver: Arc<dyn StoreResolver> =
    Arc::new(ProjectRouter::from_registry(Arc::clone(&store), default_adapter, slug_entries)?);

# /observe handle still resolves through the funnel (unchanged):
let served_store = resolver.resolve_store(&ProjectKey::Default)?;

# The HTTP `ProjectRouter::new(...)` is NO LONGER the SlugRouter's dispatch fallback — the
# discard path is removed and dispatch flows through resolver.adapter_for. If PathRouter's
# constructor still takes a `project_router` for non-seam HTTP wiring, keep it ONLY for that;
# it must NOT be reachable as a per-request MCP dispatch fallback. Confirm whether the
# parameter is still needed at all once the seam no longer dispatches through it (OQ-PR-9):
let path_router = PathRouter::new(resolver, observe_ctx);   # project_router fallback dropped if unused
```
> When `[[projects]]` is absent, `slug_entries` is empty ⇒ the resolver has only the
> default ⇒ behavior is byte-identical to Wave-1 for `/v1/tools/...` (now dispatched via
> `adapter_for(&Default)` on the SAME store/adapter) AND for `/v1/{slug}/...` (both →
> `UnknownProject`/404). AC-W2-R2 / AC-CT-C4 hold by construction — no Wave-1 client
> re-init; the change is structural (one dispatch path), not observable.

## State / lifecycle

Stateless after construction (a fixed map built once at boot; no runtime mutation —
register/delete restart the server, see registry-cli). Per-slug hot caches (ADR-003
Principle #7, tick-rebuilt) live INSIDE each slug's `UnimatrixServer`/background tick,
not in this map — out of scope for the routing map itself; the map only holds the
`Arc<Store>` + adapter handle. Flagged OQ-PR-7 (whether Wave 2 must wire the per-slug
background tick for each slug, or that is follow-up).

## Error handling

- `resolve_store(Slug(unknown))` → `RouteError::UnknownProject` → `SlugRouter` returns
  404 JSON (existing seam.rs:285 arm). Never panic, never default fallback (R-01 sc.3).
- `from_registry` duplicate → `ServerError::Config` (loud startup fail).
- `build_project_entry` on a missing/unregistered store → `ServerError::Config`, loud,
  actionable, no auto-create (C5). No `.unwrap()` anywhere.

## Key test scenarios (hints — not the test plan)

1. **AC-W2-R1:** register A and B; request `/v1/a/tools` and `/v1/b/tools`; assert each
   `resolve_store` returns A's vs B's store (`Arc::ptr_eq` against the per-slug handle;
   recall Store has no PartialEq — assert on Arc identity / error path per pattern #4958).
2. **AC-W2-R2 / AC-CT-C4:** empty `slugs`; `resolve_store(Default)` returns the one store
   (Arc identity); `/v1/tools/...` unchanged; `/v1/{slug}/...` → `UnknownProject`. Same
   observable behavior as Wave-1 `DefaultResolver` (no re-init).
3. **AC-W2-R3 isolation:** `resolve_store(Slug(a))` NEVER returns B's or the default
   store; a write through A's adapter is invisible to B's store. No cross handle.
4. **R-01 sc.2 (swap):** replacing `DefaultResolver` with `ProjectRouter` at
   `SlugRouter::new` requires no change to the grammar or the `SlugRouter` layer (compile-
   time: both coerce to `Arc<dyn StoreResolver>`).
5. **R-01 sc.3:** `Slug(unknown)` → `UnknownProject`, not a panic, not the default store.
6. **R-01 sc.4 + funnel elimination (seam-funnel record):** per-key dispatch lives in the
   resolver (`adapter_for`), not a new edge — and there is NO fixed-adapter fallback.
   Source assertions: (a) `let _store` discard is gone (the resolved store is USED);
   (b) `self.project_router.route_mcp` no longer appears in `SlugRouter::route_mcp`;
   (c) one funnel, one dispatch site. Behavioral: the two-store integration test
   (`tests/project_routing_integration.rs`) registers A and B, dispatches a tool through
   `/v1/a/...` that WRITES, and asserts B's store never sees it — a residual fixed-adapter
   fallback would fail this (it cannot fail under N=1, which is why Wave 1 missed it).
7. **AC-W2-R6:** `per_slug_data_dir` is reached only with a validated `ProjectSlug`;
   a traversal string can never construct a `ProjectSlug` (covered by projects-config +
   seam tests), so no entry's dir escapes `/data/.unimatrix/{slug}/`.
8. Store-identity idiom: use `Arc::ptr_eq` for "same store" and `.expect_err(..)` for the
   error path — Store has no PartialEq (pattern #4958), match the Wave-1 seam-test idiom.

## Open questions / gaps (flagged, not guessed)

- **OQ-PR-1 (per-slug dispatch ownership):** the pseudocode commits to the resolver
  owning the per-slug `McpAdapter` map + an `adapter_for` dispatch hook (ADR-003 "inside
  the seam"). Confirm vs. the alternative (HTTP `ProjectRouter<ReqBody>` grows the map).
  Recommendation: resolver-owns (single funnel, no bypass).
- **OQ-PR-2 (type name):** the Wave-2 resolver MUST NOT shadow the existing
  `ProjectRouter<ReqBody>`. Pick `MultiProjectRouter`/`SlugStoreResolver`; docs call it
  "ProjectRouter".
- **OQ-PR-3 (trait visibility):** `adapter_for` exposes `McpAdapter` (`pub(crate)`) on the
  `StoreResolver` trait — decide `pub(crate)` method vs an opaque dispatch handle.
- **OQ-PR-4 (resolve/dispatch agreement):** add a debug assertion that the slug's adapter
  wraps the same store `resolve_store` returned (mirror the McpAdapter R-01 idiom).
- **OQ-PR-5 (no auto-create):** ProjectRouter must NOT create a slug's store; a missing
  registered store fails loud at boot. register (CLI) is the sole creator (C5).
- **OQ-PR-6 (per-slug vs shared subsystems):** confirm which `UnimatrixServer` fields are
  per-slug (store/vector/hash-chain/analytics MUST be) vs shared (embed model MAY be, to
  keep 1× model memory). Isolation invariant requires knowledge resources per-slug.
- **OQ-PR-7 (per-slug background tick):** whether Wave 2 wires a background tick per slug
  or defers tick-driven per-slug hot-cache rebuild to follow-up.
- **OQ-PR-8 (DefaultResolver adapter threading):** the funnel-elimination requires
  `DefaultResolver` to hold its `McpAdapter` and return it via `adapter_for(&Default)`.
  Confirm the `DefaultResolver::new` call site in main.rs threads the default adapter (a
  small, additive Wave-1 touch). This is what lets the discard path be removed without
  changing single-project behavior.
- **OQ-PR-9 (drop the fixed HTTP dispatcher from the seam):** once `adapter_for` is the
  sole dispatch route, `SlugRouter` no longer dispatches through the HTTP
  `ProjectRouter<ReqBody>`. Confirm whether `PathRouter::new` still needs a `project_router`
  param for any non-seam HTTP wiring, or whether it can be dropped entirely. It MUST NOT
  remain reachable as a per-request MCP dispatch fallback (that would re-open the bypass).
