## ADR-002: Documentation Step Placement — Before /review-pr, Within Feature PR

### Context

The documentation agent (uni-docs) must be triggered from the delivery protocol. Two timing options exist:

**Option A**: After PR merge, as a follow-up commit on main. Documentation updates are separate from the feature PR.

**Option B**: Before /review-pr, within the same feature branch. Documentation updates are part of the reviewed PR.

SCOPE.md Resolved Questions explicitly chose Option B. The risk assessment (SR-07) noted that if placement is wrong relative to PR creation, doc updates may not be in the reviewed PR.

The delivery protocol Phase 4 sequence is:
1. Commit final artifacts
2. Push + open PR
3. (potential doc step)
4. /review-pr
5. Return to human

The documentation agent must commit to the feature branch. After `gh pr create`, the feature branch is still open and can receive additional commits before merge. /review-pr sees whatever is on the branch at invocation time.

### Decision

The documentation step is inserted **after PR creation, before /review-pr**:

```
Phase 4: Delivery
  1. Commit final artifacts
  2. Push feature branch + open PR
  3. [CONDITIONAL] Spawn uni-docs if trigger criteria met → commits to feature branch
  4. Invoke /review-pr (sees documentation changes as part of PR)
  5. Return SESSION 2 COMPLETE
```

The uni-docs agent receives the PR number, feature ID, and artifact paths. It reads SCOPE.md and SPECIFICATION.md, identifies sections to update, edits README.md, and commits with a message following the convention: `docs: update README for {feature-id} (#{issue})`.

If uni-docs produces no changes (feature has no user-visible impact), it returns immediately with "no documentation changes required" and Phase 4 continues.

### Consequences

- Documentation changes are reviewable as part of the same PR. The security reviewer sees README changes alongside implementation.
- If the PR is later rebased or squashed, documentation commits are included in the history.
- Concurrent feature branches both modifying README will produce merge conflicts (SR-08). Accepted risk — markdown conflicts are easily resolved and team size makes simultaneous documentation-touching features rare.
- The documentation step adds latency to Phase 4. Acceptable because the agent reads only two small markdown files and edits a single README section.
- Post-merge documentation debt (shipping features without doc updates) is eliminated for features that go through the protocol.
