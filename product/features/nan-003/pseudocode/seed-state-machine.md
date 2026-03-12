# Component 5: Seed State Machine — Pseudocode

## State Diagram

```
PREFLIGHT --> EXISTING_CHECK --> LEVEL_0 --> GATE_0 --> [LEVEL_1] --> GATE_1 --> [LEVEL_2] --> GATE_2 --> DONE
    |               |                          |                        |                        |
    v               v                          v                        v                        v
 (fail:halt)   (>=3 entries:                (reject all:             (no deeper:               DONE
                warn+ask)                    DONE)                    DONE)
```

## State Definitions

### PREFLIGHT
```
Call context_status()
IF response indicates error or MCP unavailable:
    PRINT "Unimatrix MCP is not available. Ensure the MCP server is running and wired in Claude settings."
    HALT — do not proceed
IF response is healthy (no error indicators):
    TRANSITION -> EXISTING_CHECK
```

### EXISTING_CHECK
```
count = 0
FOR category IN ["convention", "pattern", "procedure"]:
    results = context_search(query: "repo", category: category, k: 5)
    count += number of results returned

IF count >= 3:
    PRINT warning: "Found {count} existing seed entries. Running /unimatrix-seed again may create near-duplicates."
    PRINT "Would you like to supplement the existing knowledge, or skip seeding?"

    **STOP. Wait for human response before proceeding.**

    IF human says skip:
        TRANSITION -> DONE
    IF human says supplement:
        TRANSITION -> LEVEL_0
ELSE:
    TRANSITION -> LEVEL_0
```

### LEVEL_0 (Automatic — no opt-in needed)
```
// Read high-signal, small-footprint files
files_to_read = []

IF exists("README.md"):
    files_to_read.append("README.md")
IF exists("CLAUDE.md"):
    files_to_read.append("CLAUDE.md")

// Package manifests — check all common ones
FOR manifest IN ["Cargo.toml", "package.json", "pyproject.toml", "go.mod"]:
    IF exists(manifest):
        files_to_read.append(manifest)

// Top-level .claude/ structure
IF exists(".claude/"):
    list .claude/ directory structure (not deep read, just ls)

// Read all found files
FOR file IN files_to_read:
    Read(file)

// Generate 2-4 foundational entries from what was read
// Typical entries cover: repo purpose, tech stack, project structure, key conventions
candidates = generate_entries_from_reads()

// Apply quality gate to each candidate
entries = []
FOR candidate IN candidates:
    IF quality_gate(candidate) == PASS:
        entries.append(candidate)

// Enforce 2-4 entry count
IF len(entries) > 4:
    entries = top_4_by_relevance(entries)
IF len(entries) == 0:
    PRINT "Could not generate quality entries from available files. Consider adding a README.md with project context."
    TRANSITION -> DONE

TRANSITION -> GATE_0 with entries
```

### GATE_0 (HARD STOP — Batch Approval)
```
PRINT "Level 0 — Foundational Knowledge"
PRINT "================================="
PRINT "Proposed entries (batch):"
FOR i, entry IN enumerate(entries):
    PRINT "  {i+1}. [{entry.category}] {entry.what}"
    PRINT "     Why: {entry.why}"
    PRINT "     Scope: {entry.scope}"
PRINT ""
PRINT "Approve all entries as a batch? (approve / reject)"

**STOP. Wait for human response before proceeding.**

IF human approves:
    FOR entry IN entries:
        context_store(
            title: entry.what,
            content: "What: {entry.what}\nWhy: {entry.why}\nScope: {entry.scope}",
            topic: "{repo name or top-level context}",
            category: entry.category,
            tags: ["seed", "level-0"],
            agent_id: "unimatrix-seed"
        )
        // Report success/failure per entry
    PRINT "Stored {count} entries."
ELSE IF human rejects:
    PRINT "0 entries stored. Re-invoke /unimatrix-seed with more specific guidance if needed."
    TRANSITION -> DONE

PRINT ""
PRINT "Would you like to explore deeper? Options:"
PRINT "  a) Module structure — explore source directories and key modules"
PRINT "  b) Conventions — look for coding standards, linting, formatting config"
PRINT "  c) Build & test — explore build system, test framework, CI config"
PRINT "  d) Done — stop here"

**STOP. Wait for human response before proceeding.**

IF human selects one or more options:
    TRANSITION -> LEVEL_1 with selected_categories
IF human selects "done":
    TRANSITION -> DONE
```

### LEVEL_1 (Opt-in — per-category exploration)
```
FOR EACH selected_category:
    // Explore relevant files based on category
    IF category == "module structure":
        read src/ or lib/ directory listings, key module files
    IF category == "conventions":
        read .editorconfig, .eslintrc, rustfmt.toml, clippy config, etc.
    IF category == "build & test":
        read CI config, Makefile, test directories

    // Generate entries from exploration
    candidates = generate_entries_from_reads()

    // Quality gate
    FOR candidate IN candidates:
        IF quality_gate(candidate) == PASS:
            PRINT "Proposed entry:"
            PRINT "  [{candidate.category}] {candidate.what}"
            PRINT "  Why: {candidate.why}"
            PRINT "  Scope: {candidate.scope}"
            PRINT "  Store this entry? (yes / no)"

            **STOP. Wait for human response before proceeding.**

            IF human approves:
                context_store(...)
                PRINT "Stored."
            ELSE:
                PRINT "Skipped."

TRANSITION -> GATE_1
```

### GATE_1 (HARD STOP)
```
PRINT "Level 1 complete. Stored {count} entries."
PRINT ""
PRINT "Level 2 is the final level. Would you like to explore any area in more depth?"
PRINT "  a) [list available deeper explorations based on Level 1 selections]"
PRINT "  b) Done — stop here"

**STOP. Wait for human response before proceeding.**

IF human selects deeper exploration:
    TRANSITION -> LEVEL_2 with selections
IF human selects "done":
    TRANSITION -> DONE
```

### LEVEL_2 (Final opt-in level)
```
// Same pattern as Level 1 but deeper reads within selected areas
// Per-entry individual approval

FOR EACH selection:
    // Deeper file reads
    candidates = generate_entries_from_reads()
    FOR candidate IN candidates:
        IF quality_gate(candidate) == PASS:
            // Individual approval (same as Level 1)
            ...

TRANSITION -> GATE_2
```

### GATE_2 (Terminal — no Level 3)
```
PRINT "Level 2 complete. Stored {count} entries total across all levels."
PRINT ""
PRINT "Seeding complete. No further levels available."
// Explicit: do NOT offer Level 3. This is the terminal state.
TRANSITION -> DONE
```

### DONE
```
PRINT "Seed Summary"
PRINT "============"
PRINT "Total entries stored: {total_count}"
PRINT "  Level 0: {l0_count}"
PRINT "  Level 1: {l1_count}"
PRINT "  Level 2: {l2_count}"
PRINT ""
PRINT "Knowledge base is ready. Future context_briefing calls will return these entries."
```

## Critical Design Rules

1. Every gate uses **"STOP. Wait for human response before proceeding."** (ADR-001)
2. No auto-advance between levels — model must halt at every gate
3. Maximum depth: Level 0 + 2 opt-in levels. No Level 3. (NFR-02)
4. Level 0: batch approval. Level 1+: individual approval. (FR-17, FR-20)
5. Only convention/pattern/procedure categories (ADR-006)
6. Quality gate applied before human sees any entry (NFR-05)
7. context_store failure must be reported per-entry, not silently swallowed (R-05)
