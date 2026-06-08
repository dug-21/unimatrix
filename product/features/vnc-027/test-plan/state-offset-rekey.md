# Test Plan — state-offset-rekey (`lib/hook-client/state.js`)

Component 9 / ADR-006 (amended, authoritative over FR-30/AC-10 "and/or") / FR-30, FR-31 /
**AC-10, AC-12** / Risks R-04 (High), R-14 (Med). `deleteOffset` keyed to canonical `TaskCompleted`;
`pruneOffsets` (currently caller-less) goes live. `node --test` on `state.test.js` + Layer 2 multi-turn.

## Keying discrimination — AC-10 / R-04 (canonical event, NEVER frame type)

- `test_taskcompleted_deletes_offset` — `deleteOffset` fires when the carrying send succeeds AND canonical event is `TaskCompleted` (the retained branch works IF the event ever arrives — R-04 s1).
- `test_stop_must_not_delete_offset` — **assertable negative**: a `Stop` spawn (ALSO a SessionClose frame) does NOT delete the offset. Frame-type keying would wrongly delete every turn; the keying must discriminate by canonical event name (R-04 s2, ADR-006).
- `test_no_delete_on_failed_send` — `TaskCompleted` with a failed carrying send → no delete (delete only on success).
- `test_taskcompleted_unreachable_under_current_registration` — documented: `TaskCompleted` is in neither `HOOK_EVENTS` nor settings.json; the branch is unreachable end-to-end but pinned by `test_taskcompleted_deletes_offset` (FR-22-by-analogy: not silent dead code).

## Age-prune — AC-10 (the SOLE effective mechanism, ADR-006 §2)

- `test_pruneoffsets_deletes_only_files_older_than_7_days` — files with mtime > 7 days deleted; newer files kept.
- `test_pruneoffsets_fail_open` — unreadable dir / ENOENT / EACCES → returns without throwing (fail-open, R-14 / R-04 s4).
- `test_pruneoffsets_mid_session_degrades_to_one_restream` — a pruned mid-session offset → next delta re-streams from 0 once (idempotent server-side merge), no error path (R-04 s4).

## Multi-turn persistence — AC-10 (the actual FR-16 defect fix)

- `test_offsets_persist_across_n_stop_turns` (Layer 2, multi-turn integration) — across N `Stop` turns the offset file SURVIVES; delta sends after turn 1 are true deltas (no re-stream from 0 every turn) — R-04 s3. Reuse `parity-layer2.test.js` grow/hold harness with the new keying.

## HTTP-path regression guard — AC-12 / R-14 (SR-08's exact fear)

- `test_offset_write_cadence_unchanged` — offset write cadence byte-identical to F3.
- `test_delta_frame_format_unchanged` — delta frame format, 1 MiB caps, never-queue-delta rule pinned unchanged (existing F3 tests must still pass byte-identical).
- `test_f3_delta_suite_green_with_only_keying_assertions_changed` — full F3 parity + delta suites pass; the ONLY changed assertions are delete-timing, diff-reviewed against FR-31 (R-14 s1). The single externally visible HTTP change is delete timing.

## Edge cases
- Abandoned session (TaskCompleted never fires): offset survives until 7-day prune, then one full re-stream — asserted safe.
- `pruneOffsets` on an empty offsets dir → no-op, no throw.
- Concurrent spawns writing different session offsets → one-file-per-session, no cross-interference.
