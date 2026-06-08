# C2 — Snapshot Types & `snapshot()` Primitive

**Target source:** `unimatrix-server/src/infra/session_transcript.rs` (additive to vnc-025 `TranscriptBuffer`)
**Wave:** A — **NO reference to `transcript_hold.rs`.**
**ADRs:** ADR-002 (shape, single reader). **Risks:** R-06, R-12, R-16, R-19. **AC:** AC-01, AC-V-SEAM.
**Sequencing:** built FIRST — all other components depend on `TranscriptSnapshot`.

## Purpose

Define the owned snapshot return type co-designed for crt-052 selection AND #700 marker parsing, and
the single content-extraction primitive `snapshot()` on `TranscriptBuffer`. This is the **second and
last** production buffer-content reader (Constraint 4); PreCompact `contiguous_tail`
(`listener.rs:1834-1838`) is the first. No third reader.

## New Types (ARCH §4 — binding)

```
struct HoleInfo {
    start: u64    // logical stream offset of hole start
    end:   u64    // logical stream offset of hole end (exclusive)
}

struct TranscriptSnapshot {
    bytes:        Vec<u8>          // owned copy of the contiguous readable span; never crosses a hole; no zero-fill
    base_offset:  u64             // logical stream offset of bytes[0] (R-12: makes byte_offset logical)
    high_water:   u64             // max(offset+len) ever seen; survives clipping
    elided_bytes: u64             // lifetime count of content dropped from the span (ring-tail overflow)
    holes:        Vec<HoleInfo>   // remaining unwritten sub-ranges within the span
}
```

### `Debug` — manual, metadata-only (R-19, AC-06, SR-02)

DO NOT `derive(Debug)`. Hand-write `impl Debug for TranscriptSnapshot`:

```
fn fmt(self, f):
    f.debug_struct("TranscriptSnapshot")
       .field("len", &self.bytes.len())          // length only, NEVER the bytes
       .field("base_offset", &self.base_offset)
       .field("high_water", &self.high_water)
       .field("elided_bytes", &self.elided_bytes)
       .field("holes", &self.holes.len())         // count only
       .finish()
```

Same discipline for any Wave-B type wrapping content (`HeldBuffer` in C8). A grep/source gate
(AC-06 leak gate) asserts no `derive(Debug)` on content-bearing snapshot types and that the manual
impl prints no byte content. `HoleInfo` may `derive(Debug)` — it carries only offsets.

## New Method: `snapshot()` on `TranscriptBuffer`

Signature (ARCH §4): `fn snapshot(&self) -> TranscriptSnapshot`

Mirrors `contiguous_tail` extraction semantics (contiguous readable span, never crosses a hole, never
zero-fills) but returns the WHOLE snapshotted span plus all four metadata fields rather than a windowed
tail. Called by C1 under the buffer lock. Reads existing vnc-025 buffer fields:
`base_offset`, `high_water`, `elided_bytes`, `holes: Vec<(u64,u64)>`, `data: Vec<u8>`.

```
fn snapshot(self) -> TranscriptSnapshot:
    // executed UNDER the buffer lock by the caller (C1 phase 2). This method does NO locking itself —
    // it is &self; the caller holds the guard. It does byte-copy + metadata read only. NO parse, NO I/O.

    // 1. Determine the contiguous readable span identical to contiguous_tail's span logic, but full
    //    (not windowed). The span is the bytes from base_offset forward that are contiguously present.
    span_bytes = copy of self.data for the contiguous readable region   // owned Vec<u8>

    // 2. Metadata — read directly from existing fields; no recomputation that could diverge from
    //    the fallback predicate (ADR-007 warns against re-derivation).
    base   = self.base_offset
    hw     = self.high_water
    elided = self.elided_bytes
    holes  = self.holes.map(|(s,e)| HoleInfo { start: s, end: e })   // only holes within the span

    return TranscriptSnapshot { bytes: span_bytes, base_offset: base,
                                high_water: hw, elided_bytes: elided, holes: holes }
```

### `snapshot_block()` (C2b helper, ARCH §2)

The shared content-extraction primitive both `snapshot()` and (future) #700 conceptually use. Factor
the "contiguous span copy, never cross a hole, no zero-fill" logic out of `contiguous_tail` into a
private `snapshot_block(range) -> Vec<u8>` so `snapshot()` and `contiguous_tail` share one extraction
path — keeping the single-reader invariant true by construction (no parallel span logic). `snapshot()`
calls `snapshot_block` for the full span; `contiguous_tail` calls it for the windowed tail. This is a
refactor-in-place, NOT a third reader.

## Poison Recovery (handled by caller C1, #4764)

`snapshot()` itself is `&self` and does not lock. The caller (C1) takes the buffer lock with
`lock().unwrap_or_else(|p| p.into_inner())`. On a poisoned lock the recovered buffer is treated-as-empty
(returns a snapshot with empty `bytes` but real metadata where readable) and `clear_poison()` is called.
A treat-as-empty session is NOT silently dropped — it surfaces in `SessionLossInfo` as lossy (R-16,
AC-08). See C1 for the lock-acquisition pseudocode.

## Logical Offset Semantics (R-12 — load-bearing)

`base_offset` is what makes a candidate's `byte_offset` a LOGICAL stream position. Downstream (C3):
`candidate.byte_offset = snapshot.base_offset + in_snapshot_offset`.
- Non-overflowed buffer: `base_offset == 0`, so `byte_offset == in_snapshot_offset`.
- Overflowed (ring-tail) buffer: `base_offset > 0`, `byte_offset` stays meaningful across elision.
This file's contract is that `base_offset` is always populated truthfully from the buffer field;
the offset arithmetic itself lives in C3.

## Data Flow

- **Input:** the live `TranscriptBuffer` (held under the buffer lock by C1).
- **Output:** one owned `TranscriptSnapshot` (no borrow of buffer internals — fully owned so all
  parsing happens after lock release).
- **Consumers:** C1 (returns it), C3 (`select_candidates(&snap.bytes, …, snap.base_offset, …)`),
  C6 (reads `elided_bytes`/`holes` for fallback trigger + loss info), #700 (future, reuses unchanged).

## Error Handling

`snapshot()` is infallible — it returns a value, never `Result`/`Option`, never panics. An empty buffer
yields `bytes: vec![]` with truthful metadata. There is no error variant: malformed CONTENT is not this
component's concern (it copies bytes opaquely; C3 hardens parsing).

## Key Test Scenarios

- Non-overflowed buffer: `snapshot().base_offset == 0`, `bytes` equals written content (AC-V-SEAM).
- Overflowed buffer (`base_offset > 0`): `bytes` is the tail span, `elided_bytes > 0`, `base_offset`
  reflects logical start (R-12 feeds C3's offset test).
- Buffer with a hole: `holes` populated; `bytes` is the contiguous span that does not cross the hole.
- Empty buffer: `bytes.is_empty()`, metadata truthful.
- **Debug metadata-only:** `format!("{:?}", snap)` contains no byte content; lengths/counts only (R-19).
- **Single-reader source assertion (AC-V-SEAM/R-06):** only two production callers extract buffer
  content — `contiguous_tail` (PreCompact) and `snapshot()` (the seam). A grep gate fails on a third.
- **#700 reuse proof:** a marker-recovery-style caller parses `snapshot.bytes` with its own patterns
  using only `base_offset`/`high_water`/`elided_bytes`/`holes` — never calls `contiguous_tail`.
