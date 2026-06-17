# Gate 3c Report: vnc-038

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-17
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof (R-01..R-15) | PASS | Every risk maps to a passing, substantive test; R-11 superseded-by-fold-in (N/A, traceability only) |
| 2. Test coverage completeness vs Risk-Test-Strategy | PASS | All risk-to-scenario mappings exercised; N=2 isolation GREEN for MCP AND observe |
| 3. Specification compliance (FR-01..17 / AC-01..13) | PASS | All 13 ACs verified against committed tests/code; NFRs satisfied |
| 4. Architecture compliance (ADR-001..008) | PASS | Dumb-client invariant, unified resolver (Slug-only), local direct-binding bypass all confirmed in code |
| 5. Knowledge stewardship | PASS | RISK-COVERAGE-REPORT has `## Knowledge Stewardship` with Queried + "nothing novel" reason |
| Integration smoke (MANDATORY) | PASS | Independently re-run: **24 passed, rc=0** |
| N=2 isolation proof (C-11/GATE-4) | PASS (GREEN) | BOTH MCP and observe; recording resolver, two distinct stores; N=1 not accepted |
| xfail hygiene / no deleted tests | PASS | #771 FIXED in-diff (not xfail'd); Default-arm tests INVERTED not deleted, exercise the broken path |

## Detailed Findings

### Check 1 — Risk Mitigation Proof
**Status**: PASS
**Evidence**: `testing/RISK-COVERAGE-REPORT.md` maps each of R-01..R-15 to named tests with PASS results. I independently read the load-bearing test bodies (not just the report's claims):

- **R-02 / R-09 / AC-06 (ceremonial-funnel, #4974 guard)** — `router::tests::test_observe_per_slug_funnel_isolation_n2` registers **two distinct slugs** (`alpha`, `beta`) with separate stores, wraps a `RecordingResolver`, drives an observe Ping to each, and asserts (a) each reaches its own store's dispatch (`StatusCode::OK` Pong) and (b) the recorded resolve sequence `== vec!["alpha","beta"]` (each observe consulted the funnel exactly once with its own transport-derived slug). This is a genuine N=2 proof, not an N=1-disguised pass. MCP isolation independently proven by `project_resolver::test_two_slugs_route_to_distinct_stores` (Arc::ptr_eq) + `project_routing_integration` two-store write-isolation (`alpha_titles == ["alpha-w"]`, `beta_titles == ["beta-w"]`).
- **R-07 / AC-01 (delete-the-default, #4452 guard)** — the three Default-arm tests are **INVERTED to loud-error assertions, not deleted**: `test_v1_tools_default_alias_gone_is_loud_404` asserts `/v1/tools/mcp` returns `NOT_FOUND` with body containing `"unknown project"` BOTH with and without `[[projects]]`; `test_non_v1_path_is_loud_404_not_default` asserts a non-`/v1` path (formerly the `_ => Default` arm) is loud 404. These exercise the **previously-passing** path that now must fail loud — the #4452 anti-vacuous-pass requirement is met.
- **R-13 / AC-10 (local-binding guard, GATE-2)** — `test_local_path_hash_store_never_enters_the_resolver` asserts a path-hash-shaped slug `resolve_store(...)` returns `RouteError::UnknownProject` — local is **not a resolver key** (ADR-006). The funnel never sees the local path-hash.
- **R-14 / AC-11 (token redaction)** — `client_bundle_e2e::test_e2e_token_absent_from_stdout_and_stderr` asserts the token substring is absent from BOTH stdout and stderr, yet round-trips inside the decoded bundle blob (`bundle.token == SYNTH_TOKEN`). Sole-channel proven.
- **R-03 / R-04 (v:2 parity / v:1 hard-cut)** — golden corpus both sides; JS decoder rejects `obj.v !== 2` with a re-issue message; struct has no `base_url`.
- R-05/R-06/R-08/R-10/R-12/R-15 covered per the report; spot-checked AC-12 (`router.rs` = 422 lines) and AC-13 (`public_url.rs` clean) directly.

R-11 is correctly marked N/A (SUPERSEDED by fold-in, traceability only).

### Check 2 — Test Coverage Completeness
**Status**: PASS
**Evidence**: All Phase-2 risk-to-scenario mappings are exercised. The 8 Critical risks (R-01, R-02, R-03, R-04, R-07, R-09, R-12, R-13) and 5 High risks all have passing coverage. The N=2 mandate (C-11) for R-02 and R-09 is satisfied for BOTH entry points — N=1 was explicitly not accepted.

**Harness-scope discharge (validated sound, not a gap)**: The OVERVIEW Stage-3a plan proposed 6 new infra-001 (pytest) tests. infra-001 spawns the binary in `serve --stdio` mode with **zero HTTP/TCP surface** — it cannot reach `/v1/{slug}/...`, the per-slug observe HTTP route, or the `register` CLI. Per the USAGE-PROTOCOL "When NOT to Add Integration Tests," HTTP-only behavior is covered at the layer that CAN reach it. Each of the 6 was discharged at the Rust integration/lib layer (`client_bundle_e2e` + `router::tests` N=2) — the surface that reaches the actual HTTP funnel. I confirmed those discharging tests are substantive (above). This is a layer-appropriate discharge, not a coverage gap.

### Check 3 — Specification Compliance
**Status**: PASS
**Evidence**: `ACCEPTANCE-MAP.md` AC-01..13 all verified. Spot-checked against committed code/tests:
- AC-05/06/07/08: v:2 bundle `{v, mcp_url, observe_url, token, fp}` (BUNDLE_VERSION=2, no `base_url`); JS `EXPECTED_KEYS` matches exactly; verbatim-post invariant; N=2 dual-entry.
- AC-09/10: loud-first-boot (cloud) + local direct-binding bypass (the SR-04 / C-13 tension resolved by separation).
- AC-11/12/13: token redaction; `router.rs` = 422 ≤ 500; `public_url.rs` no `#![allow(dead_code)]` / "until wiring lands".
- NFR-09 hygiene: no `unsafe`, no production `.unwrap()` in vnc-038-changed non-test code (the grep hits are doc-comment text saying "No `.unwrap()`"; the TODO/unsafe-word markers in `main.rs`/`config.rs` are PRE-EXISTING and untouched, confirmed against `main`).

### Check 4 — Architecture Compliance
**Status**: PASS
**Evidence**: The system matches ARCHITECTURE.md + ADR-001..008. The unified resolver handles only `ProjectKey::Slug` (Default arms deleted, proven by the inverted tests). Observe folded onto the per-request funnel with no boot-bound handle (proven by the N=2 recording-resolver). Local STDIO/UDS bypasses the resolver entirely (ADR-006 direct binding, proven by the bypass guard). Dumb-client invariant (ADR-001): the client posts server-composed URLs verbatim, composes no paths. No architectural drift observed.

### Check 5 — Knowledge Stewardship Compliance
**Status**: PASS
**Evidence**: `testing/RISK-COVERAGE-REPORT.md` contains a `## Knowledge Stewardship` section with `Queried:` entries (context_briefing → #4452, #4974, #2398, #4781, all applied) and a `Stored:` entry with an explicit "nothing novel to store" reason (governing patterns already exist; the #771 deployment-model-change-breaks-readiness-probe mechanic is a single-feature reconciliation captured in the GH issue, not yet a 2+ cross-feature pattern). Reason is present and substantive — not a bare "nothing novel."

### Integration Test Validation (mandatory)
**Status**: PASS
- **Smoke (`pytest -m smoke`)**: independently re-run in this validation — **24 passed, 0 failed, rc=0** (208s). Report claimed 23; the +1 is harness selection, all green, no regression.
- **Relevant suites**: Rust `project_routing_integration` (10/10), `client_bundle_e2e` (4/4, v:2), `router::tests` over-the-wire observe (N=2), protocol+security (35). JS owned surface all green.
- **xfail hygiene**: all `@pytest.mark.xfail` markers in the harness reference GH issues (#111, #405, #406) or are documented pre-existing strict=False cases — none vnc-038-introduced. **#771 was FIXED in-diff, not xfail'd**: `real-server.js` (+139 lines) now registers a slug via the CLI and probes `/v1/{slug}/observe`, reconciling the downstream Layer-2 harness to the per-slug model.
- **No tests deleted/commented out**: the three Default-arm tests were INVERTED to loud-error (verified they exercise the broken path, #4452), not removed.
- **RISK-COVERAGE-REPORT includes integration counts**: yes (lib 4215; integration 10/10, 4/4; smoke 23; protocol+security 35).
- **N=2 (C-11/GATE-4) GREEN for BOTH MCP and observe**: confirmed in code, not just the report.

## Rework Required

None.

## Notes for the Delivery Leader
- The full-workspace `cargo test --workspace` transient failure (`eval::corpus` fixture) and the `export_integration` OOM-link are environment/parallel-contention events on untouched code, not vnc-038 regressions — consistent with `rust-workspace.md`. The lib suite passes deterministically in isolation. Acceptable; noted, not a gate blocker.
- #771's downstream reconciliation IS in the committed diff (real-server.js), so the report's "REWORKABLE for the Delivery Leader" framing is already discharged by HEAD; no open item remains.
