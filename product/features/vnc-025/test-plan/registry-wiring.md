# Test Plan: registry-wiring (`infra/session.rs`)

Covers R-06, R-08 (registry side), AC-03 (registry arm), AC-10, NFR-02/03, NFR-09 (poison
layer). Tests live in the existing `session.rs` test module (`:710`). Most are `#[tokio::test]`
where they interact with async registry APIs; pure-shape tests are plain `#[test]`.

## §1 `apply_transcript_delta`

- `test_apply_transcript_delta_registered_merges` — register, apply, assert bytes visible via
  the state's buffer (`contiguous_tail`) and `last_activity_at` bumped.
- `test_apply_transcript_delta_unregistered_silent_noop` — unknown session: no panic, no slot
  created (registry size unchanged), no other session's buffer affected (AC-03). Assert no
  allocation before the registry check (structural review: the byte slice is borrowed until
  after lookup — review gate, plus the no-slot assertion).
- `test_apply_transcript_delta_no_memcpy_under_registry_lock` — structural review gate
  (NFR-03): registry lock scope contains lookup + Arc clone + scalar bump only. Verified by
  code review against ADR-001; no practical runtime assertion.

## §2 Drain / sweep signature changes — R-08

- `test_drain_returns_signal_and_purge_record` — non-empty buffer:
  `Some((SignalOutput, Some(record)))`, `record.bytes_purged == buffer len`, key removed.
- `test_drain_empty_buffer_returns_none_record` — `Some((_, None))`.
- `test_drain_unknown_session_returns_none`.
- `test_sweep_returns_purge_records_for_stale` — stale sessions with non-empty buffers appear
  in the `Vec<TranscriptPurgeRecord>`; fresh sessions untouched.
- `test_sweep_silently_evicted_session_yields_purge_record` (MANDATORY, R-08.1, #4140):
  register, stream deltas, NEVER inject (empty `injection_history`), idle past threshold →
  swept with NO `SweepResult` but WITH a `TranscriptPurgeRecord`. The audit-row half of this
  case is purge-audit.md §1.
- `test_sweep_empty_buffer_session_no_purge_record` — zero-byte purges produce no record
  (ADR-004 suppression feeds from here).
- `test_signal_output_shape_unchanged` — serialized `SignalOutput` byte-identical to
  pre-change (it feeds the persisted signal queue — ADR-004 firm constraint). Snapshot
  existing serialization in a fixture before Stage 3b edits.

## §3 Lock discipline + poison recovery — R-06, NFR-09 Layer 2

- `test_concurrent_deltas_and_state_reads_no_deadlock` — N tasks streaming deltas to one
  session, M tasks looping `get_state()` + `contiguous_tail` reads; bounded-time completion
  (registry→buffer order only; tokio test with timeout).
- `test_poisoned_buffer_mutex_recovery` (MANDATORY, R-06.2, ADR-008): poison via a test
  helper that panics inside a closure holding the buffer lock (std::thread + catch_unwind).
  Then assert at EVERY lock-site class:
  - merge: `apply_transcript_delta` succeeds against a cleared buffer (treat-as-empty),
    subsequent deltas accumulate — dispatch still Acks (dispatch half in dispatch-wiring §1);
  - read: `contiguous_tail` path yields the empty-buffer result (PreCompact degrades);
  - purge: drain/sweep report best-effort `bytes_purged` without panicking.
- Review gate: grep — no `lock().unwrap()` on the buffer mutex anywhere in the crate; every
  site uses the `into_inner()` + `clear()` recovery shape.
- `test_clear_transcripts_for_feature_under_concurrent_stream` — clear while delta tasks run:
  no deadlock (Arcs cloned under registry lock, cleared after release); post-clear merges
  still apply.
- `test_orphaned_arc_merge_harmless` — clone an Arc handle, drain the session (key removed),
  then merge into the orphan: no panic; re-register same session id → fresh buffer with no
  ghost content (Edge Cases list: re-registration).

## §4 Clone cost — AC-10, NFR-02

- Structural demonstration (primary): `SessionState.transcript: Arc<Mutex<TranscriptBuffer>>`
  — review checklist item, satisfied by the field type (ADR-001).
- Optional guard test `test_get_state_does_not_deep_copy_transcript`: fill buffer near 4 MiB,
  `get_state()`, assert `Arc::strong_count` incremented and snapshot's buffer ptr-eq the
  live one (`Arc::ptr_eq`). Cheap and pins the structure — include it.

## §5 Constructor

- `test_with_transcript_cap_propagates_to_new_sessions` — construct with 128 KiB; register;
  overflow at 128 KiB not 4 MiB (registry half of config-knob.md §3 chain).
- `test_new_defaults_to_4mib` — `SessionRegistry::new()` keeps the default (ADR-006; test
  ergonomics contract).
