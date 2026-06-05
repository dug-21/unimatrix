# Agent Report — vnc-024-agent-2-testplan (Stage 3a, Test Plan Design)

## Deliverables (absolute paths)
- /workspaces/unimatrix/product/features/vnc-024/test-plan/OVERVIEW.md
- /workspaces/unimatrix/product/features/vnc-024/test-plan/ts-rs-codegen.md
- /workspaces/unimatrix/product/features/vnc-024/test-plan/contract-fixtures.md
- /workspaces/unimatrix/product/features/vnc-024/test-plan/observe-content-negotiation.md
- /workspaces/unimatrix/product/features/vnc-024/test-plan/transcript-delta-guard.md
- /workspaces/unimatrix/product/features/vnc-024/test-plan/transcript-retention.md

Component files map 1:1 to the brief Component Map and the pseudocode filenames.

## Risk coverage mapping (summary)
Every Critical/High risk has concrete per-component assertions. Full table in OVERVIEW.md.
- R-01/R-02/R-08/R-13 → contract-fixtures.md (AC-04/05/06/11; dual-sided delta; dual-direction None-omission; binding completeness; precedence note)
- R-03/R-04 → transcript-delta-guard.md — **GATE**: zero-durable-rows on HTTP + UDS + batch arm (AC-12)
- R-05/R-06/R-07 + AC-10 → observe-content-negotiation.md (byte-identity w/ production budget + truncation; allowlist; content-type at HTTP boundary; UDS parity)
- R-09/R-10/R-11 → transcript-retention.md (four touchpoints; validate REJECTS RetainDays enterprise-only; merge re-validation #3905; PartialEq)
- R-12/R-14 → ts-rs-codegen.md (dev-only footprint; CI diff-gate self-test fail-on-drift/pass-on-clean)

## Integration suite plan
- **Key finding:** infra-001 drives the server over **MCP JSON-RPC stdio**. vnc-024's integration ACs (AC-07/08/09/10/12) live on the **HTTP `/observe` tower handler** and **UDS dispatch** — surfaces the stdio harness does NOT exercise. So those ACs are covered by **server-crate Rust integration tests**, and **no new infra-001 suite** is added for them.
- infra-001 suites to run Stage 3c as regression baselines: **smoke (gate)**, protocol, tools, lifecycle, (adaptation optional). All must stay green; any new failure triaged per USAGE-PROTOCOL (feature → fix; pre-existing → GH Issue + xfail; bad assertion → fix test).

## Open questions (delivery-time, non-blocking)
1. Exact production `max_bytes` constant the UDS caller passes to `format_injection` — AC-07 must reuse it (ideally one shared symbol to prevent future byte-identity drift).
2. Whether the server-crate integration suite already has helpers to boot the HTTP `/observe` handler and count `insert_observation` rows; if not, one focused helper reused by AC-07/08/09 + AC-12.
3. `wants_text` predicate semantics for `Accept: text/plain, application/json` and `*/*` (assumed "contains text/plain").

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get — retrieved vnc-024 ADR-001..005 (#4718/#4719/#4714/#4720/#4721), the dual-direction serde test pattern (#3557), and confirmed the governing risk patterns (#4711 secrets-to-disk, #1266 col-022 anti-pattern, #4070/#2730 config hidden-site, #3905 merge re-validation, #885 serde under-test).
- Stored: nothing novel to store — the test plans **apply** existing patterns (#3557 dual-direction, #4711/#1266 guard, #4070/#2730/#3905 config) rather than discovering a new cross-feature technique. The one candidate generalization ("infra-001 stdio harness does not cover HTTP `/observe` + UDS dispatch ACs, so vnc-024 integration ACs need server-crate tests, not new infra-001 suites") is feature-specific scoping guidance, better captured at retro if it recurs across vnc HTTP/UDS features than pre-emptively stored now.
