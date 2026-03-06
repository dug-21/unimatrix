# base-002: Workflow & Branching Improvements

## Summary

Modernize the development workflow to support parallel workstreams, respect branch protection, and reduce human touchpoints. The project has outgrown its original single-stream, commit-to-main conventions. This scope addresses the structural changes needed before the project can safely scale its delivery cadence.

## Context

- Branch protection is now active on main (PR required, no review needed for owner, no force-push)
- Current protocols and skills still reference direct-to-main commits — they will fail
- Multi-workstream development in a single devcontainer causes lost commits and stash conflicts
- Implementation→deploy handoff requires an unnecessary human touchpoint
- `cargo build --release` overwrites the same binary that integration tests consume; parallel feature development would race on this artifact
- Retrospective data reveals protocol deviations (monolithic agent spawns, undocumented post-delivery phase) that compound delivery friction

## Non-Goals

- Testing infrastructure optimization (Docker caching, suite pooling, incremental selection) — deferred; parallelized workstreams reduce the blocking impact of test duration
- CI/CD pipeline (GitHub Actions) — deferred to when branch protection status checks become relevant
- Intelligence/confidence validation harness — separate scope (base-003)
- Moves away from current uni-{phase}-scrum-master orchestration model
- reduced external reviews within workflows.  Not looking to reduce the validations performed in workflows

---

## Acceptance Criteria

### AC-01: Branch-First Git Conventions

All workflows produce PRs. No workflow commits directly to main.

- [ ] `uni-git/SKILL.md` updated: remove "commit directly to main," define branch patterns for all contexts
- [ ] Branch naming convention enforced:

| Context | Pattern | Example |
|---------|---------|---------|
| Feature design (Session 1) | `design/{phase}-{NNN}` | `design/crt-009` |
| Feature delivery (Session 2) | `feature/{phase}-{NNN}` | `feature/crt-009` |
| Bug fix | `bugfix/{issue}-{desc}` | `bugfix/52-embed-retry` |
| Ad-hoc docs/config | `docs/{short-desc}` | `docs/update-product-vision` |
| Workflow/process | `workflow/{desc}` | `workflow/base-002` |

- [ ] Commit prefix table extended with `docs:` for standalone documentation changes
- [ ] PR merge strategy documented: rebase or squash only (merge commits disabled at repo level — already done)

### AC-02: Design Protocol Branch Integration

Session 1 (Design) uses a branch instead of committing to main.

- [ ] `uni-design-protocol.md` updated: create `design/{phase}-{NNN}` branch at session start
- [ ] Design artifacts committed to design branch, not main
- [ ] Session 1 ends with PR to main (aligns with existing human approval gate)
- [ ] `uni-design-scrum-master.md` updated to reflect branch creation and PR at session end
- [ ] Human approval gate becomes PR approval + merge (single action replaces two)

### AC-03: Worktree-Based Branch Isolation

Coordinator agents create isolated worktrees so multiple workstreams don't collide.

- [ ] `uni-delivery-protocol.md` updated: create worktree at initialization, remove at exit
- [ ] `uni-bugfix-protocol.md` updated: same worktree lifecycle
- [ ] `uni-implementation-scrum-master.md` updated: worktree in initialization and exit gate
- [ ] `uni-bugfix-scrum-master.md` updated: worktree in initialization and exit gate
- [ ] `.gitignore` updated: add `.claude/worktrees/`
- [ ] Worktree path convention: `.claude/worktrees/{branch-type}-{id}/`
- [ ] Exit gate checklist includes worktree cleanup (`git worktree remove`)
- [ ] Stale worktree recovery documented (what to do if session dies mid-work)

### AC-04: Build Artifact Isolation

Development builds don't overwrite the production binary or race with parallel workstreams.

- [ ] Identify current build-to-install pipeline: how `~/.local/bin/unimatrix-server` gets updated vs `target/release/unimatrix-server`
- [ ] Document the separation: hooks use installed binary (`~/.local/bin/`), integration tests use build artifact (`target/release/`)
- [ ] Worktree builds use per-worktree target dir (cargo default — each worktree checkout gets its own `target/`)
- [ ] Add protocol guidance: `cargo build --release` in a worktree does NOT affect the installed binary or other worktrees
- [ ] Add guidance for updating the installed binary: explicit `cargo install --path crates/unimatrix-server` (intentional promotion, not accidental)
- [ ] Integration test harness respects `UNIMATRIX_BINARY` env var — document that worktree tests should set this to their own `target/release/unimatrix-server`

### AC-05: Implementation → Deploy Auto-Chain

Implementation completion automatically triggers PR review without human intervention.

- [ ] `uni-implementation-scrum-master.md` updated: Phase 4 spawns `uni-deploy-scrum-master` after PR open
- [ ] `uni-deploy-scrum-master.md` updated: accepts spawn from impl-scrum-master (not just human)
- [ ] `uni-agent-routing.md` updated: swarm template shows auto-chain (Session 2 includes deploy)
- [ ] Safeguard 1: deploy verifies all gate reports exist and show PASS before proceeding
- [ ] Safeguard 2: `uni-security-reviewer` spawned with fresh context (no impl context leakage)
- [ ] Safeguard 3: blocking security findings return BLOCKED to human (not auto-merged)
- [ ] Combined return message format: impl results + deploy results + merge readiness in one response

### AC-06: Protocol Compliance Fixes

Minor protocol updates to align with branch protection and observed deviations.

- [ ] `uni-delivery-protocol.md` Phase 4: PR uses `--rebase` merge (not squash or merge commit)
- [ ] `uni-bugfix-protocol.md`: same merge strategy alignment
- [ ] Delivery protocol: Stage 3b must spawn one agent per component (document as MANDATORY, reference monolithic agent anti-pattern from retro)
- [ ] Post-delivery review: add optional step after Phase 4 for tech debt discovery and GH issue filing (formalize the organic pattern observed in crt-006)
- [ ] Cargo output truncation rules: add `cargo test --workspace -- --format json` as preferred over grep-based filtering (if available in toolchain)

### AC-07: GH Issue as Status Hub

All workflow status flows through the GH Issue. Coordinators post structured updates at each milestone.

- [ ] Standardize GH Issue comment format across all coordinators (impl, deploy, bugfix):
  ```
  ## {Phase/Gate} -- {PASS|FAIL|BLOCKED}
  - Stage: {name}
  - Files: [paths]
  - Tests: X passed, Y new
  - Issues: [if any]
  ```
- [ ] Impl-scrum-master: comment after each gate (3a, 3b, 3c) -- already specified but verify consistency
- [ ] Deploy-scrum-master: comment with security review result and merge readiness
- [ ] Bugfix-scrum-master: already uses GH Issue as single source of truth -- verify format matches
- [ ] Auto-chain (AC-05): deploy comments append to the same GH Issue thread as impl comments

### AC-08: Procedural Knowledge Integration

Agents store detailed procedures in Unimatrix and query for procedural guidance before acting.

Currently: `/query-patterns` is used before design/implementation (patterns only). `/store-procedure` is only used during retrospectives. No agent queries for procedures before executing multi-step operations.

- [ ] Worker agents (uni-rust-dev, uni-pseudocode, uni-tester) updated: query `/knowledge-search` for procedures relevant to their objective before starting (e.g., "server integration file order," "crate bootstrapping sequence," "gate verification steps")
- [ ] Coordinator agents updated: after successful delivery, use `/store-procedure` if a reusable multi-step technique was used or discovered (not just during retrospectives)
- [ ] Bugfix agents: `/store-lesson` after fix (already done); add `/store-procedure` if the fix involved a reproducible diagnostic or repair sequence
- [ ] Procedure storage guidance added to agent prompts: "If you executed a multi-step sequence that would help future agents in similar situations, store it as a procedure"
- [ ] Distinguish from workflow choreography: procedures are "how to do X" (store in Unimatrix), workflows are "what order to do things" (stay in protocol files)

### AC-09: Repository Hygiene

Clean up current state to match new conventions.

- [ ] Remove orphaned stashes (`git stash drop` after documenting contents)
- [ ] Delete fully-merged local feature branches
- [ ] Resolve uncommitted changes on current branch (commit or discard)
- [ ] Verify `.claude/worktrees/` in `.gitignore`

---

## Constraints

- All protocol/agent changes are markdown-only — no Rust code changes in this scope
- Branch protection is already active — changes must work with PR-required workflow immediately
- Hooks use `unimatrix-server` from PATH (`~/.local/bin/`) — do not change this; it's already isolated from `target/release/`
- Worktree disk overhead (~3GB per active worktree) is acceptable for a devcontainer
- No changes to integration test harness infrastructure (deferred)

## Risks

| Risk | Severity | Likelihood | Mitigation |
|------|----------|-----------|------------|
| Design PRs add friction to Session 1 | Medium | Medium | PR approval replaces existing human approval — net zero touchpoints |
| Worktree cleanup forgotten, disk fills | Low | Medium | Exit gate checklist; `git worktree prune` in session recovery |
| Auto-chain masks implementation issues | Medium | Low | 3 gates already validated; security reviewer has fresh context |
| Cargo target dirs in worktrees slow first build | Low | High | Expected; cached after first build per worktree |
| Agent confusion on branch conventions | Medium | Medium | Single source of truth: updated `uni-git/SKILL.md` |

## Scope Boundary

**In scope:**
- Git conventions, branch naming, worktree adoption (AC-01, AC-03, AC-07)
- Design protocol branch integration (AC-02)
- Build artifact isolation documentation (AC-04)
- Impl→deploy auto-chain (AC-05)
- Protocol compliance fixes from retrospective findings (AC-06)

**Out of scope:**
- Docker buildkit cache, suite pooling, incremental test selection (testing infra — deferred)
- GitHub Actions CI pipeline
- Unimatrix knowledge extraction improvements (4E from retro)
- Scope outlier detection system (4C from retro)
- Intelligence validation harness (base-003)

## Files to Modify

| File | Change |
|------|--------|
| `.claude/skills/uni-git/SKILL.md` | Branch naming, PR workflow, merge strategy, commit prefixes |
| `.claude/protocols/uni/uni-design-protocol.md` | Branch creation, PR at session end |
| `.claude/protocols/uni/uni-delivery-protocol.md` | Worktree lifecycle, Phase 4 auto-chain, merge strategy, per-component enforcement |
| `.claude/protocols/uni/uni-bugfix-protocol.md` | Worktree lifecycle, merge strategy |
| `.claude/protocols/uni/uni-agent-routing.md` | Swarm template update for auto-chain |
| `.claude/agents/uni/uni-design-scrum-master.md` | Branch creation at init, PR at session end |
| `.claude/agents/uni/uni-implementation-scrum-master.md` | Worktree init/cleanup, Phase 4 deploy spawn |
| `.claude/agents/uni/uni-deploy-scrum-master.md` | Accept spawn from impl-scrum-master |
| `.claude/agents/uni/uni-bugfix-scrum-master.md` | Worktree init/cleanup |
| `.gitignore` | Add `.claude/worktrees/` |
| `.claude/agents/uni/uni-rust-dev.md` | Add procedural knowledge query before implementation |
| `.claude/agents/uni/uni-pseudocode.md` | Add procedural knowledge query before design |
| `.claude/agents/uni/uni-tester.md` | Add procedural knowledge query before test execution |

## References

- Research: `product/workflow/base-002/RESEARCH.md`
- Current git conventions: `.claude/skills/uni-git/SKILL.md`
- Retrospective data: `product/research/ass-013/`
- Branch protection: applied via `gh api` (2026-03-06)
