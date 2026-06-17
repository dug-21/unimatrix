# Agent Report: vnc-038-agent-4-tester (Stage 3c — Test Execution)

## Outcome: PASS (with one REWORKABLE finding routed to the Delivery Leader)

## What I did
1. **Inverted the deferred integration tests (lesson #4452 — invert to loud-error, not deleted):**
   - `crates/unimatrix-server/tests/project_routing_integration.rs`: rewrote `wired_router` to the new slug-only `MultiProjectRouter::from_servers(inputs, max_body, origins)` signature (dropped the deleted default store/server). Inverted the three Default-arm tests to assert a loud `unknown project` 404 over the real `SlugRouter` funnel: `test_v1_tools_default_alias_gone_is_loud_404`, `test_non_v1_path_is_loud_404_not_default`, `test_two_slugs_interleaved_no_cross_contamination` (the renamed/inverted interleave). Kept the N=2 two-store write-isolation tests. → **10/10 green.**
   - `crates/unimatrix-server/tests/client_bundle_e2e.rs`: updated to the `v:2` bundle (`mcp_url`/`observe_url`, `v == 2`, no `base_url`); added the required `<slug>` arg to the `client-bundle` subcommand; stderr now asserted to echo both composed URLs. → **4/4 green.**
2. **Landed the N=2 over-the-wire observe isolation proof (R-02/R-09/AC-06, GATE-4)** in `crates/unimatrix-server/src/http/router/tests.rs`: a `RecordingResolver` wrapping the REAL `MultiProjectRouter` over two distinct per-slug servers, a real `ObserveContext`, injected `ResolvedIdentity`, driving `route_observe` for `/v1/alpha/observe` and `/v1/beta/observe`. Asserts each observe resolves ONCE through the funnel with its own transport slug, both reach dispatch (200 Pong), recorded sequence == `[alpha, beta]`. Plus loud-404 legs for unregistered slug and empty-resolver first-boot. **N=1 was NOT accepted.** → 3 new tests green.
3. **Executed:** server lib (4215 passed / 0 failed / 1 ignored, isolated, deterministic); the two Rust integration targets (green); `pytest -m smoke` (23 passed, mandatory gate); `pytest test_protocol test_security` (35 passed); `npm test` (vnc-038 bundle/init/transport surface all green).
4. **Triaged** the full picture per USAGE-PROTOCOL.

## Key Stage-3c findings (act on these)
- **infra-001 is stdio-only.** It spawns `serve --stdio` (local single-project) with ZERO HTTP/TCP surface — it cannot reach `/v1/{slug}/...`, the per-slug observe HTTP route, or the `register` CLI. The vnc-038 cloud-HTTP route-grammar / default-alias changes therefore do NOT route through the harness; the Stage-3a "6 new pytest tests" are not addable there. Each is instead discharged at the Rust integration/lib layer Stage 3b built (mapping in the report). The anticipated "tools/protocol assertion updates from default-alias removal" did NOT materialize — those stdio suites never exercise the HTTP alias. Harness staying green = no-regression gate for stdio.
- **GH#771 (REWORKABLE — Delivery Leader owns):** `packages/unimatrix/test/hook-client/parity-layer2-concurrency.test.js` (2 tests) fails because its vnc-026 helper `real-server.js` probes readiness via the TOP-LEVEL HTTP `POST /observe`, which vnc-038 DELETED (per-slug only) AND now gates the HTTP listener on registered `[[projects]]`. With no project, the binary boots the LOCAL UDS daemon with no HTTP TCP listener (confirmed by release-boot inspection) → 60s readiness timeout. This is a vnc-038-CAUSED regression in DOWNSTREAM test infra OUTSIDE the vnc-038 modification scope (real-server.js is not in the vnc-038 diff). Not xfail'd (it's a node:test JS suite, not the pytest harness, and not a vnc-038-owned file — HARD RULE). Fix: reconcile `real-server.js` to register a project + probe `/v1/{slug}/observe`, or move to UDS transport.
- **Workspace `--workspace` transient:** `eval::corpus::fixtures_tests::test_ac14_scenario_search_returns_non_empty_ranked_list` failed once under cross-crate parallel contention; **passes in isolation** — a known `--workspace` shared-ONNX/parallel flake (rust-workspace.md), not a vnc-038 regression. One `export_integration` link was OOM-killed under swap exhaustion (environment, not code).

## Files I edited (test files + report only — no git)
- `crates/unimatrix-server/tests/project_routing_integration.rs`
- `crates/unimatrix-server/tests/client_bundle_e2e.rs`
- `crates/unimatrix-server/src/http/router/tests.rs` (appended observe N=2 block)
- `product/features/vnc-038/testing/RISK-COVERAGE-REPORT.md`

## AC / Risk status
All AC-01..AC-13 PASS; all R-01..R-15 covered (R-11 superseded). All seven Gate-3c non-negotiables GREEN. Report: `product/features/vnc-038/testing/RISK-COVERAGE-REPORT.md`.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #4452 (invert-to-loud, not vacuous pass), #4974 (N=2 funnel proof), #2398 (call-site audit on signature change), #4781 (Stage-3c triage). All applied.
- Stored: nothing novel to store — the governing patterns (invert-to-loud-error, N=2 recording-resolver funnel proof, harness-scope triage) already exist in Unimatrix; this stage applies them. The one new mechanic (a deployment-model change that gates the HTTP listener on registered projects breaks a downstream readiness probe assuming always-on HTTP `/observe`) is a single-feature reconciliation captured in GH#771, not yet a cross-feature (2+) pattern worth generalizing.
