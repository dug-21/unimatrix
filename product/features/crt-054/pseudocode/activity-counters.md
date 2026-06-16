# Component 1 — `ActivityCounters` (fold accumulator)

**File**: `crates/unimatrix-server/src/infra/transcript_activity.rs` (new)
**Embedded in**: `TranscriptBuffer` (`session_transcript.rs`) as a private field.
**ADRs**: ADR-001 (fold in buffer, both routes), ADR-003 (Copy/cast-free widths), ADR-005 (content-opaque), ADR-006 (survives to review).

## Purpose

The in-memory running fold over transcript deltas: monotonic byte sum, delta count, and per-class match counts. Scalars only — structurally incapable of holding transcript bytes. One instance lives inside each `TranscriptBuffer`; both the registered and held delta routes fold into it by construction (the accumulator is owned by the buffer, not the route).

## Type

```
const MAX_SIGNAL_CLASSES: usize = 16    // PINNED, exactly 16 (NFR-6, AC-11); == crt-055's

#[derive(Debug, Clone, Copy, PartialEq, Eq)]   // Debug is metadata-only by virtue of all-scalar fields
struct ActivityCounters {
    bytes_total:  u64                    // u64, cast-free (NFR-5)
    delta_count:  u32                    // u32, cast-free
    class_counts: [u32; MAX_SIGNAL_CLASSES]
}
```

`derive(Debug)` is safe here because every field is a scalar/array-of-scalar — no byte-bearing field exists, so AC-08's content-opacity holds by construction. (The *public* `ActivitySnapshot` Debug is hand-written for parity with `TranscriptSnapshot`; see Component 4.)

## Functions

```
impl ActivityCounters

    fn new() -> Self
        return ActivityCounters {
            bytes_total:  0,
            delta_count:  0,
            class_counts: [0; MAX_SIGNAL_CLASSES],
        }

    // The fold. Called from apply_delta AFTER the merge (Component 3), under the
    // buffer lock already held — introduces NO new lock (FR-B6, NFR-3).
    // O(bytes), allocation-free, single scan per delta.
    fn fold(&mut self, bytes: &[u8], scanner: &SignatureScanner)
        // 1. throughput — saturating add so a pathological stream cannot panic in
        //    debug builds; native u64/u32 widths, no cast toward i64 (NFR-5, AC-14).
        self.bytes_total = self.bytes_total.saturating_add(bytes.len() as u64)
        self.delta_count = self.delta_count.saturating_add(1)
        // 2. behavioral signatures — ONE shared scan (Component 2), not one pass per pattern (FR-B5, AC-09).
        //    A delta may match multiple classes; each matched class increments once per delta.
        for class_index in scanner.scan(bytes)        // yields distinct matched indices, each < MAX_SIGNAL_CLASSES
            self.class_counts[class_index] = self.class_counts[class_index].saturating_add(1)

    // Read surface — produce the Copy snapshot (Component 4 calls this).
    fn snapshot(&self) -> ActivitySnapshot
        return ActivitySnapshot {
            bytes_total:  self.bytes_total,
            delta_count:  self.delta_count,
            class_counts: self.class_counts,        // [u32; N] is Copy
        }
```

Notes:
- `bytes.len() as u64` is a *widening* cast (usize→u64), not the forbidden narrowing toward i64 — AC-14 forbids `as i64`/`as i32` on the producer counters, not widening of the length. State this in the code comment so the AC-14 grep reviewer is not confused.
- `class_index` is guaranteed `< MAX_SIGNAL_CLASSES` because the scanner is built from an `enabled`-count-bounded config (Component 9 `validate()` enforces ≤ `MAX_SIGNAL_CLASSES`). Index into the fixed array is therefore always in bounds; no runtime bound check failure path.

## State / lifecycle

- Created once per `TranscriptBuffer` (in `TranscriptBuffer::new`, Component 3).
- Monotonic for the buffer's lifetime: `fold` only ever adds.
- crt-054 provides **no** `clear()`/`reset()` and never zeroes it (ADR-006, FR-B9, AC-07). The buffer's existing `clear()` (`session_transcript.rs:318`, used for stream-resume) MUST NOT touch the accumulator — the accumulator survives a buffer `clear()`. The only zeroing is the crt-052/crt-055-owned purge that drops the whole buffer after the review read.
- Rides the crt-052 Wave B hold across drains (it is part of the buffer that is held).

## Error handling

None — integer arithmetic under the buffer lock; cannot fail. A poisoned buffer mutex is handled one layer up (Component 4 `activity_snapshot()` degrades to empty, #4764), not here.

## Key test scenarios (hints — full plan in test-plan/)

- `new()` yields all-zero.
- `fold(b)` advances `bytes_total += b.len()`, `delta_count += 1`.
- Empty/zero-length delta: `delta_count += 1`, `bytes_total += 0`, no panic, no spurious class match (edge case).
- A delta matching both `error` and `refusal` bumps `class_counts[0]` and `class_counts[1]` from a single `fold` call (AC-09).
- No `as i64`/`as i32` narrowing cast on `bytes_total`/`delta_count`/`class_counts` anywhere in this file (AC-14 grep).
- `snapshot()` round-trips a near-`u64::MAX` `bytes_total` un-narrowed (AC-14 value-level).
- Structural: no `Vec<u8>`/`String`/`&[u8]` field on `ActivityCounters` (AC-08 spirit).
