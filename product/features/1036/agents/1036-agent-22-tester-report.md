# 1036-agent-22-tester — Stage 3c

Report: product/features/1036/testing/RISK-COVERAGE-REPORT.md

- Node: all sets green except the known environmental failures caused by the installed `@dug-21/unimatrix-linux-x64` (#1055, D-037): resolve-binary 2 fail and wiring-e2e-local 3 cancelled. Both pass with the package moved aside.
- check-test-count PASS (1421); check-zero-deps OK.
- infra-001 smoke: 37/37 pass.
- AC-1c / AC-2b real claude and opencode ran and passed.
- AC-1..10, S-1..6 PASS. R-01..R-25 covered. Accepted gaps: D-036 and Codex real-binary (#1053).
- Latency: branch matches main. Both exceed the ADR-005 absolute budgets on this host, where the `node -e 0` baseline p95 is 58 ms against a 50 ms budget.
- No GH issues filed, because all failures map to the existing #1055. No xfails added.
- Real config: install.json unchanged. The ~/.claude.json mtime change came from the live Claude session, and no test writes appear in it.

Unimatrix entries consulted: briefing (#5177, #2758, #4202 reviewed in index; no context_get needed)

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- gate lessons on verifying named tests (#2758, #4202); applied by matching AC/R test names in the TAP log
- Stored: nothing novel to store -- the stale-installed-platform-package interference is already tracked as #1055
