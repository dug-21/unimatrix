# Component 4 — `activity_snapshot()` + `ActivitySnapshot` read surface

**Files**: `ActivitySnapshot` defined in `infra/transcript_activity.rs` (Component 2 module); `activity_snapshot()` method on `TranscriptBuffer` in `infra/session_transcript.rs` (modify).
**ADRs**: ADR-003 (Copy struct, cast-free widths, no latches), ADR-005 (content-opaque, no Display).
**Precedent**: the existing `TranscriptSnapshot` + its manual metadata-only `Debug` (`session_transcript.rs:71, :112`) and `snapshot()` (`:261`) — mirror that posture exactly.

## Purpose

The single metadata-only read surface crt-055 calls at review. Returns a `Copy` `ActivitySnapshot` carrying ONLY the counters — structurally incapable of carrying transcript bytes. Poisoned buffer mutex degrades to empty (#4764), the same as `snapshot()`.

## Type

```
// Defined in transcript_activity.rs alongside ActivityCounters.
// Field set/widths/order MUST match crt-055's contract (§"Surface B") EXACTLY.
#[derive(Clone, Copy, PartialEq, Eq)]    // Copy + small; NO derive(Debug) — hand-written below; NO Display
struct ActivitySnapshot {
    bytes_total:  u64                     // cast-free native width (NFR-5, AC-14)
    delta_count:  u32
    class_counts: [u32; MAX_SIGNAL_CLASSES]
}

impl ActivitySnapshot
    fn empty() -> Self
        return ActivitySnapshot { bytes_total: 0, delta_count: 0, class_counts: [0; MAX_SIGNAL_CLASSES] }
```

**No latch fields** — explicitly no `saw_compaction` / `reload_after_compaction` (prior-scope residue; R-12, AC-15). No content field — no `Vec<u8>`/`String`/`&[u8]`.

### Manual metadata-only `Debug` (mirror `TranscriptSnapshot` :112)

```
impl fmt::Debug for ActivitySnapshot
    // Emits ONLY the scalar counters. By construction there is no byte-bearing field,
    // so this Debug cannot leak content (AC-08, NFR-1, #4740). Hand-written (not derived)
    // to make the content-opacity contract explicit and to match TranscriptSnapshot's style.
    f.debug_struct("ActivitySnapshot")
        .field("bytes_total", &self.bytes_total)
        .field("delta_count", &self.delta_count)
        .field("class_counts", &self.class_counts)
        .finish()
```

(Because all fields are scalars, a `derive(Debug)` would also be safe — but hand-writing it documents the gate and prevents a future content field from silently becoming Debug-printable. The AC-08 structural test asserts no byte-bearing field regardless.)

**No `impl Display`** — asserted absent (AC-08).

## Method on `TranscriptBuffer`

```
// crates/unimatrix-server/src/infra/session_transcript.rs
impl TranscriptBuffer
    pub fn activity_snapshot(&self) -> ActivitySnapshot
        return self.activity.snapshot()      // delegates to ActivityCounters::snapshot() (Component 1)
```

The method takes `&self` and reads under the buffer lock that the caller (Component 5 collector) holds — same lock discipline as `snapshot()`. It does NOT acquire a new lock; the caller's `lock_buffer` (with its poison→empty policy, #4764) supplies the locked `&self`.

### Poison → empty

Poison handling lives at the lock acquisition (the existing `lock_buffer` helper used by `take_transcripts_for_feature` recovers poison + treats-as-empty, #4764). So when the collector locks a poisoned buffer, the recovered guard yields whatever the recovery policy produces; `activity_snapshot()` over an empty/recovered buffer returns zeros. The empty-from-poison result is indistinguishable at the type level from a real zero — crt-055 distinguishes absence via its `raw_signals_available` flag (Component 5 / FR-B10), not crt-054. crt-054 NEVER fabricates an entry for a session that produced no buffer (see Component 5).

## Width / cast contract

`activity_snapshot()` and the `ActivitySnapshot` accessors perform NO `as i64`/`as i32` narrowing — the fields are exposed at native `u64`/`u32` (NFR-5, AC-14). The checked/saturating `→ i64` conversion is crt-055's at persist. crt-054 hands over full-width values.

## Error handling

None — infallible. Poison degrades to empty at the lock layer, not via a `Result`.

## Key test scenarios (hints)

- `activity_snapshot()` after N folds returns `bytes_total`/`delta_count`/`class_counts` equal to the folded totals (AC-05/AC-07).
- `ActivitySnapshot` is `Copy` and `{ bytes_total: u64, delta_count: u32, class_counts: [u32; MAX_SIGNAL_CLASSES] }` exactly (AC-08, contract conformance with crt-055).
- **Content-opacity structural test** (mirror `test_candidates_structurally_absent`-style): no byte-bearing field; metadata-only `Debug`; no `Display` impl exists (AC-08).
- No `saw_compaction`/`reload_after_compaction` latch field (AC-15, R-12).
- No producer-side narrowing cast on the snapshot fields (AC-14).
- `MAX_SIGNAL_CLASSES == 16` (AC-11) — the array width equals crt-055's.
- Read-before-purge ordering (with Component 5): snapshot non-zero, then `purge_cycle_transcripts` zeroes the buffer (AC-07).
