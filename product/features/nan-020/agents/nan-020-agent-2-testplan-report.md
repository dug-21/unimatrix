# Agent Report — nan-020-agent-2-testplan (Stage 3a, Test Plan Design)

## Deliverables (all under product/features/nan-020/test-plan/)
- OVERVIEW.md — strategy, full risk→test + AC→test mapping, integration-harness plan, pre-merge vs PENDING split
- docker-http-posture-smoke.md — Gates 5–7 truth table, distinct fail messages, nan-019 regression invariance (R-01/R-02/R-03/R-04/R-05/R-06/R-15)
- hermeticity-negative-control.md — REQUIRED pre-merge negative control + isolation (R-07/AC-09)
- release-yml-setup-node.md — pinned setup-node@v4 static assertions (R-04)
- docs-client-setup.md — AC-01/AC-02 greps + executable-claim classification (R-08/R-09/R-10/R-12/R-16)
- readme-bundle-example.md — multi-occurrence convergence greps (R-09/R-10/R-16)
- uni-docs-remit.md — inspection-based remit fence (R-13/R-14/R-16)

Files map 1:1 to the IMPLEMENTATION-BRIEF Component Map.

## Coverage highlights
- R-01/R-03/R-07 (Critical) each own dedicated stub-driven, pre-merge-provable scenarios; PENDING-without-proof flagged AS a gap.
- R-07 negative control (poison stale cred + broken attach → Gate 7 STILL red) is the load-bearing test; vacuous-pass discrimination required.
- R-16 recorded as accepted residual — no `--remote` round-trip owed; sole mitigation = inspectable "legacy" marker (covered in docs/readme plans).
- Python infra-001 suites declared N/A with reason (no MCP-visible behavior); the real integration is the stub-driven shell gate-logic harness extending nan-019's release-gate-logic-test.sh.

## Delivery dependency surfaced (test plan REQUIRES it)
Gate 5–7 logic must be factored so the three new external commands (docker run client-bundle, node init --bundle, observe POST + store du) are env-injectable — mirroring nan-019's run_smoke_gate SMOKE_CMD indirection (#5192). Without that seam the negative control and truth table are not stub-drivable pre-merge. Owned by pseudocode components docker-http-posture-smoke / hermeticity-sandbox.

## Open questions
1. Stub-fixture granularity: one combined env-driven fixture vs three (stub-client-bundle / stub-init-bundle / stub-observe). Plan assumes three for clarity; delivery may consolidate if cleaner — non-blocking.
2. Negative-control poison placement: must place the stale cred where a non-isolated run WOULD read it while never touching the developer's real ~/.unimatrix. Plan specifies a redirected fake-HOME for the poison; pseudocode should confirm the exact path indirection so the test is hermetic about its own poison.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search/context_get — surfaced #5249/#5256/#5257 (nan-020 ADR-001/002/005), #5189 (pre-merge-provable shell-gate test plan), #5192 (sourceable-spine), #5180 (self-skip→hard-fail), #4977 (vacuous-pass), vnc-041 AC-06/AC-02. All directly applied to the pre-merge-provable vs PENDING split, the stub truth table, and the R-07 negative control.
- Stored: nothing novel — the patterns this plan rests on (pre-merge-provable stub gate-logic #5189, sourceable-spine #5192, self-skip→hard-fail #5180, vacuous-pass negative control #4977 / vnc-041 AC-06, process-boundary isolation vnc-041 AC-02) are already captured. nan-020's test plan is a faithful reuse; re-storing would duplicate. If the executable-claim-classification-as-testable-contract pattern recurs in Feature 2, store it then.
