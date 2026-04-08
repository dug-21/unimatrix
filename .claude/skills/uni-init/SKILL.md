---
name: "uni-init"
description: "Initialize Unimatrix in a repository: append knowledge block to CLAUDE.md and produce agent orientation recommendations."
---

# /uni-init — Repository Initialization

## Prerequisites

Before running this skill:

1. **Skill files installed**: Both `uni-init/SKILL.md` and `uni-seed/SKILL.md` must be present in `.claude/skills/` in the target repository.
2. **MCP server wired** (for `/uni-seed`): The Unimatrix MCP server (`unimatrix`) must be running and configured in your Claude Code `settings.json`. This skill (`/uni-init`) does not require MCP, but `/uni-seed` does.

If you need to install the Unimatrix server or wire MCP, consult the installation documentation.

---

## What This Skill Does

Sets up Unimatrix awareness in a repository by:
1. Checking if Unimatrix is already initialized (idempotency guard)
2. Scanning agent definitions for orientation gaps (read-only)
3. Appending a self-contained Unimatrix knowledge block to CLAUDE.md

---

## Arguments

- **No arguments**: Run the full initialization (sentinel check, agent scan, CLAUDE.md append).
- **`--dry-run`**: Print what would happen without modifying any files.

---

## Execution Steps

Follow these phases in strict order. Do not skip or reorder.

### Phase 1: Pre-flight — Idempotency Check

**Check for `--dry-run` argument first.** If the user invoked `/uni-init --dry-run`, set dry-run mode. In dry-run mode, no files will be created or modified — only terminal output.

1. Check if `CLAUDE.md` exists in the repository root.

2. **If CLAUDE.md exists**, read the entire file and search for this exact sentinel string:
   ```
   <!-- uni-init v1: DO NOT REMOVE THIS LINE -->
   ```

3. **Head-check fallback for large files**: If `CLAUDE.md` has more than 200 lines, also explicitly read the last 30 lines of the file and check for the sentinel there. This catches cases where the sentinel is at the end of a large file.

4. **If the sentinel is found** (in either check): Print the following and stop immediately. Do not proceed to Phase 2 or Phase 3.
   ```
   Already initialized. Unimatrix block found in CLAUDE.md.
   ```

5. **If CLAUDE.md does not exist**: Note this — CLAUDE.md will be created in Phase 3. Continue to Phase 2.

6. **If CLAUDE.md exists but sentinel is not found**: Continue to Phase 2.

---

### Phase 2: Agent Scan (Read-Only)

This phase produces a terminal-only recommendation report. **Do not write any files. Do not modify any agent files.**

1. Glob for agent definition files: `.claude/agents/**/*.md`

2. **If no agent files are found**: Print the following and continue to Phase 3.
   ```
   No agent files found at .claude/agents/. Skipping agent scan.
   ```

3. **For each agent file found**, read its content and check for the presence of these three patterns:

   - **context_briefing**: Does the file contain `context_briefing`? (This indicates the agent calls the Unimatrix briefing tool at session start.)
   - **Outcome reporting**: Does the file contain `/uni-record-outcome` or reference `context_store` with `category: "outcome"`? (This indicates the agent records session outcomes.)
   - **uni-\* skill reference**: Does the file contain any reference to `uni-` prefixed skills (e.g., `/uni-init`, `/uni-seed`)?

4. **Print the Agent Orientation Report** to the terminal:

   ```
   Agent Orientation Report
   ========================
   Agent                          | Missing                          | Suggested Addition
   -------------------------------|----------------------------------|------------------------------------------
   ```

   For each agent file, print a row:
   - **Agent**: the filename (without path prefix)
   - **Missing**: which of the three patterns are absent
   - **Suggested Addition**: concrete, skill-level recommendation. Examples:
     - Missing context_briefing: "Add orientation section: call context_briefing at session start for relevant knowledge"
     - Missing outcome reporting: "Add session end: invoke /uni-record-outcome to capture what was learned"
     - Missing uni-* skills: "Reference /uni-init and /uni-seed for onboarding new repos"
     - All present: "fully wired" / "none"

5. **If all agents have all three patterns**: Print after the table:
   ```
   All agents fully wired.
   ```

---

### Phase 3: CLAUDE.md Append

**If in dry-run mode**: Print the following, then print the full Unimatrix block content below, and stop. Do not create or modify any files.
```
DRY RUN -- the following block would be appended to CLAUDE.md:
```
Print the block, then:
```
No files were modified.
```

**If NOT in dry-run mode**:

The exact block to append is:

```markdown

<!-- uni-init v1: DO NOT REMOVE THIS LINE -->
## Unimatrix

Knowledge engine (MCP server). Makes agent expertise searchable, trustworthy, and self-improving.

### Available Skills

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

### Knowledge Categories

| Category | What Goes Here |
|----------|---------------|
| `decision` | Architectural decisions (ADRs) — use `/uni-store-adr` |
| `pattern` | Reusable implementation patterns — use `/uni-store-pattern` |
| `procedure` | Step-by-step workflows — use `/uni-store-procedure` |
| `convention` | Project-wide coding/process standards |
| `lesson-learned` | Post-failure takeaways — use `/uni-store-lesson` |

### When to Invoke

- Before implementing anything new → search knowledge base
- After each architectural decision → store ADR
- After each shipped feature → run retrospective
- When a technique evolves → update procedure
<!-- end uni-init v1 -->
```

**If CLAUDE.md exists**: Append the block to the end of the existing file. Use Edit/append semantics — do NOT overwrite the file. Preserve all existing content. Add a blank line before the block if the file does not already end with a blank line.

**If CLAUDE.md does not exist**: Create CLAUDE.md with the block as its only content (without the leading blank line).

After writing, confirm:
```
Unimatrix block appended to CLAUDE.md.
```
Or if created:
```
Created CLAUDE.md with Unimatrix block.
```

Finally, print:
```
Initialization complete. Run /uni-seed next to populate the knowledge base.
```
