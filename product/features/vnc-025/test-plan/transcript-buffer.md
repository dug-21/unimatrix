# Test Plan: transcript-buffer (`infra/session_transcript.rs`)

Covers R-01 (Critical), R-02, R-03, R-05 (Debug arm), R-15; AC-02, AC-07, AC-12 (module arm),
NFR-09. Densest test surface in the feature — treat `apply_delta` as a pure state machine.
All tests are plain `#[test]` (no async; the buffer is sync).

Shared harness: one fixture module that (a) generates delta sets from a source byte string as
`(offset, slice)` pairs, (b) derives expected content programmatically from the covered-range
set minus elision — never hand-copied between scenarios (#2984), (c) takes the cap as a
parameter. Reuse ass-069 PoC fixtures where they exist.

## §1 Merge correctness — R-01, AC-02 below-cap arm

- `test_apply_delta_permutation_convergence_below_cap` — fixed covering delta set applied in
  N shuffled orders (include duplicates + partial overlaps in the set); assert: identical full
  content (via `contiguous_tail(len)`), identical `high_water`, identical `len()` across all
  orders. Cap high enough that no overflow occurs.
- Hole-surgery cases — for each, assert resulting content AND hole bookkeeping via behavior
  (a write into a hole becomes readable; `contiguous_tail` boundary moves correctly):
  - `test_apply_delta_fills_hole_exactly`
  - `test_apply_delta_shrinks_hole_from_start` / `_from_end`
  - `test_apply_delta_splits_hole_in_two`
  - `test_apply_delta_spans_multiple_holes`
- `test_apply_delta_below_base_after_ring_tail_is_noop` — clipped bytes counted in
  `elided_bytes()`, content unchanged, `high_water` still updated (FR-02 defined no-op).
- `test_apply_delta_beyond_span_creates_hole_tail_never_crosses` — delta at
  span_end + gap; `contiguous_tail(window)` returns only post-hole bytes; assert no zero-fill
  byte appears in any returned tail (FR-19).
- Edge cases (from strategy Edge Cases list):
  - `test_apply_delta_zero_length_bytes_noop_high_water_defined` — pin len-0 semantics.
  - `test_apply_delta_offset_zero_empty_buffer_then_exact_duplicate`
  - `test_apply_delta_invalid_utf8_bytes_accepted` — API is `&[u8]`; crt-052 reads raw bytes.

## §2 Arithmetic soundness — R-02, NFR-09

- `test_apply_delta_near_u64_max_drops_whole` — `apply_delta(u64::MAX - 10, &[0u8; 100])`:
  no panic, NO state change at all — `len()`, `high_water()`, `elided_bytes()` all unchanged
  (ADR-008 drop-whole; do NOT assert partial clip).
- `test_apply_delta_far_offset_jump_allocation_bounded` — offset `1 << 40`, small payload:
  `len() <= max_bytes`, base advances, prior content counted as elided.
- `test_apply_delta_one_mib_into_4mib_cap` and `test_apply_delta_one_mib_into_64kib_cap` —
  frame-ceiling delta (FR-05) merges without panic; small-cap case ring-tails correctly.
- `test_apply_delta_fuzz_no_panic` — randomized (offset: full u64 range incl. near-MAX band,
  len: 0..=1 MiB) for ≥10k iterations against caps {64 KiB, 4 MiB}; assert only: no panic,
  `len() <= max_bytes`, `high_water` monotonic. This is the named NFR-09 verification.
- Review gate (not a test): every u64→usize conversion site carries an invariant comment
  proving span-relative ≤ max_bytes; no raw `offset as usize` (grep gate, see §5).

## §3 Overflow / ring-tail — R-03, AC-07

- `test_overflow_reorder_tail_window_equivalence` — cap-crossing delta sequence in multiple
  arrival orders: final `contiguous_tail(window)` byte-identical across orders; full content
  explicitly NOT asserted (Variance 1 — do not strengthen).
- `test_overflow_size_never_exceeds_cap` — drive 3× cap; `len() <= max_bytes` after every
  apply; tail content equals programmatically-derived newest bytes.
- `test_overflow_no_marker_bytes_in_content` — elision is metadata only: returned tail
  contains only source-string bytes (AC-07; ADR-002).
- `test_high_water_monotonic_across_overflow` — including when the clipping delta itself
  carries the new maximum.
- `test_elided_bytes_accounting_exact` — hand-computable fixture: elided = clipped-below-base
  + ring-dropped; assert no double-count when a hole drops below base.
- `test_cap_exactly_equal_to_delta_size` and off-by-one at the cap boundary.
- `test_contiguous_tail_window_larger_than_len`, `_window_zero`, `_window_on_hole_boundary`.

## §4 Hole-metadata bound — R-15

- `test_hole_collapse_at_cap` — drive 64 disjoint holes, apply the 65th: collapse-to-newest
  contiguous segment, abandoned span counted in `elided_bytes`, no panic, hole count bounded.
- `test_post_collapse_merge_and_tail_correct` — merges after collapse land correctly;
  `contiguous_tail` serves the newest segment.
- `test_pathological_sparse_stream_bounded` — alternating far offsets for ~10k deltas
  completes in sane wall time (coarse bound, e.g. < a few seconds; the bounded-metadata
  property is the assertion, the 64 constant is tunable).

## §5 Content opacity — R-05.1, AC-12 module arm

- `test_debug_output_contains_no_payload_bytes` — populate with sentinel; `format!("{:?}")`
  contains metadata field names/values and NOT the sentinel.
- `test_clear_returns_bytes_purged` — `clear()` returns prior `len()` as u64; post-clear
  `len() == 0`; pin `high_water`/`elided_bytes` post-clear semantics by assertion (crt-052
  inherits — see cycle-review-purge.md §3 for the registry-level pinning).
- Static gate (review, Stage 3c): no `Display` impl; no `tracing` call in this module touches
  content-bearing values; no content-bearing type in the public API; no `lock()` here (the
  buffer itself holds no mutex — locking is the registry's job).
- `session_key()` — `test_session_key_oss_returns_session_id_unchanged` (ADR-007; exact
  string equality, all three args exercised).
