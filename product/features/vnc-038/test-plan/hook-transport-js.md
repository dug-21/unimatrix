# Test Plan — Hook Transport (JS, `transport-http.js`)

> Component: `packages/unimatrix/lib/hook-client/transport-http.js` · Surface: existing hook-client transport tests (`packages/unimatrix/test/hook-client/`) · Risks: R-01 (Crit), R-12 (Crit) · AC-05, AC-08

## Scope
Posts observe telemetry to the bundle's `observe_url` VERBATIM. The C-3 path-composition site is DELETED: `transport-http.js:84` `u.pathname.replace(/\/+$/,"") + "/observe"`. No `/observe` append; the URL is the bundle field, untouched.

## Unit Test Expectations

### Closed-set deletion (R-01 — load-bearing for SR-01)
- `transport has no /observe append` — assert (grep/AST invariant test) `transport-http.js` contains NO `+ "/observe"`, no `pathname.replace(...)+"/observe"`, no route grammar. The compose-site set in `transport-http.js` is **empty** (NFR-01).

### Verbatim post (R-01 sc.2)
- `posts observe to observe_url byte-for-byte` — capture the outgoing observe request URL; assert string equality with `bundle.observe_url` (no normalization, no trailing-slash mutation, no `/observe` suffix logic). The only source of the URL is the validated bundle field.

### Runtime hook telemetry over the per-slug route (R-12 / AC-08 — #766 wider blast radius)
- `runtime hook event posts to observe_url and gets 200` — a runtime hook event posts to `bundle.observe_url`; assert the per-slug `/v1/{slug}/observe` route accepts it (**200**) and resolves to the bundle's project store. (End-to-end pairing with observe-route.md; harness coverage = infra-001 gap #1 hook variant.)
- `hook and init Ping use the SAME observe_url verbatim` — assert both observe entry points (init Ping AND every runtime hook) read the same `observe_url`; neither re-derives the route (closes R-12 asymmetry — both halves proven).

## Edge Cases
- Multiple hook events in a session → each posts to the identical `observe_url` (no per-event re-composition).
- `observe_url` already ending in `/observe` from the server → posted unchanged (NOT double-suffixed — proves the append is truly gone).

## Coverage Requirement
C-3 deleted and asserted absent; runtime hook telemetry routes per-slug (200); init-Ping and runtime-hook both use the bundle's `observe_url` verbatim — R-12 asymmetry closed (neither entry point left to a separate, untested path).
