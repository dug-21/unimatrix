# Implementation Brief: base-002 — Workflow & Branching Improvements

## Overview

Modernize the development workflow to support branch-first PR workflow, worktree-based isolation, impl-to-deploy auto-chaining, and procedural knowledge integration. All changes are markdown file edits.

## Source Documents

- Scope: `product/workflow/base-002/SCOPE.md`
- Scope Risk Assessment: `product/workflow/base-002/SCOPE-RISK-ASSESSMENT.md`
- Architecture: `product/workflow/base-002/architecture/ARCHITECTURE.md`
- Specification: `product/workflow/base-002/specification/SPECIFICATION.md`
- Risk Strategy: `product/workflow/base-002/RISK-TEST-STRATEGY.md`
- Alignment Report: `product/workflow/base-002/ALIGNMENT-REPORT.md`

## ADR References (Unimatrix)

| ADR | Entry ID | Decision |
|-----|----------|----------|
| ADR-001 | #510 | Worktree isolation via Claude Code's native `isolation: "worktree"` (validated) |
| ADR-002 | #511 | Auto-chain as protocol extension (deploy remains independent) |
| ADR-003 | #512 | Design-scrum-master owns Session 1 PR (not synthesizer) |
| ADR-004 | #513 | Non-blocking knowledge queries with 5s timeout |

## Constraints

- All changes are markdown-only — no Rust code
- Must work with branch protection (PR required) immediately
- Hooks use installed binary (`~/.local/bin/`) — do not change
- Worktree isolation uses Claude Code's native `isolation: "worktree"` parameter (validated 2026-03-06)
- Knowledge queries are non-blocking with graceful degradation
- **Token budget**: agent defs ≤ ~150 lines, protocols ≤ ~250 lines. Replace text in-place; reference uni-git skill for detail rather than inlining in every file

---

## Component Map

Components are organized by the file being modified. Each component corresponds to one file (or small group of closely related files).

| Component | File(s) | AC(s) | Pseudocode | Test Plan |
|-----------|---------|-------|-----------|-----------|
| git-conventions | `.claude/skills/uni-git/SKILL.md` | 01, 03, 04 | pseudocode/git-conventions.md | test-plan/git-conventions.md |
| design-protocol | `.claude/protocols/uni/uni-design-protocol.md` | 02 | pseudocode/design-protocol.md | test-plan/design-protocol.md |
| delivery-protocol | `.claude/protocols/uni/uni-delivery-protocol.md` | 03, 05, 06 | pseudocode/delivery-protocol.md | test-plan/delivery-protocol.md |
| bugfix-protocol | `.claude/protocols/uni/uni-bugfix-protocol.md` | 03, 06 | pseudocode/bugfix-protocol.md | test-plan/bugfix-protocol.md |
| agent-routing | `.claude/protocols/uni/uni-agent-routing.md` | 05 | pseudocode/agent-routing.md | test-plan/agent-routing.md |
| design-scrum-master | `.claude/agents/uni/uni-design-scrum-master.md` | 02 | pseudocode/design-scrum-master.md | test-plan/design-scrum-master.md |
| impl-scrum-master | `.claude/agents/uni/uni-implementation-scrum-master.md` | 03, 05, 07 | pseudocode/impl-scrum-master.md | test-plan/impl-scrum-master.md |
| deploy-scrum-master | `.claude/agents/uni/uni-deploy-scrum-master.md` | 05, 07 | pseudocode/deploy-scrum-master.md | test-plan/deploy-scrum-master.md |
| bugfix-scrum-master | `.claude/agents/uni/uni-bugfix-scrum-master.md` | 03, 07 | pseudocode/bugfix-scrum-master.md | test-plan/bugfix-scrum-master.md |
| worker-agents | `.claude/agents/uni/uni-rust-dev.md`, `uni-pseudocode.md`, `uni-tester.md` | 08 | pseudocode/worker-agents.md | test-plan/worker-agents.md |
| repo-hygiene | `.gitignore` + git cleanup | 09 | pseudocode/repo-hygiene.md | test-plan/repo-hygiene.md |

## Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | All components, Gate 3a |
| Test Strategy | test-plan/OVERVIEW.md | Gate 3a, Gate 3c |

---

## Implementation Order

The specification defines ordering constraints (Spec section "Ordering Constraints"). Recommended implementation sequence:

1. **Wave 1**: git-conventions (AC-01) + repo-hygiene (AC-09) — foundation
2. **Wave 2**: design-protocol + design-scrum-master (AC-02) — design branch integration
3. **Wave 3**: delivery-protocol + bugfix-protocol + impl-scrum-master + bugfix-scrum-master (AC-03, AC-06) — worktree + compliance
4. **Wave 4**: deploy-scrum-master + agent-routing (AC-05) — auto-chain
5. **Wave 5**: worker-agents (AC-08) — knowledge integration
6. **Cross-cutting**: GH Issue format (AC-07) applied during waves 3-4

Waves 1-2 are sequential. Waves 3-5 can be parallelized if components within each wave are handled by separate agents.

---

## Verification Approach

Since all deliverables are markdown files, verification is done through:
1. **Text search**: Grep for prohibited terms ("directly to main"), required terms (worktree, isolation, timeout)
2. **Cross-reference**: Verify consistency between uni-git skill and all protocols/agents
3. **Checklist**: Walk through each AC sub-criterion in the specification

No unit tests, integration tests, or compilation required.
