# Test Plan — ProjectRouter (`StoreResolver` impl) + per-slug routing/isolation

> Component: `crates/unimatrix-server/src/http/router.rs` (the `ProjectRouter`
> `StoreResolver` impl; drop-in swap at `SlugRouter::new`).
> Source: FR-C1, FR-C3, FR-C7, FR-X2/X3/X5; AC-W2-R1, R3, R5; AC-CT-C4, AC-CT-C6.
> Risks: R-01 (Critical, swap), R-04 (seam parity), R-06 (transport 1:1),
> R-10/R-12 (seam preservation / no new surface), R-13.
> Locked refinement (funnel-honesty record): Wave 2 MUST eliminate the Wave-1
> `let _store` discard / residual fixed-adapter path — the single-funnel invariant
> is now load-bearing. See `test_no_residual_fixed_adapter_path` (§A) and
> `test_dispatch_through_adapter_for_no_fixed_bypass` (HTTP integration).
>
> The merged seam (`StoreResolver`, `SlugRouter`, `DefaultResolver`, `PathRouter`)
> is the substrate. Wave 2 adds `ProjectRouter: StoreResolver` resolving
> `ProjectKey::Slug(s)` → the per-slug `Arc<Store>` from a slug→store map, with
> per-slug hot caches, AND `ProjectKey::Default` → the default store unchanged.
> Per ADR-003: per-slug selection lives INSIDE `resolve_store`, NOT a new edge.

---

## Unit test expectations (cargo, extend `http/router/tests.rs` conventions)

### A. R-01 (Critical) — additive swap, single funnel, routing inside the seam

- `test_projectrouter_resolves_slug_to_its_store` — `ProjectRouter` built over a map
  `{alpha → store_a, beta → store_b}`. `resolve_store(&Slug("alpha"))` returns an
  `Arc` to `store_a`; `Slug("beta")` → `store_b`. Assert `Arc::ptr_eq` to the
  intended store (distinct instances).
- `test_projectrouter_unknown_slug_returns_unknown_project` — `resolve_store(&Slug(
  "ghost"))` (not in map) → `Err(RouteError::UnknownProject)`. **Never a panic,
  never a fall-through to the default store** (R-01 sc.3, mirrors `DefaultResolver`).
- `test_projectrouter_default_key_returns_default_store` — `resolve_store(&Default)`
  → the one default store, unchanged (R-04; backward-compat for `/v1/tools/…`).
- `test_projectrouter_swaps_at_slugrouter_callsite` — build `SlugRouter::new(
  Arc::new(project_router), project_router_inner)` with NO change to the
  `SlugRouter` layer or route grammar vs. the `DefaultResolver` wiring. Assert the
  swap compiles and routes (R-01 sc.2). Demonstrates the resolver arg is the sole
  swap point.
- `test_pathrouter_new_takes_resolver_trait_object` — **structural** (per pattern
  #4963): `PathRouter::new` accepts `Arc<dyn StoreResolver>` at the MCP edge. A
  reverted bypass would not take a resolver and would fail this test to compile.
  Guards AC-CT-C4 (no bypass added).
- `test_projectrouter_routing_inside_resolve_store` — assert the slug→store
  selection happens INSIDE `resolve_store` (the funnel), not as a new `PathRouter`/
  `SlugRouter` arm: structural check that no new public routing edge was added; the
  `SlugRouter::route_mcp` body still does `parse → resolve_store → dispatch`
  (R-01 sc.4, ADR-003 SR-07).
- `test_no_residual_fixed_adapter_path` — **the no-bypass funnel test (Wave-1 honesty
  record).** Wave-1 `route_mcp` discarded the resolved store (`let _store`) and
  dispatched through a parallel **fixed** adapter holding the single store. Wave 2
  MUST eliminate that discard path entirely. Assert: (1) `route_mcp` consumes the
  result of `resolve_store`/`adapter_for(key)` — no `let _ =`/`_store` discard of the
  resolved handle; (2) there is NO residual fixed/default-fallback adapter field that
  a request could still reach when a slug resolves. Structural + behavioral: a request
  is serviced by the adapter `adapter_for(key)` returns, never a leftover fixed one.
  (Gate 3a / Stage 3b VERIFY — AC-CT-C4; this is where the single-funnel invariant
  becomes load-bearing, not ceremonial.)

### B. R-04 — seam parity (slug ⟂ path-hash, no cloud-only branch)

- `test_projectrouter_default_path_unchanged` — under `ProjectRouter`, the `Default`
  key resolves identically to `DefaultResolver` over the same store (same `Arc`
  semantics, no I/O per call). Proves the resolver swap does not regress the local/
  cloud-default common path (NFR-10, AC-W1-X2 stays intact in Wave 2).
- `test_slug_never_leaks_into_default_resolution` — `resolve_store(&Default)` ignores
  the slug map entirely; `resolve_store(&Slug(_))` never consults the default store.
  Assert disjoint resolution (A2: path-hash assumption never enters slug mode and
  vice-versa).

### C. R-06 / FR-C7 — N clients : 1 slug, transport-derived identity

- `test_n_clients_one_slug_shared_store` — two resolve calls for `Slug("alpha")`
  return clones of the SAME `Arc<Store>` (`Arc::ptr_eq`), so N clients on one slug
  share state. (Behavioral attribution by `session_id` is asserted at HTTP
  integration — §below.)
- `test_no_payload_project_field` — **structural** (R-06, FR-X2): `ProjectKey` is
  constructed ONLY from the transport path via `parse_project_key`; no request
  payload field names a project. Assert no public constructor of `ProjectKey::Slug`
  takes a payload-derived value. Mis-targeting unrepresentable, not runtime-rejected.

### D. R-10 / R-12 — enterprise seams intact; no new unauth surface (AC-CT-C6, D3)

- `test_auth_scope_transport_seams_intact` — **structural**: `BearerValidator`
  (token authorizes), `TlsConfig` (cert secures), and the slug seam (scopes data)
  are three separate, present interfaces — Wave 2 collapses none. Assert each type/
  trait still exists and the `ProjectRouter` does not fold auth or TLS into slug
  resolution.
- `test_no_per_slug_health_endpoint` — **negative (D3)**: assert NO per-slug HTTP
  health/topology route is added. `PathRouter` still answers `GET /health` only as
  the sole unauthenticated route; `/v1/{slug}/health` (or any per-slug health path)
  is NOT a special arm — it flows into MCP/`UnknownProject`, never a health handler.
- `test_only_health_unauthenticated` — **negative (R-12, AC-W1-S6)**: probe a set of
  paths (`/metrics`, `/v1/alpha/health`, `/projects`, `/slugs`) unauthenticated;
  assert none is an unauthenticated handler beyond `GET /health`. (D3: any per-slug
  health is registry/CLI-side only — see `project-registry-cli.md`.)

---

## HTTP integration test expectations (`tests/project_routing_integration.rs`)

Per-slug routing + isolation are observable ONLY at the HTTP `/v1/{slug}/` edge
(infra-001 cannot reach it — OVERVIEW §4). Build two real `UnimatrixServer`/`Store`
instances over temp dirs, a `ProjectRouter` keyed `{alpha, beta}`, inject via
`PathRouter::new(resolver, project_router, observe_ctx)`. Drive MCP store/search
requests through the path edge using the router-test body helpers (`collect_body`,
`Request::builder`, `BoxBody`).

| Test | AC / Risk | Arrange → Act → Assert |
|------|-----------|------------------------|
| `test_two_slugs_route_to_distinct_stores` | AC-W2-R1 | store entry E via `POST /v1/alpha/…`; store entry F via `/v1/beta/…` → each lands in its own store; `/v1/alpha/` search finds E not F |
| `test_slug_a_write_unreadable_from_slug_b` | AC-W2-R3 | store E via `/v1/alpha/`; identical search via `/v1/beta/` → returns nothing (read isolation) |
| `test_slug_a_write_does_not_appear_in_slug_b` | AC-W2-R3 | after A writes, B's entry count / hash-chain head unchanged (write isolation) |
| `test_v1_tools_default_unchanged_with_projects` | AC-W2-R2, AC-CT-C4 | with `{alpha,beta}` registered, `POST /v1/tools/…` resolves Default store; behavior identical to no-projects |
| `test_unregistered_slug_returns_unknown_project` | R-01 sc.3 | `POST /v1/ghost/…` → 404 `unknown project`; never the default store, never a panic, no store created |
| `test_n_clients_one_slug_share_store` | AC-W2-R5, FR-C7 | two sessions (distinct `session_id`) on `/v1/alpha/`; both see the shared store; assert per-`session_id` attribution preserved on stored entries |
| `test_invalid_slug_path_rejected_at_edge` | AC-W2-R6 | `POST /v1/..%2fetc/…` and `/v1/Alpha/…` → 400 `invalid project slug`; no path join, no store touched |
| `test_dispatch_through_adapter_for_no_fixed_bypass` | AC-CT-C4, R-01 | **no-bypass funnel.** With ≥2 distinct slugs `{alpha,beta}` + Default registered, every per-slug request dispatches through `adapter_for(key)`: a `/v1/alpha/…` request is serviced ONLY by alpha's adapter/store, `/v1/beta/…` ONLY by beta's — never a leftover default/fixed adapter. Assert per-slug writes land in the matching store (cross-check with isolation tests) AND the Default path (`/v1/tools/…`) still routes unchanged. The Wave-1 discard path is gone — proves dispatch correctness, not just seam shape. |

---

## Edge cases

- Concurrent requests to the same slug (≥2 in flight) — no store re-open per
  request; the per-slug `Arc<Store>` is shared (assert via `Arc::ptr_eq` of resolved
  handles across concurrent calls).
- A slug registered but whose store dir was removed out-of-band → resolve surfaces a
  loud error, not a panic (fail-loud, NFR-03). (Coordinate with registry plan.)
- `Default` and `Slug` interleaved in one process — no cross-contamination.

## Notes
- No `.unwrap()` in non-test resolver code; `resolve_store` is total over
  `ProjectKey`.
- Hot-path per-slug caches (ADR-003 Principle #7) must be tick-rebuildable and
  keyed; if a cache is added, assert a cache hit returns the SAME `Arc` as the
  underlying map (no divergence).
