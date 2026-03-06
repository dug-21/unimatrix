# uni-git — Git Conventions for Unimatrix

## Branch-First Workflow

All workflows produce PRs. No workflow commits directly to main.

### Branch Naming

| Context | Pattern | Example | Creator |
|---------|---------|---------|---------|
| Feature design (Session 1) | `design/{phase}-{NNN}` | `design/crt-009` | uni-design-scrum-master |
| Feature delivery (Session 2) | `feature/{phase}-{NNN}` | `feature/crt-009` | uni-implementation-scrum-master |
| Bug fix | `bugfix/{issue}-{desc}` | `bugfix/52-embed-retry` | uni-bugfix-scrum-master |
| Ad-hoc docs/config | `docs/{short-desc}` | `docs/update-vision` | Human or primary agent |
| Workflow/process | `workflow/{desc}` | `workflow/base-002` | Human or primary agent |

### Branch Lifecycle

```bash
# Create branch at session start
git checkout -b {branch-pattern}

# Commit after each gate pass or milestone
git add <files> && git commit -m "{prefix}: {description} (#{issue})"
git push -u origin {branch}

# Open PR when session completes
gh pr create --title "[{feature-id}] {title}" --body "..."

# After merge: branch auto-deletes (repo setting enabled)
```

## Commit Format

```
{prefix}: {description} (#{issue})
```

| Prefix | When |
|--------|------|
| `design:` | Design docs (Session 1) |
| `pseudocode:` | Stage 3a artifacts |
| `impl({component}):` | Component implementation |
| `test:` | Test execution |
| `fix({gate}):` | Gate rework |
| `fix:` | Bug fix |
| `docs:` | Standalone documentation changes |

## PR Merge Strategy

**Rebase-only** (`gh pr merge --rebase`). Squash acceptable for single-commit PRs. Merge commits are disabled at repo level.

## PR Template

```bash
gh pr create \
  --title "[{feature-id}] {short description}" \
  --body "$(cat <<'EOF'
## Summary
Implements {feature-id} per approved design.

## Source Documents
- Architecture: product/features/{id}/architecture/ARCHITECTURE.md
- Specification: product/features/{id}/specification/SPECIFICATION.md
- Risk Strategy: product/features/{id}/RISK-TEST-STRATEGY.md

## Gate Results
- Gate 3a (Design Review): PASS
- Gate 3b (Code Review): PASS
- Gate 3c (Risk Validation): PASS

## GH Issue
Closes #{N}
EOF
)"
```

## Worktree Isolation

Coordinators use Claude Code's native `isolation: "worktree"` parameter when spawning worker agents. Claude Code automatically creates worktrees at `.claude/worktrees/agent-{id}/` and cleans up afterward.

**Coordinator responsibilities:**
- Spawn agents with `isolation: "worktree"` for parallel workstreams
- Exit gate includes worktree cleanup: `git worktree remove .claude/worktrees/agent-{id}/`
- If removal fails (dirty state): warn human, do NOT force-remove

**Stale worktree recovery:** `git worktree prune` removes entries for deleted directories. Human can `git worktree remove --force` if needed.

## Build Artifact Isolation

| Binary | Location | Used by |
|--------|----------|---------|
| Installed | `~/.local/bin/unimatrix-server` | Hooks, MCP server |
| Build artifact | `target/release/unimatrix-server` | Integration tests |

- `cargo build --release` in a worktree does NOT affect `~/.local/bin/` or other worktrees (each worktree has its own `target/`)
- To update the installed binary: `cargo install --path crates/unimatrix-server`
- Integration tests in worktrees: set `UNIMATRIX_BINARY` to the worktree's own `target/release/unimatrix-server`

## Rules

- Never force-push to main
- Never commit `.env`, credentials, or build artifacts
- Never skip pre-commit hooks (`--no-verify`)
- Feature branches auto-delete after merge
