# crt-027 Spec Update Agent Report

Agent ID: crt-027-agent-2-spec-update

## Summary

Applied three targeted updates to SPECIFICATION.md to resolve items from the vision alignment report. No sections were rewritten; only the specific FRs, ACs, and supporting text were modified.

## Changes Applied

### WARN-1 — `MIN_QUERY_WORDS` compile-time constant (FR-05)

- Updated FR-05 to state explicitly that word count is computed via `query.trim().split_whitespace().count()` (was "whitespace-delimited words", now names the exact method chain).
- Updated the MIN_QUERY_WORDS Guard domain model to document `.trim()` stripping and give a concrete example (`"  approve  "` counts as 1 word).

### WARN-2 — SubagentStart stdout format (AC-SR01 CONFIRMED + new FR + new ACs)

**FR added:**
- FR-04b — SubagentStart hook MUST write `hookSpecificOutput` JSON envelope to stdout (not plain text). Documents the required JSON structure, the `additionalContext` field content, and that the two paths produce structurally different stdout output.

**FR updated:**
- FR-02 — Added `.trim().is_empty()` explicit method name and the whitespace-only example (`"   "` treated as absent).

**ACs updated:**
- AC-SR01 — Changed from OPEN/blocking to CONFIRMED. Citation: "Confirmed via Claude Code hooks documentation — SubagentStart supports additionalContext injection via hookSpecificOutput JSON envelope." Verification updated to point to AC-SR02.

**ACs added:**
- AC-SR02 — SubagentStart produces valid `hookSpecificOutput` JSON envelope with non-empty `additionalContext`. Unit test on `write_stdout` path.
- AC-SR03 — UserPromptSubmit produces plain text (not JSON envelope). Stdout MUST NOT contain `"hookSpecificOutput"`. Unit test confirms format divergence.

**Open Questions updated:**
- OQ-SR01 changed from BLOCKING to RESOLVED. No spike or pivot needed.

**User Workflow updated:**
- Step 7 and 8 in "Subagent receives knowledge at spawn" updated to describe the envelope format.

### WARN-3 — `.trim()` whitespace guards (new ACs)

**ACs added:**
- AC-23b — SubagentStart with whitespace-only `prompt_snippet` (`"   "`) falls through to `RecordEvent` via `.trim().is_empty()` guard (not `.is_empty()`). Unit test specified.
- AC-23c — `UserPromptSubmit` with `"  approve  "` (1 real word, surrounding whitespace) counts as 1 word via `.trim().split_whitespace().count()`, falls through to `RecordEvent`. Unit test specified.

**FR updated:**
- FR-02 — `.trim().is_empty()` guard made explicit with whitespace-only example.

## AC Count Delta

| Change | Count |
|---|---|
| ACs confirmed/status-changed | 1 (AC-SR01: OPEN → CONFIRMED) |
| ACs added | 5 (AC-SR02, AC-SR03, AC-23b, AC-23c) + updated FR-02 references |
| FRs added | 1 (FR-04b) |
| FRs updated | 2 (FR-02, FR-05) |
| Open Questions resolved | 1 (OQ-SR01) |

Pre-update AC count (numbered): AC-01 through AC-25, AC-02b, AC-SR01 = 28 ACs
Post-update AC count: added AC-SR02, AC-SR03, AC-23b, AC-23c = **+4 ACs (32 total)**

## Knowledge Stewardship

- Queried: /uni-query-patterns for hook stdout format, SubagentStart injection — no results (this is new territory introduced by WARN-2 resolution; the hookSpecificOutput envelope is a Claude Code platform detail not yet recorded as a convention).
