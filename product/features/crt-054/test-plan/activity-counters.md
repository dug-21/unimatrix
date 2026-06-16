# Test Plan — `ActivityCounters` (fold accumulator embedded in `TranscriptBuffer`)

**Component**: `ActivityCounters { bytes_total: u64, delta_count: u32, class_counts: [u32; MAX_SIGNAL_CLASSES] }` — pure fold arithmetic. `Copy`/metadata-only, embedded as a field in `TranscriptBuffer`.
**Pseudocode**: `pseudocode/activity-counters.md` · **Layer**: unit.
**Anchor ACs**: AC-05 (accumulation rules), AC-08 (Copy/no-content), AC-09 (multi-class). **Risks**: R-07 (width), R-12 (no latch).

## Unit Test Expectations

`crates/unimatrix-server/src/infra/transcript_activity_tests.rs` (sibling to the new module).

1. `test_fold_bytes_total_sums_delta_lengths` — Arrange: fresh `ActivityCounters::default()`. Act: `fold(b"abc", &scanner)` then `fold(b"de", &scanner)`. Assert: `bytes_total == 5`, `delta_count == 2`. (FR-B3, AC-05.)
2. `test_fold_delta_count_increments_per_call` — N folds → `delta_count == N` exactly; one increment per delta, never per byte or per match.
3. `test_fold_empty_delta_counts_no_bytes` — Act: `fold(b"", &scanner)`. Assert: `delta_count += 1`, `bytes_total` unchanged, no class match, no panic. (Edge case: empty/zero-length delta.)
4. `test_fold_is_allocation_free` — fold runs under the buffer lock with no heap allocation (NFR-3): a structural review note + (if a counting allocator harness exists) assert zero allocations across a fold; otherwise document as a code-review assertion. Lower-bound: no `Vec`/`String` constructed in `fold`.
5. `test_counters_are_copy_and_scalar_only` — compile-time: `ActivityCounters: Copy`; struct holds only `u64`/`u32`/`[u32; _]`; no `Vec<u8>`/`String`/`&[u8]` field. (Content-opacity by construction.)
6. `test_no_saw_compaction_or_reload_latch_field` — structural/grep: `ActivityCounters` has NO `saw_compaction`/`reload_after_compaction`/latch field (R-12 stale-residue guard). Removed-scope residue must not reappear.

## Boundary / width (R-07 → AC-14, paired with activity-snapshot.md)

7. `test_bytes_total_accepts_large_values_un_narrowed` — Arrange counters with `bytes_total` near `u64::MAX` (set directly or via large simulated deltas). Assert the value is held at full `u64` width with no `as`/narrowing inside `fold` or the accumulator. The producer side never casts toward `i64` (that conversion is crt-055's at persist).

## Negative / mutation

- A fold that incremented `delta_count` by byte length (instead of by 1) must fail `test_fold_delta_count_increments_per_call`.
- Removing the `bytes_total += len` line must fail `test_fold_bytes_total_sums_delta_lengths`.

## Notes for Stage 3b/3c
- Multi-class scan behavior is verified in `transcript-activity.md` (it owns the `SignatureScanner`); here `fold` is tested with a stub/real scanner only for arithmetic.
- The poison→empty path is a `TranscriptBuffer`/`activity_snapshot()` concern (activity-snapshot.md), not the counters themselves.
