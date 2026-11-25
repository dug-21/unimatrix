# vnc-050-agent-2-testplan Report (Stage 3a)

Output: product/features/vnc-050/test-plan/ — OVERVIEW.md + 14 component plans (token-table, scoped-validator, slug-guard, principal, cap-ceiling, legacy-token-config, observe-gate, observe-403-client, client-bundle-cli, bundle-decoder, remote-resolve, bridge-transports, docs-smoke, backward-compat).

- All 22 risks mapped (OVERVIEW risk table). Critical risks discharged only at real boundary (HTTP listener, spawned binary/bridge/hook, byte-capturing TLS). Each rejection has a companion.
- R-02 step 0 permanent test: `principal_visible_in_tool_request_context_over_http` (cap-ceiling.md).
- AC-13 live / AC-14: no tests; scripted local AC-13 in docs-smoke.md.
- infra-001: smoke, tools, protocol, security, lifecycle, confidence, project_isolation, transport_health, https_uds_parity, edge_cases; new `suites/test_headless_access.py` on extended `multi_slug_http_server` fixture.

## Open questions
1. Test hooks (principal strip, channel flush, dispatcher/resolve_store spies, scan counter) need a `#[cfg(test)]`/test-feature seam in pseudocode.
2. Which tools are Admin-gated for `rw_token_human_admin_tool_denied` — pseudocode to list.
3. Non-ASCII header value policy (ADR-007) to be stated explicitly.

## Knowledge Stewardship
- Queried: context_briefing / context_search not run (MCP tools deferred; proceeded from ADR files and RISK-TEST-STRATEGY per fallback).
- Stored: nothing novel to store — plans apply existing patterns (#4970, #4416); new helpers are proposals until implemented in 3c.

## Gate 3a Rework (iteration 1)
- T1: observe-403-client.md rewritten for approved per-credential marker (SHA-256 key, 0600, no expiry, fail-open); `latch_per_process` removed; AC-18 cross-spawn, credential-change, corrupt-marker, rw-no-marker tests plus conditions 1/2/4 rows.
- T2: bridge-transports.md R-18: bridge exit 1, hook exit 0 + one stderr line naming var, no leak.
- Seams aligned to pseudocode/OVERVIEW.md names (cap-ceiling, slug-guard, observe-gate, scoped-validator, token-table, OVERVIEW); Admin tools quarantine+enroll; non-ASCII header rows in remote-resolve.md.
- Open questions 1–3 resolved by pseudocode rework.
- Queried (rework): context_search vnc-050 decisions — ADR-001..007 (#5806, #5814–#5826). Stored: nothing novel.
