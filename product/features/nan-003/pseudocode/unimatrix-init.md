# Component 1: `/unimatrix-init` Skill — Pseudocode

## SKILL.md Structure

```
---
name: "unimatrix-init"
description: "Initialize Unimatrix in a repository: append knowledge block to CLAUDE.md and produce agent orientation recommendations."
---

# Prerequisites section (first non-frontmatter section)
# Disambiguation notice (vs uni-init agent)
# Phase 1: Pre-flight (sentinel check)
# Phase 2: Agent scan (read-only)
# Phase 3: CLAUDE.md append (or dry-run print)
```

## Detailed Flow

### Arguments
- `--dry-run` (optional): print what would happen, no file writes

### Prerequisites Section
```
PRINT prerequisites:
  - Skill files present in .claude/skills/unimatrix-init/ and .claude/skills/unimatrix-seed/
  - For /unimatrix-seed: MCP server (unimatrix-server) must be running and wired in Claude settings.json
  - See installation documentation for MCP wiring setup
```

### Disambiguation Notice
```
PRINT notice:
  This skill (/unimatrix-init) sets up CLAUDE.md and produces agent recommendations.
  It is different from the uni-init agent (.claude/agents/uni/uni-init.md), which
  performs brownfield bootstrap by extracting knowledge from existing .claude/agents/
  and .claude/protocols/ files into Unimatrix entries.

  Use /unimatrix-init for new repo setup.
  Use uni-init agent for migrating existing agent knowledge into Unimatrix.
```

### Phase 1: Pre-flight (Sentinel Check)

```
// Check for --dry-run argument
dry_run = arguments contain "--dry-run"

// Read CLAUDE.md
IF file "CLAUDE.md" exists:
    content = Read("CLAUDE.md")

    // Primary sentinel check: search full content
    IF content contains "<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->":
        PRINT "Already initialized. Unimatrix block found in CLAUDE.md."
        HALT — no further action

    // ADR-002 fallback for large files: also check last 30 lines
    IF line_count(content) > 200:
        tail_content = last_30_lines(content)
        IF tail_content contains "<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->":
            PRINT "Already initialized. Unimatrix block found in CLAUDE.md."
            HALT — no further action

    claude_md_exists = true
ELSE:
    claude_md_exists = false
```

### Phase 2: Agent Scan (Read-Only)

```
// See agent-scan.md for detailed algorithm
RUN agent_scan()  // produces terminal recommendation report
// No files modified (C-07)
```

### Phase 3: CLAUDE.md Append

```
block = CLAUDE_MD_BLOCK_TEMPLATE  // see claude-md-template.md for exact content

IF dry_run:
    PRINT "DRY RUN — the following block would be appended to CLAUDE.md:"
    PRINT ""
    PRINT block
    PRINT ""
    PRINT "No files were modified."
    HALT

IF claude_md_exists:
    // APPEND to existing file — preserve all existing content
    // Use Edit tool with append semantics, NOT Write (overwrite)
    Append block to end of CLAUDE.md (with preceding blank line separator)
    PRINT "Unimatrix block appended to CLAUDE.md."
ELSE:
    // CREATE new file with block as only content
    Write("CLAUDE.md", block)
    PRINT "Created CLAUDE.md with Unimatrix block."

PRINT "Initialization complete. Run /unimatrix-seed next to populate knowledge base."
```

## Phase Ordering (Critical)

1. Pre-flight FIRST (fail fast — NFR-06)
2. Agent scan SECOND (read-only, always runs unless sentinel found)
3. CLAUDE.md write THIRD (after scan, so dry-run can show both scan results and block)

## Error Handling

- CLAUDE.md unreadable: print error, halt
- No .claude/agents/ directory: skip scan, note in output
- Glob returns no .md files: skip scan, note in output
- --dry-run: print block + scan results, write nothing
