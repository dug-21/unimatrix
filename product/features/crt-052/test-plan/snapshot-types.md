# Test Plan — C2 Snapshot Types & Primitive

**Component**: `TranscriptSnapshot` / `HoleInfo` + `snapshot()` on `TranscriptBuffer`
(`infra/session_transcript.rs`). **ADRs**: ADR-002. **Wave**: A.
**Tests live in**: `infra/session_transcript_tests.rs` / `_overflow.rs` (extend the existing harness —
`apply_all`, `covered_union`, `src_bytes`; honor the 500-line split). **Merge gates**: AC-V-SEAM,
content-leak (Debug), AC-01 (byte-copy-only under lock).

## Unit Test Expectations

### snapshot() primitive correctness
- `test_snapshot_returns_contiguous_span_no_holes_crossed` — apply a covering delta set with no
  overflow; assert `snapshot().bytes == contiguous_tail(data_len)` content AND that the returned span
  never crosses a hole / never contains zero-fill (reuse the FR-19 invariant: no zero byte from
  `src_bytes`, which never emits 0).
- `test_snapshot_metadata_matches_buffer_state` — assert `base_offset`, `high_water`, `elided_bytes`,
  and `holes` (mapped to `Vec<HoleInfo>`) equal the buffer's internal counters after a known delta
  sequence. Programmatic expectation from `covered_union`, never hand-copied.
- `test_snapshot_returns_whole_span_not_windowed` — distinguish from `contiguous_tail(window)`:
  `snapshot()` returns the WHOLE snapshotted readable span plus metadata, not a clipped tail window
  (ADR-002 — both consumers need the full span). Assert `bytes.len()` equals the contiguous span
  length, not a passed window.
- `test_snapshot_empty_buffer` — fresh buffer → `bytes` empty, `base_offset==0`, `high_water==0`,
  `elided_bytes==0`, `holes` empty.

### Overflow / ring-tail (extend `_overflow.rs`) — R-12, R-09 boundary
- `test_snapshot_base_offset_advances_under_overflow` — cap-crossing sequence (e.g. CAP=256, 3.9x
  coverage like the existing reorder test); assert `base_offset > 0` and `elided_bytes > 0` after
  ring-tail engages. (Variance 1: tail-window equivalence only — do NOT assert full content.)
- `test_snapshot_high_water_survives_clipping` — assert `high_water == max(offset+len)` ever sent,
  unchanged by clipping (monotone, tracks sent-not-retained).
- `test_snapshot_holes_reported` — induce holes (non-contiguous deltas); assert `holes: Vec<HoleInfo>`
  are disjoint, sorted ascending, strictly inside the span, `len() <= MAX_HOLE_RANGES`.
- `test_snapshot_at_exactly_cap_boundary` — buffer at exactly the cap, then one byte over: assert the
  ring-tail-just-engaged transition surfaces `base_offset` advance / `elided_bytes>0` (the SR-08
  calibration edge the fallback trigger reads).

### Poison recovery (R-16)
- `test_snapshot_poisoned_lock_treats_as_empty` — poison the buffer mutex, call the seam's snapshot
  path; assert treat-as-empty recovery (`unwrap_or_else` into_inner) + `clear_poison` per #4764, and
  the snapshot surfaces as empty/lossy (so the loss is visible downstream, not silently absent).
  (Co-asserted with seam/loss-section in snapshot-seam.md + response-types.md.)

## Merge-Gate Tests

### AC-V-SEAM — single content reader (R-06)
- `test_only_two_production_buffer_content_readers` — source/grep assertion: the only production call
  sites extracting buffer content are PreCompact `contiguous_tail` (`listener.rs:1834-1838`) and
  `snapshot()` via the seam. Enumerate via grep over non-test code; fail if a third content-reading
  accessor call site appears. (All other `contiguous_tail` callers are tests.)
- `test_700_reuse_parses_snapshot_bytes_without_contiguous_tail` — a #700-shaped marker-recovery
  caller runs its OWN patterns over `TranscriptSnapshot.bytes` and `base_offset`, asserting it needs
  NO `contiguous_tail` call and no other buffer accessor. Proves the seam is reusable for #700.
- `test_snapshot_exposes_all_four_metadata_fields` — assert `base_offset`, `high_water`,
  `elided_bytes`, `holes` are all public and populated, so #700 needs zero byte re-read.

### Content-leak — metadata-only Debug (R-19, AC-06(e))
- `test_snapshot_debug_metadata_only` — `format!("{:?}", snapshot)` contains `len`, `base_offset`,
  `high_water`, `holes:<n>`, `elided_bytes` and does NOT contain any byte of `bytes` (assert no
  src_bytes content substring present). Manual Debug impl, NOT `derive`.
- `test_holeinfo_debug_safe` — `HoleInfo` Debug is `{start,end}` only (no content).
- Grep gate (in distill-handler.md content-leak gate, applied here): no `#[derive(Debug)]` on
  `TranscriptSnapshot` or any content-bearing snapshot type.

### AC-01 — byte-copy only under lock
- `test_snapshot_does_no_parse` — source assertion: `snapshot()` body performs only a byte copy +
  metadata read; no JSONL parse, no marker match, no I/O, no tracing of content. (Co-gated with the
  seam's no-parse-under-lock assertion in snapshot-seam.md.)

### AC-12 — throughput
- `test_snapshot_4mib_copy_fast` (or bench) — `snapshot()` byte copy over a 4 MiB buffer completes
  within the latency class; the downstream rule pass (selection-module) carries the <50ms off-lock
  assertion. Co-located note: the copy is bounded by the 4 MiB cap.

## Edge Cases (from Risk Strategy)
- Buffer at exactly 4 MiB cap; one byte over (R-09/R-12 boundary).
- Truncated final-line boundary is a parser concern (selection-module), but the snapshot must return
  the truncated tail bytes intact (no zero-fill, no panic) — `test_snapshot_returns_truncated_tail_bytes`.

## Assertions Summary (concrete)
- `snapshot().bytes` never contains a zero byte unless content legitimately had one (FR-19 leak guard).
- `byte_offset` of any downstream candidate is `base_offset + in_snapshot_offset` — the metadata that
  makes this logical is owned here, asserted in selection-module.md (R-12).
- Debug output asserted by substring-absence of known content, not by shape inspection alone.
