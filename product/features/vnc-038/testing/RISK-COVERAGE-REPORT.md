# Risk Coverage Report: vnc-038

> Stage 3c (Test Execution). Mandatory Project Identity at the Deployment Entrypoint. Two deferred Rust integration targets were inverted to loud-error (not deleted, lesson #4452); the N=2 cross-store isolation proof and the over-the-wire per-slug observe isolation proof are GREEN; the `v:2` bundle e2e is updated to `{mcp_url, observe_url}` (no `base_url`). One PRE-EXISTING-INFRA regression caused by the deployment-model change is filed (GH#771) and surfaced to the Delivery Leader.

## Headline

- **Unit/lib (server crate): 4215 passed, 0 failed, 1 ignored** (deterministic, in isolation — up from the 4212 Gate-3b baseline by the +3 new over-the-wire observe tests).
- **Rust integration targets: GREEN** — `project_routing_integration` (10/10), `client_bundle_e2e` (4/4).
- **Integration smoke (MANDATORY gate): PASS** — `pytest -m smoke` → 23 passed (re-run in isolation; the single first run had 1 server-startup timeout from concurrent-cargo OOM contention, confirmed a flake on clean re-run).
- **Integration suites: protocol + security → 35 passed.**
- **JS bundle/init/transport (vnc-038-owned surface): all GREEN** (v:2 parity, strict-reject, v:1 fail-closed, guard ordering, token-no-leak, empty-compose invariant, verbatim post).
- **N=2 isolation proof (C-11 / GATE-4): DISCHARGED** for BOTH MCP and observe — see R-02/R-09 below. N=1 was NOT accepted.
- **1 GH issue filed (#771)**: a vnc-038-caused regression in DOWNSTREAM (vnc-026) Layer-2 JS test infra — out of vnc-038's modification scope; flagged REWORKABLE for the Delivery Leader.

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Missed client path-composition site (#766 class) | JS `init.js — empty-compose invariant`, `transport-http verbatim` suites; `client_bundle_e2e` verbatim-URL echo | PASS | Full |
| R-02 | Ceremonial observe funnel (#4974 repeat) | `router::tests::test_observe_per_slug_funnel_isolation_n2` (recording resolver, **N=2**); `ObserveContext` holds `Arc<dyn StoreResolver>`, resolves per-call (structural) | PASS | Full |
| R-03 | `v:2` partial-rollout / decode-parity break | `bundle_codec.rs` round-trip + `test_c1_bundle_golden_is_stable`; JS `bundle decode — v:2 parity` + strict-reject; golden corpus is the shared oracle | PASS | Full |
| R-04 | Stale `v:1` hard-cut breakage | `bundle_codec.rs::reject_v1_*`; JS `v:1 fails closed with re-issue message`; `client_bundle_e2e` asserts emitted bundle is `v:2` | PASS | Full |
| R-05 | Genesis-clobber on re-register | `projects.rs` register-lifecycle unit tests (server lib suite) | PASS | Full (unit) |
| R-06 | Partial/corrupt `[[projects]]` write | `projects.rs` atomic-write unit tests; boot re-read (server lib suite) | PASS | Full (unit) |
| R-07 | Delete-default over-reach breaks MCP seam | `seam.rs` grammar tests; `project_resolver/tests.rs` (no Default arm); **INVERTED** `project_routing_integration::test_v1_tools_default_alias_gone_is_loud_404`, `test_non_v1_path_is_loud_404_not_default` | PASS | Full |
| R-08 | Reserved-slug drift | `config.rs` RESERVED_SLUGS unit tests; `tools` pinned reserved (server lib suite) | PASS | Full (unit) |
| R-09 | Cross-pollination at N≥2 | `project_resolver/tests.rs::test_two_slugs_route_to_distinct_stores` (Arc::ptr_eq); `project_routing_integration` two-store write-isolation; `router::tests::test_observe_per_slug_funnel_isolation_n2` (**N=2 observe**) | PASS | Full |
| R-10 | Loud-first-boot regression | `seam.rs::test_parse_unmatched_is_loud_error`; `project_resolver::test_empty_resolver_no_servable_store`; `router::tests::test_observe_empty_resolver_first_boot_is_loud_404`; main.rs loud-boot path | PASS | Full |
| R-11 | (#735 collision) | — SUPERSEDED by fold-in (traceability only) | N/A | N/A |
| R-12 | Init-Ping vs hook observe asymmetry | `client_bundle_e2e` (bundle→decode→verbatim URLs, v:2); JS init posts `observe_url` verbatim (AC-07) + transport posts `observe_url` verbatim (AC-08); `router::tests` observe N=2 reaches dispatch (200 Pong) for the per-slug route | PASS | Full (see note) |
| R-13 | Local routed through resolver | `router::tests` local-direct-binding/resolver-bypass guards; release-boot check confirms empty `[[projects]]` boots LOCAL UDS daemon, NO resolver, NO HTTP listener | PASS | Full |
| R-14 | Token to stdout/logs | `client_bundle_e2e::test_e2e_token_absent_from_stdout_and_stderr`; `client_bundle.rs` render_output unit tests | PASS | Full |
| R-15 | #735 cleanup verification | File-check: `router.rs` = 422 lines (≤500, AC-12); grep: `public_url.rs` has no `allow(dead_code)`/"until wiring lands" (AC-13) | PASS | Full |

## Test Results

### Unit / Lib Tests (`cargo test -p unimatrix-server --lib`, isolated)
- Total: 4216 (4215 passed, 0 failed, 1 ignored)
- New this stage (Stage 3c, over-the-wire observe funnel — `http/router/tests.rs`):
  - `test_observe_per_slug_funnel_isolation_n2` — **N=2** recording-resolver observe proof (each observe resolves ONCE through the funnel with its own transport slug; both reach dispatch → 200 Pong; recorded sequence == `[alpha, beta]`).
  - `test_observe_unregistered_slug_is_loud_404_not_default` — unregistered slug observe → loud 404, funnel consulted once.
  - `test_observe_empty_resolver_first_boot_is_loud_404` — empty `[[projects]]` → every observe is loud 404 (R-10 at the observe entry point).

### Rust Integration Tests
- `project_routing_integration`: 10 passed / 0 failed. Three Default-arm tests INVERTED to loud-error (R-07/AC-01); the `wired_router` harness updated to the slug-only `MultiProjectRouter::from_servers(inputs, max_body, origins)` signature (no default store/server). N=2 two-store write-isolation retained.
- `client_bundle_e2e`: 4 passed / 0 failed. Updated to `v:2` (`mcp_url`/`observe_url`, `v == 2`); subcommand now takes the `<slug>` arg; stderr echoes both composed URLs.
- Full-workspace `cargo test --workspace`: 4214 passed, 1 transient failure (`eval::corpus::fixtures_tests::test_ac14_scenario_search_returns_non_empty_ranked_list`) under cross-crate parallel contention — **passes in isolation (rc=0)**; a known `--workspace` shared-ONNX/parallel flake (rust-workspace.md), NOT a vnc-038 regression (eval/corpus is untouched by route grammar/bundle/observe). One unrelated `export_integration` test-binary link was OOM-killed (`ld signal 9`) under swap exhaustion during a concurrent run — an environment memory event, not a code/test defect.

### Integration Tests (infra-001 pytest harness)
- `pytest -m smoke`: **23 passed** (mandatory gate — PASS, clean isolated re-run).
- `pytest test_protocol.py test_security.py`: **35 passed.**
- **Harness scope note (load-bearing):** infra-001 spawns the binary in **local stdio MCP mode** (`serve --stdio`) — it has NO HTTP/TCP surface and cannot reach `/v1/{slug}/...`, the per-slug observe HTTP route, or the `register` CLI. The vnc-038 changes are CLOUD/CONTAINER-HTTP-only, so the existing stdio suites are UNAFFECTED by the route-grammar / default-alias change (the test plan's anticipated "tools/protocol assertion updates from default-alias removal" did NOT materialize — those suites never exercise the HTTP alias). The harness staying green is therefore a no-regression gate for the stdio path, exactly as expected.

### JS Client Tests (`npm test`, `node --test`)
- vnc-038-owned surface (bundle.js / init.js / transport-http.js): **all GREEN** — v:2 golden-row decode, exact-key strict-reject, v:1 fail-closed + re-issue message, guard ordering, token-never-in-errors (NFR-06), zero-dependency (NFR-08), init verbatim store, empty-compose invariant, transport verbatim post.
- 2 FAILED (pre-existing infra, NOT vnc-038-owned): see GH#771 below.

## Gaps

The OVERVIEW Stage-3a "Integration Harness Plan" proposed 6 new infra-001 (pytest) tests (observe-200, first-boot-loud, observe-N2, default-alias-gone, reserved-slug-reject, token-not-in-logs). **These are NOT addable to infra-001 as designed**: the harness is stdio-only with zero HTTP surface (confirmed in Stage 3c — see scope note above), and per USAGE-PROTOCOL "When NOT to Add Integration Tests," HTTP-only behavior the harness cannot reach is covered at the layer that CAN reach it. Each is discharged at the Rust integration/lib layer instead (the surface Stage 3b built):

| OVERVIEW gap test | Discharged by |
|-------------------|---------------|
| observe per-slug 200 (#766) | `client_bundle_e2e` (v:2 verbatim URLs) + `router::tests::test_observe_per_slug_funnel_isolation_n2` (200 Pong via the real funnel) |
| no-slug first-boot loud | `router::tests::test_observe_empty_resolver_first_boot_is_loud_404`; `project_resolver::test_empty_resolver_no_servable_store` |
| observe isolation N=2 | `router::tests::test_observe_per_slug_funnel_isolation_n2` (recording resolver, N=2) |
| default-alias gone | `project_routing_integration::test_v1_tools_default_alias_gone_is_loud_404`, `test_non_v1_path_is_loud_404_not_default` (INVERTED) |
| reserved-slug reject | `config.rs` RESERVED_SLUGS unit tests (CLI `register` is not reachable from the stdio harness — OQ-3 disposition) |
| token-not-in-logs | `client_bundle_e2e::test_e2e_token_absent_from_stdout_and_stderr` |

No vnc-038 risk (R-01..R-15) is left without coverage.

## GH Issues Filed (Pre-Existing / Out-of-Scope Failures)

- **GH#771 — `[vnc-038] parity-layer2-concurrency.test.js: startRealServer times out`.** The vnc-026 Layer-2 JS test helper `real-server.js` spawns `serve --foreground` and probes readiness via the TOP-LEVEL HTTP `POST /observe`. vnc-038 deleted that route (per-slug only) AND gates the HTTP listener on registered `[[projects]]` — so with no project registered the binary now boots the LOCAL UDS daemon with no HTTP TCP listener (confirmed by release-boot inspection), and the readiness probe times out at 60s. This is a vnc-038-CAUSED regression in DOWNSTREAM test infra that is OUTSIDE the vnc-038 modification scope (real-server.js / the Layer-2 suite are not in the vnc-038 Files-to-Modify and not in the diff). **Flagged REWORKABLE for the Delivery Leader:** reconcile `real-server.js` (register a project + probe `/v1/{slug}/observe`, or move to the UDS transport). The vnc-038-owned observe surface IS proven green at the Rust layer. No `xfail` applied — this is a `node --test` JS suite, not the infra-001 pytest harness, and the files are not vnc-038-owned (HARD RULE: tester edits only owned test files).

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `seam.rs::test_parse_unmatched_is_loud_error`; INVERTED `project_routing_integration::test_v1_tools_default_alias_gone_is_loud_404` / `test_non_v1_path_is_loud_404_not_default` — no-slug / `/v1/tools` resolves nothing servable, loud 404 |
| AC-02 | PASS | `projects.rs` register-lifecycle unit tests — `register <slug>` creates data dir + writes `[[projects]]`, routable after boot re-read; reserved-slug rejection unit-tested |
| AC-03 | PASS | `projects.rs` — instruction print removed, atomic `[[projects]]` write; boot reads it (unit) |
| AC-04 | PASS | `project_resolver/tests.rs` N=2 (two slugs via the same path); `projects.rs` second-stanza unit test |
| AC-05 | PASS | `bundle_codec.rs` round-trip + golden-stability; JS exact-key strict-reject both sides; `client_bundle_e2e` verbatim composed URLs; JS empty-compose invariant |
| AC-06 | PASS | **N=2 proof, BOTH entry points**: MCP — `project_resolver::test_two_slugs_route_to_distinct_stores` + `project_routing_integration` two-store isolation; Observe — `router::tests::test_observe_per_slug_funnel_isolation_n2` (recording resolver, sequence == `[alpha, beta]`, each reaches its own store's dispatch) |
| AC-07 | PASS | `client_bundle_e2e` (bundle→decode→`observe_url` is `…/v1/alpha/observe`, posted verbatim by JS init); `router::tests` observe per-slug route returns 200 Pong (the #766 repro through the real funnel). Live-binary HTTP repro is blocked by the stdio-only harness; covered at the Rust funnel + JS verbatim-post layers |
| AC-08 | PASS | JS `transport-http` posts `observe_url` verbatim (no `/observe` append); `router::tests` runtime-hook-shaped observe (RecordEvent path exercised in the observe handler tests) reaches the per-slug route |
| AC-09 | PASS | `router::tests::test_observe_empty_resolver_first_boot_is_loud_404`; `project_resolver::test_empty_resolver_no_servable_store`; main.rs loud-boot ("register a project to begin") |
| AC-10 | PASS | `router::tests` local-direct-binding/resolver-bypass guards; release-boot check — empty `[[projects]]` boots the LOCAL UDS daemon, never the resolver, no HTTP listener (R-13) |
| AC-11 | PASS | `client_bundle_e2e::test_e2e_token_absent_from_stdout_and_stderr` (token in neither stdout nor stderr; round-trips inside the blob only); `client_bundle.rs` render_output unit tests |
| AC-12 | PASS | `router.rs` = 422 lines (≤500) |
| AC-13 | PASS | `public_url.rs` — no `#![allow(dead_code)]`, no "until wiring lands" comment |

## Non-Negotiable Gates (Gate 3c)

| Gate | Status | Evidence |
|------|--------|----------|
| N=2 isolation proof (C-11/GATE-4) — MCP AND observe | **GREEN** | MCP: `project_resolver` Arc::ptr_eq N=2 + `project_routing_integration` two-store write isolation. Observe: `router::tests::test_observe_per_slug_funnel_isolation_n2` recording resolver. N=1 NOT accepted. |
| #766 closure (AC-07/AC-08) | **GREEN** | `client_bundle_e2e` v:2 verbatim URLs + JS init/transport verbatim post + `router::tests` observe per-slug 200 |
| Dumb-client invariant (AC-05) | **GREEN** | JS empty-compose invariant + verbatim post; `client_bundle_e2e` composed-URL byte-equality |
| v:2 parity (R-03/R-04) | **GREEN** | golden corpus both sides; strict-reject + v:1 fail-closed both sides |
| Local-regression guard (R-13/AC-10) | **GREEN** | local direct-binding guards + release-boot confirmation |
| Token redaction (R-14/AC-11) | **GREEN** | `client_bundle_e2e` stdout/stderr token absence |
| Carry-items (R-15) | **GREEN** | router.rs 422 ≤500; public_url.rs clean |

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #4452 (gate-fix tests must exercise the previously-broken path, not a vacuous pass — applied to the Default-arm inversions), #4974 (prove the funnel at N=2, not N=1 — applied to the observe recording-resolver), #2398 (call-site audit before removing a widely-used signature — applied to the `from_servers` signature change), #4781 (Stage-3c pre-existing-failure triage). All applied.
- Stored: nothing novel to store — the patterns used (invert-to-loud-error, N=2 funnel proof, recording-resolver double, harness-scope triage) already exist in Unimatrix; this stage applies them. The one new mechanic worth noting is captured in GH#771 (a deployment-model change that gates the HTTP listener on registered projects breaks a downstream readiness probe that assumed an always-on HTTP `/observe`) — that is a single-feature reconciliation, not yet a cross-feature (2+) pattern; left as the GH issue for the Delivery Leader rather than a generalized lesson.
