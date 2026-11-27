# 1036-agent-2-testplan report (Stage 3a)

Output: product/features/1036/test-plan/ — OVERVIEW.md + 17 component files matching Component Map (C1–C17).

- Every R-01..R-25 mapped (OVERVIEW table). All High risks have named assembled-path tests.
- AC discharge only via `init()`/`wire()`/`status.collect()` with injected env or spawned CLI; unit plans marked support-only.
- Integration: infra-001 is Rust-server centric; no suite applies functionally. Smoke gate run as no-regression only; no new infra-001 tests; infra-002 link smoke N/A (no Rust). Feature integration = spawned-CLI Node suites + real-binary AC-1(c) `claude mcp get`, AC-2(b) `opencode debug config --standalone` (skip naming binary + #1053).

Open questions:
1. SPEC NFR-07 names a "status" suite; existing file is `launcher-status.test.js` (no `status.test.js`). Implementer to confirm target file.
2. R-03 scenario 4 (escape-and-return chain): plan assumes final-realpath-inside is written through; confirm against FR-22a.
3. Hook-path latency re-measure (R-23): no numeric budget stated in brief; 3c will record against nan-024 ADR-005.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- #5958 (no silent unwired harness NFR), #5763/#5768 (nan-023 wiring/verification ADRs); consistent with plan.
- Stored: nothing novel to store -- plan-stage only; patterns to be confirmed in 3c execution.

Unimatrix entries consulted: #5958, #5763, #5768
