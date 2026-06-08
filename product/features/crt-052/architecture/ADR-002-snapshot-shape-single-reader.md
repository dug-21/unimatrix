## ADR-002: `TranscriptSnapshot` Shape Co-Designed for crt-052 Selection AND #700 Marker Parsing — the Single Content Reader

### Context
Constraint 4 (load-bearing, vnc-030 ADR-007 §4 #4819, #700) pins a **single buffer content reader**
invariant. Today the buffer's only production content reader is the PreCompact path
(`contiguous_tail` → `extract_transcript_block_from_bytes`, `listener.rs:1834-1838`); all other
`contiguous_tail` callers are tests. crt-052's snapshot becomes the **second and last** content reader.
#700 (review-time MARKER recovery) is specified to consume crt-052's seam with its OWN different
patterns and MUST NOT add a third `contiguous_tail`-style reader. This makes the seam's return type a
co-equal design input for two consumers — retrofitting #700's needs later is the expensive mistake
(SR-04, OQ-2). The buffer is NOT lossless: tail-window-equivalence only (vnc-025 ADR-002 #4740,
ADR-008 #4764) — full-content equality holds below the 4 MiB cap; under ring-tail overflow it converges
on the tail window, and `base_offset > 0` / `elided_bytes > 0` record the loss. A snapshot that exposed
only bytes (no metadata) would let either consumer mistake a clipped tail for a full session.

### Decision
Introduce one snapshot primitive on `TranscriptBuffer` and one owned return type, designed so **both**
consumers parse the same in-memory bytes with no re-read:

```rust
pub struct HoleInfo { pub start: u64, pub end: u64 }

pub struct TranscriptSnapshot {
    pub bytes: Vec<u8>,      // owned copy of the contiguous span (never crosses a hole; no zero-fill)
    pub base_offset: u64,    // logical offset of bytes[0] in the session stream
    pub high_water: u64,     // max(offset+len) ever seen — survives clipping
    pub elided_bytes: u64,   // lifetime count of content dropped from the span
    pub holes: Vec<HoleInfo>,// remaining unwritten sub-ranges within the span
}
// manual metadata-only Debug: prints { len, base_offset, high_water, holes: n, elided_bytes };
// NEVER bytes (SR-02 content opacity, vnc-025 ADR-002).

impl TranscriptBuffer {
    pub fn snapshot(&self) -> TranscriptSnapshot { /* copy contiguous span + metadata */ }
}
```

`snapshot()` follows `contiguous_tail` semantics — the copied `bytes` are the contiguous readable span,
never crossing a hole and never returning zero-fill — but returns the **whole** snapshotted span (not a
windowed tail) plus the metadata both consumers need. It is the one content-extraction primitive;
`take_transcripts_for_feature` (ADR-001) calls it under the buffer lock.

**Consumer split (the single-reader invariant in practice):**
- crt-052 selection (ADR-003): `select_candidates(&snapshot.bytes, session_id, snapshot.base_offset, …)`
  — parses Claude Code JSONL for the four marker families.
- #700 MARKER recovery (future): consumes the SAME `TranscriptSnapshot` returned by the SAME seam, runs
  its own marker patterns over `snapshot.bytes`. No new buffer access path; no `contiguous_tail` call.

`base_offset` is what makes a candidate's `byte_offset` a logical stream position (meaningful across
elision), not an array index. `elided_bytes` + `holes` are what ADR-006's fallback trigger and ADR-007's
loss visibility read. Exposing all four metadata fields now is what lets #700 reuse the seam without a
single byte re-read.

### Consequences
Easier: Constraint 4 holds by construction — one primitive, one seam, two pure consumers; #700 lands as
an added pass over an existing return value, not a buffer-access change; loss is always carried with the
bytes so no consumer can silently mistake a clipped tail for full content (SR-08). Harder: the snapshot
copies the full span (up to 4 MiB) rather than a small tail window — bounded by the cap, off-lock, and
within AC-12's <50 ms budget, but heavier than PreCompact's windowed read; the metadata-only `Debug`
must be hand-written and grep-gated (AC-12 extension) since `derive(Debug)` would leak bytes.
Cross-refs: ADR-001 (the seam returning this), ADR-003 (crt-052 consumer), ADR-006/007 (metadata
consumers), vnc-025 ADR-002 #4740 / ADR-008 #4764, vnc-030 ADR-007 §4 #4819, #700.
