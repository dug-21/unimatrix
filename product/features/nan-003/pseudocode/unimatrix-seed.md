# Component 2: `/unimatrix-seed` Skill — Pseudocode

## SKILL.md Structure

```
---
name: "unimatrix-seed"
description: "Populate Unimatrix with foundational repository knowledge through human-directed, gated exploration."
---

# Prerequisites section
# Overview (what the skill does)
# State machine: PREFLIGHT -> EXISTING_CHECK -> LEVEL_0 -> GATE_0 -> ...
# Quality gate rules
# Category rules
```

## Detailed Flow

### Prerequisites Section
```
PRINT prerequisites:
  - MCP server (unimatrix-server) must be running and wired in Claude settings.json
  - Verify by running: context_status() should return a healthy response
  - For best results: run /unimatrix-init first to set up CLAUDE.md
```

### Entry Point
```
// No arguments — conversational skill

// STEP 1: PREFLIGHT (absolutely first — before any file reads)
Call context_status()
IF call fails or returns error indicators:
    PRINT "Unimatrix MCP is not available."
    PRINT "Ensure unimatrix-server is running and wired in your Claude settings.json."
    PRINT "See installation documentation for setup instructions."
    HALT — do not proceed

// STEP 2: EXISTING_CHECK
// See seed-state-machine.md EXISTING_CHECK state
total_existing = 0
FOR category IN ["convention", "pattern", "procedure"]:
    results = context_search(query: "repo", category: category, k: 5)
    total_existing += count(results)

IF total_existing >= 3:
    PRINT "Found {total_existing} existing entries in seeding categories (convention/pattern/procedure)."
    PRINT "Re-seeding may create near-duplicates."
    PRINT ""
    PRINT "Options:"
    PRINT "  supplement — add new knowledge alongside existing entries"
    PRINT "  skip — exit without changes"

    **STOP. Wait for human response before proceeding.**

    IF human says "skip":
        PRINT "No changes made."
        HALT

// STEP 3: LEVEL_0
// See seed-state-machine.md LEVEL_0 state
// Read files, generate entries, quality gate, batch present

// STEP 4: GATE_0
// See seed-state-machine.md GATE_0 state
// Batch approval, store, ask about deeper exploration

// STEP 5-8: LEVEL_1 -> GATE_1 -> LEVEL_2 -> GATE_2 -> DONE
// See seed-state-machine.md for full state machine
```

## Key Behavioral Rules in SKILL.md

The SKILL.md must instruct the model to:

1. Call context_status() as the VERY FIRST action (before Read, Glob, or any other tool call)
2. Use "**STOP. Wait for human response before proceeding.**" at every gate
3. Never advance to the next level without explicit human opt-in
4. Apply quality gate (entry-quality-gate.md) to every candidate entry before showing to human
5. Use only convention/pattern/procedure categories
6. Report context_store success/failure per entry
7. Print summary at DONE state
8. Enforce depth limit: Level 2 is terminal, no Level 3

## Error Handling

- context_status fails: print actionable error, halt immediately
- context_search fails during EXISTING_CHECK: warn "could not check for existing entries", proceed with caution
- context_store fails for an entry: report failure, continue with remaining entries
- No README or manifests found: propose 0 entries, explain why, go to DONE
- Human rejects entire Level 0 batch: print "0 entries stored", go to DONE
