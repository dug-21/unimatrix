# Specification: base-002 — Workflow & Branching Improvements

## Overview

This specification defines the functional requirements, acceptance criteria with verification methods, constraints, and domain model for base-002. All deliverables are markdown file edits — no Rust code changes.

---

## Domain Model

### Entities

| Entity | Definition |
|--------|-----------|
| **Session** | A bounded interaction (design or delivery) that produces artifacts and ends with human review |
| **Branch** | A git branch following the naming convention in uni-git/SKILL.md |
| **Worktree** | An isolated git checkout at `.claude/worktrees/{type}-{id}/` for parallel workstreams |
| **Coordinator** | A scrum-master agent that orchestrates a session (design, impl, bugfix, deploy, retro) |
| **Auto-chain** | Automatic spawn of deploy-scrum-master by impl-scrum-master after PR creation |
| **Gate** | A validation checkpoint that must PASS before proceeding |
| **Procedure** | A reusable multi-step technique stored in Unimatrix (category: procedure) |

### Relationships

- A Session operates on exactly one Branch
- A Branch is isolated via a Worktree (Claude Code's native `isolation: "worktree"` on Agent tool)
- A Coordinator owns the lifecycle of its Session's Branch (create, commit, push, PR, cleanup)
- An Auto-chain links exactly two Coordinators (impl -> deploy) within one interaction
- Worker agents query Procedures before starting their task

---

## Acceptance Criteria — Detailed Requirements

### AC-01: Branch-First Git Conventions

**Requirement:** All workflows produce PRs. No workflow commits directly to main.

| Sub-criteria | Verification |
|-------------|-------------|
| uni-git/SKILL.md removes all "commit directly to main" language | Manual review: grep for "directly to main" returns zero matches |
| Branch naming table covers all 5 contexts (design, feature, bugfix, docs, workflow) | Manual review: table present with all 5 rows |
| `docs:` commit prefix added to prefix table | Manual review: prefix table includes `docs:` row |
| PR merge strategy documented as rebase-only | Manual review: merge strategy section present |
| Merged branch deletion documented (auto-delete enabled at repo level) | Manual review: branch lifecycle section updated |

### AC-02: Design Protocol Branch Integration

**Requirement:** Session 1 uses a branch instead of committing to main.

| Sub-criteria | Verification |
|-------------|-------------|
| uni-design-protocol.md specifies branch creation at session start | Manual review: Initialization section includes `git checkout -b design/{id}` |
| Design artifacts committed to design branch (not main) | Manual review: no reference to committing on main in design protocol |
| Session 1 ends with PR to main | Manual review: Phase 2d includes `gh pr create` |
| uni-design-scrum-master.md reflects branch creation and PR | Manual review: init section and exit gate updated |
| Human approval gate = PR approval + merge | Manual review: Phase 2d return message references PR |

**Constraint (from SR-06):** The design-scrum-master creates and owns the PR. The synthesizer creates the GH Issue only.

### AC-03: Worktree-Based Branch Isolation

**Requirement:** Coordinator agents create isolated worktrees for parallel workstreams.

| Sub-criteria | Verification |
|-------------|-------------|
| uni-delivery-protocol.md includes worktree creation at Initialization | Manual review: worktree add command present |
| uni-delivery-protocol.md includes worktree removal at exit | Manual review: worktree remove in Phase 4 or exit gate |
| uni-bugfix-protocol.md includes same worktree lifecycle | Manual review: worktree add/remove present |
| uni-implementation-scrum-master.md updated with worktree in init and exit gate | Manual review: both sections reference worktree |
| uni-bugfix-scrum-master.md updated with worktree in init and exit gate | Manual review: both sections reference worktree |
| `.gitignore` includes `.claude/worktrees/` | Manual review: line present in .gitignore |
| Worktree path convention: `.claude/worktrees/{branch-type}-{id}/` | Manual review: convention documented in uni-git skill |
| Exit gate includes worktree cleanup | Manual review: exit gate checklist includes worktree remove |
| Stale worktree recovery documented | Manual review: recovery section in uni-git skill |

**Note (SR-01 RESOLVED):** Claude Code's native `isolation: "worktree"` handles worktree lifecycle. Coordinators spawn agents with this parameter — no manual worktree management or fallback path needed.

### AC-04: Build Artifact Isolation

**Requirement:** Development builds don't race with parallel workstreams.

| Sub-criteria | Verification |
|-------------|-------------|
| Build pipeline documented: hooks use `~/.local/bin/`, tests use `target/release/` | Manual review: separation documented in uni-git skill |
| Worktree builds use per-worktree target dir (cargo default) | Manual review: documented as expected behavior |
| Protocol guidance: `cargo build --release` in worktree does NOT affect installed binary | Manual review: explicit statement in uni-git skill |
| Installed binary update guidance: `cargo install --path crates/unimatrix-server` | Manual review: promotion command documented |
| `UNIMATRIX_BINARY` env var documented for worktree integration tests | Manual review: env var guidance present |

### AC-05: Implementation-to-Deploy Auto-Chain

**Requirement:** Implementation completion automatically triggers PR review.

| Sub-criteria | Verification |
|-------------|-------------|
| uni-implementation-scrum-master.md Phase 4 spawns uni-deploy-scrum-master after PR | Manual review: spawn command present in Phase 4 |
| uni-deploy-scrum-master.md accepts spawn from impl-scrum-master | Manual review: "source" field handling documented |
| uni-agent-routing.md swarm template shows auto-chain | Manual review: delivery template includes deploy step |
| Safeguard 1: deploy verifies gate reports before proceeding | Manual review: already present in deploy-SM Step 1 — verify preserved |
| Safeguard 2: security reviewer spawned with fresh context | Manual review: already present — verify preserved |
| Safeguard 3: blocking findings return BLOCKED to human | Manual review: already present — verify preserved |
| Combined return format: impl + deploy results together | Manual review: return format shows both |
| Error handling: deploy failure does not lose impl results | Manual review: error handling section present |

**Constraint (from SR-03):** Deploy-scrum-master must remain independently invocable. Auto-chain is additive, not replacing.

### AC-06: Protocol Compliance Fixes

**Requirement:** Minor protocol updates for branch protection and observed deviations.

| Sub-criteria | Verification |
|-------------|-------------|
| Delivery protocol Phase 4 uses `--rebase` merge | Manual review: `gh pr merge --rebase` in Phase 4 |
| Bugfix protocol uses same merge strategy | Manual review: merge command matches |
| Stage 3b one-agent-per-component documented as MANDATORY | Manual review: explicit "MANDATORY" label with anti-pattern reference |
| Post-delivery review step added (optional) | Manual review: optional step after Phase 4 |
| Cargo JSON output format documented as preferred | Manual review: `--format json` mentioned |

### AC-07: GH Issue as Status Hub

**Requirement:** All coordinators post structured updates to GH Issues.

| Sub-criteria | Verification |
|-------------|-------------|
| Standard comment format defined | Manual review: format template present |
| Impl-scrum-master comments after each gate | Manual review: already specified — verify format matches standard |
| Deploy-scrum-master comments with security review result | Manual review: comment step present |
| Bugfix-scrum-master format matches standard | Manual review: reconcile with existing format (from SR-07) |
| Auto-chain deploy comments append to same GH Issue | Manual review: deploy uses same issue number from impl |

### AC-08: Procedural Knowledge Integration

**Requirement:** Agents query and store procedural knowledge.

| Sub-criteria | Verification |
|-------------|-------------|
| uni-rust-dev.md includes knowledge query before implementation | Manual review: `context_search` call in agent prompt |
| uni-pseudocode.md includes knowledge query before design | Manual review: `context_search` call in agent prompt |
| uni-tester.md includes knowledge query before test execution | Manual review: `context_search` call in agent prompt |
| Coordinator agents include procedure storage after successful delivery | Manual review: `store-procedure` guidance present |
| Bugfix agents include procedure storage for reproducible diagnostics | Manual review: `store-procedure` guidance present |
| Procedure storage guidance distinguishes procedures from workflows | Manual review: distinction documented |
| Knowledge queries are non-blocking with graceful degradation | Manual review: timeout and fallback documented |

**Constraint (from SR-04):** All knowledge queries must have a 5-second timeout and proceed without knowledge if unavailable.

### AC-09: Repository Hygiene

**Requirement:** Clean up current repo state to match new conventions.

| Sub-criteria | Verification |
|-------------|-------------|
| Orphaned stashes removed (after documenting contents) | Manual: `git stash list` returns empty |
| Fully-merged local branches deleted | Manual: `git branch --merged main` returns only main |
| Uncommitted changes resolved | Manual: `git status` is clean on target branch |
| `.claude/worktrees/` in `.gitignore` | Manual: line present |

---

## Non-Functional Requirements

| Requirement | Constraint |
|-------------|-----------|
| All changes are markdown-only | No Rust code, no build system changes, no test infrastructure changes |
| Token budget | Agent defs ≤ ~150 lines, protocols ≤ ~250 lines. Prefer referencing uni-git skill over inlining detail. Replace existing text, don't append parallel sections. |
| Backward compatibility | Existing feature branches (in progress) must not break |
| Branch protection compatible | All changes must work with PR-required workflow immediately |
| Worktree disk overhead | Acceptable for devcontainer (~3GB per active worktree) |
| Knowledge query latency | Max 5 seconds; non-blocking, proceed without if unavailable |

---

## File Change Matrix

| File | AC(s) | Change Summary |
|------|-------|---------------|
| `.claude/skills/uni-git/SKILL.md` | 01, 03, 04 | Branch naming, merge strategy, worktree lifecycle, build isolation |
| `.claude/protocols/uni/uni-design-protocol.md` | 02 | Branch creation at init, PR at Phase 2d |
| `.claude/protocols/uni/uni-delivery-protocol.md` | 03, 05, 06 | Worktree lifecycle, auto-chain, merge strategy, per-component enforcement |
| `.claude/protocols/uni/uni-bugfix-protocol.md` | 03, 06 | Worktree lifecycle, merge strategy |
| `.claude/protocols/uni/uni-agent-routing.md` | 05 | Swarm template update for auto-chain |
| `.claude/agents/uni/uni-design-scrum-master.md` | 02 | Branch init, PR at session end |
| `.claude/agents/uni/uni-implementation-scrum-master.md` | 03, 05, 07 | Worktree, deploy spawn, GH Issue format |
| `.claude/agents/uni/uni-deploy-scrum-master.md` | 05, 07 | Accept auto-chain spawn, GH Issue format |
| `.claude/agents/uni/uni-bugfix-scrum-master.md` | 03, 07 | Worktree, GH Issue format verification |
| `.gitignore` | 03, 09 | Add `.claude/worktrees/` |
| `.claude/agents/uni/uni-rust-dev.md` | 08 | Knowledge query before implementation |
| `.claude/agents/uni/uni-pseudocode.md` | 08 | Knowledge query before design |
| `.claude/agents/uni/uni-tester.md` | 08 | Knowledge query before test execution |

---

## Ordering Constraints

1. AC-09 (hygiene) should execute first to clean up repo state
2. AC-01 (git conventions) must be complete before AC-02 and AC-03 (they reference it)
3. AC-03 (worktree) and AC-04 (build isolation) are independent but both feed into delivery/bugfix protocol updates
4. AC-05 (auto-chain) depends on AC-03 being in the delivery protocol already
5. AC-06 (compliance) can be applied alongside AC-03/AC-05 during protocol edits
6. AC-07 (GH Issue format) can be applied alongside coordinator agent edits
7. AC-08 (knowledge integration) is independent of all other ACs
