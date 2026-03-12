---
name: "unimatrix-seed"
description: "Populate Unimatrix with foundational repository knowledge through human-directed, gated exploration."
---

# /unimatrix-seed — Knowledge Base Seeding

## Prerequisites

Before running this skill:

1. **MCP server running**: The Unimatrix MCP server (`unimatrix-server`) must be running and wired in your Claude Code `settings.json`. This skill calls `context_status`, `context_search`, and `context_store` — all require an operational MCP server.
2. **Recommended**: Run `/unimatrix-init` first to set up the CLAUDE.md knowledge block. Seeding works without it, but the CLAUDE.md block provides ongoing awareness.

If `context_status` fails at startup, the MCP server is not available. Consult the installation documentation for wiring setup.

---

## What This Skill Does

Guides you through populating Unimatrix with foundational knowledge about your repository. The skill explores your repo structure in bounded levels, proposes knowledge entries, and stores only what you approve.

**Depth limit**: Level 0 (automatic) + up to 2 opt-in levels. No deeper. You control how far to go.

**Categories used**: Only `convention`, `pattern`, and `procedure`. Categories like `decision`, `outcome`, and `lesson-learned` are excluded from seeding — they emerge from real feature work, not initial exploration.

---

## Entry Quality Rules

Every proposed entry must pass this quality gate before being shown to you. Entries that fail are silently discarded — you only see quality entries.

| Field | Rule |
|-------|------|
| **What** | One sentence, max 200 characters. Describes the knowledge. |
| **Why** | Min 10 characters. Explains the consequence or motivation — what goes wrong without this knowledge. Must not be tautological (restating "what" without adding value). |
| **Scope** | Where this applies — component, module, or workflow context. Must be present. |

---

## Execution Steps

Follow these steps in strict order. At every gate marked with **STOP**, halt and wait for the human to respond before proceeding. Do not auto-advance.

### Step 1: Pre-flight — MCP Availability Check

**This must be the very first action. Do not read any files before this step.**

Call `context_status()`.

- **If the call fails or returns an error**: Print the following and halt immediately. Do not proceed to any further steps.
  ```
  Unimatrix MCP is not available.
  Ensure unimatrix-server is running and wired in your Claude settings.json.
  See installation documentation for setup instructions.
  ```

- **If the response is healthy** (no error indicators, returns system status): Continue to Step 2.

---

### Step 2: Existing-Entries Check

Check whether seed entries already exist to avoid near-duplicates.

Call `context_search` for each seeding category:
- `context_search(query: "repository", category: "convention", k: 5)`
- `context_search(query: "repository", category: "pattern", k: 5)`
- `context_search(query: "repository", category: "procedure", k: 5)`

Count the total results across all three searches.

**If 3 or more entries found**:

Print:
```
Found {count} existing entries in seeding categories (convention/pattern/procedure).
Re-seeding may create near-duplicates. You can save tokens by skipping if the knowledge base already has a good foundation.

Options:
  supplement — continue and add new knowledge alongside existing entries
  skip — exit without changes
```

**STOP. Wait for human response before proceeding.**

- If the human says **skip**: Print "No changes made." and halt.
- If the human says **supplement**: Continue to Step 3.

**If fewer than 3 entries found**: Continue to Step 3 (clean first run).

---

### Step 3: Level 0 — Automatic Foundational Exploration

Read the following files without requiring human confirmation. Skip any that do not exist:

- `README.md`
- `CLAUDE.md`
- `Cargo.toml` (Rust)
- `package.json` (Node.js)
- `pyproject.toml` (Python)
- `go.mod` (Go)
- List the `.claude/` directory structure (if present) — directory listing only, not deep file reads

From what you read, generate 2 to 4 high-level foundational entries. Typical entries cover:

- **Repository purpose**: What this project does and why it exists
- **Technology stack**: Primary language, framework, key dependencies
- **Project structure**: How the codebase is organized (monorepo, workspace, key directories)
- **Key conventions**: Any obvious conventions visible in top-level config (naming, formatting, etc.)

Apply the quality gate (What/Why/Scope) to each candidate. Silently discard any that fail. If fewer than 2 entries pass the gate, include only what passes — do not pad with low-quality entries.

If 0 entries pass the quality gate:
```
Could not generate quality entries from available files. Consider adding a README.md with project context, then re-run /unimatrix-seed.
```
Skip to the Done summary.

---

### Step 4: Gate 0 — Batch Approval

Present all Level 0 entries as a batch:

```
Level 0 — Foundational Knowledge
=================================
Proposed entries (approve or reject as a batch):

  1. [convention] {what}
     Why: {why}
     Scope: {scope}

  2. [pattern] {what}
     Why: {why}
     Scope: {scope}

  ...

Approve all entries? (approve / reject)
```

**STOP. Wait for human response before proceeding.**

- **If approved**: Store each entry via `context_store`:
  ```
  context_store(
    title: "{what}",
    content: "What: {what}\nWhy: {why}\nScope: {scope}",
    topic: "{repo name or top-level context}",
    category: "{convention|pattern|procedure}",
    tags: ["seed", "level-0"],
    agent_id: "unimatrix-seed"
  )
  ```
  Report success or failure for each entry individually. If a `context_store` call fails, report which entry failed and continue storing the remaining entries.

  Print: "Stored {count} entries."

- **If rejected**: Print "0 entries stored. Re-invoke /unimatrix-seed with more specific guidance if needed." and skip to the Done summary.

After storing (or rejecting), present the Level 1 exploration menu:

```
Would you like to explore deeper? Options:
  a) Module structure — explore source directories and key modules
  b) Conventions — look for coding standards, linting, formatting config
  c) Build & test — explore build system, test framework, CI config
  d) Done — stop here
```

**STOP. Wait for human response before proceeding.**

- If the human selects one or more options (a, b, c, or combinations): Continue to Step 5 with those selections.
- If the human selects **d (Done)**: Skip to the Done summary.

---

### Step 5: Level 1 — Category Exploration (Opt-in)

For each category the human selected, explore relevant files:

- **Module structure**: Read `src/` or `lib/` directory listings, key module entry points, workspace member crates
- **Conventions**: Read `.editorconfig`, `.eslintrc`, `rustfmt.toml`, `.clippy.toml`, `.prettierrc`, similar config files
- **Build & test**: Read CI config (`.github/workflows/`, `Makefile`, `justfile`), test directory structure, build scripts

For each entry generated from exploration, apply the quality gate. For entries that pass, present them **individually** (not as a batch):

```
Proposed entry:
  [{category}] {what}
  Why: {why}
  Scope: {scope}

Store this entry? (yes / no)
```

**STOP. Wait for human response before proceeding.**

- **yes**: Store via `context_store` with tags `["seed", "level-1"]`. Report success or failure.
- **no**: Skip this entry.

Continue until all Level 1 entries have been presented.

---

### Step 6: Gate 1 — Level 2 Decision

```
Level 1 complete. Stored {count} new entries.

Level 2 is the final exploration level. Would you like to go deeper into any area?
  a) {list deeper explorations based on Level 1 selections — e.g., "specific module internals", "test patterns", "CI pipeline details"}
  b) Done — stop here
```

**STOP. Wait for human response before proceeding.**

- If the human selects a deeper exploration: Continue to Step 7.
- If the human selects **Done**: Skip to the Done summary.

---

### Step 7: Level 2 — Deep Exploration (Final Level)

Explore the selected areas in greater depth. Read specific files within the directories explored at Level 1.

For each entry generated, apply the quality gate and present individually (same as Level 1):

```
Proposed entry:
  [{category}] {what}
  Why: {why}
  Scope: {scope}

Store this entry? (yes / no)
```

**STOP. Wait for human response before proceeding.**

Store approved entries with tags `["seed", "level-2"]`.

---

### Step 8: Gate 2 — Terminal

After Level 2 entries are processed:

```
Level 2 complete. This is the final exploration level. No further levels are available.
```

Proceed directly to the Done summary. **Do not offer a Level 3 option. Level 2 is the terminal level.**

---

### Done Summary

```
Seed Summary
============
Total entries stored: {total}
  Level 0: {l0_count}
  Level 1: {l1_count}
  Level 2: {l2_count}

Knowledge base is ready. Future context_briefing calls will return these entries
to agents working in this repository.
```
