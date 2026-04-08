## ADR-004: Skills MCP Format Audit — grep Pattern and Content Scope

### Context

AC-10 requires zero bare Unimatrix tool name invocations (without the `mcp__unimatrix__` prefix) across all 14 skill files. SR-04 identified that a naive grep will match prose references too, creating false positives. The SCOPE.md explicitly exempts prose references in descriptive text from the prefix requirement — only code-block and invocation-format tool calls require the full prefix.

The current grep audit of `.claude/skills/` shows the following files contain bare invocations:
- `uni-seed/SKILL.md` line 49: `Call \`context_status()\`` — bare invocation in backtick
- `uni-retro/SKILL.md` lines 146, 161: bare `context_search(` and `context_store(` in non-code-block text within a spawn prompt string

All other skill files already use the full `mcp__unimatrix__context_*` prefix in their actual invocations.

### Decision

**1. grep pattern for AC-10 verification**

The correct grep pattern to detect bare invocations (not prose) is:

```
grep -n '`context_[a-z_]*(' .claude/skills/*/SKILL.md
```

This matches backtick-wrapped bare names, which are the invocation-format calls (code spans in markdown). It does not match prose text like "call context_search to find entries" unless the prose wraps the name in backticks followed by `(`.

For the spawn-prompt embedded invocations in uni-retro (lines 146, 161), these are inside agent spawn strings and contain bare `context_search(` and `context_store(` without backticks. A secondary pattern is needed:

```
grep -n 'context_[a-z_]*(' .claude/skills/*/SKILL.md | grep -v 'mcp__unimatrix__'
```

This catches any line containing a bare tool call pattern (with opening paren) that lacks the prefix. False positives from prose are possible but manageable at 14 files.

**The implementer must use this two-pass approach:**
1. Run the combined pattern to find candidates.
2. Manually review each match to confirm it is an invocation (not prose).
3. Fix confirmed invocations only.

**2. Audit findings — files requiring MCP format changes**

Confirmed bare invocations requiring prefix addition:

| File | Line | Bare form | Fix required |
|---|---|---|---|
| `uni-seed/SKILL.md` | ~49 | `` `context_status()` `` | Change to `` `mcp__unimatrix__context_status()` `` |
| `uni-retro/SKILL.md` | ~146 | `context_search({feature-id}, k=20)` inside spawn prompt | Change to `mcp__unimatrix__context_search(...)` |
| `uni-retro/SKILL.md` | ~161 | `context_store(category: 'pattern')` inside spawn prompt | Change to `mcp__unimatrix__context_store(...)` |

**3. Files requiring content accuracy review beyond format**

Per SCOPE.md, these 4 skills need content review:

- **`uni-release`**: Add protocol packaging step (Step 7a) and uni-retro packaging step (Step 7b). Update Step 7 `git add` and Step 10 summary. Binary name already correct (skill uses `cargo check` not binary path — no binary name fix needed here). No bare MCP invocations detected.

- **`uni-init`**: The CLAUDE.md block appended by this skill lists only 2 skills (`/uni-init`, `/uni-seed`). It must be updated to list all 14 current skills. Additionally, line 13 references `unimatrix-server` (old binary name) in the prerequisites text — fix to `unimatrix`. No bare MCP invocations detected.

- **`uni-retro`**: Fix bare invocations in lines ~146, ~161. Check for any HookType or col-023 predecessor references — none detected in current content review. The retro skill uses `context_cycle_review` (full prefix, line 43) correctly.

- **`uni-seed`**: Fix bare `context_status()` invocation (line 49). Update description to use `mcp__unimatrix__context_status`. Verify all seed entry categories against `INITIAL_CATEGORIES` (5 categories: lesson-learned, decision, convention, pattern, procedure). The skill currently seeds only `convention`, `pattern`, `procedure` categories — verify no seed entries use removed categories. Add the blank-installation warning ("Do not re-run on an established installation").

**4. Files requiring no changes**

After audit, these 10 skills have correct MCP prefix format and no material accuracy issues:
- `uni-git`, `uni-review-pr`, `uni-zero`, `uni-store-lesson`, `uni-store-adr`, `uni-store-pattern`, `uni-store-procedure`, `uni-knowledge-lookup`, `uni-knowledge-search`, `uni-query-patterns`

### Consequences

- The two-pass grep approach prevents false positives from prose references while catching all invocation-format bare names.
- Fixing invocations inside spawn-prompt strings in `uni-retro` requires careful edit targeting — the surrounding string context must be preserved exactly.
- `uni-init`'s CLAUDE.md template block lists only 2 of 14 skills — this is a content gap, not a format issue, but it falls under the accuracy audit scope.
- The `unimatrix-server` reference in `uni-init` prerequisites (line 13) is a binary name error — fixing it is explicitly in scope per SCOPE.md AC-04 (binary name fix applies throughout).
