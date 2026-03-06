# Risk-Based Test Strategy: base-002

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Protocol files reference stale "commit to main" language after update, causing agent confusion | High | Medium | High |
| R-02 | Worktree fallback path untested — agents may fail silently when worktree creation fails | High | Medium | High |
| R-03 | Auto-chain error propagation loses impl results when deploy fails | High | Low | Medium |
| R-04 | Design session PR creation breaks existing human approval flow | Medium | Medium | Medium |
| R-05 | GH Issue comment format inconsistency across coordinators after standardization | Low | Medium | Low |
| R-06 | Knowledge query guidance missing timeout/fallback in one or more worker agents | Medium | Medium | Medium |
| R-07 | .gitignore missing worktree entry causes worktree directories to appear in git status | Low | Low | Low |
| R-08 | Circular references between uni-git skill and protocol files create contradictions | Medium | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Stale "Commit to Main" References
**Severity**: High
**Likelihood**: Medium
**Impact**: Agents attempt direct-to-main commits, fail against branch protection, waste context window on retries.

**Test Scenarios**:
1. Grep all modified files for "directly to main", "commit to main", "commit directly" — zero matches expected
2. Grep all protocol and agent files for "git push origin main" — zero matches expected (except in PR merge context)
3. Verify uni-git/SKILL.md Session 1 section references design branch, not main

**Coverage Requirement**: Exhaustive text search across all `.claude/` markdown files

### R-02: Worktree Fallback Path
**Severity**: High
**Likelihood**: Medium
**Impact**: If worktree add fails and no fallback exists, the coordinator hangs or errors without creating a branch.

**Test Scenarios**:
1. Verify delivery protocol includes conditional: "if worktree creation fails, fall back to checkout"
2. Verify bugfix protocol includes same conditional
3. Verify both scrum-master agent definitions include fallback language
4. Verify fallback path still creates a branch (just not in a worktree)

**Coverage Requirement**: Each protocol and agent file that mentions worktree must also mention fallback

### R-03: Auto-Chain Error Propagation
**Severity**: High
**Likelihood**: Low
**Impact**: If deploy-scrum-master errors, human loses visibility into successful impl results.

**Test Scenarios**:
1. Verify impl-scrum-master Phase 4 includes error handling for deploy spawn failure
2. Verify combined return format includes both impl and deploy sections
3. Verify "deploy auto-chain failed" message path exists
4. Verify impl results are returned even when deploy returns BLOCKED

**Coverage Requirement**: Error handling section must cover: deploy spawn failure, deploy BLOCKED, deploy error

### R-04: Design Session PR Integration
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Human workflow changes — approval was "review artifacts", now "merge PR". If PR creation fails or format is wrong, session stalls.

**Test Scenarios**:
1. Verify design protocol Phase 2d includes `gh pr create` command
2. Verify PR body template references design artifacts
3. Verify design-scrum-master exit gate includes PR creation
4. Verify return message references PR URL

**Coverage Requirement**: Design protocol and scrum-master aligned on PR creation step

### R-05: GH Issue Comment Format Inconsistency
**Severity**: Low
**Likelihood**: Medium
**Impact**: Inconsistent formatting makes issue timelines harder to read; no functional impact.

**Test Scenarios**:
1. Extract comment format from all three coordinator agents
2. Verify all use the same template structure
3. Verify bugfix existing format was reconciled (not broken)

**Coverage Requirement**: Diff each coordinator's comment format against the standard

### R-06: Knowledge Query Guidance Gaps
**Severity**: Medium
**Likelihood**: Medium
**Impact**: A worker agent blocks on a slow Unimatrix query, wasting context window time.

**Test Scenarios**:
1. Verify uni-rust-dev.md includes knowledge query with timeout/fallback guidance
2. Verify uni-pseudocode.md includes same
3. Verify uni-tester.md includes same
4. Verify all three mention "proceed without knowledge if unavailable"

**Coverage Requirement**: All three worker agent files checked for both query and fallback

### R-07: .gitignore Missing Worktree Entry
**Severity**: Low
**Likelihood**: Low
**Impact**: Worktree directories pollute git status; minor annoyance.

**Test Scenarios**:
1. Verify `.gitignore` contains `.claude/worktrees/` line

**Coverage Requirement**: Single line check

### R-08: Circular References Between Skill and Protocols
**Severity**: Medium
**Likelihood**: Low
**Impact**: Contradictory merge strategy or branch naming between uni-git skill and protocol files.

**Test Scenarios**:
1. Extract merge strategy from uni-git skill and each protocol — verify consistency
2. Extract branch naming from uni-git skill and each coordinator agent — verify consistency
3. Verify no protocol overrides uni-git skill conventions

**Coverage Requirement**: Cross-reference check between uni-git and all protocol/agent files

## Integration Risks

Since base-002 is markdown-only, traditional integration risks (API boundaries, data flow) don't apply. The integration surface is **cross-file consistency**:

- Branch naming in uni-git must match branch creation in each protocol
- Merge strategy in uni-git must match merge commands in each protocol
- Worktree conventions in uni-git must match worktree lifecycle in each protocol
- GH Issue format in each coordinator must match the defined standard
- Auto-chain contract in impl-scrum-master must match what deploy-scrum-master expects

**Verification**: Cross-reference grep across all modified files for key terms (branch patterns, merge flags, worktree paths).

## Edge Cases

- **Session 1 with no GH Issue yet**: Design session creates GH Issue via synthesizer, then scrum-master creates PR. PR body should reference the GH Issue URL returned by synthesizer.
- **Auto-chain when deploy-scrum-master definition is missing**: Impl-scrum-master should handle spawn failure gracefully.
- **Worktree cleanup when session crashes mid-work**: Stale worktree recovery documented in uni-git skill. Human runs `git worktree prune`.
- **Multiple concurrent worktrees on same branch name**: Git rejects this. Protocols should not attempt to create a worktree if one already exists for that branch.

## Security Risks

This scope involves no code execution, no external input processing, and no data handling changes. Security assessment: **not applicable** for markdown-only changes.

The auto-chain (AC-05) does not introduce new security surface — it reuses the existing deploy-scrum-master flow which already includes fresh-context security review.

## Failure Modes

| Failure | Expected Behavior |
|---------|------------------|
| Worktree creation fails | Fall back to standard checkout; log warning |
| PR creation fails in Session 1 | Return artifacts to human without PR; human creates manually |
| Deploy spawn fails in auto-chain | Return impl results; note deploy failure |
| Knowledge query times out | Agent proceeds without knowledge; logs "no procedural knowledge" |
| GH Issue comment fails | Non-blocking; session continues |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (worktree platform support) | R-02 | Optional layer with fallback; fallback tested |
| SR-02 (cargo target isolation) | -- | Documented as cargo default behavior; no architecture risk |
| SR-03 (auto-chain error handling) | R-03 | Protocol extension model with explicit error propagation |
| SR-04 (knowledge query availability) | R-06 | Non-blocking with 5s timeout; all 3 workers checked |
| SR-05 (rebase on non-linear history) | -- | AC-09 hygiene runs first; accepted risk |
| SR-06 (Session 1 PR lifecycle) | R-04 | Design-scrum-master owns PR per ADR-003 |
| SR-07 (GH Issue format conflict) | R-05 | Bugfix format adopted as standard; reconciled |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 2 | 7 scenarios |
| Medium | 4 | 14 scenarios |
| Low | 2 | 4 scenarios |
| **Total** | **8** | **25 scenarios** |
