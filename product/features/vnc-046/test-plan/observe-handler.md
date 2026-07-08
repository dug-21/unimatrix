# Test Plan — observe-handler (`route_observe`)

Source: `http/router/handlers.rs`. Resolves registry/pending/services from the already-parsed
`key`, passes them to `dispatch_request`; maps a post-store `*_for` `Err` to `500` (not `404`).
Risks: R-14 (error mapping), R-11 (parse-once), R-09 (per-slug read path).

## Unit Test Expectations

1. **`test_route_observe_resolves_per_slug_state_from_key`** — POST to `/v1/{A}/observe`; assert
   the handler calls `resolve_store` + `registry_for` + `pending_for` + `services_for` with the
   SAME `ProjectKey::Slug("A")` and threads the resolved `&registry`/`&pending`/`&services` into
   `dispatch_request` (not `ObserveContext` fields). This is the write-path threading the census
   cannot see (#5427) — the behavioral suite is the real enforcement (isolation-suite.md).
2. **`test_route_observe_parses_key_once`** (R-11) — the `ProjectKey` is parsed once (Step 0)
   and reused across all four resolver calls; no re-parse per `*_for`.
3. **`test_post_store_star_for_err_maps_to_500_not_404`** (R-14 — Critical mapping) — with a
   store resolvable but a `*_for` forced to `Err` (boot-wiring contradiction, foreclosed by
   ADR-003), assert `route_observe` returns **500**, never **404**, and never panics.
4. **`test_unknown_slug_returns_404_upstream_of_write`** (R-14 / NFR-3) — an unregistered slug
   returns `404 UnknownProject` **before** any registry write; unchanged surface.
5. **`test_invalid_slug_rejected_at_edge`** — malformed/hostile slug → `InvalidSlug`/404 before
   any store or registry access; no path-traversal into store/analytics dir (inherited vnc-034,
   confirm not weakened — security floor).

## Integration Expectations (assembled wiring)

6. The handler is the entry point for **every** behavioral test in isolation-suite.md. All
   INV-T/K/C writes go POST → `route_observe` → `resolver.*_for(&key)` → `dispatch_request` →
   the slug's registry/pending/services. No behavioral test may bypass this by hand-passing a
   handle into `dispatch_request` (R-02 / AC-06 grep-gate).

## Security (R-09)
- The per-slug `services_for` result governs the observe-path briefing/search/compact READ — the
  P2 privacy fix. A regression that reads a global `ServiceLayer` here surfaces a co-hosted
  tenant's knowledge; caught behaviorally by INV-K2 (isolation-suite.md).

## Coverage Trace
| Risk / AC | Test |
|-----------|------|
| R-14 | #3, #4 |
| R-11 | #2 |
| R-09 | #6 + INV-K2 back-stop |
| security (slug) | #5 |
