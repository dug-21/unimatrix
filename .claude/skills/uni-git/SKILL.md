# uni-git — Git Conventions for Unimatrix

## Branch Strategy

- **Session 1 (Design)**: Commit design docs directly to `main` (markdown only, non-destructive)
- **Session 2 (Delivery)**: Work on `feature/{phase}-{NNN}` branch. PR to main after Gate 3c.

## Branch Lifecycle

```bash
# Delivery Leader creates at Session 2 start
git checkout -b feature/{phase}-{NNN}

# Commit after each gate pass (Delivery Leader)
git add <files> && git commit -m "{stage}: {description} (#{issue})"
git push -u origin feature/{phase}-{NNN}

# After Gate 3c — open PR
gh pr create --title "[{feature-id}] {title}" --body "..."
```

## Commit Format

```
{stage}: {description} (#{issue})
```

| Stage | Prefix |
|-------|--------|
| Design docs (Session 1) | `design:` |
| Stage 3a artifacts | `pseudocode:` |
| Component implementation | `impl({component}):` |
| Test execution | `test:` |
| Gate fix (rework) | `fix({gate}):` |

Examples:
- `design: architecture + specification for nxs-001 (#12)`
- `pseudocode: component design + test plans (#12)`
- `impl(storage-engine): implement from pseudocode (#12)`
- `test: risk coverage validation (#12)`

## Commit Checkpoints (Delivery Leader)

| When | What to Commit |
|------|---------------|
| After Gate 3a PASS | pseudocode/, test-plan/, updated IMPLEMENTATION-BRIEF.md |
| After each Stage 3b agent returns | Component code + tests |
| After Gate 3c PASS | testing/RISK-COVERAGE-REPORT.md, gate reports |

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

## Rules

- Never force-push to main
- Never commit `.env`, credentials, or build artifacts
- Never skip pre-commit hooks (`--no-verify`)
- Feature branches are deleted after merge
