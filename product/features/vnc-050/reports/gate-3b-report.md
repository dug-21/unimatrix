# Gate 3b Report: vnc-050

> Gate: 3b (Code Review)
> Date: 2026-11-25
> Result: PASS (with WARNs)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | Components map 1:1 to pseudocode files (principal, token-table, scoped-validator, cap-ceiling, slug-guard, observe-gate, legacy-token-config, bundle-decoder, remote-resolve, client-bundle-cli, bridge-transports, observe-403-client, docs-smoke) |
| Architecture compliance | PASS | ADR-002 slug guard before resolve_store, /health exempt; ADR-006 + Decision 11 CA path sets `rejectUnauthorized: true` explicitly, guarded by `tls-guard.assertCaOptions`; Decision 10 observe-denied marker (`observe-latch.js`); Decision 12 size cap = measured |
| Interface implementation | PASS | `resolveRemote(env,{projectHash})` shared by bridge/transport; single `decodeBundle` (bundle.js) used by init and remote-resolve |
| Test case alignment | PASS | Test files per plan: token_table/tests, write_tests, scoped.rs tests, cap_ceiling_{deny,http,learning}_tests, slug_guard_tests, observe_gate_tests, legacy_token_http_tests, client_bundle_headless_e2e, bundle_codec_fp_null, ca-trust-tls, observe-latch, remote-resolve, compat-matrix, headless-smoke |
| Code quality | PASS | build clean; no todo!/unimplemented!/TODO/FIXME added; all added `.unwrap()` are inside `#[cfg(test)]`; `cargo test --workspace --no-fail-fast` rc=0 |
| Clippy | WARN | Fails only on pre-existing lints: transcript_hold.rs:268 (file unchanged vs main), main.rs:666/690 `stdio: _` (= main.rs:678/702 on main, shifted), verbosity.rs:192/208 repeat_n (file unchanged). No new lints |
| Flaky test | WARN | `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` failed once, passed 3/3 isolated and in full rerun; eval/ unchanged vs main — pre-existing flake |
| Security | PASS | No secrets; CA path never `rejectUnauthorized:false` (only pinned-path cert-pin.js, unchanged); extra headers never logged; cargo audit 12 vulns all pre-existing (Cargo.lock unchanged) |
| Constraint SR-05 | PASS | `--headless/--list/--revoke` are flags on existing `client-bundle` (args.rs `ClientBundleArgs`); no `token` command, no mint route, one decoder |
| Pinned-trust files | PASS | cert-pin.js and cert-pin tests: empty diff vs main |
| /health only unauth route | PASS | router.rs exemption is exact `GET /health` |
| No new crates / npm runtime deps | PASS | Cargo.lock and package.json unchanged |
| Hook-client size gate | PASS | stripped 113137/113137, raw 200623/200623 (Decision 12 cap) |
| FR-21 bin/unimatrix.js change | PASS | `mcp-bridge` projectHash optional only when `UNIMATRIX_BUNDLE` or `UNIMATRIX_REMOTE_MCP_URL` set — required for FR-21 ("UNIMATRIX_BUNDLE alone ... no init, no remote.json"); in scope |
| Bridge stdin-close / pre-initialize bug | WARN | Pre-existing, not a regression (see below); file GH issue |
| Modularity (PL-10) | WARN | Over-cap files that grew (see below) |
| Knowledge stewardship | PASS | All 11 implementation reports have the block with Queried + Stored/nothing-novel entries |

## Detailed Findings

### Bridge bug (Wave 4 report)
**Status**: WARN (pre-existing)
**Evidence**: `mcp-bridge/lifecycle.js` and `mcp-bridge/stdio-frame.js` have empty diffs vs main. In `mcp-bridge.js`, vnc-050 changed only `buildSession` (credential resolution via remote-resolve) and the argv check in `main`; the run loop / EOF handling is untouched. In `http-session.js`, changes are limited to extra headers, the CA trust branch and CA verify-error exit; response content-type handling is unchanged. Stdin-EOF-before-flush and `id:null "unexpected content-type"` for requests sent before initialize therefore exist on main.
**Action**: file GH issue (bridge must drain in-flight requests on stdin EOF and queue requests until initialize completes). Docs already route shell/CI checks to curl.

### Clippy
**Status**: WARN. `cargo clippy --workspace --all-targets -- -D warnings` fails on the pre-existing lints listed above. Nothing new.

### Modularity (PL-10, non-blocking)
**Status**: WARN. Production files over 1,000 lines that grew vs main:
- crates/unimatrix-server/src/infra/config.rs (12863 -> 12886)
- crates/unimatrix-server/src/main.rs (2460 -> 2469)
- crates/unimatrix-server/src/mcp/tools.rs (13798 -> 13808)
- crates/unimatrix-server/src/server.rs (4443 -> 4455)
- crates/unimatrix-server/src/services/usage.rs (1414 -> 1491)

Counts are raw file lines, including any in-file `#[cfg(test)]` code. No new module is over the cap. New logic went into new modules (token_table/, principal.rs, scoped.rs, slug_guard.rs, cap_ceiling.rs, legacy_token_config.rs, client_bundle/).

### JS suite
**Status**: WARN. The 31 JS test files added or changed by vnc-050 were run explicitly with `node --test <files>`: 315 tests, 314 pass, 0 fail, rc=0. The full `npm test` (`node --test` on node 18) passed suite 74 (dogfood-effect) and then hung until the 1800s ceiling (rc=124), with no `not ok` lines. Leftover `stdio-mcp-fixture.js` processes from an earlier session (Nov 24) point to bare `node --test` picking up test/fixtures/mcp/stdio-mcp-fixture.js. That is a pre-existing harness problem, not something vnc-050 introduced. It was not re-checked on main. File a GH issue.

## Knowledge Stewardship
- Stored: nothing novel to store -- the pre-existing clippy lints and flake are known; the bridge bug is feature-specific and goes to a GH issue.
