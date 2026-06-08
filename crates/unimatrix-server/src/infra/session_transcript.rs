//! Per-session, in-memory, never-persisted transcript accumulator (vnc-025).
//!
//! `TranscriptBuffer` is a pure state machine: idempotent offset-bounded merge of
//! transcript deltas with ring-tail overflow, hole tracking, and a contiguous-tail
//! reader for PreCompact. It performs no I/O, holds no locks (locking is the
//! caller's job — see `infra/session.rs`), and emits no `tracing` (AC-12 grep gate).
//!
//! Content opacity (SR-02, ADR-002): in-memory + purge IS the secrets guarantee.
//! No `Display`, no derived `Debug`, no `Result` in the public API that could
//! carry bytes. The only content-bearing output is `contiguous_tail`, consumed
//! solely by the PreCompact block builder.
//!
//! Never-panics contract (NFR-09, ADR-008 Layer 1): no input reachable from the
//! wire — any `offset: u64`, any `bytes` up to the 1 MiB frame ceiling — can
//! panic here. Offset-end overflow drops the whole delta (deliberate: no partial
//! clip). All u64→usize conversions are on span-relative values proven
//! `<= max_bytes` (invariant I5, documented at each conversion site).

use std::fmt;

/// Default accumulated-transcript cap per session: 4 MiB.
/// Shared with `RetentionConfig.transcript_buffer_max_bytes` serde default (ADR-006).
pub const DEFAULT_TRANSCRIPT_BUFFER_MAX_BYTES: usize = 4_194_304;

/// Bounded hole metadata (ADR-002): a delta that would create a 65th hole range
/// collapses the buffer to the newest contiguous segment. The bound is the
/// requirement; the constant is tunable.
const MAX_HOLE_RANGES: usize = 64;

/// Per-session transcript accumulator.
///
/// # Invariants
///
/// - I1: `data.len() <= max_bytes` at all times (ring-tail runs before any extension).
/// - I2: span = `[base_offset, base_offset + data.len())`; `holes` are disjoint,
///   sorted ascending, strictly inside the span, and `holes.len() <= MAX_HOLE_RANGES`
///   after every `apply_delta`.
/// - I3: `high_water` is monotonic non-decreasing; tracks what was *sent*, not what
///   is retained.
/// - I4: hole regions in `data` are zero-filled but NEVER served (`contiguous_tail`
///   stops at the last hole end).
/// - I5: u64→usize conversions occur ONLY on span-relative values already proven
///   `<= max_bytes` (ADR-008 review gate; comment at each conversion site).
pub struct TranscriptBuffer {
    /// Logical offset of `data[0]`.
    base_offset: u64,
    /// Spans `[base_offset, base_offset + data.len())`.
    data: Vec<u8>,
    /// Unwritten sub-ranges within the span (zero-filled in `data`); disjoint,
    /// sorted ascending, capped at `MAX_HOLE_RANGES`.
    holes: Vec<(u64, u64)>,
    /// `max(offset + len)` ever seen — monotonic, survives clipping.
    high_water: u64,
    /// Bytes dropped by ring-tail advancement or below-base clipping
    /// (metadata, never content).
    elided_bytes: u64,
    /// Cap, injected at construction (ADR-006).
    max_bytes: usize,
}

/// Counts-only purge record (ADR-004). Named crt-052 seam shape — never content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptPurgeRecord {
    pub session_id: String,
    pub bytes_purged: u64,
}

/// An unwritten sub-range within a snapshotted span, in LOGICAL stream offsets
/// (crt-052 ADR-002). `[start, end)` half-open; `end` exclusive. Offsets only —
/// safe to `derive(Debug)` (no content).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoleInfo {
    /// Logical stream offset of the hole start (inclusive).
    pub start: u64,
    /// Logical stream offset of the hole end (exclusive).
    pub end: u64,
}

/// Owned, never-persisted snapshot of a `TranscriptBuffer`'s readable content
/// plus loss metadata (crt-052 ADR-002). Co-designed for crt-052 candidate
/// selection AND #700 marker recovery — both parse `bytes` with their own
/// patterns, placing matches at LOGICAL stream offsets via `base_offset`. This
/// is the second and last production buffer-content reader's return value;
/// there is no third reader (Constraint 4).
///
/// `bytes` is an owned copy of the contiguous readable span: it never crosses a
/// hole and carries no zero-fill (FR-19). All parsing happens AFTER the buffer
/// lock is released, on this owned value (AC-01).
///
/// Content opacity (SR-02, AC-06): `Debug` is hand-written and metadata-only —
/// it MUST NOT carry any byte of `bytes`. Do NOT `derive(Debug)`.
#[derive(Clone)]
pub struct TranscriptSnapshot {
    /// Owned copy of the contiguous readable span (never crosses a hole; no
    /// zero-fill). Parse only after lock release.
    pub bytes: Vec<u8>,
    /// Logical stream offset of `bytes[0]` (R-12: makes a candidate's
    /// `byte_offset = base_offset + in_snapshot_offset` a logical position,
    /// meaningful across ring-tail elision). `0` for a non-overflowed buffer.
    pub base_offset: u64,
    /// `max(offset + len)` ever seen — monotonic, survives clipping.
    pub high_water: u64,
    /// Lifetime count of content dropped from the span (ring-tail / clip).
    pub elided_bytes: u64,
    /// Remaining unwritten sub-ranges within the span (logical offsets).
    pub holes: Vec<HoleInfo>,
}

/// Manual, metadata-only `Debug` (R-19, AC-06 content-leak gate): emits the
/// span length and counts ONLY — NEVER any byte of `bytes`. `derive(Debug)`
/// would leak content and is forbidden by the leak gate.
impl fmt::Debug for TranscriptSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TranscriptSnapshot")
            .field("len", &self.bytes.len())
            .field("base_offset", &self.base_offset)
            .field("high_water", &self.high_water)
            .field("elided_bytes", &self.elided_bytes)
            .field("holes", &self.holes.len())
            .finish()
    }
}

/// Enterprise seam (ADR-007): collapses the (tenant, project, session) composite
/// dimension to the registry's string key. OSS: tenant is always "default";
/// returns `session_id` unchanged (transport namespacing via the existing `http-`
/// prefix is orthogonal, applied earlier in `prefix_session_id`). Enterprise
/// re-key changes THIS function only; no call-site re-key. LOAD-BEARING despite
/// being degenerate — do not "simplify" away.
pub fn session_key(_tenant: &str, _project: &str, session_id: &str) -> String {
    session_id.to_string()
}

impl TranscriptBuffer {
    /// New empty buffer with the given accumulated-byte cap.
    pub fn new(max_bytes: usize) -> Self {
        TranscriptBuffer {
            base_offset: 0,
            data: Vec::new(),
            holes: Vec::new(),
            high_water: 0,
            elided_bytes: 0,
            max_bytes,
        }
    }

    /// Merge one delta. Idempotent: duplicates and overlaps are in-place rewrites;
    /// final state below the cap depends only on the set of covered ranges, not
    /// arrival order (AC-02). Never panics for any `(offset, bytes)` (NFR-09).
    pub fn apply_delta(&mut self, offset: u64, bytes: &[u8]) {
        let len_u64 = bytes.len() as u64;

        // ADR-008 Layer 1: drop-whole on end overflow. DELIBERATE: no partial
        // clip, no state change, no high_water update, no elided accounting
        // (the bytes never entered the span), no log line.
        let end = match offset.checked_add(len_u64) {
            Some(end) => end,
            None => return,
        };

        // high_water updates for every non-overflowing delta, including len-0
        // and below-floor deltas (FR-02).
        self.high_water = self.high_water.max(end);

        // Zero-length delta: defined no-op beyond the high_water update.
        if len_u64 == 0 {
            return;
        }

        // Step 1: ring-tail BEFORE writing (I1). required_base is the lowest
        // base that lets [.., end) fit within max_bytes.
        let required_base = end.saturating_sub(self.max_bytes as u64);
        if required_base > self.base_offset {
            self.advance_base(required_base);
        }

        // Step 2: clip the incoming delta against the (possibly advanced) floor.
        if end <= self.base_offset {
            // Entirely below floor — defined no-op: clipped, counted,
            // high_water already updated (FR-02).
            self.elided_bytes = self.elided_bytes.saturating_add(len_u64);
            return;
        }
        let mut write_offset = offset;
        let mut write_bytes = bytes;
        if offset < self.base_offset {
            let clip = self.base_offset - offset; // safe: offset < base_offset
            self.elided_bytes = self.elided_bytes.saturating_add(clip);
            // I5: clip < len_u64 (since end > base_offset) and len_u64 came
            // from a usize — clip fits usize.
            write_bytes = &bytes[clip as usize..];
            write_offset = self.base_offset;
        }

        // INVARIANT here: base_offset <= write_offset, and
        // end <= base_offset + max_bytes (because base_offset >= required_base
        // = end - max_bytes). So all span-relative indices below are
        // <= max_bytes (I5).

        // Step 3: extend span, creating a hole if the delta starts past the
        // current end. span_end cannot overflow beyond `end` (checked above);
        // saturating_add for defense in depth.
        let span_end = self.base_offset.saturating_add(self.data.len() as u64);
        if write_offset > span_end {
            // New hole starts at span_end >= every existing hole end (I2:
            // holes are strictly inside the span), so push keeps the list
            // sorted; normalized in Step 6 if over cap.
            self.holes.push((span_end, write_offset));
        }
        if end > span_end {
            // I5: end - base_offset <= max_bytes (invariant above).
            // Zero-fills both the gap and the write region.
            self.data.resize((end - self.base_offset) as usize, 0);
        }

        // Step 4: in-place write (idempotent: duplicates/overlaps are rewrites).
        // I5: rel and rel + write_bytes.len() = end - base_offset are
        // span-relative, proven <= max_bytes.
        let rel = (write_offset - self.base_offset) as usize;
        self.data[rel..rel + write_bytes.len()].copy_from_slice(write_bytes);

        // Step 5: hole surgery — remove [write_offset, end) from `holes`.
        self.subtract_range_from_holes(write_offset, end);

        // Step 6: bounded metadata (ADR-002). If Step 3's push or Step 5's
        // split exceeded the cap, collapse to the newest contiguous segment.
        if self.holes.len() > MAX_HOLE_RANGES {
            self.collapse_to_newest();
        }
    }

    /// Up to `window` bytes from the end of the span, truncated at the most
    /// recent hole boundary — never crosses a hole, never returns zero-fill
    /// (FR-19). `None` when empty or `window == 0`.
    pub fn contiguous_tail(&self, window: usize) -> Option<Vec<u8>> {
        if self.data.is_empty() || window == 0 {
            return None;
        }
        // The contiguous readable run is `[start_rel, data.len())` where
        // `start_rel` is the post-hole floor; `snapshot_block` copies it.
        let start_rel = self.contiguous_run_start_rel();
        let avail = self.data.len() - start_rel;
        let take = window.min(avail);
        // Windowed tail: take the newest `take` bytes of the contiguous run.
        Some(self.snapshot_block(self.data.len() - take))
    }

    /// Owned snapshot of the WHOLE contiguous readable span plus the four
    /// metadata fields both content consumers need (crt-052 ADR-002; #700
    /// marker recovery reuses the SAME return type — no third buffer reader).
    ///
    /// Unlike [`contiguous_tail`], this returns the entire contiguous run (not
    /// a windowed tail) and all metadata, so a consumer can parse the bytes and
    /// place every match at a LOGICAL stream offset (`base_offset + in-span`).
    ///
    /// Byte copy + metadata read ONLY: no parse, no marker match, no I/O, no
    /// `tracing` of content (Constraint 1, AC-01). Infallible — an empty buffer
    /// yields `bytes: vec![]` with truthful metadata; never panics, never
    /// returns `Result`/`Option`. Locking is the caller's job (C1 holds the
    /// buffer guard, poison-recovers per #4764); this method is `&self`.
    pub fn snapshot(&self) -> TranscriptSnapshot {
        // Contiguous readable span: from the post-hole floor to the span end,
        // never crossing a hole, never zero-fill — same span logic as the
        // tail reader, just unwindowed.
        let bytes = if self.data.is_empty() {
            Vec::new()
        } else {
            self.snapshot_block(self.contiguous_run_start_rel())
        };
        TranscriptSnapshot {
            bytes,
            base_offset: self.base_offset,
            high_water: self.high_water,
            elided_bytes: self.elided_bytes,
            holes: self
                .holes
                .iter()
                .map(|&(start, end)| HoleInfo { start, end })
                .collect(),
        }
    }

    /// Span-relative start of the contiguous readable run: the most recent hole
    /// end (relative to `base_offset`), or 0 when there are no holes. The run is
    /// `data[start_rel..]` — the bytes that never cross a hole and carry no
    /// zero-fill (I4). Caller must ensure `!data.is_empty()`.
    fn contiguous_run_start_rel(&self) -> usize {
        match self.holes.last() {
            // I5: hole_end - base_offset <= data.len() <= max_bytes (holes are
            // strictly inside the span per I2).
            Some(&(_, hole_end)) => (hole_end - self.base_offset) as usize,
            None => 0,
        }
    }

    /// The one content-extraction primitive: an owned copy of
    /// `data[start_rel..]`. Both `snapshot()` (full span) and `contiguous_tail`
    /// (windowed tail) route their copy through here, so a single span-copy path
    /// keeps the single-reader invariant true by construction (ADR-002). Does no
    /// locking, no parse, no I/O. `start_rel` must already sit on or after the
    /// post-hole floor so the result never crosses a hole or returns zero-fill.
    fn snapshot_block(&self, start_rel: usize) -> Vec<u8> {
        self.data[start_rel..].to_vec()
    }

    /// Purge all content; returns bytes purged (span length — the value
    /// ADR-004 audits as `bytes=<n>`). Post-clear semantics pinned for crt-052:
    /// `base_offset = high_water` so a resumed stream at offsets >= high_water
    /// continues cleanly (no giant hole); deltas below high_water after a clear
    /// are defined no-ops (clipped + counted, same as the ring-tail floor).
    /// `high_water` is unchanged (monotonic, I3); `elided_bytes` is unchanged
    /// (lifetime counter — clear() is a purge, not an elision; the purged count
    /// is returned, not added to elided_bytes).
    pub fn clear(&mut self) -> u64 {
        let purged = self.data.len() as u64;
        self.data.clear();
        self.holes.clear();
        self.base_offset = self.high_water;
        purged
    }

    /// Span length in bytes (includes zero-filled hole regions);
    /// 0 ⇒ empty ⇒ purge emits nothing.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// `max(offset + len)` ever seen — monotonic.
    pub fn high_water(&self) -> u64 {
        self.high_water
    }

    /// Lifetime count of bytes dropped by ring-tail advancement or
    /// below-base clipping.
    pub fn elided_bytes(&self) -> u64 {
        self.elided_bytes
    }

    /// Sum of hole lengths within the current span.
    fn total_hole_bytes(&self) -> u64 {
        self.holes
            .iter()
            .fold(0u64, |acc, &(start, end)| acc.saturating_add(end - start))
    }

    /// Advance the ring-tail floor, dropping head content. Precondition:
    /// `new_base > base_offset`. Elision accounting counts only *received*
    /// bytes — hole bytes are zero-fill that was never received, so they are
    /// never double-counted when a hole is dropped below base (R-03.4).
    fn advance_base(&mut self, new_base: u64) {
        let span_end = self.base_offset.saturating_add(self.data.len() as u64);
        if new_base >= span_end {
            // Whole existing span dropped.
            let received = (self.data.len() as u64).saturating_sub(self.total_hole_bytes());
            self.elided_bytes = self.elided_bytes.saturating_add(received);
            self.data.clear();
            self.holes.clear();
            self.base_offset = new_base;
        } else {
            // I5: new_base - base_offset < data.len() <= max_bytes.
            let drop_len = (new_base - self.base_offset) as usize;
            let hole_bytes_below: u64 = self
                .holes
                .iter()
                .map(|&(hole_start, hole_end)| {
                    // Overlap of the hole with the dropped range
                    // [base_offset, new_base).
                    let overlap_start = hole_start.max(self.base_offset);
                    let overlap_end = hole_end.min(new_base);
                    overlap_end.saturating_sub(overlap_start)
                })
                .fold(0u64, |acc, n| acc.saturating_add(n));
            self.elided_bytes = self
                .elided_bytes
                .saturating_add((drop_len as u64).saturating_sub(hole_bytes_below));
            self.data.drain(0..drop_len);
            // Drop holes entirely below new_base; truncate a straddling hole
            // to (new_base, hole_end). Order is preserved.
            self.holes.retain_mut(|hole| {
                if hole.1 <= new_base {
                    return false;
                }
                hole.0 = hole.0.max(new_base);
                true
            });
            self.base_offset = new_base;
        }
    }

    /// Remove `[start, end)` from the hole list. Four mutation classes per
    /// overlapping hole (R-01.2): fully filled (remove), shrunk from left,
    /// shrunk from right, split in two. A single write may span multiple holes
    /// — the per-hole rules compose. Result stays sorted + disjoint by
    /// construction.
    fn subtract_range_from_holes(&mut self, start: u64, end: u64) {
        let mut rebuilt = Vec::with_capacity(self.holes.len());
        for &(hole_start, hole_end) in &self.holes {
            if hole_end <= start || hole_start >= end {
                rebuilt.push((hole_start, hole_end)); // no overlap
            } else if start <= hole_start && end >= hole_end {
                // fully filled — remove
            } else if start <= hole_start {
                rebuilt.push((end, hole_end)); // shrunk from left
            } else if end >= hole_end {
                rebuilt.push((hole_start, start)); // shrunk from right
            } else {
                // split in two
                rebuilt.push((hole_start, start));
                rebuilt.push((end, hole_end));
            }
        }
        self.holes = rebuilt;
    }

    /// ADR-002 bounded metadata: collapse the buffer to the newest contiguous
    /// segment; the abandoned span is counted as elided (received bytes only).
    fn collapse_to_newest(&mut self) {
        let Some(&(_, last_hole_end)) = self.holes.last() else {
            return; // unreachable: only called when holes exceed the cap
        };
        let dropped_received =
            (last_hole_end - self.base_offset).saturating_sub(self.total_hole_bytes());
        self.elided_bytes = self.elided_bytes.saturating_add(dropped_received);
        // I5: last_hole_end - base_offset <= data.len() <= max_bytes (holes
        // are strictly inside the span per I2).
        self.data
            .drain(0..(last_hole_end - self.base_offset) as usize);
        self.holes.clear();
        self.base_offset = last_hole_end;
        // Post: single contiguous segment [last_hole_end, span_end);
        // invariants I1..I4 hold.
    }

    /// Test-only visibility into hole bookkeeping (R-01 hole-surgery assertions).
    #[cfg(test)]
    pub(crate) fn holes_for_test(&self) -> &[(u64, u64)] {
        &self.holes
    }

    /// Test-only visibility into the ring-tail floor.
    #[cfg(test)]
    pub(crate) fn base_offset_for_test(&self) -> u64 {
        self.base_offset
    }
}

/// Manual, metadata-only Debug (SR-02/ADR-002): NEVER any byte of `data`.
/// `SessionState` keeps `derive(Debug)` — this impl is what it prints.
impl fmt::Debug for TranscriptBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TranscriptBuffer {{ len: {}, base_offset: {}, high_water: {}, holes: {}, elided_bytes: {} }}",
            self.data.len(),
            self.base_offset,
            self.high_water,
            self.holes.len(),
            self.elided_bytes
        )
    }
}

#[cfg(test)]
#[path = "session_transcript_tests.rs"]
mod tests;
