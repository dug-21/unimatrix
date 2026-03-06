# base-002: Workflow Process Improvements — Research

## Problem Statement

Feature delivery has slowed due to compounding friction across four areas:
1. Testing takes hours (build + 1,634 unit + 167 integration tests)
2. Git branching fails under multi-workstream development (lost commits, stash conflicts)
3. Unnecessary human touchpoint between implementation completion and PR review
4. Undiscovered friction patterns hiding in retrospective data

This document synthesizes parallel research across all four areas plus retrospective mining.

---

## 1. Testing Bottleneck

### Current State
- **1,634 unit tests** across 9 crates (~5.8s cached run)
- **167 integration tests** across 9 Python suites (~20 min full run)
- **19 smoke tests** (~30-60s)
- Unit tests are fast; integration tests are the bottleneck
- Docker rebuilds the binary from scratch every run (no layer caching)
- Each of 167 tests spawns its own server process (fixture scope: function)
- Embedding model loads per server instance
- All tests sequential (TEST_WORKERS=1, "not designed for parallel execution")

### Bottlenecks (Ranked by Impact)

| # | Bottleneck | Impact | Root Cause |
|---|-----------|--------|------------|
| 1 | Docker binary rebuild per run | 10-15 min | No buildkit cache mounts for cargo registry |
| 2 | Per-test server spawn | 2-3 min total | 167 subprocess spawns × ~500ms each |
| 3 | Embedding model cold start | 1-2 min | Model loads into memory per server instance |
| 4 | Sequential test execution | 5-10 min potential | Fixtures assume exclusive server access |
| 5 | Gate 3c redundancy | 30-60s | Smoke runs twice (stage 3c + gate 3c) |
| 6 | Heavy dependency compilation | Amortized | rusqlite bundled, ort, hnsw_rs from source |
| 7 | Suite imbalance | <1 min | test_tools.py = 68 tests (41% of total) |

### Proposed Optimizations

**Phase 1 — Quick Wins (Week 1, ~20 min savings):**

| Optimization | Impact | Effort | Risk |
|-------------|--------|--------|------|
| Docker buildkit cache mounts | -8 to 10 min | Low (1-2h) | Low |
| Smoke-test-first delivery gate | -10 to 15 min | Low (2-3h) | Low |
| Parallel unit + integration | -5 to 8 min | Low (1-2h) | Low |
| Embedding model pre-cache | -1 to 2 min | Low (1h) | Low |

**Phase 2 — Deep Optimizations (Week 2-3, ~5-10 min additional):**

| Optimization | Impact | Effort | Risk |
|-------------|--------|--------|------|
| Suite-scoped server pooling | -3 to 5 min | Medium (4-6h) | Medium — state leakage |
| Incremental suite selection by crate | -5 to 10 min per feature | Medium (3-4h) | Medium — missed cross-crate bugs |
| Pre-built Docker image registry | -5 to 8 min CI only | Medium (3-4h) | Low |

**Expected Result:**
- Full suite: 20 min → 8-10 min (Phase 1) → 5-6 min (Phase 2)
- Feature delivery: 21 min → 8-12 min (Phase 1) → 3-5 min (Phase 2)

---

## 2. Git Branching Strategy

### Current State
- Single VSCode devcontainer = single working directory
- 17 local branches (8 feature/*, 3 bugfix/*)
- 2 orphaned stashes from context switches
- 7 uncommitted changes on current branch (mix of feature + bugfix work)
- Design phase commits to main; delivery creates feature branches
- No isolation between concurrent workstreams

### Pain Points

| Pain Point | Root Cause | Impact |
|-----------|-----------|--------|
| Lost commits on branch switch | Uncommitted changes + forced stash | High — rework |
| Staged commit delays | Gate progression blocked by context switch | Medium — delays |
| Multi-workstream collision | Single working directory, both branches share state | High — confusion |
| Untracked feature dirs on wrong branch | Session 1→2 transition across branches | Medium — orphaned work |
| Branch divergence during long delivery | Feature branches live weeks | Medium — merge conflicts |

### Breaking Change: No More Direct Commits to Main

With branch protection now active (§5), the longstanding convention of committing directly to main is **no longer possible**. This affects:

**Session 1 (Design):**
- `uni-git/SKILL.md` line 5: "Commit design docs directly to `main` (markdown only, non-destructive)"
- `uni-design-protocol.md`: Design agents write to `product/features/{id}/` and commit to main
- This includes SCOPE.md, architecture, specification, risk strategy, alignment report, implementation brief

**Ad-hoc changes:**
- Small doc fixes, CLAUDE.md updates, agent definition tweaks, product vision edits
- Currently pushed directly to main with no PR

**Required changes:**

1. **Design sessions need a branch**: Create `design/{phase}-{NNN}` at Session 1 start, PR to main at Session 1 end (before human approval gate). Natural fit — human approval already exists, now it's a PR approval too.

2. **Ad-hoc changes need lightweight PRs**: Small doc/config changes use a short-lived branch + PR. Self-review adds ~30 seconds but prevents accidental pushes.

3. **Update `uni-git/SKILL.md`**: Replace "commit directly to main" with branch-based workflow for all sessions.

4. **Update `uni-design-protocol.md`**: Add branch creation at initialization, PR at session end.

5. **Update commit prefixes**: Add `docs:` prefix for standalone documentation PRs outside feature scope.

**Proposed branch naming:**

| Context | Branch Pattern | Example |
|---------|---------------|---------|
| Feature design (Session 1) | `design/{phase}-{NNN}` | `design/crt-009` |
| Feature delivery (Session 2) | `feature/{phase}-{NNN}` | `feature/crt-009` |
| Bug fix | `bugfix/{issue}-{desc}` | `bugfix/52-embed-retry` |
| Ad-hoc docs/config | `docs/{short-desc}` | `docs/update-product-vision` |
| Workflow/process changes | `workflow/{desc}` | `workflow/base-002` |

### Recommended Solution: Git Worktrees

**Why worktrees:** Complete branch isolation per workstream. No stashes, no lost commits, no context-switch overhead. Each coordinator gets its own checked-out copy.

**How it works:**
```
/workspaces/unimatrix                        (main — always clean)
/.claude/worktrees/feature-col-011/          (isolated feature work)
/.claude/worktrees/bugfix-52/                (isolated bugfix)
```

**Protocol changes:**
- Delivery protocol: create worktree at initialization, remove at exit
- Bugfix protocol: same pattern
- Add `.claude/worktrees/` to `.gitignore`
- Claude Code agents already support `isolation: "worktree"` parameter

**Migration path:**
1. Week 1: Clean up stashes + merged branches, trial worktree on next bugfix
2. Week 2: Update coordinator agent definitions + protocols
3. Week 3: Full rollout, monitor for cleanup issues
4. Week 4+: Enforce via agent spawn checklist

**Trade-offs:**
- Disk: ~3GB per active worktree (source + cargo target)
- Cleanup: must `git worktree remove` on session exit
- Cargo: separate build cache per worktree (slower first build)

---

## 3. Implementation → Deploy Auto-Transition

### Current Flow (3 human touchpoints)
```
Human → spawn impl-scrum-master → [gates 3a/3b/3c] → PR opened
  → RETURN TO HUMAN (touchpoint 1)
Human reviews PR → spawn deploy-scrum-master (touchpoint 2)
  → security review → merge readiness
  → RETURN TO HUMAN (touchpoint 3: merge decision)
```

### Proposed Flow (1 human touchpoint)
```
Human → spawn impl-scrum-master → [gates 3a/3b/3c] → PR opened
  → AUTO-SPAWN deploy-scrum-master (no human step)
    → verify gate reports → security review (fresh context) → merge readiness
  → RETURN TO HUMAN (single touchpoint: merge decision)
```

### Design: Option A — Direct Spawn (Recommended)

The bugfix coordinator already uses this pattern: single-session with security review spawn at the end. Extend to feature delivery.

**Changes required:**
1. `uni-implementation-scrum-master.md`: Add deploy-scrum-master spawn after PR open
2. `uni-deploy-scrum-master.md`: Accept spawn from impl-scrum-master (not just human)
3. `uni-agent-routing.md`: Update swarm template to show auto-chain

**Three safeguards:**
1. **Gate report verification**: Deploy step 1 already checks all gates PASS — if impl crashed, deploy stops
2. **Fresh-context security review**: uni-security-reviewer gets ONLY the PR diff, no impl context contamination
3. **Blocking escalation**: Security findings → deploy returns BLOCKED → human decides

**Value of removed touchpoint:** Low — human was rubber-stamping "yes, proceed to security review." The actual review happens when merging.

**Risk assessment:**
| Risk | Likelihood | Severity | Mitigation |
|------|-----------|----------|------------|
| Security review on broken code | Low | High | 3 gates already validated |
| Context window overflow | Low | Medium | Deploy is thin; reviewer gets fresh context |
| No human PR review before deploy | Medium | Low | Human reviews when merging |

---

## 4. Retrospective Mining — Additional Findings

### Top 5 New Patterns (Beyond the 3 threads above)

#### 4A. Monolithic Agent Anti-Pattern (High Impact)
Stage 3b collapsed to 1 agent across 8 components instead of spawning per-component agents. Result: 40% wasted context, no parallelization, sequential file edits.
- **Evidence**: crt-006 telemetry — 1 SubagentStart for 9 source files
- **Fix**: Hotspot detection for SubagentStart count < expected components

#### 4B. Post-Delivery Review Phase (Medium Impact, Undocumented)
After "SESSION COMPLETE," agents continue investigating — producing GH issues, patterns, and tech debt discovery. This phase generated real value in crt-006 (2 issues, 1 knowledge entry) but is not in the protocol.
- **Fix**: Formalize as post-delivery review step; track "post-completion tool calls"

#### 4C. Scope Outlier Detection (High Impact, Rare)
crt-006 was 2-4x every baseline metric (files, artifacts, ADRs, duration). No early warning fired.
- **Fix**: Compound hotspot: flag features exceeding 3+ scope metrics above baseline

#### 4D. Large-File Edit Bloat (High Impact)
Edit responses echo entire files. 17 edits to files >50KB generated 1,793KB of response data (44% of all tool bytes in crt-006).
- **Fix**: Design patterns that isolate large integration files; flag "context-expensive files" in briefing

#### 4E. Zero Knowledge Consultation During Delivery (Strategic)
Unimatrix usage: 10% during design, 0% during implementation, 4% post-delivery. Agents ignore knowledge when coding.
- **Fix**: Auto-extract procedural knowledge from delivery (e.g., "server integration file order"); proactive surfacing during briefing

### Additional Patterns

| Pattern | Category | Impact | Actionable? |
|---------|---------|--------|-------------|
| Cold restart context reload (31K tokens wasted) | Context | Medium | Yes — proactive timeout warning |
| context_store permission retries (10/18 failed first try) | Tooling | Medium | Yes — error messaging, backoff |
| Coordinator respawn cycles (5 spawns in crt-006) | Session | Medium | Yes — track respawns, improve design feedback |
| Search-via-Bash compliance violations | Compliance | Low | Yes — hotspot detection |
| Cargo test output parsing struggles | Build | Low-Medium | Yes — structured output |
| GH API latency (70+ min in Phase 4) | Infrastructure | Medium | Set expectations, track metric |
| Phase duration baselines available | Anomaly detection | Medium | Collect metrics for 5+ features |

---

## 5. GitHub Branch Protection

### Current State
- Public repo (dug-21/unimatrix), no branch protection on any branch
- No collaborators beyond owner (dug-21, admin)
- No GitHub Actions workflows configured
- All merge strategies enabled, auto-merge disabled
- Forks allowed (public repo)
- Solo maintainer — all changes currently via direct push to main or feature branches

### Threat Model

| Threat | Likelihood | Severity |
|--------|-----------|----------|
| Accidental force-push overwrites main | Medium | High |
| External spam PRs from forks | Low | Low |
| Compromised auth token pushes malicious code | Low | Critical |
| Accidental branch/tag deletion | Low | High |
| Non-linear history (merge commits break bisect) | Medium | Medium |

### Recommended Protection: "Safe Solo Maintenance"

All recommendations are available on **GitHub Free tier**.

| Protection | Rationale | Friction |
|-----------|-----------|---------|
| Require PR for main | Every change vetted, even your own | 3 extra CLI commands per merge |
| PR required, 0 reviews | Owner can merge own PRs; external contributors still need owner to merge | None beyond creating PR |
| Require linear history | Clean git log, safe bisect, no accidental merges | Must use "Rebase and merge" |
| Prevent force-push | Cannot overwrite main history | None |
| Prevent branch deletion | main cannot be deleted | None |
| Auto-delete merged branches | Stale branches cleaned up automatically | None |
| Dismiss stale reviews | N/A with 0 reviews — available if review count raised later | N/A |

### Implementation: All-in-One Command

```bash
# Set all branch protections on main
gh api repos/dug-21/unimatrix/branches/main/protection \
  --input - << 'EOF'
{
  "required_status_checks": null,
  "enforce_admins": true,
  "required_pull_request_reviews": {
    "required_approving_review_count": 1,
    "dismiss_stale_reviews": true,
    "require_code_owner_reviews": false,
    "require_last_push_approval": true
  },
  "restrictions": null,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "require_linear_history": true,
  "required_deployment_environments": []
}
EOF

# Auto-delete merged branches
gh api repos/dug-21/unimatrix -f delete_branch_on_merge=true

# Verify
gh api repos/dug-21/unimatrix/branches/main/protection
```

### Workflow Change After Protection

**Before:** `git push origin main` (direct push)

**After:**
```bash
git push origin feature/my-work
gh pr create --title "..." --body "..."
gh pr review <PR#> --approve
gh pr merge <PR#> --rebase
```

### Minimum-Friction Alternative

If PR ceremony feels too heavy, absolute minimum (no PRs required):

```bash
gh api repos/dug-21/unimatrix/branches/main/protection \
  --input - << 'EOF'
{
  "enforce_admins": true,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "require_linear_history": true,
  "required_pull_request_reviews": null,
  "required_status_checks": null
}
EOF
```

This prevents force-push, deletion, and non-linear history — but still allows direct push. Not recommended for production, but zero friction.

### What NOT to Protect Now

- **Feature/bugfix branches** — throwaway, you're the only contributor
- **Status checks (CI)** — no GitHub Actions yet; add when CI exists
- **Code owners** — only 1 person, unnecessary
- **Tag protection** — add when releasing stable versions

### Future Additions (When CI Exists)

- Required status checks (cargo test, cargo clippy, integration smoke)
- Auto-close spam PRs via GitHub Actions
- Dependabot for dependency updates
- Tag protection for releases

---

## Proposed Scope: base-002

### Work Items

| ID | Item | Source | Effort | Priority |
|----|------|--------|--------|----------|
| W-01 | Docker buildkit cache mounts | Testing §1 | S | P0 |
| W-02 | Smoke-first delivery gate | Testing §1 | S | P0 |
| W-03 | Parallel unit + integration | Testing §1 | S | P0 |
| W-04 | Embedding model pre-cache | Testing §1 | S | P1 |
| W-05 | Worktree-based branch isolation | Git §2 | M | P0 |
| W-06 | Impl→Deploy auto-chain | Transition §3 | M | P0 |
| W-07 | Suite-scoped server pooling | Testing §1 | M | P1 |
| W-08 | Incremental suite selection | Testing §1 | M | P1 |
| W-09 | Monolithic agent detection | Retro §4A | S | P1 |
| W-10 | Post-delivery review formalization | Retro §4B | S | P2 |
| W-11 | Scope outlier early warning | Retro §4C | M | P2 |
| W-12 | GitHub branch protection on main | Protection §5 | S | P0 — DONE |
| W-13 | Eliminate direct-to-main commits (uni-git, design protocol, branch naming) | Git §2 | M | P0 |

### Phasing

**Phase 1 (P0 — immediate):** W-01, W-02, W-03, W-05, W-06, W-12 (DONE), W-13
- Testing quick wins + worktree adoption + auto-chain + branch protection + no-direct-main
- Expected: 50% reduction in delivery cycle time + repo safety baseline

**Phase 2 (P1 — next sprint):** W-04, W-07, W-08, W-09
- Deeper test optimization + protocol enforcement
- Expected: additional 20-30% reduction

**Phase 3 (P2 — backlog):** W-10, W-11
- Retrospective-driven improvements
- Expected: quality/predictability gains

---

## References

- Testing bottleneck: `product/test/infra-001/USAGE-PROTOCOL.md`, Dockerfile, conftest.py
- Git branching: `.claude/skills/uni-git/SKILL.md`, delivery + bugfix protocols
- Auto-chain: Agent definitions in `.claude/agents/uni/`
- Retrospective data: `product/research/ass-013/` (deep-findings, compound-signals, auto-knowledge)
- Observation analysis: crt-006 telemetry, vnc-002 gate reports
