# Agent Report — vnc-027-agent-3-state-offset-rekey

Component 9 / state.js / ADR-006 (amended, authoritative over FR-30/AC-10 "and/or") / FR-30, FR-31 / AC-10, AC-12 / R-04, R-14. Merge step 4.

## Outcome: COMPLETE

## Files modified
- `packages/unimatrix/lib/hook-client/state.js` (comment-only; logic byte-stable)
- `packages/unimatrix/test/hook-client/state.test.js` (tests added/renamed)

Committed: `37028cc0` on `feature/vnc-027`.

## What changed (and what deliberately did NOT)
Per the pseudocode "most of this file is UNCHANGED": `deleteOffset` and `pruneOffsets` already did exactly what ADR-006 needs. The behavioral keying change lives in the CALLER (index.js, Wave 5), NOT here — and was deliberately NOT added to state.js per the Gate 3a adjudication.

- `deleteOffset` doc comment: corrected from "on successful SessionClose — FR-16" to the ADR-006 canonical-event-keying wording (TaskCompleted, NEVER frame type; Stop and TaskCompleted both build SessionClose frames; unreachable under current HOOK_EVENTS; pinned by unit test; fail-open). Function body unchanged.
- `pruneOffsets` doc comment: already read "called opportunistically on FNF spawns after replay" — now true once index.js wires it (Wave 5). No edit needed (confirmed per pseudocode).
- No logic edits. Offset write cadence, delta format, 1 MiB caps, never-queue-delta rule all untouched (SR-08 scope guard, R-14 s3).

## Tests
- `node --test test/hook-client/state.test.js`: **31 pass / 0 fail / 0 skipped**.
- Added/changed:
  - `test_pruneoffsets_deletes_only_files_older_than_7_days` — 7-day cutoff + strict `< cutoff` boundary (at-cutoff kept).
  - `test_pruneoffsets_mtime_fallback_for_unreadable_json` — corrupt `updated` → mtime decides.
  - `test_pruneoffsets_skips_tmp_remnants` — only `*.json` considered.
  - `test_pruneoffsets_mid_session_degrades_to_one_restream` — pruned mid-session → readOffset 0 (R-04 s4).
  - `test_pruneoffsets_fail_open` — ENOENT dir / empty dir / EACCES readdir → no-op, no throw (R-14 s2).
  - `test_pruneoffsets_unlink_error_best_effort` — unlink failure on one stale file does not abort prune of siblings.
  - `test_delete_offset_unlinks_fail_open` — renamed from `test_offset_deleted_on_sessionclose_success` (old name was keyed to the removed SessionClose-delete behavior; updated deliberately per ADR-006 Consequences, not deleted). Pins the event-agnostic unlink contract this module owns; the TaskCompleted-vs-Stop keying tests belong to index.test.js.
- Pre-existing `test_offset_prune_7days` retained.

### Keying-discrimination tests are NOT in this file (by design)
The test plan's keying tests (`test_taskcompleted_deletes_offset`, `test_stop_must_not_delete_offset`, `test_no_delete_on_failed_send`, `test_taskcompleted_unreachable_under_current_registration`) exercise the CALLER's canonical-event keying, which lives in index.js (Wave 5). Per the spawn directive ("do not add index.js's keying logic here"), they are owned by the index-dispatch component's tests. state.js can only assert the unlink primitive, which it does.

## Size gate
- `node test/check-hook-client-size.js`: **OK** within both budgets.
- state.js: stripped 6278 (was 6272; +6 B = multi-line-comment newline residue), raw 9781 (was 9489; +292 B comment).
- Totals: stripped 68399/100000, raw 110832/160000. Comment-only edits cost effectively zero stripped budget (the C-04 redefinition's intent).

## Self-check confirmations
- pruneOffsets 7-day cutoff + fail-open: CORRECT (tested, including ENOENT/empty/EACCES/unlink-error).
- No SessionClose-keyed delete added here: CONFIRMED (no logic change; keying is index.js's job).
- Size gate passes: CONFIRMED (byte counts above).
- Diff is comment-only on state.js (logic byte-stable, R-14 s3): CONFIRMED via `git diff`.

## Issues / blockers
None for this component.

Note (informational, not my scope): the full hook-client suite has 7 failures in `parity-layer1.test.js` (request goldens, AC-01). These stem from committed `b7c779e3 impl(build-request-sentinel): null no-send sentinel for non-cycle PreToolUse` — the PreToolUse reduction landed but the parity goldens (owned by the parity-corpus-uds component) are not yet regenerated. Confirmed independent of state.js (my change touches no parity path; failures reproduce without my changes). Flagging for the Delivery Leader's wave sequencing.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing / context_search -- found #4809 (key behavior to a hook event only after verifying install-surface registration — directly governs why TaskCompleted keying is unreachable/age-prune is the effective mechanism) and #4772 (pass raw session_id to state offset helpers; offsetPath sanitizes internally). Applied both.
- Stored: nothing novel to store -- the governing trap (recognized-but-unregistered hook event = dead keying path; age-prune is the honest fallback) is already captured by #4809, and this component was a comment-only doc correction plus tests. No new runtime-invisible gotcha surfaced.
