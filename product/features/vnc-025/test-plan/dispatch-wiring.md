# Test Plan: dispatch-wiring (`uds/listener.rs` arms + `handle_compact_payload`)

Covers R-04 (hard gate), R-05.2, R-09.4/.5/.6, R-12, integration risk #3902; AC-01, AC-03,
AC-04, AC-05, AC-06, AC-11 (server side). Tests live in the existing `listener.rs` test module
(`:2951`), following the vnc-024 direct-dispatch pattern (`:5253` block).

## §1 Single-arm merge — AC-01, AC-03

- `test_transcript_delta_uds_merges_into_registered_buffer` — direct dispatch of a
  well-formed delta for a registered session: response is `Ack`; buffer content equals
  streamed bytes (via registry state → `contiguous_tail`).
- `test_transcript_delta_unregistered_acks_no_slot` — unknown session: `Ack`, registry size
  unchanged, a second registered session's buffer unaffected (AC-03).
- `test_transcript_delta_poisoned_buffer_still_acks` — dispatch half of registry-wiring §3:
  after poisoning, dispatch returns `Ack` (always-Ack survives recovery, ADR-008/Constraint 4).
- Always-Ack matrix: malformed / unregistered / over-cap / poisoned — every outcome `Ack`,
  never `Error` (FR-06). One parameterized test or four named tests.

## §2 Batch tee + non-persistence — R-04 (HARD GATE), AC-05

- **vnc-024 zero-rows suite runs UNMODIFIED** (test bodies untouched — review-diff gate):
  ```
  test_transcript_delta_uds_acks_zero_rows
  test_transcript_delta_in_batch_dropped_rest_persist
  test_transcript_delta_malformed_payload_still_acks_zero_rows
  test_transcript_delta_requires_session_write
  test_transcript_delta_parses_into_typed_payload
  ```
  These now run with the buffer ACTIVE — they must pass without edits. Any edit to make them
  pass is a gate failure.
- `test_mixed_batch_persists_non_delta_merges_delta` (new) — batch of deltas + normal events
  over direct dispatch: exact row count for normal events; deltas merged into the buffer;
  zero delta-derived rows AND delta bytes absent from every persisted column (assert row
  content, not just count). Repeat through the HTTP-shaped path (post-`prefix_session_id`).
- Review-diff gate: the filter line at `listener.rs:1009` is byte-identical pre/post
  (ADR-003 — explicit one-line diff check in Stage 3c).
- `test_delta_dispatch_emits_no_new_audit_events` (#3902 signature) — a normal delta dispatch
  fires zero audit events (merge sits after the existing `sanitize_session_id` inside the
  same arm; a new audit row on the happy path is the regression signature).

## §3 PreCompact block build — R-09, AC-11

- `test_compact_payload_nonempty_buffer_prepends_tail_block` — stream fixture JSONL bytes as
  shuffled deltas, send `CompactPayload`: `BriefingContent` starts with the block produced by
  `extract_transcript_block(path)` on the same fixture (end-to-end golden — complements
  transcript-block §2's unit-level golden).
- `test_compact_payload_empty_buffer_byte_identical` (HARD GATE, R-09.4, FR-18): snapshot the
  full `CompactPayload` response for a never-streamed session BEFORE Stage 3b changes; assert
  post-change response is byte-identical (the no-double-prepend guard).
- `test_compact_payload_contiguous_tail_none_identical_to_empty` — hole in the tail window →
  `None` block → response identical to the empty-buffer path (FR-18/FR-19 failure-mode row).
- `test_compact_payload_token_count_includes_prepended_block` — `token_count` computed AFTER
  prepend (R-09.5 ordering parity).
- `test_compact_read_concurrent_with_deltas_point_in_time` — deltas stream while compact is
  handled: block parses (no torn read; the buffer lock guarantees it — R-09.6, pins the
  snapshot-aliases-live-buffer integration risk).

## §4 Content-free logging — R-05.2, AC-04

- `test_delta_paths_never_log_sentinel` — with a tracing capture layer: dispatch malformed
  AND well-formed deltas carrying the shared sentinel through single arm, batch tee, merge,
  overflow (small-cap registry), and purge; assert sentinel never appears in captured output.
  Malformed case additionally asserts `Ack` (AC-04).
- vnc-024's malformed-payload test (already in the unmodified suite §2) keeps the parse-
  failure arm honest.

## §5 HTTP convergence — R-12, AC-06 (pattern #4725)

- `test_prefix_session_id_preserves_event_type_single` and `_batch_every_element` (incl.
  mixed batch) — pre-dispatch transform tests in `http/router.rs`/observe tests; if vnc-024
  already has them, extend only for the mixed-batch shape if missing.
- `test_http_delta_lands_in_http_prefixed_buffer` — delta via the HTTP-shaped request merges
  into `http-{id}`'s buffer, NOT a bare-`{id}` buffer.
- `test_http_delta_without_session_write_rejected_before_dispatch` — missing/insufficient
  bearer or absent `SessionWrite` → rejected, no merge occurred (buffer empty). vnc-024's
  `test_transcript_delta_requires_session_write` covers the UDS-arm capability check.
- Shared-arm proof: §1 direct-dispatch tests are the once-only proof per #4725 — do not
  duplicate merge assertions per transport.

## §6 Drain/sweep call-site updates (`listener.rs:1796/:1814`)

- Compile-enforced by the signature change; behavioral half is purge-audit.md §1 (audit
  emission from the new tuple/vec shapes). Assert here only that session-close and sweep
  flows still complete end-to-end (existing close/sweep tests pass, updated call sites only).
