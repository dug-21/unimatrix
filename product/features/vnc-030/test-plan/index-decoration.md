# Test Plan — C2 `index.js` FNF-path stamp decoration

Source: ADR-002. ACs: AC-03, AC-06 (dispatch side), AC-01 (lifecycle dispatch), seam. Risks: R-02, R-05, R-06, R-09, R-07 (dispatch). File: extend `packages/unimatrix/test/hook-client/index.test.js`. Reuse `freshProject()`/`childStateDir()`/`startStubServer`; in-process helper-unit + spawn-level layers (existing two-layer idiom). `npm test -- index-decoration` (and `index`).

Decoration mutates the in-memory `request` **between `buildRequest` and dispatch**, upstream of `selectTransport` (`index.js:410`). `build-request*.js` gets **zero** vnc-030 logic.

## Lifecycle dispatch keyed on CYCLE_* frames (R-02, AC-01)

### test_cycle_start_frame_writes_tracker
- CYCLE_START frame → `cycles.writeCycle(topic, next_phase)` called once; tracker created.

### test_cycle_phase_end_frame_updates_phase (FR-02)
- CYCLE_PHASE_END frame → `cycles.updatePhase(next_phase)`; tracker phase bumped.

### test_cycle_stop_frame_deletes_tracker (FR-03)
- CYCLE_STOP frame → `cycles.deleteCycle`; tracker removed.

### test_lifecycle_events_never_touch_tracker (FR-04, R-02 CRITICAL)
- Fire SessionStart (startup/resume/clear/compact), SessionClose, Stop frames with a tracker present → assert `writeCycle`/`updatePhase`/`deleteCycle` NOT called; tracker file **byte-unchanged** after each.

### test_multiturn_stop_does_not_kill_stamp (R-02 CRITICAL)
- Sequence: cycle_start → 3×(Stop + RecordEvent). After each Stop the tracker is byte-unchanged; **every** post-Stop RecordEvent attaches the stamp. (Stop fires per assistant turn — the delete-on-close lifecycle would kill the stamp after turn 1.)

## Stamp attach (FR-06, AC-02 client side)

### test_recordevent_present_tracker_attaches_cycle_stamp
- Tracker present → outgoing `ImplantEvent.cycle_stamp == {topic, phase}` (phase key omitted when null — implantEvent omit-when-null parity).

### test_recordevent_missing_tracker_no_stamp (FR-06)
- No tracker → no `cycle_stamp` key on the frame; event sent unstamped.

### test_corrupt_tracker_sends_unstamped_no_throw (R-03)
- Corrupt tracker JSON → `readCycle` null → unstamped, exit 0, no throw.

## Extraction suppression strip (R-05, AC-03 — CRITICAL, both directions)

### test_suppression_strip_on_non_cycle_frame (FR-08)
- Same prompt content, tracker present → outgoing frame carries `cycle_stamp` and **no** `topic_signal` (stripped). Tracker absent → carries `topic_signal`, no stamp. (Over-strip and under-strip both fail this.)

### test_cycle_frame_keeps_topic_signal (R-05, ADR-002 §3/§5)
- A CYCLE_* frame with a tracker present **keeps** `topic_signal = topic` (byte-identical to Rust-hook cycle frames) and gets no extra stamp churn. Strip applies to non-CYCLE_* frames ONLY.

### test_unstamped_session_extraction_byte_unchanged
- For a never-declare session, the extraction code path is byte-unchanged vs today (no behavior change).

## Batch / replay decoration (R-06, AC-02 batch)

### test_recordevents_batch_every_event_stamped (R-06)
- A `RecordEvents` batch of mixed CYCLE_*/RecordEvent frames → **every** `ImplantEvent` carries the stamp; `topic_signal` stripped on every non-CYCLE_* member. (Single-frame stamping passing is insufficient — the decoration loop must iterate the batch.)

### test_send_failure_enqueue_replay_carries_stamp (R-06, ADR-002 §2.5)
- Send fails → enqueue (post-decoration) → replay → the replayed batch carries the stamp that was true at event time. (Queue stores the decorated `request`.)

## Canary dispatch (subagent-gated miss branch) (AC-06, R-19 — detail in state-canary.md)

### test_miss_branch_calls_bumpStampMiss_only_for_subagent_drift
- The decoration miss branch calls `state.bumpStampMiss` **iff** subagent-context (depth≥1) AND no `cycles/{root_id}.json`. Depth-0 never-declare miss → NOT called. Full quartet in `state-canary.md` / `seam-and-roundtrip.md` §4.

### test_lifecycle_frames_never_reach_decoration_canary (R-19)
- SessionStart/SessionRegister/SessionClose frames never reach decoration → never increment (verified by frame class); the vnc-027 null sentinel short-circuits non-cycle PreToolUse before decoration.

## Never-declare floor (R-09, AC-04 client side)

### test_never_declare_session_unchanged_pipeline (FR-19)
- No tracker, no stamp → extraction emits `topic_signal` exactly as today; no decoration mutation beyond the (skipped) stamp attach.

## Sync-path isolation (NFR-02, C-06)

### test_sync_trio_no_tracker_io
- Extend the existing AC-08 sync-isolation fs-spy: the sync trio (ContextSearch/CompactPayload/Ping) performs **zero** tracker file I/O; tracker read happens only on the FNF branch.

## Seam dispatch (R-07) — see `seam-and-roundtrip.md` §1 for the gate-blocking seam-survival tests driven through this pipeline.

## Coverage requirement
Suppression is strip-at-decoration on non-CYCLE_* only; extraction byte-unchanged for unstamped sessions; the decoration loop covers single AND batch shapes; lifecycle dispatch keys exclusively on CYCLE_* frames; sync trio gains zero file I/O.
