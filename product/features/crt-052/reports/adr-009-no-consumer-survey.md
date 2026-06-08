# ADR-009 No-Consumer Audit Survey — `transcript_session_purged` Per-Close Cadence

**Feature:** crt-052 (GH #689) · **Gate:** SR-03 / R-03 / RISK Coverage Gap 1
**ADR:** architecture/ADR-009-audit-shape-move-and-wave-staging.md
**Survey date:** 2026-06-08 · **Surveyor:** crt-052-agent-survey-adr009

## Purpose

Prerequisite gate before Wave B (C8) moves the `transcript_session_purged` audit
emission from the per-turn **session_close** purge point onto the
**review / stale_sweep / cap-eviction** cadence. The gate is clean only if **no
downstream consumer keys on the per-close timing or one-row-per-close count**. A
consumer that aggregates, counts, or correlates these rows expecting one-per-close
would silently break when the cadence moves.

## Method

Searched the Rust workspace for: the event string `transcript_session_purged`,
the `TranscriptPurgeRecord` type and all its producers, the `emit_purge_audits`
emitter and every call site + trigger, `gc_audit_log` (crt-036 GC machinery), every
production `audit_log` reader / `operation`-keyed query / aggregation, and every test
asserting per-close emission.

## 1. Emission sites (the producer side)

Single emitter: `emit_purge_audits(audit_log, records, trigger)` —
`crates/unimatrix-server/src/uds/listener.rs:2039`. It is content-free by
construction: `detail = "bytes={u64} trigger={static token}"`, `agent_id="server"`,
empty `target_ids`, `Outcome::Success`. Fire-and-forget; purge never depends on audit.

Call sites and their trigger token:

| Trigger | Call site | Cadence |
|---------|-----------|---------|
| `session_close` | `uds/listener.rs:2138` (after `drain_and_signal_session`) | **PER-CLOSE — this is what Wave B moves** |
| `stale_sweep` | `uds/listener.rs:2127` (after `sweep_stale_sessions`, UDS close path) | per stale sweep |
| `stale_sweep` | `services/status.rs:1593` (maintenance-tick stale sweep) | per maintenance tick |
| `cycle_review` | `server.rs:549` (`purge_cycle_transcripts`) | per cycle review |

`TranscriptPurgeRecord` producers (the rows that feed the emitter):
`session.rs:331` (`clear_transcripts_for_feature`), `session.rs:757`
(`drain_and_signal_session`), `session.rs:789` (`sweep_stale_sessions`),
`session.rs:900` (`purge_record_for`). Wave B adds held-store producers
(`sweep_expired`, `purge_held_for_feature`) per the brief; those route through the
SAME `emit_purge_audits` emitter with `stale_sweep` / `cycle_review` triggers, so the
event shape is unchanged (ADR-009 decision).

## 2. crt-036 / `gc_audit_log` machinery

`crates/unimatrix-store/src/retention.rs:271` — **`gc_audit_log` is a no-op since
vnc-014.** The `audit_log` table is append-only, protected by BEFORE-DELETE triggers
(schema v25, vnc-014 / ASS-050); the function logs a warn and returns `Ok(0)`,
deleting nothing. It takes only a `retention_days` argument, **does not filter or read
by `operation`**, and never inspects `transcript_session_purged` rows or their timing.
Called from `services/status.rs:1565` (cycle GC tick) purely for signature
preservation. Tests (`retention.rs:937`, `:1374`) assert the no-op.

**Verdict for this consumer:** does not read, count, or aggregate
`transcript_session_purged` at all — cadence-agnostic, in fact row-blind.

## 3. Retention / analytics / dashboard readers

Every production `audit_log` reader was inspected. None keys on
`transcript_session_purged`:

- `export.rs:620` (`export_audit_log`) — dumps **all** rows verbatim
  (`SELECT … FROM audit_log ORDER BY event_id`), no operation filter, no aggregation.
  Cadence-agnostic: serializes whatever rows exist whenever they exist.
- `server.rs:1968` — filters `operation = 'context_quarantine'` (test helper).
- `server.rs:2298` — `operation = 'context_quarantine'`.
- `import/mod.rs:1687` — `operation = 'import'`.
- `services/graph_enrichment_tick.rs:311` — `operation = 'context_search'`.
- `background.rs:2254` (`get_recent_audit_events`, test helper) — reads recent rows
  generically, then filters for `auto_quarantine` / `tick_skipped` (`:2314`, `:2369`).
- `audit.rs:81` — `COUNT(*) … operation IN ('context_store','context_correct')`.

No metric, dashboard, GROUP BY, or correlation query references
`transcript_session_purged`, the `session_close` trigger token, or the per-close
count. No retention reader correlates these rows by timing.

## 4. The only `transcript_session_purged` reader in the codebase

`server.rs:3320` — `SELECT session_id, detail FROM audit_log WHERE operation =
'transcript_session_purged' AND agent_id='server' AND detail LIKE '%trigger=cycle_review'`.
This is **test code**: it lives inside `#[cfg(test)] pub(crate) mod tests` (module
opens at `server.rs:1143`), is the helper `poll_cycle_review_purge_audits` used by
vnc-025 cycle-review-purge tests, and filters specifically on the **cycle_review**
trigger (not session_close). It is not a production consumer and is unaffected by the
session_close → review/sweep/evict move.

## 5. SQL/code counting or correlating rows by timing

None found. No production code counts `transcript_session_purged` rows, correlates
them by emission time, or assumes a one-row-per-close invariant. The audit is a
write-only forensic trail (append-only table); the only consumers are forensic export
(verbatim) and tests.

## 6. Per-close-emission tests Wave B must update

All per-close (`trigger=session_close`) assertions live in one file:
`crates/unimatrix-server/src/uds/listener/tests/purge_audit.rs`. When Wave B removes
the per-turn `session_close` emission for held buffers, these tests assert the OLD
cadence and must be updated to the new contract ("exactly once per held session at
review/sweep/eviction", AC-11):

| Test | Line | Asserts |
|------|------|---------|
| `test_session_close_purge_emits_audit_row` | `:92` | close path emits ONE row, `trigger=session_close`, pinned shape — **directly contradicts the move; primary update** |
| `test_purge_completes_when_audit_store_unavailable` | `:174` | calls `emit_purge_audits(…, "session_close")` directly (FR-14 failure independence) |
| `test_purge_never_blocks_on_audit_latency` | `:242` | asserts `"bytes=15 trigger=session_close"` lands; fire-and-forget timing |
| `test_purge_audit_row_sentinel_free` | `:282` | content-free assertion with `trigger=session_close` |
| `test_close_and_sweep_in_one_pass_emit_both_triggers` | `:309` | asserts a `trigger=session_close` row co-emits with a `stale_sweep` row |

Note for Wave B: vnc-025's `session_close` path still exists for sessions whose
buffers are NOT held (the close-time drain of a non-held, non-empty buffer still
purges immediately). The move applies to **held buffers** — their close no longer
purges; they purge at review/sweep/evict. So these tests are updated, not all deleted:
the ones exercising a non-held buffer's close-time purge stay valid; the held-buffer
lifecycle moves its audit to the AC-11 `continuity_simulated_lifecycle` test. Wave B
delivery must decide per-test whether the scenario is held (move) or non-held (keep),
but every one of the five above asserts `session_close` cadence and must be reviewed.

Cadence-agnostic tests in the same file that need NO change:
`test_sweep_purge_emits_audit_rows` (`:108`), `test_silently_evicted_session_gets_audit_row`
(`:129`), `test_empty_buffer_purge_emits_nothing` (`:150`), `test_sweep_burst_all_audits_land`
(`:215`) — all assert `stale_sweep` or zero-emit, unaffected by the session_close move.

## Conclusion

The `transcript_session_purged` audit is a write-only, content-free forensic trail on
an append-only table. The crt-036 `gc_audit_log` machinery is a no-op that never reads
the operation. No retention reader, analytics query, dashboard, metric, GROUP BY, or
correlation keys on the per-close cadence or the one-row-per-close count. The single
code reader of the event is a vnc-025 **test** helper filtering on `cycle_review` (not
session_close). The only artifacts that assume per-close cadence are the five
`session_close` tests in `purge_audit.rs`, which Wave B updates in lockstep with the
move (the survey is the guard, per ADR-009 Consequences).

VERDICT: CLEAN — no downstream consumer keys on per-close transcript_session_purged cadence; Wave B audit-shape move is safe.
