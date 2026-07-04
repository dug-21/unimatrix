# Component: Backstop reclaim (sole reclamation path) [UNCHANGED behavior + re-homed match]

Files: `transcript_hold.rs:308` (`sweep_expired`), `:353` (`enforce_cap`), registry session-close
(`sweep_stale_sessions` / `drain_and_signal_session`), audit via `uds/listener.rs`
(`emit_purge_audits`, `process_session_close`).

## Purpose

With the review-purge removed, the three backstops are the SOLE reclamation path (NG-2 — they stay
as-is). Confirm they carry the full load, host the re-homed exhaustive retention gate
(orphan-deletion.md), and keep emitting content-free terminal audits.

## The three backstops (UNCHANGED — NG-2)

```
sweep_expired(ttl)         # transcript_hold.rs:308 — 24h TTL stale-sweep; emits trigger=stale_sweep
enforce_cap(guard)         # transcript_hold.rs:353 — 64-session cap eviction; oldest-first; emits records
session-close reclaim      # registry sweep_stale_sessions + drain_and_signal_session;
                           #   uds/listener.rs process_session_close → emit_purge_audits(..., "session_close"/"stale_sweep")
```

None of these is modified by crt-057. They already reclaim independently of whether a review ran, so
leaning on them adds NO new memory risk (ADR-001 §b). They remain the ONLY writers of the content-free
`transcript_session_purged` audit (`bytes=<n> trigger=<token>`).

## Re-homed exhaustive retention gate (from orphan-deletion.md)

The exhaustive `TranscriptRetention` match relocated here as `reclaim_permitted_by_retention(r)` (no `_`
arm; `RetainDays` a no-op). It is consulted at the server-side DRIVE point of the backstop reclamation
(recommended: the background tick that calls `sweep_expired`):

```
# background reclamation tick (server-driven):
if reclaim_permitted_by_retention(&self.retention_config.transcript_retention):
    let records = self.transcript_hold.sweep_expired(ttl)
    emit_purge_audits(&self.audit, records, "stale_sweep")     # content-free, after-lock, fire-and-forget
# RetainDays(_) ⇒ skip (no-op) — unreachable in OSS (startup-rejected), preserves compile-gate
```

Under OSS this is always-true → behavior byte-unchanged (R-06). The gate exists so a future third
retention variant forces a compile error (C-5 / #4831).

## Content-free audit invariant (SR-02 / R-03 / AC-14)

- Every reclamation audit carries `session_id` + `bytes=<n> trigger=<token>` ONLY — never buffer/candidate
  bytes. This is UNCHANGED and now the SOLE audit point (the `trigger=cycle_review` emission is DELETED
  with `purge_cycle_transcripts`).
- Audit-trail readers must no longer assume "purge audit ⇒ a review occurred" (ADR-001 §Consequences) —
  a doc note, not a code change.

## Residency envelope (NFR-3 / R-15)

- Worst case: up to 64 held buffers × per-buffer cap-bytes, for up to the 24h TTL. Bounded, memory-only,
  human-ratified. A never-reviewed cycle IS still reclaimed by a backstop (proves the envelope cannot
  grow unbounded).
- Holding the cycle open across a multi-day merge does NOT extend buffer life (buffers governed solely by
  cap+TTL, independent of cycle open/closed). Aged degradation surfaces via loss propagation
  (distill-before-purge.md), never silently.

## Negative-assertion construction (R-10 / #4879)

"No purge on review" is proven by SYNCHRONOUS buffer-present reads after the review, never by counting
async purge-audit rows. The POSITIVE backstop-reclamation event (a real purge fired) MAY poll for its
audit's appearance — the asymmetry is explicit.

## Key test scenarios

- Each backstop reclaims a NEVER-reviewed cycle's buffer + emits a content-free audit (24h TTL / 64-cap /
  session-close) — buffer gone AND audit byte-free (R-06 sc.2, AC-13).
- Re-homed exhaustive gate: `PurgeOnCycleClose→reclaim`, `RetainDays→no-op`; match exhaustive (R-06 sc.3).
- Aged-buffer degradation is VISIBLE: age/evict a subset, retrieve → `Reconstructed`/empty WITH per-session
  loss, never silently absent, never a crash (R-15).
- Post-reclamation retrieval → empty/`Reconstructed`, no panic, no stale verbatim (R-16).
- Content-scan on the reclamation-without-review path (R-03 sc.2): no buffer/candidate bytes in any
  SQL/file/log sink.
