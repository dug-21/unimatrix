# Component: `snapshot()` reuse [UNCHANGED]

File: `unimatrix-server/src/infra/session_transcript.rs:296` (`TranscriptBuffer::snapshot(&self)`);
seam `session.rs:502` (`take_transcripts_for_feature`).

## Purpose

Confirm scoped retrieval introduces NO new buffer-content reader (CON-3/#4848). The single content
reader `snapshot()` is reused exactly as-is; the scoped filter layers ON TOP of the candidate pipeline,
not on a new read path.

## Interface (UNCHANGED — do not modify)

```
fn TranscriptBuffer::snapshot(&self) -> TranscriptSnapshot      # &self, non-mutating (session_transcript.rs:296)
    # returns { bytes, base_offset, high_water, elided_bytes, holes }

fn SessionRegistry::take_transcripts_for_feature(&self, feature_cycle) -> Vec<(String, TranscriptSnapshot)>
    # session.rs:502 — Phase-1 collect matching sessions, Phase-2 per-buffer lock → buf.snapshot() (read only)
    # NAME is legacy ("take") but it does NOT clear/purge — it snapshots (session.rs:483,3483 assert this)
```

## Why this satisfies the invariants

- `snapshot()` is `&self` and copies bytes + metadata; it does NOT clear the buffer. So a retrieval
  leaves the buffer intact (FR-11) — the non-destructive property is inherited from the existing reader.
- `take_transcripts_for_feature` scans by the `feature_cycle` string, independent of cycle open/closed
  state — this is what lets a POST-CLOSE retro retrieve (ADR-005 / retro-lifecycle.md).
- No second reader is added: `retrieve_scoped_candidates` filters the candidates that
  `select_candidates` / `reconstruct_from_observations` already derive from this one snapshot.

## Delivery note (naming, non-blocking)

`take_transcripts_for_feature` is a legacy name that reads as destructive but is snapshot-only. Renaming
it is OUT OF SCOPE for crt-057 (it is not in the Component Map and would widen the blast radius); leave
it. Flagged only so reviewers do not mistake it for a purge.

## Key test scenarios

- Buffer content unchanged before/after a `transcript:{}` retrieval (synchronous read — R-10) — this is
  the load-bearing "buffer intact" proof (AC-03).
- No new reader symbol introduced (grep guard: only `snapshot()` reads content; the fold uses the
  separate content-opaque `activity_snapshot()` — activity-fold.md).
- Post-close retrieval still returns candidates (seam scans by `feature_cycle`, lifecycle-independent) —
  ties AC-17 / R-08.
