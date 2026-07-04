# Test Plan — `snapshot()` reuse `[UNCHANGED]` — single content reader

**File:** `unimatrix-server/src/infra/session_transcript.rs:296` (`TranscriptBuffer::snapshot(&self)`)
**Risks:** R-16 (Med), R-03 (single-reader invariant) · **ACs:** AC-03, AC-05 (support)

> Scoped retrieval reuses the EXISTING `snapshot()` (already `&self`, non-mutating). **No new buffer reader
> is introduced** (crt-052 ADR-002, #4848 — CON-4). The scope block is a filter layer, not a new content
> path.

---

## Single-content-reader invariant (CON-4)
- `test_no_new_buffer_content_reader_introduced` — source/reachability assertion that scoped retrieval flows
  through the single existing `snapshot()` path; no second content reader of the buffer is added (#4848).
  (Grep the crate for buffer-content read sites; assert the set is unchanged.)

## R-16 — second retrieval / buffer survival (AC-03, AC-05)
- `test_second_retrieval_same_cycle_returns_same_candidates` — the buffer survives the first (non-destructive)
  retrieval; a second returns the SAME candidates. After a backstop reclaims → empty/`Reconstructed`, no
  panic. (R-16 sc.1.)
- `test_snapshot_is_non_mutating` — `snapshot(&self)` leaves the buffer content unchanged (synchronous
  before/after read). The read side of the "buffer intact after review" guarantee (R-10).

## Notes
- Degraded/aged retrieval (TTL/cap/partial) and the no-stale-verbatim assertion live in `backstop-reclaim.md`
  (R-16 sc.2–4). This file proves the reader is REUSED and NON-MUTATING; that file proves behavior once the
  source is gone.
