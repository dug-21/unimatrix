# Test Plan: delivery-protocol-mod (Component 3)

## Component Scope

File: `.claude/protocols/uni/uni-delivery-protocol.md`
Change: Adds a conditional documentation step to Phase 4 (additive only).

Testing verifies: step present, correct position (after `gh pr create`, before
`/review-pr`), trigger criteria complete, advisory/no-gate behavior stated,
and no existing Phase 4 structure modified.

---

## R-06: Protocol Step Position (High)

Position must be: after `gh pr create` and before `/review-pr` invocation.

### T-01: Documentation step is present in Phase 4

```bash
grep -n 'uni-docs\|documentation.*step\|spawn.*uni-docs\|Documentation.*update' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md
# Expected: at least one match with a line number in the Phase 4 region
```

### T-02: Documentation step line appears after gh pr create line

```bash
DOC_LINE=$(grep -n 'uni-docs\|documentation.*step\|spawn.*uni-docs' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md \
  | head -1 | cut -d: -f1)
PR_LINE=$(grep -n 'gh pr create' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md \
  | head -1 | cut -d: -f1)
echo "gh pr create line: $PR_LINE"
echo "Documentation step line: $DOC_LINE"
[ "$DOC_LINE" -gt "$PR_LINE" ] && echo "PASS: doc step after PR create" \
  || echo "FAIL: doc step not after PR create"
```

### T-03: Documentation step line appears before /review-pr line

```bash
DOC_LINE=$(grep -n 'uni-docs\|documentation.*step\|spawn.*uni-docs' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md \
  | head -1 | cut -d: -f1)
REVIEW_LINE=$(grep -n '/review-pr\|review-pr' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md \
  | tail -1 | cut -d: -f1)
echo "Documentation step line: $DOC_LINE"
echo "/review-pr line: $REVIEW_LINE"
[ "$DOC_LINE" -lt "$REVIEW_LINE" ] && echo "PASS: doc step before /review-pr" \
  || echo "FAIL: doc step not before /review-pr"
```

### T-04: Documentation step does NOT appear after /review-pr block

```bash
# /review-pr invocation line number
REVIEW_LINE=$(grep -n 'Invoke.*review-pr\|/review-pr.*PR number\|spawn.*review-pr' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md \
  | tail -1 | cut -d: -f1)
# uni-docs line number — must be before /review-pr
DOC_LINE=$(grep -n 'uni-docs\|spawn.*documentation' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md \
  | head -1 | cut -d: -f1)
[ "$DOC_LINE" -lt "$REVIEW_LINE" ] && echo "PASS" || echo "FAIL: positioned after /review-pr"
```

---

## R-07: Trigger Criteria Completeness (Medium)

### T-05: Mandatory conditions listed — new/modified MCP tool

```bash
grep -i 'MCP tool\|new.*tool\|tool.*change\|tool.*added' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md | \
  grep -i 'mandatory\|required\|must' | head -5
# Expected: at least one match
# OR: if criteria presented as a table, verify MCP tool row exists:
grep -i 'MCP tool' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md | head -3
```

### T-06: Mandatory conditions listed — new/modified skill

```bash
grep -i 'skill\|new.*skill' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md | \
  grep -i 'mandatory\|required\|must\|MANDATORY' | head -5
# Expected: at least one match
```

### T-07: Mandatory conditions listed — CLI subcommand

```bash
grep -i 'CLI\|subcommand\|cli.*subcommand' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md | head -5
# Expected: at least one match in trigger criteria region
```

### T-08: Mandatory conditions listed — knowledge category

```bash
grep -i 'knowledge category\|new.*category\|category.*change' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md | head -5
# Expected: at least one match
```

### T-09: Skip conditions listed — internal refactor

```bash
grep -i 'refactor\|internal.*only\|skip\|SKIP' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md | \
  grep -v '^#' | head -5
# Expected: at least one match indicating skip for internal changes
```

### T-10: Skip conditions listed — test-only feature

```bash
grep -i 'test.only\|testing.*only\|SKIP' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md | head -5
# Expected: at least one match
```

### T-11: Criteria are deterministic — decision table or explicit list present

```bash
# A decision table has | characters in the trigger criteria region
grep -A20 'uni-docs\|documentation.*step' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md | \
  grep '|' | head -10
# Expected: at least some | rows (table) OR numbered/bulleted list entries
# Manual review: criteria must not require consulting ADR-003 to interpret
```

---

## Advisory/No-Gate Behavior (FR-12f)

### T-12: Step stated as advisory/no gate

```bash
grep -i 'advisory\|no gate\|does not block\|not.*block.*delivery\|optional.*failure' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md | head -5
# Expected: at least one match near the documentation step
```

---

## Spawn Template (FR-12c, SR-07)

The protocol must include a concrete spawn template so Delivery Leaders have
a copy-paste invocation. Without it, spawns are inconsistent (SR-07 risk).

### T-13: Spawn template references feature ID

```bash
grep -i 'feature.*id\|feature_id\|{feature\|{id' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md | \
  grep -i 'uni-docs\|spawn\|documentation' | head -5
# Expected: at least one match near documentation step block
```

### T-14: Spawn template references SCOPE.md path

```bash
grep 'SCOPE\.md' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md | head -5
# Expected: at least one match in documentation step region
```

### T-15: Spawn template references README.md path

```bash
grep 'README\.md' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md | head -5
# Expected: at least one match in documentation step region
```

---

## Additive Constraint (C-03, NFR-06)

The modification must not remove or reorder existing Phase 4 steps.

### T-16: Existing Phase 4 steps still present

```bash
for STEP in 'gh pr create' '/review-pr\|review-pr' 'record-outcome\|/record-outcome'; do
  grep -i "$STEP" \
    /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md > /dev/null \
    && echo "PASS: $STEP still present" || echo "FAIL: $STEP removed"
done
```

### T-17: Diff shows only additions at the insertion point

```bash
# Run from worktree root — check git diff for the protocol file
git -C /workspaces/unimatrix-nan-005 diff HEAD -- \
  .claude/protocols/uni/uni-delivery-protocol.md | grep '^-' | grep -v '^---' | head -20
# Expected: no removal lines (all diff lines are additions '+')
# If no diff yet (file not modified), this test is deferred to Stage 3c.
```

---

## Documentation Step Commits to Feature Branch (FR-12e)

### T-18: Protocol states docs commit to feature branch before /review-pr

```bash
grep -i 'feature.*branch\|commit.*branch\|branch.*commit\|before.*review' \
  /workspaces/unimatrix-nan-005/.claude/protocols/uni/uni-delivery-protocol.md | \
  grep -i 'doc\|uni-docs' | head -5
# Expected: at least one match confirming commits land in feature branch
```
