# Test Plan — Observe Route + Handler

> Components: `crates/unimatrix-server/src/http/router.rs` (route), `main.rs` (handler/context) · Surface: `tests/project_routing_integration.rs`, `tests/client_bundle_e2e.rs` + infra-001 harness · Risks: R-02 (Crit), R-09 (Crit), R-12 (Crit) · AC-06, AC-07, AC-08

## Scope
Top-level `/observe` (router.rs:188) REMOVED. Observe becomes `/v1/{slug}/observe` resolved per-request through the SAME `resolve_store` funnel as MCP. The boot-bound `resolve_store(&ProjectKey::Default)` (main.rs:1045-1052) and the `ObserveContext`-holds-single-store construction are DELETED. The handler holds `Arc<dyn StoreResolver>` and resolves per call. `/health` stays top-level (store-independent). **N=2 observe isolation is mandatory (C-11 / GATE-4); a boot-bound or parallel path is the exact #4974 bypass.**

## Apply the #4974 VERIFY-THE-FUNNEL checklist

### Structure / no-ceremony guard (R-02 sc.1)
- `test_observe_context_holds_no_pre_resolved_store` — assert `ObserveContext` has NO pre-resolved `store` field; it holds `Arc<dyn StoreResolver>` and resolves per call.
- `test_no_boot_bound_observe_resolve` — grep/structure guard: assert NO `resolve_store(&ProjectKey::Default)` survives at boot (main.rs:1045) and no discarded `let _store` handle (the #4974 ceremonial-discard signature).
- `test_top_level_observe_route_removed` — assert the top-level `(Method::POST, "/observe")` split is gone; `/v1/{slug}/observe` is the SOLE observe route. `/health` remains top-level.

### N=2 counting-resolver proof (R-02 sc.2 / R-09 sc.1 / AC-06) — MANDATORY, NOT N=1
- `test_observe_consults_resolver_once_per_call_n2` — a recording/counting `StoreResolver`; register A and B; assert each observe request consults the resolver EXACTLY ONCE with the transport-derived `ProjectKey::Slug` and resolves the matching store.
- `test_observe_slug_a_write_isolated_from_b` — observe POST to `/v1/{A}/observe` writes A's store ONLY; `/v1/{B}/observe` writes B ONLY; A's store untouched by a B-bound observe and vice-versa.
- `test_no_parallel_observe_dispatch` (R-02 sc.3) — assert no boot-bound or alternate observe adapter exists beside the funnel; the resolved per-request handle is the SOLE route. A test that FAILS if a parallel/boot-bound path is reintroduced.

### #766 end-to-end closure (R-12 / AC-07 / AC-08) — NON-NEGOTIABLE
- `test_init_ping_observe_returns_200` (AC-07, the #766 repro) — drive `init --bundle <v:2>`; the init-time Ping posts to the bundle's `observe_url` over the real `/v1/{slug}/observe` route → **200** (was 404). Reuse `tests/client_bundle_e2e.rs`.
- `test_runtime_hook_observe_returns_200` (AC-08) — a runtime hook event posts to the same per-slug observe route → **200**, resolving to the bundle's project store.
- `test_both_observe_entry_points_route` — assert BOTH init-Ping and runtime-hook reach the per-slug route through the one funnel; neither uses a separate path (R-12 asymmetry closed).

## Edge Cases
- Observe POST to an UNREGISTERED slug → loud `RouteError::UnknownProject`, never a default store.
- Observe before any project registered (empty `[[projects]]`) → loud, never silent default (cross-ref boot-wiring.md / R-10).

## Integration (infra-001)
- Gap #1: `test_observe_per_slug_route_returns_200` — #766 repro through the live binary (AC-07/08).
- Gap #3: `test_two_projects_observe_isolation_n2` — N=2 observe isolation through the live MCP/HTTP surface (R-02/R-09). Together with the Rust counting-resolver test, discharges GATE-4 for observe.

## Coverage Requirement
Observe isolation proven at **N=2** (not N=1, C-11); the resolved per-request handle is load-bearing — a test fails if a parallel/boot-bound path is reintroduced; BOTH observe entry points (init Ping AND every runtime hook) are proven reachable and correctly routed per-slug (#766 closed by construction).
