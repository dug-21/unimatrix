# crt-027-docs Agent Report

**Agent ID:** crt-027-docs
**Feature:** crt-027 (WA-4 Proactive Knowledge Delivery)
**Issue:** #349

## Status: COMPLETE

## Sections Modified

1. **Core Capabilities — Hook-Driven Invisible Delivery (Cortical Implant)**
   - Updated hook count from five to six.
   - Added `SubagentStart` to the event list.
   - Documented subagent injection behavior (synchronous, fires before first token).
   - Documented `UserPromptSubmit` 5-word minimum guard (`MIN_QUERY_WORDS`).
   - Updated PreCompact description to reflect flat indexed table output (replaces prior section-based description).

2. **Getting Started — Configure Hooks**
   - Added `SubagentStart` hook entry to the JSON configuration example.

3. **Getting Started — First Use Examples**
   - Replaced old `context_briefing(role: ..., task: ..., feature: ...)` example with the new topic-first form: `context_briefing(topic: "crt-027", max_tokens: 1000)`.

4. **MCP Tool Reference — `context_briefing` row**
   - Replaced "orientation briefing for a role and task" description with flat indexed table semantics.
   - Documented: Active-only entries, flat table columns (row, id, topic, category, confidence, snippet), k=20 default.
   - Documented query derivation priority (task > session signals > topic fallback).
   - Documented `role` backward-compat acceptance but ignored status.
   - Documented `UNIMATRIX_BRIEFING_K` deprecation.
   - Updated Key params from `role (required), task (required)` to `topic (required fallback), task, session_id, k (default 20)`.
   - Updated "When to use" to mention calling after `context_cycle(type: "phase-end", ...)`.

## Commit

`dd5e27f` — `docs: update README for crt-027 (#349)`

## Artifact Sources

All claims trace to:
- `SCOPE.md` Goals 1, 2, 5; Proposed Approach sections WA-4a and WA-4b; AC-07, AC-08, AC-09, AC-14.
- `SPECIFICATION.md` FR-01, FR-05, FR-08, FR-12, FR-13, FR-14; AC-07, AC-08, AC-SR01.

## Files Modified

- `/workspaces/unimatrix/README.md` (only)

## Self-Check

- [x] Read SCOPE.md before making edits
- [x] Read current README.md to identify affected sections
- [x] All edits trace to specific claims in SCOPE.md or SPECIFICATION.md
- [x] No source code was read
- [x] Only README.md was modified
- [x] Commit message uses `docs:` prefix
- [x] No aspirational language added
- [x] Terminology consistent: Unimatrix, context_briefing, SubagentStart, SQLite
- [x] Table row count for MCP Tool Reference unchanged (12 tools — one row updated, none added or removed)
