# Agent Report — vnc-027-agent-2-testplan (Stage 3a, Test Plan Design)

## Deliverables (all under product/features/vnc-027/test-plan/)

| File | Component / Coverage |
|------|---------------------|
| OVERVIEW.md | Test strategy, risk→AC→component matrix, integration harness plan, merge-order gate |
| size-gate.md | AC-09 (Critical, merges FIRST), R-02, R-03 — stripper self-test, dual-limit, git-log audit |
| wire-accept-text.md | AC-11, R-07, R-08 — wire.rs additivity, accept↔Text coupling |
| listener-preformatted.md | AC-03, AC-11, R-08, R-09 — wants_text, allowlist, shared injection core, header |
| transport-uds.md | AC-01, AC-03, AC-04, AC-05, R-01, R-06, R-18, R-15, R-13 — framing, SendResult, lifecycle |
| config-transport-selection.md | AC-02, R-05, R-13 — mode matrix, single-derivation, hash parity |
| index-dispatch.md | AC-04, AC-08, AC-10, AC-12, R-11, R-14 — selection, null-sentinel, rekey wiring |
| build-request-sentinel.md | AC-08, R-11 — sentinel matrix, F-02 exact-equality gate |
| merge-settings-reduction.md | AC-08, R-11, R-12 — matcher narrowing, opt-in matrix |
| state-offset-rekey.md | AC-10, AC-12, R-04, R-14 — canonical-event keying, age-prune, HTTP guard |
| parity-corpus-uds.md | AC-01/02/03/04/06/07/11, R-01/05/08/09/10/12/16/17 — integration backbone |

Test-plan files map 1:1 to the IMPLEMENTATION-BRIEF Component Map.

## Risk Coverage Mapping
Every risk R-01..R-18 maps to ≥1 concrete test expectation (see OVERVIEW risk→AC table). Priority emphasis:
- **R-02 (Critical)**: size-gate self-test + dual-limit triggers + git-log first-commit audit.
- **R-01/R-06/R-11/R-04 (High)**: FNF flush-before-FIN order + 1 MiB live truncation; chunked read loop + settle-once + no-process.exit grep-gate; sentinel matrix + matcher snapshot + F-02 gate; TaskCompleted-deletes / Stop-must-NOT discrimination + multi-turn persistence.
- AC-09 (merges first), AC-11 (frozen byte-unchanged via unmodified Rust suites + frozen-binary e2e R-08 s4), AC-10/AC-12 (offset-delete rekey per amended ADR-006 + full F3 delta suite green) given the special attention the spawn prompt called out.

## Integration Suite Plan
- **infra-001 (MCP-server regression, minimum gate)**: `smoke` (mandatory) + `protocol` + `tools` — regression-only, confirms the additive wire change does not regress the live binary's MCP surface. No new infra-001 tests needed (UDS hook transport is not on the MCP JSON-RPC surface infra-001 drives).
- **node:test Layer 2 (primary feature harness, NEW UDS layer)**: extend `test/helpers/real-server.js` with a UDS connect helper (cumulative). 6 new scenarios: live UDS round-trip, FNF 1 MiB truncation, cross-transport replay both directions, delta-over-UDS buffer merge, no-SubagentStop lifecycle, PreCompact single-block.
- **Rust cargo**: wire.rs round-trip/additivity units; listener wants_text/allowlist units; AC-11 unmodified parity suite + `regen-parity.sh` zero-diff + ts-rs drift; R-08 frozen-binary e2e.

## Open Questions
1. UDS Layer 2 helper: extend `real-server.js` (recommended, cumulative) vs sibling `real-server-uds.js` — Stage 3b architect call; plans assume extension.
2. AC-05 p95 over UDS in CI: confirm `benchmark-spawn` can target a live local socket (Linux-only Layer-2-scoped job); if a soak machine is required, AC-05 may degrade to a documented local-run check.
3. (Inherited, resolved by ADRs not by me) R-04 register-TaskCompleted vs age-prune-only — ADR-006 decided age-prune-only, authoritative over FR-30/AC-10 "and/or"; test plan implements the unreachable-but-pinned TaskCompleted branch + assertable Stop-negative.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #4798 (transport sync-formatting asymmetry → drove ADR-001 coverage), #4802/#4806 (vnc-027 ADRs), #4790 (vnc-026 parity regen/run procedure — grounded the Layer 1/Layer 2 conventions, regen-parity.sh, hard-fail-never-skip #4452, eol-lf #4782, realpath-tmpdir #4784), #4789 (port-with-authoritative-original pattern), #4780 (size-gate Gate-3b rework lesson → reinforced AC-09-first ordering).
- Stored: nothing novel to store — Stage 3a produced test PLANS that consume existing, already-stored procedures (#4790 regen/run, #4452 vacuous-pass, #4768/#4774 spawn-safety, #4769/#4782/#4784 corpus gotchas). No new test-infra pattern was discovered; new fixtures/techniques (UDS live-listener helper, hash-fixture corpus binding) are planned but not yet implemented — they belong in a Stage 3c retro store if they prove reusable.
