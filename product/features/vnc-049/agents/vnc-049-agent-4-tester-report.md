# Agent Report — vnc-049 Agent 4 (Tester, Stage 3c Test Execution)

Role: uni-tester (Phase 2 — Test Execution). Feature vnc-049 (#986), branch
`feature/vnc-049`, schema v32.

## Outcome: PASS

All entry gates green; all R-01..R-17 covered; all AC-01..07 discharged.
Full report: `product/features/vnc-049/testing/RISK-COVERAGE-REPORT.md`.

## What I ran
- **Rust workspace unit** (hardened `setsid -w timeout … cargo test --workspace`,
  file-not-pipe): 6307 passed, 0 failed, 31 ignored (after fixing one stale pin).
- **#878 LINK smoke** (`infra-002/check-workspace-link-smoke.sh`): PASS.
- **JS unit**: C1 plugin 38, C9 installer+init 38, C3 hook-client 829 (+1 skip). 0 fail.
- **infra-001 smoke gate** (MANDATORY): 37 passed, 0 failed (incl. 2 new opencode smoke tests).
- **infra-001 lifecycle+security**: 136 passed, 6 xfailed, 1 xpassed (pre-existing markers), 0 fail.
- **infra-001 tools+protocol+edge_cases** (schema regression sentinels): 264 passed, 2 xfailed, 0 fail.

## What I wrote (8 new infra-001 tests + cumulative harness support)
Extended the GH#819 `daemon_server` + `UnimatrixHookClient` + row-count model with a
read-back SELECT of `source_domain`/`model_id`, under the anti-seed contract (#5285):
- test_lifecycle.py: `test_opencode_event_stored_source_domain_is_opencode` (AC-03/R-02,
  +neg), `test_opencode_local_vs_cloud_model_distinct_on_queried_row` (AC-06 GATING/R-01),
  `test_opencode_two_local_models_mutually_distinct` (R-01.3),
  `test_opencode_provider_evidenced_by_stored_source_domain` (AC-02c/R-03.4),
  `test_opencode_cloud_event_without_model_id_stored_null` (R-10.2),
  `test_context_retrieval_unaffected_by_attribution_columns` (AC-05c/R-04).
- test_security.py: `test_source_domain_stamp_fires_only_for_opencode` (R-05),
  `test_model_id_invalid_charset_not_persisted_raw` (R-15).
- harness/hook_client.py: `record_event`/`record_post_tool_use` gained `provider`/`model_id` kwargs.
- harness/assertions.py: `read_observation_attribution(store_dir)` read-back helper.

## Triage / fix (in-PR)
- **R-04 cascade gap (fixed):** `verify_integration.rs::test_schema_version_still_31`
  was a stale HEAD-tracking schema pin the 31→32 bump missed (gate-3b's `== 31` grep
  didn't match the comma-form `assert_eq!(…, 31)`). Updated to `_32`; test-code only,
  no production change. Re-run GREEN.
- **My test bug (fixed):** `test_context_retrieval_unaffected` first passed the content
  string to `assert_search_contains` (expects entry_id). Fixed; re-run GREEN.
- **No GH Issues filed** — no genuinely pre-existing integration failures surfaced;
  known Rust parallel flakes did not trigger. No new xfail markers added.

## Coverage gap (documented, not a hole)
AC-04 subagent alignment / AC-07 seam are not driven end-to-end through infra-001 (the
harness reaches CLI→store, not plugin→CLI). Covered by C1 node plugin tests + Rust
crossing/structural tests. No fabricated harness proxy written (anti-proxy discipline).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (task: vnc-049 Stage 3c execution) —
  surfaced ADR-001 (#5748), ADR-007 (#5744), ADR-009 (#5746), hook-client UDS pattern
  (#4823); applied anti-seed derive-don't-seed (#5285) + the GH#819 row-count model.
- Stored: entry #5758 "infra-001 UDS-observe attribution read-back pattern (anti-seed)"
  via context_store (topic: testing, category: pattern).
