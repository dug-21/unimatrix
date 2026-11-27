# 1036-agent-23-clean-e2e report

Clean-checkout verification (D-038) of `fbdbf47e` in a detached worktree (removed afterwards). `npm install --omit=optional`, so no platform binary. Tmp HOME only (D-035).

- wiring-e2e: 45/45 pass, 0 fail, 0 cancelled. D-038 met.
- hook-client 978/979 (1 skip), parity-layer2 24/24, opencode-plugin 37/37, audit PASS, resolve-binary pass.
- package: 942 pass, **29 fail**, 2 skip. This is a #1036 test defect. Four new spawned-init groups (`test/init-integration.test.js:482`, `test/wire.test.js:913,1057,1132`) need `@dug-21/unimatrix-linux-x64` installed: `lib/wire.js:165` sets requireBinary to true for local on linux, and `lib/install-store.js:109` then throws. lib is unchanged from main. CI has no npm install, so it will fail too. The fix goes in the tests. Not fixed here.
- Hook latency is recorded as report-only per D-039.

Details: testing/RISK-COVERAGE-REPORT.md, section "D-038 clean-checkout verification".

Unimatrix entries consulted: #5393, #5778, #5911, #4841

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing: worktree-testing and tmp-home patterns (#5393, #5911), plus the serial-runner gotcha (#5778).
- Stored: nothing. The finding is specific to this feature's tests and belongs in the rework, not a pattern yet.
