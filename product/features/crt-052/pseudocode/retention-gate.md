# C7 — TranscriptRetention Gate

**Target source:** `unimatrix-server/src/server.rs` — exhaustive `match` in `purge_cycle_transcripts`
(`:541`/`:543`/`:551`)
**Wave:** A — **NO reference to `transcript_hold.rs`.**
**ADRs:** ADR-005 (exhaustive gate). **Risks:** R-18. **AC:** AC-10. **Constraint:** C2 (enterprise seam).
**Sequencing:** server-side; before/with C6.

## Purpose

Gate both distillation AND purge on an EXHAUSTIVE `TranscriptRetention` match (no wildcard arm). The
match IS the enterprise seam (vnc-024 ADR-005 #4721, AC-10). `PurgeOnCycleClose` is the only
OSS-honored arm; `RetainDays(_)` is unreachable (rejected at `validate()`) and must neither distill nor
purge.

## The Gate Predicate (used by C6 step 1, ADR-005)

C6's `distill_before_purge` opens with the same exhaustive match before doing any work:

```
match cfg.transcript_retention {
    PurgeOnCycleClose => proceed with distill + (caller) purge,   // server.rs:543 arm
    RetainDays(_)     => return None,                              // server.rs:551 arm — no distill, no purge
}
// NO wildcard `_ =>` arm. Adding a third TranscriptRetention variant must force a compile error here
// (and at the purge site) so the enterprise seam stays explicit (AC-10).
```

`RetainDays` is structurally dead in OSS (rejected at `validate()` — C9) but the arm is written
explicitly. If a `RetainDays` config somehow reaches here (test bypassing validate), the helper returns
`None`: no candidates produced, no purge performed.

## Purge-site match (existing `purge_cycle_transcripts`, extend in lockstep)

The existing `purge_cycle_transcripts` already matches `TranscriptRetention` at `server.rs:543/:551`.
crt-052 keeps this match exhaustive and ensures the distill gate (C6) and the purge gate use the SAME
variant semantics, so distill and purge stay in lockstep at every one of the four call sites (ADR-005):

```
fn purge_cycle_transcripts(...):
    match cfg.transcript_retention {
        PurgeOnCycleClose => {
            // existing: clear_transcripts_for_feature(feature_cycle)   (registered buffers)
            // Wave B addition: purge_held_for_feature(feature_cycle)   (held buffers, C8) — emits audit
        }
        RetainDays(_) => { /* no purge — OSS-unreachable, validate-rejected */ }
    }
```

## Ordering invariant (AC-05)

At each of the four success returns (C6/tools.rs): **distill strictly BEFORE purge.** C6 runs
distillation (which snapshots via C1), attaches the section at assembly level (C4/ADR-004), THEN
`purge_cycle_transcripts` fires. Error paths keep transcripts and produce no candidates (existing
behavior, untouched).

## Wave B note

The `purge_held_for_feature` call inside the `PurgeOnCycleClose` arm is the ONLY Wave B addition in this
component. With Wave B reverted, that line is removed and the arm purges registered buffers only — Wave
A still gated and correct. The match exhaustiveness itself is Wave A.

## Data Flow

- **Input:** `cfg.transcript_retention` (from C9 `RetentionConfig`).
- **Output:** a proceed/skip decision (boolean-equivalent via the match) governing C6 and the purge.

## Error Handling

No panic. `RetainDays` → skip (return `None` in C6 / no purge). The match has no wildcard, so a new
enum variant is a compile error (the desired enterprise-seam guard, AC-10).

## Key Test Scenarios

- AC-10 compile-level: the `TranscriptRetention` match has no wildcard arm (exhaustive) at both the
  distill gate (C6) and the purge site.
- AC-10: `RetainDays` config rejected at `validate()` (C9) — OSS unreachable.
- R-18: construct `RetainDays` in a test bypassing `validate()` → assert the helper returns `None` (no
  distill, no purge).
- AC-05: at each of the four success returns, distill runs before purge; error paths retain transcripts.
