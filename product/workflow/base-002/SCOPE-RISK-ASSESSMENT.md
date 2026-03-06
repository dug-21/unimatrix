# Scope Risk Assessment: base-002

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Git worktrees may not be fully supported by Claude Code's `EnterWorktree` tool — agent isolation depends on platform behavior not fully validated | High | Medium | Architect should define a fallback path if worktree support is incomplete; keep worktree adoption optional initially |
| SR-02 | Worktree target directory isolation depends on cargo default behavior (per-checkout `target/`); custom CARGO_TARGET_DIR settings or `.cargo/config.toml` overrides could break isolation | Medium | Low | Document the exact cargo behavior relied upon; add verification step to worktree initialization |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | AC-05 (auto-chain) merges two previously independent coordinator lifecycles — deploy-scrum-master error handling may not account for being spawned mid-session by impl-scrum-master | Medium | Medium | Spec should define explicit error propagation contract between chained coordinators |
| SR-04 | AC-08 (procedural knowledge) introduces runtime Unimatrix queries in worker agents — if the server is unavailable or slow, it could block delivery | Medium | Medium | All knowledge queries should be non-blocking with graceful degradation (proceed without knowledge if unavailable) |
| SR-05 | AC-06 mandates `--rebase` merge strategy in protocols, but existing PRs may have non-linear history from previous merge commits | Low | Low | AC-09 (hygiene) should run before protocol changes take effect |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Design protocol branch integration (AC-02) changes the human approval gate from "review artifacts + approve" to "PR approval + merge" — agents must handle PR creation in Session 1 where they currently only commit | Medium | Medium | Architect should define which agent creates the PR (design-scrum-master vs synthesizer) and when |
| SR-07 | GH Issue comment format standardization (AC-07) may conflict with existing bugfix protocol which already defines its own comment format | Low | Medium | Specification should reconcile existing bugfix format with the proposed standard format |

## Assumptions

- **Worktree disk overhead acceptable**: SCOPE.md assumes ~3GB per active worktree is fine for a devcontainer. If the devcontainer has limited disk, multiple concurrent worktrees could exhaust storage. (SCOPE.md Constraints section)
- **Branch protection is already active and working**: SCOPE.md states this is done. If the protection configuration is incomplete or differs from what protocols assume, all branch-first changes will need adjustment. (SCOPE.md Context, line 9)
- **All changes are markdown-only**: SCOPE.md Constraints section. If any AC requires runtime behavior changes (e.g., AC-08 knowledge queries need server-side changes), scope will expand.

## Design Recommendations

- **SR-01, SR-02**: Architecture should separate worktree adoption into a distinct "isolation layer" so the rest of the workflow changes work regardless of whether worktrees are used.
- **SR-03**: Spec should define the auto-chain contract as a protocol extension, not a replacement — deploy-scrum-master must remain independently invocable.
- **SR-04**: Architect should specify that all `/knowledge-search` calls in worker agents use a timeout with fallback to "proceed without knowledge."
- **SR-06**: Architecture should clarify the PR lifecycle for Session 1 — who creates it, what triggers it, and how it maps to the existing human checkpoint.
