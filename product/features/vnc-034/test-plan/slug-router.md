# Test Plan — SlugRouter + StoreResolver / ProjectKey / ProjectSlug (the resolve_store seam)

> `crates/unimatrix-server/src/http/router.rs` (new `SlugRouter` layer + `StoreResolver` trait + `ProjectKey` enum + `ProjectSlug` newtype). **Wave-1 surface only: route-shape parse, the seam call site, `ProjectKey::Slug`→`UnknownProject`, and the `ProjectSlug::TryFrom` parse-edge guard. Wave-2 slug *routing* (per-slug stores, AC-W2-R1/R3) is OUT OF SCOPE.** Lead risks: R-01 (Critical), R-03, R-06, R-13.

## AC-IDs covered
AC-W1-X1 (single funnel), AC-W1-X3 (unrepresentable mis-target), AC-CT-C4 (additive route shape — Wave-1 half), AC-CT-C6 (seam present). R-03 parse-edge guard is Wave-1 even though AC-W2-R6 (routing escape proof) is Wave-2.

---

## Unit / source-grade tests (Rust)

### R-01 (Critical) — single funnel, no bypass
- `test_single_funnel_source_assertion` (AC-W1-X1, FR-X1/X5) — **source-grade**: assert the Wave-1 single store is reachable ONLY via `resolve_store(ProjectKey::Default)`; no call site obtains `Arc<Store>` by another route. Implemented as a source-inspection test (grep/AST over `crates/unimatrix-server/src` for store-handle acquisition outside the seam) — zero bypass call sites is the pass condition. The failure mode is silent corruption, so this is structural, not behavioral.
- `test_resolved_store_is_sole_write_capability` (FR-X3) — the `Arc<Store>` threaded from `SlugRouter` into the `McpAdapter` is the only write handle downstream; no downstream path re-derives a store.
- `test_per_slug_hotpath_inside_seam_method` — per-project hot-path routing lives inside `resolve_store`, not in a new edge layer (SR-07). Structural assertion.

### R-01 — resolver-swap (the Wave1↔Wave2 boundary IS the trait)
- `test_resolver_swap_requires_no_callsite_change` (AC-CT-C4) — replace `DefaultResolver` with a stub `StoreResolver` impl (a fake `ProjectRouter`); assert the `SlugRouter` call site and route grammar compile and behave **unchanged** — only the trait object swapped. Proves Wave 2 injects into a proven seam, not a bypass.
- `test_slug_key_under_default_resolver_returns_unknown_project` — `resolve_store(ProjectKey::Slug(_))` against the Wave-1 `DefaultResolver` returns `RouteError::UnknownProject` — **not a panic, not the default store** (R-01 failure mode).

### R-13 / AC-CT-C4 — route grammar (additive shape)
- `test_route_v1_tools_maps_to_default` — `/v1/tools/...` parses to `ProjectKey::Default`.
- `test_route_v1_slug_tools_parses_to_slug` — `/v1/{slug}/tools/...` parses to `ProjectKey::Slug(slug)` (the shape exists in Wave 1; the resolver is inert → `UnknownProject`).
- `test_v1_slug_inert_until_wave2` — a slug request returns `UnknownProject` (shape present, resolver inert) — proves Wave 2 is additive with no Wave-1 client re-init (SR-05).
- `test_health_observe_routes_unaffected` — `/health` (GET, auth-bypass) and `/observe` (POST) still dispatch as before through the new layer.

### R-03 — slug allowlist parse-edge guard (Wave-1: the guard itself)
> The `ProjectSlug::TryFrom<&str>` allowlist is Wave-1 work — it must reject before Wave 2 can ever route a slug to a path. The full *routing* escape proof (AC-W2-R6) is Wave 2; the *parser* is tested now.
- `test_projectslug_accepts_valid` — `^[a-z0-9][a-z0-9-]{0,62}$`: accept `a`, `my-proj`, a 63-char max, `abc123`.
- `test_projectslug_rejects_traversal_corpus` — reject `../`, `..`, `a/../b`, encoded `%2e%2e`, `%2f`, `a%2fb`, absolute `/etc`, bare `.`, bare `/`, leading `-`, uppercase `Abc`, empty string, 64-char over-length.
- `test_projectslug_validates_before_filesystem` — assert `TryFrom` runs at the parse edge; a rejected slug never reaches a path join (no `data_dir.join(raw)` before validation).
- `test_projectslug_reserved_words` (edge) — `tools`, `health`, `observe`, `v1` as slug values: assert the route grammar cannot confuse a reserved path segment with a slug (these parse as their route, not as a slug), so a slug named like a reserved word is either rejected or unambiguously scoped.

### R-06 — 1:1 at transport, mis-target unrepresentable
- `test_project_identity_has_no_payload_carrier` (AC-W1-X3, FR-X2) — **source assertion**: inspect MCP request types; assert NO field names a project. Identity comes only from the transport (URL slug / path-hash). Unrepresentability, not runtime rejection.
- `test_projectkey_constructed_only_from_transport` — `ProjectKey` is built from the parsed path, never from a deserialized request body. Structural.

### R-10 / AC-CT-C6 — enterprise seam present, degenerate-but-documented
- `test_storeresolver_trait_present_and_documented` — the `StoreResolver` trait + `ProjectKey` + `ProjectSlug` exist as named interfaces (the enterprise extension point), documented-but-degenerate per the `session_key` precedent (NFR-09). Slug-as-scope held as an interface even though Wave-1 only exercises `Default`.

## Edge cases (assigned here)
- Empty / max-length (63) / single-char slug (in `ProjectSlug::TryFrom`).
- Slug == reserved word.
- `/v1/{slug}` request in Wave 1 → `UnknownProject` (not a 500, not default store).

## Concrete assertions
R-01 and R-06 are **structural assertions** (source inspection / type inspection) because the risk is silent — a behavioral test cannot prove "no other route exists". The resolver-swap test compiles a stub impl to prove the seam is the only injection point.
