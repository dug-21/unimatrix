# Test Plan: purge-audit (three purge points → `transcript_session_purged`)

Covers R-07, R-08 (audit half), R-05.3; AC-08, FR-12/13/14. Tests span `listener.rs`
(session-close, sweep call sites) and `tools.rs` (cycle-review — emission mechanics here,
clear semantics in cycle-review-purge.md). Audit rows asserted via the test `AuditLog`/store.

## §1 Emission at each purge point — AC-08

Expected row shape (all three triggers): `operation == "transcript_session_purged"`,
`agent_id == "server"`, `session_id` set, `detail == "bytes=<n> trigger=<t>"` with
`t ∈ {session_close, stale_sweep, cycle_review}`, `outcome: Success`, `target_ids: []`.

- `test_session_close_purge_emits_audit_row` — register, stream, close via
  `handle_session_close`: row lands with `trigger=session_close`, `bytes=<buffer len>`.
- `test_sweep_purge_emits_audit_rows` — stale sessions swept: one row per non-empty buffer,
  `trigger=stale_sweep`.
- `test_silently_evicted_session_gets_audit_row` (MANDATORY, R-08.1) — the audit half of
  registry-wiring §2's named case: empty `injection_history`, non-empty buffer, swept → audit
  row present despite no `SweepResult`.
- `test_cycle_review_purge_emits_audit_rows` — `trigger=cycle_review`, one row per cleared
  non-empty session.
- `test_empty_buffer_purge_emits_nothing` — all three triggers with empty buffers: assert
  ABSENCE of any `transcript_session_purged` row (ADR-004 zero-byte suppression).
- `test_sweep_and_cycle_review_racing_single_audit` (Edge Cases list) — both purge the same
  session's buffer: at most one non-zero row for those bytes (the second sees an empty buffer
  → suppressed; no double-count).

## §2 Emission mechanics + failure independence — R-07

- Review gate (#4379 pattern, emission context): all emissions use `log_event_async`
  fire-and-forget; no `log_event`/`block_in_place` from async contexts; emission happens
  AFTER lock release (collect-under-lock/emit-after-release, ADR-004). For
  `handle_session_close`, the existing `Arc<AuditLog>` dispatch param is threaded in — no
  new audit plumbing.
- `test_purge_completes_when_audit_store_unavailable` (FR-14) — inject audit-write failure
  (closed/failing store): purge stands (buffer gone/cleared), close path completes, a
  content-free `tracing::warn!` fired, no retry loop (bounded call count).
- `test_sweep_burst_all_audits_land` — sweep 20+ sessions with non-empty buffers in one
  pass: all rows eventually land (async drain, poll with timeout); write pool does not
  starve (#2266 single-connection precedent — run against the default test pool).
- `test_purge_never_blocks_on_audit_latency` — slow audit sink (sleeping store wrapper):
  drain/sweep return promptly (fire-and-forget; assert wall-time bound well under the sink
  delay).

## §3 Content-free audit — R-05.3, AC-12 arm

- `test_purge_audit_row_sentinel_free` — buffer content holds the shared sentinel; purge;
  fetch the row and assert EVERY column (incl. `detail`) is sentinel-free.
- Static gate: the `detail` format string interpolates only `bytes_purged` and the trigger
  token — no content-typed value in scope at the emission sites (review checklist).
