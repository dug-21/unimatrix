# Scope Risk Assessment: nan-003 (Unimatrix Onboarding Skills)

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `/unimatrix-seed` is a multi-turn conversational skill — model must maintain approval-gate state across many turns; no mechanism enforces depth limit or forces pauses | High | High | Architect should design skill instructions with explicit "STOP and wait for human response" gates; each level transition must be a hard pause, not a soft suggestion |
| SR-02 | Sentinel-based idempotency (`<!-- unimatrix-init v1 -->`) depends on Claude reading the full CLAUDE.md before deciding to append — partial reads or large files may miss the marker | Med | Med | Architect should consider whether a secondary idempotency check (e.g., search for sentinel string via `context_search` or a short head/tail read) provides a fallback |
| SR-03 | Both skills are instructions for Claude, not executable code — quality and correctness depend entirely on model instruction-following fidelity; no automated test harness verifies skill behavior | Med | High | Architect should design skill acceptance criteria as observable, verifiable outcomes (concrete assertions, not subjective) and plan manual verification scenarios |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Bootstrap paradox: `/unimatrix-init` is the onboarding entry point, yet it requires skills to already be manually copied to `.claude/skills/` and MCP to be wired — the very thing onboarding is meant to guide | Med | High | Skill documentation must open with "prerequisites" section stating what must be in place before running `/unimatrix-init`; separate from nan-004 scope clearly |
| SR-05 | `uni-init` agent (brownfield bootstrap) and `/unimatrix-init` skill share near-identical names — high risk of developer confusion about which to invoke and when | Low | High | Skill must include a disambiguation notice in its first section; spec should define the canonical invocation order |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `/unimatrix-seed` calls `context_store` — if MCP server is not running or not wired, the skill fails mid-conversation after potentially exploring significant repo content with no entries saved | Med | Med | Architect should add a pre-flight MCP check at skill entry (Level 0 start): call `context_status` and fail fast with a clear error before any exploration begins |
| SR-07 | Server-side dedup (0.92 cosine threshold) blocks exact duplicates but not near-duplicates; re-running `/unimatrix-seed` could populate similar-but-distinct entries, degrading retrieval quality | Low | Med | AC-13 (`context_search` pre-check) mitigates this; architect should ensure the check happens before ANY Level 0 stores, not just as a warning |

## Assumptions

- **§ Goals / Proposed Approach**: Assumes the model will reliably pause for human approval between seed levels. No prior evidence this works reliably for conversational skills with 3+ decision points. The uni-init prototype failure (67 low-quality automated entries) was caused by exactly this failure mode.
- **§ Constraints**: Assumes target repos have CLAUDE.md as a standard markdown file readable by Claude. Repos with encrypted, large, or machine-generated CLAUDE.md may silently defeat idempotency.
- **§ Non-Goals**: Scoping out `settings.json` wiring (to nan-004) is correct, but leaves a documentation gap — users arriving via `/unimatrix-init` have no guided path to the prerequisite MCP setup.

## Design Recommendations

- **SR-01**: Spec writer should model `/unimatrix-seed` as a state machine with explicit STOP gates, not a free-flowing conversation. Each level transition is a discrete decision point with a yes/no response expected.
- **SR-03, SR-04**: Both skills need a "pre-flight" section at the top of their SKILL.md that verifies prerequisites before taking any action. Fail fast and clearly is better than silent partial execution.
- **SR-06**: Architect should specify that `context_status` is called at skill entry (before any file reads or stores) to validate MCP availability.
