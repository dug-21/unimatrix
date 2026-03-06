# Acceptance Map: base-002

## How to Read This Map

Each acceptance criterion from SCOPE.md is mapped to its verification method, the component that satisfies it, and the risk(s) it mitigates.

---

## AC-01: Branch-First Git Conventions

| Sub-criterion | Component | Verification | Risk |
|--------------|-----------|-------------|------|
| Remove "commit directly to main" from uni-git | git-conventions | Grep: zero matches for "directly to main" in SKILL.md | R-01 |
| Branch naming table (5 contexts) | git-conventions | Manual: table has 5 rows (design, feature, bugfix, docs, workflow) | -- |
| `docs:` commit prefix | git-conventions | Manual: prefix table includes `docs:` | -- |
| PR merge strategy documented | git-conventions | Manual: rebase-only section present | R-08 |

## AC-02: Design Protocol Branch Integration

| Sub-criterion | Component | Verification | Risk |
|--------------|-----------|-------------|------|
| Branch creation at session start | design-protocol, design-scrum-master | Manual: `git checkout -b design/{id}` in init | R-04 |
| Artifacts committed to design branch | design-protocol | Grep: no "commit to main" in design protocol | R-01 |
| PR to main at session end | design-protocol, design-scrum-master | Manual: `gh pr create` in Phase 2d | R-04 |
| Scrum-master reflects branch + PR | design-scrum-master | Manual: init and exit gate updated | R-04 |
| Human approval = PR approval + merge | design-protocol | Manual: return message references PR | -- |

## AC-03: Worktree-Based Branch Isolation

| Sub-criterion | Component | Verification | Risk |
|--------------|-----------|-------------|------|
| Delivery protocol worktree at init | delivery-protocol | Manual: `git worktree add` in Initialization | R-02 |
| Delivery protocol worktree at exit | delivery-protocol | Manual: `git worktree remove` in Phase 4 | -- |
| Bugfix protocol worktree lifecycle | bugfix-protocol | Manual: worktree add/remove present | R-02 |
| Impl-scrum-master worktree in init + exit | impl-scrum-master | Manual: both sections reference worktree | R-02 |
| Bugfix-scrum-master worktree in init + exit | bugfix-scrum-master | Manual: both sections reference worktree | R-02 |
| `.gitignore` updated | repo-hygiene | Manual: `.claude/worktrees/` line present | R-07 |
| Path convention documented | git-conventions | Manual: convention in worktree section | -- |
| Exit gate includes cleanup | impl-scrum-master, bugfix-scrum-master | Manual: exit gate checklist includes worktree | -- |
| Stale recovery documented | git-conventions | Manual: recovery section present (`git worktree prune`) | R-02 |

## AC-04: Build Artifact Isolation

| Sub-criterion | Component | Verification | Risk |
|--------------|-----------|-------------|------|
| Build pipeline documented | git-conventions | Manual: hooks vs tests separation explained | -- |
| Per-worktree target dir | git-conventions | Manual: cargo default behavior documented | -- |
| Worktree build guidance | git-conventions | Manual: "does NOT affect installed binary" | -- |
| Install promotion guidance | git-conventions | Manual: `cargo install` command documented | -- |
| UNIMATRIX_BINARY env var | git-conventions | Manual: env var guidance present | -- |

## AC-05: Implementation-to-Deploy Auto-Chain

| Sub-criterion | Component | Verification | Risk |
|--------------|-----------|-------------|------|
| Phase 4 spawns deploy-SM | impl-scrum-master | Manual: spawn command in Phase 4 | R-03 |
| Deploy-SM accepts auto-chain spawn | deploy-scrum-master | Manual: "source" field handling | R-03 |
| Routing template updated | agent-routing | Manual: delivery template shows deploy step | -- |
| Safeguard 1: gate verification | deploy-scrum-master | Manual: Step 1 preserved | R-03 |
| Safeguard 2: fresh-context security | deploy-scrum-master | Manual: fresh context preserved | -- |
| Safeguard 3: blocking returns BLOCKED | deploy-scrum-master | Manual: BLOCKED path preserved | R-03 |
| Combined return format | impl-scrum-master | Manual: both impl + deploy in return | R-03 |

## AC-06: Protocol Compliance Fixes

| Sub-criterion | Component | Verification | Risk |
|--------------|-----------|-------------|------|
| Delivery Phase 4 `--rebase` merge | delivery-protocol | Manual: `gh pr merge --rebase` | R-08 |
| Bugfix same merge strategy | bugfix-protocol | Manual: merge command matches | R-08 |
| Stage 3b per-component MANDATORY | delivery-protocol | Manual: "MANDATORY" label present | -- |
| Post-delivery review step | delivery-protocol | Manual: optional step after Phase 4 | -- |
| Cargo JSON output format | delivery-protocol | Manual: `--format json` mentioned | -- |

## AC-07: GH Issue as Status Hub

| Sub-criterion | Component | Verification | Risk |
|--------------|-----------|-------------|------|
| Standard comment format defined | impl-scrum-master | Manual: format template present | R-05 |
| Impl-SM comments after each gate | impl-scrum-master | Manual: comment steps present | R-05 |
| Deploy-SM security review comment | deploy-scrum-master | Manual: comment step present | R-05 |
| Bugfix-SM format matches standard | bugfix-scrum-master | Manual: format reconciled | R-05 |
| Auto-chain uses same GH Issue | impl-scrum-master | Manual: deploy receives issue number | -- |

## AC-08: Procedural Knowledge Integration

| Sub-criterion | Component | Verification | Risk |
|--------------|-----------|-------------|------|
| uni-rust-dev knowledge query | worker-agents | Manual: `context_search` in agent prompt | R-06 |
| uni-pseudocode knowledge query | worker-agents | Manual: `context_search` in agent prompt | R-06 |
| uni-tester knowledge query | worker-agents | Manual: `context_search` in agent prompt | R-06 |
| Coordinator procedure storage | impl-scrum-master, bugfix-scrum-master | Manual: `store-procedure` guidance | -- |
| Bugfix procedure storage | bugfix-scrum-master | Manual: `store-procedure` guidance | -- |
| Procedure vs workflow distinction | worker-agents | Manual: distinction documented | -- |
| Non-blocking with fallback | worker-agents | Manual: timeout and fallback present | R-06 |

## AC-09: Repository Hygiene

| Sub-criterion | Component | Verification | Risk |
|--------------|-----------|-------------|------|
| Orphaned stashes removed | repo-hygiene | `git stash list` returns empty | -- |
| Merged branches deleted | repo-hygiene | `git branch --merged main` returns main only | -- |
| Uncommitted changes resolved | repo-hygiene | `git status` clean | -- |
| `.claude/worktrees/` in .gitignore | repo-hygiene | Line present | R-07 |

---

## Coverage Summary

| AC | Sub-criteria | Risks Covered |
|----|-------------|---------------|
| AC-01 | 4 | R-01, R-08 |
| AC-02 | 5 | R-01, R-04 |
| AC-03 | 10 | R-02, R-07 |
| AC-04 | 5 | -- |
| AC-05 | 7 | R-03 |
| AC-06 | 5 | R-08 |
| AC-07 | 5 | R-05 |
| AC-08 | 7 | R-06 |
| AC-09 | 4 | R-07 |
| **Total** | **52** | **All 8 risks covered** |
