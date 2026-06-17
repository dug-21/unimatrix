# Test Plan — Route Grammar + Unified Resolver

> Components: `crates/unimatrix-server/src/http/router/seam.rs`, `http/router/project_resolver.rs` · Surface: `src/http/router/tests.rs`, `src/http/router/project_resolver/tests.rs`, `tests/project_routing_integration.rs` · Risks: R-07 (Crit), R-09 (Crit), R-10 (High), R-08 (cross-ref) · AC-01, AC-06

## Scope
`parse_project_key` collapses to a single `/v1/{slug}/...` → `ProjectKey::Slug` rule (loud error otherwise). `ProjectKey::Default` removed for served-project routing. `MultiProjectRouter` is the sole `StoreResolver`: drops `default` field, `from_servers` default params, and `Default` arms in `resolve_store`/`adapter_for` — slug-keyed only. **N=2 isolation is mandatory (C-11 / GATE-4).**

## CRITICAL: Existing tests to INVERT (call-site audit, R-07 sc.2 / #2398)
The current `tests/project_routing_integration.rs` ASSERTS the Default arm. These MUST be rewritten, not deleted silently — each asserts the previously-passing path now fails loud (avoid vacuous pass, lesson #4452):
- `test_v1_tools_default_unchanged_with_projects` → invert: `/v1/tools/...` no longer dispatches a servable Default; returns loud error.
- `test_non_v1_path_routes_default` → invert: the `_` arm no longer routes Default; returns loud error.
- `test_default_and_slug_interleaved_no_cross_contamination` → drop the Default leg; keep slug-only interleave.
- KEEP and reuse: `test_two_slugs_route_to_distinct_stores`, `test_slug_a_write_unreadable_from_slug_b`, `test_slug_a_write_does_not_appear_in_slug_b`, `test_unregistered_slug_returns_unknown_project`, `test_n_clients_one_slug_share_store`, `test_dispatch_through_adapter_for_no_fixed_bypass`.

## Unit Test Expectations

### Route grammar single-rule (R-07 sc.1 / AC-01)
- `test_parse_v1_slug_returns_slug` — `/v1/alpha/tools/...` → `Ok(ProjectKey::Slug("alpha"))`.
- `test_parse_v1_slug_observe_returns_slug` — `/v1/alpha/observe` → `Ok(Slug("alpha"))` (observe is a segment under the slug, ADR-003).
- `test_parse_v1_tools_no_longer_default` — `/v1/tools/...` → `Err(RouteError)`, NEVER `Default` (the `(Some("v1"),Some("tools"))→Default` arm removed).
- `test_parse_unmatched_is_loud_error` — any no-slug path → `Err(RouteError)`, NEVER `Ok(Default)` (the `_ => Ok(Default)` fallback removed).
- `test_parse_invalid_slug_rejected_at_edge` — `/v1/UPPER/...`, `/v1/-lead/...`, `/v1/trail-/...`, `/v1/../...` → `RouteError::InvalidSlug` before any filesystem use.

### Resolver Slug-only (R-07 sc.1, R-09 sc.2)
- `test_resolver_has_no_default_arm` — `MultiProjectRouter` exposes no `default` field; `resolve_store`/`adapter_for` have no `Default` match arm (structure/compile-level + behavioral).
- `test_resolve_unregistered_slug_unknown_project` — a valid-grammar but unregistered slug → `RouteError::UnknownProject`, never a default-store fall-through.
- `test_adapter_for_no_fallback` — `adapter_for` keeps NO trait default and NO `Option` fallback returning a boot store (the #4974 guard); unregistered slug is hard `UnknownProject`.
- `test_single_deployment_is_n1` — a one-registered-slug resolver resolves through the same slug-keyed path with no special-case arm (RD-5).

### N=2 cross-pollination proof (R-09 / AC-06 / C-11) — MANDATORY, NOT N=1
- `test_two_slugs_route_to_distinct_stores` (existing, keep) — register A and B; assert each resolves to its own `Arc<Store>`.
- `test_slug_a_write_does_not_appear_in_slug_b` (existing, keep) — a write bound to B is absent from A and vice-versa.
- `test_resolve_dispatch_same_map` — `adapter_for(key)` wraps EXACTLY the store `resolve_store(key)` returned (the existing `wraps_store` debug-assert tie); resolve and dispatch cannot diverge.
- **N=2 prefix-collision edge** — `test_prefix_related_slugs_no_misresolution` — register `proj` and `project`; assert `/v1/proj/...` and `/v1/project/...` resolve to distinct stores (no path-prefix mis-resolution).

### Loud-first-boot at the grammar (R-10 / AC-01) — see also boot-wiring.md
- `test_no_servable_store_for_no_slug` — the unified resolver returns an error (no servable store) for a no-slug path; no silent default.

## Edge Cases
- Slug at the `^[a-z0-9][a-z0-9-]{0,62}$` boundary (max length 63; leading/trailing hyphen rejected).
- Unregistered slug → `UnknownProject` (loud), never default.
- Prefix-related slugs (`proj`/`project`) — no mis-resolution.

## Security (slug at the route-parse edge)
- `ProjectSlug` regex validated BEFORE any filesystem use — no `..`, `/`, no reserved name → no path traversal into `{base}/{slug}/`.
- The resolver only ever maps a validated, registered slug (R-09).

## Coverage Requirement
The cutover is bounded to the served-project HTTP model; the MCP seam is proven unbroken (slug path still resolves); no `Default` consumer is left dangling (call-site audit complete); the resolver has exactly one slug-keyed code path; isolation is proven at **N=2** for MCP — the proof fails against any residual bypass (would pass only at N=1, the #4974 trap). Local preservation is covered separately by local-binding-guard.md (R-13).
