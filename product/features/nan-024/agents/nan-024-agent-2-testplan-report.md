# Agent Report: nan-024-agent-2-testplan (Stage 3a)

## Report
Produced test-plan/OVERVIEW.md plus 14 component plans (shim, shim-entry, install-store, resolve-runtime, trust, project-state, launcher, hook-ownership, merge-settings, wire-init, codex-install, opencode-install, hook-client, ci-and-docs) under product/features/nan-024/test-plan/. All R-01..R-40 mapped. Key design points: new `wiring-e2e` file-list set (package set exits 0 on win32) registered in runner + orphan audit; Windows/macOS proof is CI-only (E2E matrix, latency gate, Q9); D-053 headless negative case specified; D-072 fallback MCP form asserted; infra-001: smoke + protocol/tools/lifecycle/headless/isolation/parity regression only, no new tests required.

Unimatrix entries consulted: #5781, #5784, #5788, #5790, #5128, #4774, #4779, #5768, #5889, #5892, #5893, #5891, #5894 (ADR-001..006 via ADR-INDEX).

## Open questions
1. wiring-e2e files double-run in `package` set or exclusive (plan: exclusive).
2. Test-count baseline storage (plan: script + fixtures JSON).
3. R-38 key normalisation can't be verified against real Claude in CI; use captured fixture.
4. Land latency/E2E jobs non-required until one green grid (#5790).

## Knowledge Stewardship
- Queried: context_briefing, context_search (nan-024 decisions; CI/hook-client patterns), context_get #5790 -- CI topology, win32 skip-guard and orphan-audit conventions applied.
- Stored: nothing novel to store -- plan applies existing patterns (#5790, #4779, #4774); the `wiring-e2e` Windows set pattern is worth storing after Stage 3c proves it.

## Rework (Stage 3a rework, agent nan-024-agent-2-testplan-rework)
Applied D-077..D-083 to test-plan/ in place (appended "Rework" sections; OVERVIEW has a new Rework section with risk/AC mapping).
- Files changed: test-plan/OVERVIEW.md, shim.md, shim-entry.md, install-store.md, resolve-runtime.md, trust.md, project-state.md, launcher.md, merge-settings.md, wire-init.md, codex-install.md, ci-and-docs.md.
- New wiring-e2e files planned: shim-superset (D-079, MANDATORY), mcp-stdio-spawn (D-077), pkg-binary-copy (D-078), codex-detect (D-081), init-preflight (D-082); docs-grep rule fixtures (D-083); measure-pkg-size step for Stage 3c (D-078).
- H1-pending items marked SEPARABLE: effectiveEnv, sibling trust.js bootstrap.
- Earlier open questions resolved: wiring-e2e exclusive set; baseline test/fixtures/test-counts.json + diff script; new CI jobs non-required until grid green once.
- Invalid/relative $CODEX_HOME: ignore + warn (flagged default).
- Remaining open: H1 outcome; CODEX_HOME policy confirmation; D-078 size threshold (post-measurement); Windows signal-mirror semantics vs pseudocode; pseudocode/OVERVIEW.md rework was in progress and not yet reflecting D-077..D-083 when read (re-read at end: still shows pre-rework text, e.g. no pre-Node clause (d)); a final alignment pass is needed once it lands.

Unimatrix entries consulted: #5896, #5897, #5899, #5898, #5890, #5894 (via ADR-INDEX list in task), briefing results (no entries bearing on the rework).

## Rework 2 (agent nan-024-agent-2-testplan-rework2)
Applied D-084..D-089. Files changed: test-plan/OVERVIEW.md, codex-install.md, launcher.md, shim.md, ci-and-docs.md. Added Codex MCP gating rows M1-M3; CODEX_HOME ignore+warn decided; Windows exit-code-only with per-platform signal assertions; D-078 size report-only; Windows pre-Node-disabled assertion + escalation note. H1 items (effectiveEnv, trust bootstrap) remain SEPARABLE.
Unimatrix entries consulted: briefing (no entries bearing on this rework).
Knowledge Stewardship: Queried briefing; Stored nothing novel (in-place decision application).

## Rework 3 (agent nan-024-agent-2-testplan-rework3)
Applied D-090 and D-091 (human-decided); removed all PENDING H1/SEPARABLE markers. Files changed: test-plan/OVERVIEW.md, shim-entry.md, trust.md, install-store.md, merge-settings.md, codex-install.md, resolve-runtime.md, project-state.md, launcher.md. New planned file: `test/trust-selfcontained.test.js`, `test/status-cloud-env.test.js`. D-086 Windows exit-code-only tests kept.
Unimatrix entries consulted: #5900, #5901 (headers only returned by context_get), briefing (#5778, #5781 not bearing).
Knowledge Stewardship: Queried briefing; Stored nothing novel (in-place decision application).

## Rework 4 (gate 3a iteration 1)
Files changed (test-plan/): install-store.md (F1 size guard removed, report-only D-087), launcher.md (F2 `binary: MISSING` removed; status field set per FR-12g), trust.md (F5 symlinks refused), wire-init.md (F3 D-082 branch matrix warn/fail-no-PATH/fail-Windows with PATH fixtures, init exits 1 only if node unrecordable; R-37 init may exit 1 after 3 retries, assert no corruption), OVERVIEW.md (W1 D-001..D-091, ADRs by number; R-39 harness-display item), ci-and-docs.md (W3 built server for Linux UDS E2E; AC-12 suites enumerated; R-39 evidence), project-state.md (third arg required), resolve-runtime.md (`clientSource === "install"`), hook-ownership.md (legacy MCP forms 1-3), opencode-install.md (XDG fixture isolation only).
Re-read pseudocode/wire-init.md D-082 rule: aligned (win32: any missing recorded node -> exit 1; non-win32: warn if PATH node, else exit 1; init exit 1 if unrecordable).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- lessons on multi-doc consistency (#3611, #3612); no new entries.
- Stored: nothing novel to store -- text-level consistency fixes.
Unimatrix entries consulted: #3611, #3612, #3598
