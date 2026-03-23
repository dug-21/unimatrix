# crt-027 Architect Update Report

Agent ID: crt-027-agent-1-architect-update

## Task

Resolve three vision alignment warnings (WARN-1, WARN-2, WARN-3) in the crt-027 architecture
artifacts.

## Changes Made

### WARN-2 — SubagentStart stdout JSON envelope (ADR-006)

**File:** `product/features/crt-027/architecture/ADR-006-subagentstart-stdout-json-envelope.md`
**Unimatrix ID:** #3251

New ADR written and stored. Decision: add `write_stdout_subagent_inject(entries_text: &str)`
helper in `uds/hook.rs` that wraps formatted entries in the `hookSpecificOutput` JSON envelope
required by Claude Code for SubagentStart context injection. The dispatch path branches on
`source == "SubagentStart"` to call this helper; all other sources use the existing plain-text
`write_stdout`. No server-side changes.

ARCHITECTURE.md updated:
- SR-01 section rewritten from "Unconfirmed / graceful fallback" to "Confirmed — requires
  JSON envelope" with the exact envelope structure documented.
- Component Interactions diagram updated to show `write_stdout_subagent_inject` in the
  SubagentStart path and plain `write_stdout` in the UserPromptSubmit path.
- Technology Decisions table gains ADR-006 row.
- Integration Surface table gains `write_stdout_subagent_inject` row with signature.
- Open Questions section updated: SR-01 now resolved (not unconfirmed).

### WARN-3 — `.trim()` guards

**Files updated:** ARCHITECTURE.md, ADR-002

Both guards now explicitly specify `.trim()`:
- SubagentStart: `query.trim().is_empty()` (was `query.is_empty()`)
- UserPromptSubmit empty check: `query.trim().is_empty()` (was `query.is_empty()`)
- UserPromptSubmit word count: `query.trim().split_whitespace().count()` (was `query.split_whitespace().count()`)

ADR-002 Consequences section extended with an explanation of the trim semantics: trimming
is evaluation-only (the stored `query` value remains untrimmed); whitespace-only prompts are
treated as empty by both guards.

### WARN-1 — `MIN_QUERY_WORDS` in Goals

**File:** `product/features/crt-027/SCOPE.md`

Goal 5 added:
> "5. Add `MIN_QUERY_WORDS: usize = 5` compile-time constant in `hook.rs`. UserPromptSubmit
> with fewer than 5 trimmed words produces no injection."

ARCHITECTURE.md Integration Surface table updated: `MIN_QUERY_WORDS` entry now reads
`const usize = 5` with the note that word count is evaluated on `query.trim().split_whitespace()`.

## ADR File Paths

| ADR | File | Unimatrix ID |
|-----|------|--------------|
| ADR-001 | `architecture/ADR-001-contextsearch-source-field.md` | (prior feature) |
| ADR-002 | `architecture/ADR-002-subagentstart-routing-and-word-guard.md` | #3243 |
| ADR-006 | `architecture/ADR-006-subagentstart-stdout-json-envelope.md` | #3251 |

## No Open Questions

All three warnings are fully resolved. No new unknowns introduced.
