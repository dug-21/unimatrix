# Component: Orphan deletion + exhaustive-match re-home

Files: `unimatrix-server/src/server.rs:661` (`purge_cycle_transcripts` + its retention match),
`session.rs:445` (`clear_transcripts_for_feature`), `transcript_hold.rs:331` (`purge_held_for_feature`).

## Purpose

Once the four review-site purge calls are removed (cycle-review-handler.md), these three functions lose
ALL non-test callers. ANTI-STUB (CLAUDE.md rule 2 / clippy dead-code): DELETE them — do NOT `#[allow]`.
Re-home the exhaustive `TranscriptRetention` match that lived inside `purge_cycle_transcripts` so the
C-5 compile-gate obligation survives the deletion.

## What to delete (verified sole callers = the four review sites)

```
DELETE server.rs:661        fn purge_cycle_transcripts(&self, feature_cycle)   # + its exhaustive match body
DELETE session.rs:445       fn clear_transcripts_for_feature(&self, feature_cycle) -> Vec<TranscriptPurgeRecord>
DELETE transcript_hold.rs:331 fn purge_held_for_feature(&self, feature_cycle) -> Vec<TranscriptPurgeRecord>
```

Pre-deletion check (delivery, #5383 reachability): grep each name; confirm the ONLY remaining references
after cycle-review-handler.md are in tests. Delete/adjust those tests (they asserted purge behavior that
no longer exists). Do NOT leave a `#[allow(dead_code)]` — the gate is that clippy is clean with the
functions gone.

`clear_transcripts_for_feature` and `purge_held_for_feature` are the registered-buffer and held-buffer
purge primitives respectively; both were called ONLY from inside `purge_cycle_transcripts`. `snapshot()`
/ `take_transcripts_for_feature` are the READ seams and STAY (snapshot-reuse.md) — do not confuse them.

## Re-home the exhaustive `TranscriptRetention` match (C-5 / R-06 / #4831)

The deleted `purge_cycle_transcripts` held the exhaustive gate:

```
# DELETED from server.rs:662 —
match self.retention_config.transcript_retention:
    PurgeOnCycleClose => { clear + purge_held + emit audits }
    RetainDays(_)     => { /* no-op */ }
```

The C-5 obligation ("surviving reclaim paths honor retention exhaustively; `RetainDays` a no-op; no `_`
arm") relocates onto the SOLE surviving reclamation driver — the backstops. Introduce ONE named,
exhaustive gate and consult it where the server drives backstop reclamation (backstop-reclaim.md):

```
# server.rs (or a small retention module) — the re-homed exhaustive gate, no `_` arm:
fn reclaim_permitted_by_retention(r: &TranscriptRetention) -> bool:
    match r:
        PurgeOnCycleClose => true      # OSS default — reclaim as today
        RetainDays(_)     => false     # enterprise-only; OSS rejects at startup (#4721); a no-op reclaim
```

Placement (drive point): the background-tick `sweep_expired` invocation and any server-driven
cap-eviction / session-close reclamation consult `reclaim_permitted_by_retention(&self.retention_config
.transcript_retention)` before reclaiming. Under OSS (`RetainDays` startup-rejected) the gate is always
`true`, so runtime behavior is BYTE-UNCHANGED (NG-2 / R-06 honored); the value is the preserved
compile-gate — a future third `TranscriptRetention` variant forces a compile error at this match.

**FLAG (design fork, non-blocking) for delivery/architect:** whether the re-homed gate actually gates
the TTL/cap backstops (which would change behavior only under a hypothetical `RetainDays` — currently
unreachable in OSS) vs. is a pure compile-gate at a decision point. Recommended: gate the background
`sweep_expired` driver (semantically "RetainDays does not reclaim") since OSS behavior is identical
either way and this keeps the exhaustive match on a live reclaim path. The existing test at
`server.rs:3747-3777` (`gate_decision`) already encodes this exact match shape and can be repointed at
`reclaim_permitted_by_retention`.

## Error handling

- Deleting the functions is a compile-time change; the guard is `cargo clippy -- -D warnings` clean
  (no dead code, no `#[allow]`).
- The re-homed match MUST NOT introduce a `_` arm; adding one is a C-5 violation.

## Key test scenarios

- Dead-code guard: `purge_cycle_transcripts`, `clear_transcripts_for_feature`, `purge_held_for_feature`
  no longer exist; no non-test caller remains (#5383). Removal, not `#[allow]` (AC-13, R-06 sc.1).
- Exhaustive-match re-homed: supply each `TranscriptRetention` variant to the reclaim gate; assert
  `PurgeOnCycleClose→reclaim`, `RetainDays→no-op`; assert the match is exhaustive (a new variant would
  fail to compile — #4831 discipline) (R-06 sc.3).
- No new purge trigger at review or at `context_cycle(stop)` (R-06 sc.4 / R-08).
- Grep for hidden dependents on the removed strings (R-11 / #4044).
