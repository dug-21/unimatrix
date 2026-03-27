# Agent Report: bugfix-391-retro-architect

**Feature**: col-025 (bugfix-391 retrospective — architect role)
**Mode**: retrospective (not design)
**Date**: 2026-03-26

## Scope

Retrospective review of bugfix-391: a minimal single-file fix (1 production line changed, test updated) to guard `set_current_goal` in `handle_cycle_event`'s cycle_start arm.

## 1. Patterns

**Skipped** — the `if goal.is_some()` guard is not a new pattern; it mirrors `set_current_phase` which already existed in the same function. No new component pattern to extract.

## 2. Procedure Review

Procedure #840 ("Integration test harness: how to run smoke and full suite") does not require updating. It correctly documents *what* commands to run and the harness structure. The sleep outlier came from *how* the tester agent awaited results (polling output files), not from the harness design itself.

The appropriate home for this finding is the lesson-learned entry. See Lessons section below.

## 3. ADR Status

**ADR-004 col-025** (entry #3399 — "Session Resume Goal Lookup Degrades to None on Any Failure"): **Validated**.

The gate report confirms the fix explicitly preserves ADR-004's mandate: the `set_current_goal` call at the session-resume call site (~L588) remains unconditional, as required by the ADR. The guard was added only to the cycle_start event arm. No supersession or deprecation needed.

All other col-025 ADRs (#3396, #3397, #3402, #3404, #3405): unaffected by this bugfix.

## 4. Lessons

### A. Cycle_start handler guard pattern — NEW (entry #3499)

**Stored**: "handle_cycle_event: new optional state fields must mirror the if-Some guard used by existing fields"

The bug was a missing application of the `if field.is_some()` guard that all existing optional fields in the same handler use. The pseudocode showed the call unconditionally; the implementer followed the pseudocode without cross-checking surrounding code. The lesson captures the checklist for future implementers adding optional state fields to handle_cycle_event, and notes the explicit exemption for the session-resume arm (ADR-004).

### B. Sleep polling recurrence — UPDATED (entry #3388 -> #3498)

**Corrected** existing entry #3388 to add bugfix-391 as the third confirmed recurrence (24 sleep instances, 3.7σ above mean). The tester agent polled `/tmp/tools_results.txt` and task output files with sleep delays rather than using `run_in_background + TaskOutput`. Updated "When to apply" to explicitly include tester agent verification runs. Recurrence count is now 3 (crt-023, col-024, bugfix-391).

### C. ToolSearch pre-load sequencing — NEW (entry #3500)

**Stored**: "ToolSearch must complete before any context_get or dependent MCP tool calls in discovery phase"

The coordinator fired `context_get` 6 times before ToolSearch returned the Unimatrix schema, producing 6 tool-not-found failures in the first 5 minutes. The lesson captures the sequencing rule: ToolSearch must be the first call and must complete before any dependent MCP tool is invoked. Detection signature: `tool_failure_hotspot` on `context_get` all in the first 5 minutes of a session.

## 5. Retrospective Findings

| Hotspot | Action |
|---------|--------|
| sleep_workaround_count (24, 3.7σ) | Updated existing lesson #3388 → #3498 with third recurrence |
| tool_failure_hotspot (context_get, 6 failures, discovery) | Stored new lesson #3500 |
| compile_cycles (19) | No lesson stored — 19 cycles across an 84-minute session for a single-file fix is above average but not a novel failure mode; covered by existing "batch field additions before compiling" recommendation already in retrospective output |
| edit_bloat (265.6 KB average, 5.3×) | No lesson stored — single-file test updates with large inline test bodies; not generalizable |
| reread_rate (13 files, multiple times) | No action — consistent with investigation of an unfamiliar call site; not actionable beyond existing search-before-read conventions |

**Recommendation dispositions**:
- `[sleep_workarounds]` Use run_in_background + TaskOutput — **actioned**: lesson #3498 updated
- `[compile_cycles]` Batch field additions before compiling — noted; no new entry needed (existing recommendation is correct, this was a straightforward guard addition not a multi-field type change)

**Positive baselines** (no action needed): 0 permission friction events, correct Grep/Glob usage (52 vs mean 150 bash-for-search), 0 coordinator respawns, 0 post-completion work.

## Knowledge Stewardship

| Action | Entry | Details |
|--------|-------|---------|
| Corrected | #3388 → #3498 | Added bugfix-391 as third sleep-polling recurrence |
| Stored | #3499 | handle_cycle_event optional field guard lesson |
| Stored | #3500 | ToolSearch pre-load sequencing lesson |
| Validated | #3399 | ADR-004 col-025 — no change needed |
