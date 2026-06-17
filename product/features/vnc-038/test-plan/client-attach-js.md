# Test Plan — Client Attach (JS, `init.js`)

> Component: `packages/unimatrix/lib/init.js` · Surface: `packages/unimatrix/test/init.test.js`, `init-integration.test.js`, `init-remote.test.js` · Risks: R-01 (Crit), R-12 (Crit) · AC-05, AC-07

## Scope
After bundle validation, `init` stores `mcp_url` VERBATIM and posts to it byte-for-byte. The two path-composition sites are DELETED: C-1 slug append (`init.js:305` `endpointBase + "/v1/" + options.slug`) and C-2 default append (`init.js:307` `endpointBase + "/v1"`). `--slug` flag retired.

## Unit Test Expectations

### Closed-set deletion / empty-compose invariant (R-01 — load-bearing for SR-01)
- `init has no slug-append composition` — assert (grep/AST invariant test) `init.js` contains NO `+ "/v1/"`, NO `+ "/v1"`, NO `options.slug` path concatenation. The set of compose sites in `init.js` is **empty** post-feature (NFR-01).
- `--slug flag removed` — assert the `--slug` CLI option no longer exists / is not accepted.

### Verbatim store + post (R-01 sc.2)
- `stores mcp_url verbatim` — given a decoded `v:2` bundle, assert the value written to client config equals `bundle.mcp_url` byte-for-byte (no normalization, no trailing-slash mutation, no host substitution).
- `posts MCP to the bundle url byte-for-byte` — capture the outgoing MCP request URL; assert string equality with `bundle.mcp_url`. The ONLY source of the request URL is the validated bundle field, not `base_url + grammar` (R-01 sc.3 regression guard).

### Init-time Ping over the real per-slug observe route (R-12 / AC-07 — #766 repro)
- `init --bundle pings observe_url verbatim` — `init --bundle <v:2>` performs its validation Ping to `bundle.observe_url` exactly (not a re-derived `/observe` append).
- `init Ping returns 200 over /v1/{slug}/observe` — the #766 concrete repro: against a running per-slug route, the init Ping returns **200** (was 404). (End-to-end coordination with observe-route.md; covered at the harness level by infra-001 gap #1.)

## Edge Cases
- A `v:1` bundle reaching `init` → `BundleError` re-issue message (delegated to bundle-decoder-js, but assert `init` surfaces it actionably, not a stack trace).
- `mcp_url` with a path the client must NOT touch (e.g. trailing segments) → posted unchanged.

## Coverage Requirement
The closed set C-1/C-2 is deleted and asserted absent; the verbatim-post invariant holds for MCP; init-time Ping routes to the bundle's `observe_url` over the real per-slug route (R-12 half 1, paired with hook-transport-js half 2).
