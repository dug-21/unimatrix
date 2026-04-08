# Component 3 — Skills MCP Format Audit

## Purpose

Audit all 14 skill SKILL.md files in `.claude/skills/` for correct MCP tool invocation
format. Fix 4 files with confirmed content or format issues. Leave 10 files unchanged.
This component is the source-fix step; Component 4 and Component 5 depend on its
completion for `uni-retro`.

---

## Dependency

This component MUST complete before:
- Component 4 (protocols-dir.md) — protocol file validation
- Component 5 (npm-package.md) — `skills/uni-retro/SKILL.md` copy to repo root

The `.claude/skills/uni-retro/SKILL.md` file is the source of truth. It must be
corrected here before the npm dist copy is made.

---

## Pre-Work: Two-Pass Audit

Run the two-pass grep from ADR-004 to locate all bare invocations:

```bash
# Pass 1 — backtick-wrapped bare invocations (code spans)
grep -rn '`context_[a-z_]*(' .claude/skills/*/SKILL.md

# Pass 2 — any line with bare invocation (no mcp__ prefix)
grep -rn 'context_[a-z_]*(' .claude/skills/*/SKILL.md | grep -v 'mcp__unimatrix__'
```

Review every match from Pass 2:
- If the match is in a code block or formatted as a function call → invocation → MUST FIX
- If the match is in plain prose without `(` immediately after the name → exempt
- Document each match and its disposition

Expected confirmed violations (from ADR-004):
- `uni-seed/SKILL.md` ~line 49: `` `context_status()` `` — invocation context
- `uni-retro/SKILL.md` ~line 146: `context_search(` inside spawn prompt string
- `uni-retro/SKILL.md` ~line 161: `context_store(` inside spawn prompt string

---

## Files Requiring No Changes (10 of 14)

After audit, if the two-pass grep returns no invocation-format matches, mark as clean
and do not edit:

- `uni-git/SKILL.md`
- `uni-review-pr/SKILL.md`
- `uni-zero/SKILL.md`
- `uni-store-lesson/SKILL.md`
- `uni-store-adr/SKILL.md`
- `uni-store-pattern/SKILL.md`
- `uni-store-procedure/SKILL.md`
- `uni-knowledge-lookup/SKILL.md`
- `uni-knowledge-search/SKILL.md`
- `uni-query-patterns/SKILL.md`

If the audit finds unexpected matches in these files, fix them and document in the
agent report.

---

## File 1: `.claude/skills/uni-seed/SKILL.md`

### Operation S1-1: Fix Bare context_status() Invocation

LOCATE: Line ~49. Current text:

```
Call `context_status()`.
```

CHANGE to:

```
Call `mcp__unimatrix__context_status({})`.
```

Note the `{}` argument — `context_status` requires an empty object argument
per the correct MCP invocation pattern.

### Operation S1-2: Add Idempotency Warning Before First Tool Call

LOCATE: The first `mcp__unimatrix__context_store` call in the file (or any
first tool invocation in the execution steps).

INSERT the following warning block BEFORE the first tool invocation. The warning
must be readable before the user begins execution — placement matters:

```
> **Important:** Run once per new project before the first delivery session.
> Do not re-run on an established installation — seed entries will duplicate
> existing knowledge.
```

VERIFY: After editing, the idempotency warning appears before any `mcp__unimatrix__`
call.

### Operation S1-3: Verify Categories Against INITIAL_CATEGORIES

READ: `crates/unimatrix-server/src/infra/categories/mod.rs` — locate `INITIAL_CATEGORIES`.
Current verified value: `["lesson-learned","decision","convention","pattern","procedure"]`

REVIEW: Every category string referenced in uni-seed SKILL.md (both in seed entry
calls and in the skill description).

IF any category string in uni-seed does not match INITIAL_CATEGORIES:
- Remove seed entries referencing removed/renamed categories
- Update the skill description category list to the current 5 categories

IF uni-seed only seeds `convention`, `pattern`, `procedure` categories (which is
acceptable — it does not have to seed all 5), verify these are in INITIAL_CATEGORIES.
If they are, no change needed for categories themselves.

### Operation S1-4: Verify blank-installation use case description

The skill intro or purpose section must state:
"A fresh Unimatrix install starts with an empty database; this skill provides an
initial curated knowledge set."

If this description is absent or inaccurate, update the intro paragraph.

### Error Handling for uni-seed

If the `context_status()` invocation is on a different line than ~49, use the two-pass
grep to locate it. If there are multiple bare invocations, fix all of them.

---

## File 2: `.claude/skills/uni-retro/SKILL.md`

### Operation R1-1: Fix Bare context_search( in Spawn Prompt (~line 146)

LOCATE: Line ~146. The line contains `context_search(` inside a spawn-prompt string.
Current form (approximately):

```
     a. Query: `context_search({feature-id}, k=20)`. Also try feature_cycle tag...
```

CHANGE to use the full MCP prefix. The invocation format should become:

```
     a. Query: `mcp__unimatrix__context_search({"query": "{feature-id}", "k": 20})`. Also try feature_cycle tag...
```

IMPORTANT: Preserve the surrounding spawn-prompt string context exactly. Only replace
the bare invocation. Do not alter the surrounding indentation or surrounding text.

### Operation R1-2: Fix Bare context_store( in Spawn Prompt (~line 161)

LOCATE: Line ~161. The line contains `context_store(` inside a spawn-prompt string.
Current form (approximately):

```
        If the component established a NEW reusable structure ... store it via context_store(category: 'pattern').
```

CHANGE to:

```
        If the component established a NEW reusable structure ... store it via mcp__unimatrix__context_store({...}).
```

The exact argument structure in the original (`category: 'pattern'`) should be
preserved as closely as possible in the fixed form, adapted to valid MCP JSON format:

```
mcp__unimatrix__context_store({"category": "pattern", ...})
```

### Operation R1-3: Verify No HookType / col-023 Predecessor References

Run:
```bash
grep -n "HookType\|closed.enum\|event_types enum\|UserPromptSubmit\|SubagentStart\|PreCompact\|PreToolUse\|PostToolUse\|Stop hook" .claude/skills/uni-retro/SKILL.md
```

Expected: zero matches. If any match is found, assess by context:
- If referencing HookType as a fixed vocabulary: REMOVE
- If referencing Claude Code hook names as examples of domain events (not as
  a closed vocabulary): evaluate and document the disposition in the agent report

---

## File 3: `.claude/skills/uni-init/SKILL.md`

### Operation I1-1: Fix Binary Name in Prerequisites

LOCATE: Line ~13. Current text:

```
The Unimatrix MCP server (`unimatrix-server`) must be running and configured...
```

CHANGE to:

```
The Unimatrix MCP server (`unimatrix`) must be running and configured...
```

### Operation I1-2: Update CLAUDE.md Append Block — All 14 Skills

LOCATE: Phase 3: CLAUDE.md Append section. Find the "Available Skills" table inside
the markdown code block that will be appended to CLAUDE.md. Current table contains
only 2 skills:

```
| `/uni-init` | First-time setup: wire CLAUDE.md and get agent recommendations |
| `/uni-seed` | Populate Unimatrix with foundational repo knowledge |
```

REPLACE the Available Skills table with all 14 skills. Use this exact list and
keep descriptions concise (one line each):

```
| Skill | When to Use |
|-------|-------------|
| `/uni-init` | First-time setup: wire CLAUDE.md and get agent orientation |
| `/uni-seed` | Populate Unimatrix with foundational repo knowledge |
| `/uni-store-adr` | After each architectural decision — stores the ADR |
| `/uni-store-lesson` | After failures and gate rejections — prevents recurrence |
| `/uni-store-pattern` | When a reusable implementation pattern emerges |
| `/uni-store-procedure` | When a step-by-step how-to technique evolves |
| `/uni-knowledge-search` | Semantic search across knowledge before implementing |
| `/uni-knowledge-lookup` | Deterministic lookup by feature, category, or ID |
| `/uni-query-patterns` | Query component patterns before designing or coding |
| `/uni-retro` | Post-merge retrospective — extract and store what was learned |
| `/uni-review-pr` | PR security review and merge readiness check |
| `/uni-release` | Version bump, changelog, tag, and push to release pipeline |
| `/uni-git` | Git conventions reference for Unimatrix commits and PRs |
| `/uni-zero` | Strategic advisor for product evolution and vision alignment |
```

VERIFY: Count the rows — must be exactly 14. Every skill from the canonical list
is present:
`uni-git`, `uni-release`, `uni-review-pr`, `uni-init`, `uni-seed`,
`uni-store-lesson`, `uni-store-adr`, `uni-store-pattern`, `uni-store-procedure`,
`uni-knowledge-lookup`, `uni-knowledge-search`, `uni-query-patterns`,
`uni-zero`, `uni-retro`

No skill appears twice. No skill from the list is missing. No non-existent skill
is listed.

### Operation I1-3: Verify MCP Format in uni-init Tool Calls

Run the two-pass grep on uni-init:
```bash
grep -n '`context_[a-z_]*(' .claude/skills/uni-init/SKILL.md
grep -n 'context_[a-z_]*(' .claude/skills/uni-init/SKILL.md | grep -v 'mcp__unimatrix__'
```

If any bare invocations are found (ADR-004 states none detected), fix them using the
same prefix addition pattern as uni-seed and uni-retro.

---

## File 4: `.claude/skills/uni-release/SKILL.md`

### Operation U1-1: Insert Step 7a — Copy protocols/ and Verify

INSERT a new step BEFORE the current "Step 7: Create Release Commit" (which starts
with `git add`). Label it "Step 7a":

```markdown
## Step 7a: Sync protocols/ Distribution Copy

Copy the four protocol files from the internal `.claude/protocols/uni/` directory
to the distributable `protocols/` directory at repo root:

```bash
cp .claude/protocols/uni/uni-design-protocol.md protocols/uni-design-protocol.md
cp .claude/protocols/uni/uni-delivery-protocol.md protocols/uni-delivery-protocol.md
cp .claude/protocols/uni/uni-bugfix-protocol.md protocols/uni-bugfix-protocol.md
cp .claude/protocols/uni/uni-agent-routing.md protocols/uni-agent-routing.md
```

Verify each copy is identical to its source:

```bash
diff .claude/protocols/uni/uni-design-protocol.md protocols/uni-design-protocol.md
diff .claude/protocols/uni/uni-delivery-protocol.md protocols/uni-delivery-protocol.md
diff .claude/protocols/uni/uni-bugfix-protocol.md protocols/uni-bugfix-protocol.md
diff .claude/protocols/uni/uni-agent-routing.md protocols/uni-agent-routing.md
```

All four diffs must produce zero output. If any diff shows differences, resolve them
before proceeding. The `.claude/protocols/uni/` directory is the source of truth —
apply any needed corrections there first, then re-copy.
```

### Operation U1-2: Insert Step 7b — Copy uni-retro Skill

INSERT a new step AFTER Step 7a and BEFORE Step 7 (the git add step). Label it "Step 7b":

```markdown
## Step 7b: Sync uni-retro Distribution Copy

Copy the uni-retro skill to the distributable `skills/` directory at repo root:

```bash
cp .claude/skills/uni-retro/SKILL.md skills/uni-retro/SKILL.md
```

Verify the copy is identical to its source:

```bash
diff .claude/skills/uni-retro/SKILL.md skills/uni-retro/SKILL.md
```

The diff must produce zero output.
```

### Operation U1-3: Update Step 7 git add Command

LOCATE: The current Step 7 git add command:

```bash
git add Cargo.toml packages/unimatrix/package.json packages/unimatrix-linux-x64/package.json packages/unimatrix-linux-arm64/package.json CHANGELOG.md
```

CHANGE to include the distribution copies:

```bash
git add Cargo.toml packages/unimatrix/package.json packages/unimatrix-linux-x64/package.json packages/unimatrix-linux-arm64/package.json CHANGELOG.md protocols/ skills/uni-retro/
```

### Operation U1-4: Update Step 10 Summary

LOCATE: The "Files modified" block in Step 10 Print Summary.

ADD these entries to the list:

```
  - protocols/ (synced from .claude/protocols/uni/)
  - skills/uni-retro/SKILL.md (synced from .claude/skills/uni-retro/)
```

---

## Final Verification After All Skill Edits

Run both passes on the full skills directory:

```bash
# Pass 1
grep -rn '`context_[a-z_]*(' .claude/skills/*/SKILL.md

# Pass 2
grep -rn 'context_[a-z_]*(' .claude/skills/*/SKILL.md | grep -v 'mcp__unimatrix__'
```

Both passes must return zero uninvestigated matches. Review every match returned
by Pass 2 and confirm it is either:
(a) Fixed, or
(b) Confirmed prose reference (exempt) — document the exemption

---

## Error Handling

If line numbers have shifted from the ~146, ~161 estimates in ADR-004, use the
two-pass grep to locate the actual lines. Do not guess line numbers.

If fixing the spawn-prompt invocations in uni-retro would change the semantic
meaning of the spawn instructions (i.e., an argument structure must be adapted),
preserve semantics and document the adaptation in the agent report.

---

## Key Test Scenarios

1. Pass 1 grep returns zero matches after all edits.
2. Pass 2 grep returns zero uninvestigated matches after all edits.
3. uni-seed idempotency warning appears before the first mcp__unimatrix__ call.
4. uni-init CLAUDE.md block lists exactly 14 skills — grep the block for each name.
5. uni-init prerequisite says `unimatrix` not `unimatrix-server`.
6. uni-release Step 7a copies all 4 protocol files with diff verification.
7. uni-release Step 7b copies uni-retro with diff verification.
8. uni-release Step 7 git add includes `protocols/` and `skills/uni-retro/`.
9. uni-retro: HookType grep returns zero matches.
10. uni-seed: `context_status` invocation uses full prefix with `{}` argument.
