# Agent Report — vnc-025-agent-7-dispatch-purge (retry)

Components: **dispatch-wiring** + **purge-audit** (combined; both edit `uds/listener.rs`).

## Prior-State Audit

The inherited working-tree changes were audited line-by-line against
`pseudocode/dispatch-wiring.md` and `pseudocode/purge-audit.md`:

- **Single arm (listener.rs:~783)** — correct: typed parse, `apply_transcript_delta`,
  early-return `Ack`, persistence below unreachable. One fix applied (see Deviation below).
- **Batch tee (listener.rs:~1021)** — correct: tee loop sits BEFORE the vnc-024 filter;
  the filter line is byte-identical in the diff (R-04.3 review gate holds). Same parse-error
  fix applied.
- **`handle_compact_payload` (listener.rs:~1630)** — correct: `lock_buffer` (poison-safe,
  pattern #4748) → `contiguous_tail(12_000)` → `extract_transcript_block_from_bytes` →
  `prepend_transcript`, all BEFORE `token_count`; `None` block leaves `content` untouched.
- **`emit_purge_audits` (listener.rs:~1840)** — correct pinned shape (`operation
  "transcript_session_purged"`, `agent_id "server"`, `detail "bytes=<n> trigger=<t>"`,
  `Success`, `target_ids []`), `tokio::spawn` + `log_event_async`, fire-and-forget,
  content-free warn on failure. `AuditEvent::default()` supplies `metadata: "{}"` so the
  store's empty-metadata warn never fires.
- **`process_session_close`** — audit threaded via the EXISTING `services.store_ops.audit`
  (no dispatch signature change); drain + sweep purge records consumed per Edit 4.
- **`services/status.rs` sweep site** — wired (not dropped): the maintenance tick is the
  primary stale-sweep driver; dropping records there would leave most sweep purges
  unaudited. `AuditLog` is a stateless handle over the same store — handle construction,
  not new audit plumbing.
- **`infra/session.rs`** — only addition is test-support `backdate_session_for_test`
  (cfg(test)/test-support gated), used by the purge-audit tests.

What was missing: ALL component tests. Implemented this session.

## Deviation from Pseudocode (flagged, deliberate)

Pseudocode's parse-failure arm logs `error = %e` claiming "serde error Display carries
position/type info, not payload bytes". **That is false for `serde_json::from_value`**:
invalid-type errors embed the offending string VALUE in Display (e.g.
`invalid type: string "<payload bytes>", expected u64`) — a malformed delta with
transcript/secret content in a wrong-typed field would leak into logs, violating the
R-05/AC-04/AC-12 content-free hard gate. Both arms now log `category = ?e.classify()`
(serde_json `Category` — content-free) instead. The sentinel test
`test_delta_paths_never_log_sentinel` pins exactly this case (sentinel as the wrong-typed
`offset` value) and fails against the pseudocode's version.

## Files Modified / Created

- `crates/unimatrix-server/src/uds/listener.rs` — production wiring (inherited + parse-log
  fix) + test-module includes
- `crates/unimatrix-server/src/uds/listener/tests/transcript.rs` — NEW: shared helpers
  (`Deps`, `capture_tracing`, `buffer_contents`, `poison_buffer`) + §1/§2 tests (397 lines)
- `crates/unimatrix-server/src/uds/listener/tests/compact.rs` — NEW: §3 PreCompact, §4
  sentinel logging, §5 HTTP convergence tests (325 lines)
- `crates/unimatrix-server/src/uds/listener/tests/purge_audit.rs` — NEW: purge-audit tests
  (331 lines)
- `crates/unimatrix-server/src/http/router/tests.rs` — event_type-preservation tests
  (single + mixed batch incl. `transcript_delta`), pattern #4725
- `crates/unimatrix-server/src/infra/session.rs` — inherited test-support backdate helper
- `crates/unimatrix-server/src/services/status.rs` — inherited sweep audit wiring

## Tests Implemented

dispatch-wiring (`uds/listener/tests/transcript.rs`):
- `test_transcript_delta_uds_merges_into_registered_buffer` (AC-01)
- `test_transcript_delta_unregistered_acks_no_slot` (AC-03)
- `test_transcript_delta_poisoned_buffer_still_acks` (ADR-008)
- `test_transcript_delta_over_cap_still_acks` (FR-06 matrix; malformed arm covered by the
  unmodified vnc-024 suite)
- `test_mixed_batch_persists_non_delta_merges_delta` (AC-05/R-04.2 — UDS + HTTP-shaped,
  asserts row CONTENT across all persisted columns)
- `test_delta_dispatch_emits_no_new_audit_events` (#3902 signature)
- `test_compact_payload_nonempty_buffer_prepends_tail_block` (AC-11 end-to-end golden,
  expectation computed via `extract_transcript_block(path)`, shuffled deltas)
- `test_compact_payload_token_count_includes_prepended_block` (R-09.5)
- `test_compact_payload_contiguous_tail_none_identical_to_empty` (FR-18/FR-19)
- `test_compact_read_concurrent_with_deltas_point_in_time` (R-09.6, multi-thread)
- `test_delta_paths_never_log_sentinel` (R-05.2/AC-04 — tracing capture across single arm,
  batch tee, merge, overflow, compact, purge)
- `test_http_delta_lands_in_http_prefixed_buffer` (AC-06)
- `test_http_delta_without_session_write_rejected_before_dispatch` (R-12)

purge-audit (`uds/listener/tests/purge_audit.rs`):
- `test_session_close_purge_emits_audit_row` (AC-08, pinned shape)
- `test_sweep_purge_emits_audit_rows` (AC-08)
- `test_silently_evicted_session_gets_audit_row` (R-08.1 MANDATORY named case)
- `test_empty_buffer_purge_emits_nothing` (R-07.4)
- `test_purge_completes_when_audit_store_unavailable` (FR-14/R-07.1/.2 — closed write
  pool; purge stands; exactly one content-free warn, no retry)
- `test_sweep_burst_all_audits_land` (R-07.3, 22 sessions)
- `test_purge_never_blocks_on_audit_latency` (FR-14 — emission returns <200ms while the
  write path is lock-blocked; row lands after release)
- `test_purge_audit_row_sentinel_free` (R-05.3 — every column)
- `test_close_and_sweep_in_one_pass_emit_both_triggers` (dispatch-wiring §6 end-to-end)

http convergence (`http/router/tests.rs`):
- `test_prefix_session_id_preserves_event_type_single`
- `test_prefix_session_id_preserves_event_type_batch_every_element` (mixed batch)

## Hard Gates Verified

- **R-04**: vnc-024 zero-rows suite (5 tests) untouched — zero test-body edits in the diff;
  all pass with the buffer active. Batch filter line byte-identical.
- **R-09.4/FR-18**: `test_compact_payload_empty_buffer_byte_identical` passes against the
  committed Wave 0 fixtures (`compact_payload_empty_buffer.*.json`).
- **AC-12**: content-opaque everywhere; sentinel tests (logs + audit row) pass; production
  parse-log hardened (see Deviation).

## Out of Scope (deliberate)

- `cycle_review` trigger tests (`test_cycle_review_purge_emits_audit_rows`,
  sweep/cycle-review race) — owned by the cycle-review-purge component (`mcp/tools.rs`,
  which I was instructed not to touch). The close/sweep race variant is structurally
  single-row (drain removes the session before sweep can see it).
- Integration tests — Stage 3c (not run, not modified).

## Test Results

- `uds::listener::tests`: 196 passed, 0 failed (includes all new + the unmodified vnc-024
  suite + the Wave 0 empty-buffer baseline hard gate)
- `http::router::tests::test_prefix_session_id*`: 11 passed, 0 failed
- Full `cargo test -p unimatrix-server --lib`: **3594 passed, 0 failed** (the
  `http::token` flake noted in the spawn prompt passed this run)
- `cargo fmt` clean; `cargo clippy --all-targets` introduces ZERO new warnings (remaining
  warnings in listener.rs:18 / session.rs / status.rs pre-exist at HEAD)
- No `todo!`/`unimplemented!`/TODO/FIXME; no `.unwrap()` in production hunks

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-001..006 (#4739–#4744),
  lesson #3902 (no new audit on dispatch happy path — pinned by
  `test_delta_dispatch_emits_no_new_audit_events`), and pattern #4723 (vnc-024 dual-arm
  drop). All applied.
- Stored: entry #4749 "serde_json error Display leaks string payload values — log
  e.classify() in content-free paths" via context_store (pattern, topic
  `unimatrix-server`) — the gotcha behind the one production deviation.
