# Test Plan: Skills MCP Format Audit

## Component Scope

Files under test:
- All 14 SKILL.md files in `.claude/skills/*/SKILL.md`
- `.claude/skills/uni-seed/SKILL.md` (format fix + idempotency warning)
- `.claude/skills/uni-retro/SKILL.md` (format fix + stale reference removal)
- `.claude/skills/uni-init/SKILL.md` (skill list update)
- `skills/uni-retro/SKILL.md` (repo root npm copy — tested separately)

Acceptance criteria covered: AC-10, AC-11, AC-12, AC-16, AC-17
Risks covered: R-05 (High), R-07 (High), R-11 (Med), R-12 (Med), R-14 (Low)

---

## AC-10: Zero Bare MCP Invocations — Two-Pass Grep

**Risk**: R-05 (High/Med) — a single-pass grep may miss backtick-wrapped bare invocations
in spawn-prompt strings; ADR-004 defines the two-pass requirement.

### Pass 1 — Backtick-wrapped bare invocations

```bash
grep -rn '`context_[a-z_]*(' .claude/skills/*/SKILL.md
```

Expected: zero matches. If any matches appear, manually read the surrounding line to
determine whether it is:
- An invocation (e.g., `` `context_search({"query": "..."}` ``) — MUST be fixed
- A prose mention of the function name — exempt if clearly not a tool call

### Pass 2 — Any bare invocation line without the mcp__unimatrix__ prefix

```bash
grep -rn 'context_[a-z_]*(' .claude/skills/*/SKILL.md | grep -v 'mcp__unimatrix__'
```

Expected: zero matches after filtering. If matches remain, manually review each one:
- If the line calls the tool (e.g., as part of agent instructions), it is a bare invocation
  violation and MUST be fixed.
- If the line is a comment explaining the old format, it may be exempt if it does not
  constitute an agent instruction.

### Pass 1 + Pass 2 on repo-root copy (R-12)

Run both passes independently against the npm distribution copy:

```bash
grep -rn '`context_[a-z_]*(' skills/uni-retro/SKILL.md
grep -rn 'context_[a-z_]*(' skills/uni-retro/SKILL.md | grep -v 'mcp__unimatrix__'
```

Expected: zero matches in both cases. The npm copy must be the corrected copy, not
a pre-fix copy.

### Known Confirmed Violations to Verify Are Fixed (from ADR-004)

| File | Location | Pattern |
|------|----------|---------|
| `.claude/skills/uni-seed/SKILL.md` | ~line 49 | bare `context_status()` |
| `.claude/skills/uni-retro/SKILL.md` | ~line 146 | bare `context_search(` in spawn-prompt string |
| `.claude/skills/uni-retro/SKILL.md` | ~line 161 | bare `context_store(` in spawn-prompt string |

Assert: each of these is now `mcp__unimatrix__context_status({})`,
`mcp__unimatrix__context_search(`, and `mcp__unimatrix__context_store(` respectively.

**Pass criteria**: Both passes return zero matches against all 14 .claude/skills files.
Both passes return zero matches against skills/uni-retro/SKILL.md (repo root). All three
known violations are confirmed fixed at their specific locations.

---

## AC-11: uni-init Lists All 14 Skills

**Risk**: R-07 (Med/High) — currently only 2 of 14 skills are listed; new projects get
an incomplete orientation

### Step 1 — Extract skills list from CLAUDE.md append block

```bash
grep -n "uni-git\|uni-release\|uni-review-pr\|uni-init\|uni-seed\|uni-store-lesson\|uni-store-adr\|uni-store-pattern\|uni-store-procedure\|uni-knowledge-lookup\|uni-knowledge-search\|uni-query-patterns\|uni-zero\|uni-retro" .claude/skills/uni-init/SKILL.md
```

Expected: exactly 14 matches, one per skill. No skill appears twice.

### Step 2 — Cross-reference against canonical list

Assert each of these 14 names appears exactly once in the result:
1. `uni-git`
2. `uni-release`
3. `uni-review-pr`
4. `uni-init`
5. `uni-seed`
6. `uni-store-lesson`
7. `uni-store-adr`
8. `uni-store-pattern`
9. `uni-store-procedure`
10. `uni-knowledge-lookup`
11. `uni-knowledge-search`
12. `uni-query-patterns`
13. `uni-zero`
14. `uni-retro`

Note: `uni-init` must be in the list even though it is "already run" — it is a real skill
that can be invoked again (e.g., on a new repo).

### Step 3 — No phantom entries

Assert: no skill name appears in the CLAUDE.md append block that is NOT in the canonical
14-item list above.

### Step 4 — Binary name fix

```bash
grep -n "unimatrix-server" .claude/skills/uni-init/SKILL.md
# Expected: zero matches
```

**Pass criteria**: Exactly 14 skills listed, no duplicates, no phantom entries. Zero
binary name violations.

---

## AC-12: uni-retro Contains No HookType / col-023-Predecessor References

**Risk**: R-11 (stale reference subcategory) — col-023 removed the closed HookType enum

```bash
grep -rn "HookType\|closed.enum\|UserPromptSubmit\|SubagentStart\|PreCompact\|PreToolUse\|PostToolUse\|Stop hook" .claude/skills/uni-retro/SKILL.md
# Expected: zero matches
```

**Pass criteria**: Command produces no output.

---

## AC-16: uni-seed Idempotency Warning + Format Fix

**Risk**: R-14 (Low/Med) — operator re-runs uni-seed, duplicating knowledge entries

### Step 1 — Format fix: no bare context_status or context_store invocations

```bash
grep -n "context_store\|context_status" .claude/skills/uni-seed/SKILL.md | grep -v "mcp__unimatrix__"
# Expected: zero matches
```

### Step 2 — Idempotency warning placement

Read `.claude/skills/uni-seed/SKILL.md`. Locate the idempotency warning. Assert:
- The warning is present (it warns against re-running on an established installation)
- It appears BEFORE the first `mcp__unimatrix__context_store` call in the file

```bash
# Get line number of first context_store call
grep -n "mcp__unimatrix__context_store" .claude/skills/uni-seed/SKILL.md | head -1
# Record this line number N

# Get line number of idempotency warning
grep -n "idempotency\|re-run\|duplicate\|established installation" .claude/skills/uni-seed/SKILL.md | head -1
# Record this line number M

# Assert: M < N
```

### Step 3 — Warning text accuracy

Assert the warning text conveys: "Do not re-run on an established installation — seed
entries will duplicate existing knowledge." (or substantively equivalent text).

### Step 4 — Blank-install use case description

```bash
grep -in "blank\|fresh\|new install\|no entries\|empty" .claude/skills/uni-seed/SKILL.md | head -5
# Expected: at least one match describing the blank-installation use case
```

**Pass criteria**: No bare invocations; warning present before first tool call; warning
text is accurate; blank-install use case is described.

---

## AC-17: uni-seed Category Values Match INITIAL_CATEGORIES

**Risk**: Low — a stale category in seed entries would be rejected by the running server

### Step 1 — Read authority

Read `crates/unimatrix-server/src/infra/categories/mod.rs`. Extract the INITIAL_CATEGORIES
array. At time of specification, the canonical list is:

```
["lesson-learned", "decision", "convention", "pattern", "procedure"]
```

Tester must read the file at delivery time — do not assume the list is unchanged.

### Step 2 — Extract all category values from uni-seed

```bash
grep -n '"category":\|category:' .claude/skills/uni-seed/SKILL.md
```

For each `category:` value in tool call arguments, assert it is in INITIAL_CATEGORIES.
Categories that appear only in prose descriptions (not as tool argument values) are exempt.

**Pass criteria**: Every category value in tool call arguments is in the INITIAL_CATEGORIES
list at delivery time. No stale or invented categories.
