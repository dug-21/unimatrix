# Agent Report — vnc-038-agent-2-testplan (Stage 3a, Test Plan Design)

## Deliverables (all under `product/features/vnc-038/test-plan/`)
- `OVERVIEW.md` — test strategy, full risk-to-test mapping, integration harness plan, non-negotiable gates
- `bundle-codec-rust.md` (R-03/R-04, AC-05)
- `bundle-decoder-js.md` (R-03/R-04, AC-05)
- `client-attach-js.md` (R-01/R-12, AC-05/AC-07)
- `hook-transport-js.md` (R-01/R-12, AC-05/AC-08)
- `route-grammar-resolver.md` (R-07/R-09/R-10/R-08, AC-01/AC-06)
- `observe-route.md` (R-02/R-09/R-12, AC-06/AC-07/AC-08)
- `boot-wiring.md` (R-10, AC-01/AC-09)
- `register-cli.md` (R-05/R-06, AC-02/AC-03/AC-04)
- `reserved-slugs.md` (R-08, AC-02/FR-13)
- `token-redaction.md` (R-14, AC-11)
- `local-binding-guard.md` (R-13, AC-10 — load-bearing GATE-2 guard)
- `wave1-cleanups.md` (R-15, AC-12/AC-13)

12 component plans map 1:1 to the IMPLEMENTATION-BRIEF Component Map.

## Critical-coverage demands — all addressed
- **N=2 isolation (C-11/R-02/R-09):** route-grammar-resolver.md + observe-route.md both require TWO registered projects with counting/recording resolver asserting one resolve per request to the matching store; no boot-bound/parallel observe path. Existing `test_two_slugs_route_to_distinct_stores` / `test_slug_a_write_*` reused; new observe-side N=2 + a prefix-collision (`proj`/`project`) edge added. Plus infra-001 gap #3 proves observe N=2 through the live binary.
- **#766 closure E2E (AC-07/AC-08):** observe-route.md + client-attach-js.md (init Ping 200) + hook-transport-js.md (runtime hook 200), both verbatim `observe_url`. infra-001 gap #1 is the harness-level repro.
- **Dumb-client invariant (AC-05/R-01):** client-attach-js + hook-transport-js assert the compose-site set is EMPTY (no `+ "/v1"`, no slug append, no `/observe` append) and verbatim byte-for-byte post.
- **v:2 parity (R-03/R-04):** EXTEND the existing `bundle_codec.rs` corpus + `bundle-golden.json` (regenerated from the Rust oracle) — confirmed real surface; strict-reject matrix mirrored both sides; v:1 hard-cut with actionable message.
- **Local-regression guard (R-13/AC-10):** local-binding-guard.md — STDIO `:1158` / UDS `:859` direct-bind, never `parse_project_key`/resolver/`Default`/bundle; HTTP-only-deletion cross-check.
- **Token redaction (R-14/AC-11):** token-redaction.md — no token substring in first-boot stdout/`tracing`; bundle carries it; deployment-context-gated for local non-regression.
- **Carry-items (R-15):** wave1-cleanups.md — `router.rs` ≤500 (AC-12), `public_url.rs` dead_code removed (AC-13).

## Grounding (real surfaces confirmed)
- Bundle corpus: `crates/unimatrix-server/tests/bundle_codec.rs` + `tests/fixtures/c1c2-parity/bundle-golden.json`; JS consumer `packages/unimatrix/test/remote-client.test.js`.
- N=2 routing: `tests/project_routing_integration.rs` ALREADY proves N=2 for MCP. Flagged the three `Default`-arm tests (`test_v1_tools_default_unchanged_with_projects`, `test_non_v1_path_routes_default`, `test_default_and_slug_interleaved_no_cross_contamination`) that delivery must INVERT (assert loud-error, not Default dispatch) — avoids vacuous pass (lesson #4452) and is the R-07 call-site audit (#2398).

## Integration Suite Plan (infra-001, for Stage 3c)
- Run: `smoke` (mandatory gate), `protocol`, `tools`, `lifecycle`, `security`, `volume`; `edge_cases` as regression sweep.
- 6 NEW tests required (gaps in OVERVIEW): observe-per-slug 200 (#766), first-boot-fails-loud, observe N=2 isolation, `/v1/tools` default-alias-gone, reserved-slug-rejection, first-boot-token-not-in-logs.
- Triage note: the route-grammar rewrite WILL legitimately change existing `tools`/`protocol` harness assertions that assumed the `/v1/tools→Default` alias — those are EXPECTED test-assertion updates (triage class 3), to be documented in Stage 3c, NOT pre-existing-xfail.

## Open Questions
- OQ-3 (`tools` reservation) — reserved-slugs.md pins it reserved + a lock test; one-row change if un-reserved.
- OQ-2 (`token.rs:101` scope) — covered both ways via the local-non-regression assertion; delivery confirms shared vs HTTP-only.
- Harness `register` invocation — if infra-001 can't drive the `register` CLI, the reserved-slug rejection harness test (gap #5) drops to Rust-unit-only; documented.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get — surfaced vnc-038 ADRs #5080 (dumb-client), #5082 (per-slug observe funnel), #5083 (delete-the-default); #4974 (ceremonial-funnel N=2), #4956 (parity-corpus mechanics), #4452 (vacuous-pass gate-fix), #2398 (call-site audit), #4792 (synthetic tokens not provider secrets). All applied to the plans.
- Stored: nothing novel to store — the governing patterns (ceremonial-funnel-at-N=2, parity-corpus atomicity, call-site audit before signature removal, redact-secrets-from-logs, synthetic-token fixtures) already exist in Unimatrix and were applied, not discovered. The "invert an existing Default-arm test rather than delete it" mechanic is a single-feature reconciliation, not yet a cross-feature (2+) generalizable pattern.
