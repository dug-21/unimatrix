//! Transcript-activity fold types (crt-054, Surface B foundation).
//!
//! This sibling module owns the in-memory fold over transcript deltas: the
//! [`ActivityCounters`] accumulator embedded in `TranscriptBuffer`, the
//! [`ActivitySnapshot`] `Copy` read surface crt-055 reads at review, and the
//! [`SignatureScanner`] that compiles `[transcript_signals]` into one shared
//! `regex::bytes::RegexSet` and performs exactly one byte scan per delta.
//!
//! It exists as a sibling to `session_transcript.rs` because that file is
//! already near the 500-line cap; the buffer file gains only fields, one fold
//! call, and one accessor (later waves).
//!
//! Content opacity (ADR-005, NFR-1, AC-08): every type here is scalars only —
//! structurally incapable of holding transcript bytes. No `Vec<u8>`/`String`/
//! `&[u8]` field, metadata-only `Debug`, no `Display`. The scanner matches in
//! the BYTES domain (`regex::bytes::RegexSet`) so arbitrary non-UTF-8 deltas
//! scan without a validation pass and without panic.
//!
//! Bytes-only honest unit (NFR-2, AC-15): no `token_*` symbol anywhere.
//!
//! Cast-free producer widths (ADR-003, NFR-5, AC-14): `bytes_total: u64`,
//! `delta_count: u32`, `class_counts: [u32; N]` are emitted at native widths;
//! the checked/saturating `-> i64` conversion is crt-055's at persist.

use std::fmt;

use regex::bytes::RegexSet;

/// PINNED shared constant — MUST equal crt-055's constant EXACTLY (NFR-6, AC-11);
/// it crosses the producer/consumer boundary via [`ActivitySnapshot::class_counts`].
/// v1 catalog indices: `0 = error`, `1 = refusal`.
pub(crate) const MAX_SIGNAL_CLASSES: usize = 16;

// Compile-time guard: the array width every dependent crt-054/crt-055 type
// relies on is exactly 16, not "<= 16" (AC-11). A drift here is a build error.
const _: () = assert!(MAX_SIGNAL_CLASSES == 16);

/// In-memory running fold over transcript deltas (ADR-001, ADR-003, ADR-005).
///
/// Monotonic byte sum, delta count, and per-class match counts. Scalars only —
/// no byte-bearing field EVER (AC-08). One instance lives inside each
/// `TranscriptBuffer`; both the registered and held delta routes fold into it
/// by construction (the accumulator is owned by the buffer, not the route).
///
/// `derive(Debug)` is safe here because every field is a scalar/array-of-scalar;
/// this is NOT the public read surface (that is [`ActivitySnapshot`], with a
/// hand-written metadata-only `Debug`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ActivityCounters {
    /// Monotonic sum of delta payload lengths (cast-free `u64`, NFR-5).
    bytes_total: u64,
    /// +1 per delta merged (cast-free `u32`).
    delta_count: u32,
    /// Per-class match counts, in config order (index == `RegexSet` pattern index).
    class_counts: [u32; MAX_SIGNAL_CLASSES],
}

impl ActivityCounters {
    /// Fresh accumulator: all counters zero.
    pub(crate) fn new() -> Self {
        ActivityCounters {
            bytes_total: 0,
            delta_count: 0,
            class_counts: [0; MAX_SIGNAL_CLASSES],
        }
    }

    /// The fold. Called from `apply_delta` AFTER the merge (later wave), under the
    /// buffer lock already held — introduces NO new lock. O(bytes), allocation-free,
    /// a single shared scan per delta (FR-B5, AC-09).
    ///
    /// Saturating arithmetic so a pathological stream cannot panic in debug builds.
    /// `bytes.len() as u64` is a WIDENING cast (usize -> u64), not the forbidden
    /// narrowing toward `i64` — AC-14 forbids `as i64`/`as i32` on the producer
    /// counters, not widening of a length.
    pub(crate) fn fold(&mut self, bytes: &[u8], scanner: &SignatureScanner) {
        self.bytes_total = self.bytes_total.saturating_add(bytes.len() as u64);
        self.delta_count = self.delta_count.saturating_add(1);
        // A delta may match multiple classes; each matched class increments once
        // per delta (the RegexSet iterator yields each matched index at most once).
        for class_index in scanner.scan(bytes) {
            // class_index < class_count <= MAX_SIGNAL_CLASSES, so the index is
            // always in bounds (validate() bounds the enabled count).
            self.class_counts[class_index] = self.class_counts[class_index].saturating_add(1);
        }
    }

    /// Produce the `Copy` snapshot (the read surface crt-055 reads at review).
    pub(crate) fn snapshot(&self) -> ActivitySnapshot {
        ActivitySnapshot {
            bytes_total: self.bytes_total,
            delta_count: self.delta_count,
            class_counts: self.class_counts,
        }
    }
}

/// Counters-only read surface returned by `TranscriptBuffer::activity_snapshot()`
/// (ADR-003, ADR-005). Field set/widths/order MUST match crt-055's contract
/// (§"Surface B") exactly.
///
/// No byte-bearing field (`Vec<u8>`/`String`/`&[u8]`) anywhere (AC-08). No latch
/// fields — explicitly no `saw_compaction`/`reload_after_compaction` (R-12,
/// AC-15). `Copy` + small. No `Display`; metadata-only hand-written `Debug` below.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ActivitySnapshot {
    /// Cast-free native width (NFR-5, AC-14).
    pub bytes_total: u64,
    pub delta_count: u32,
    pub class_counts: [u32; MAX_SIGNAL_CLASSES],
}

impl ActivitySnapshot {
    /// The empty snapshot — all counters zero. Returned when a buffer produced
    /// no fold (and by the poison->empty degrade at the lock layer, #4764).
    pub fn empty() -> Self {
        ActivitySnapshot {
            bytes_total: 0,
            delta_count: 0,
            class_counts: [0; MAX_SIGNAL_CLASSES],
        }
    }
}

/// Manual, metadata-only `Debug` (AC-08, NFR-1) — mirrors `TranscriptSnapshot`'s
/// posture (`session_transcript.rs`). Emits ONLY the scalar counters. By
/// construction there is no byte-bearing field, so this cannot leak content;
/// hand-written (not derived) to make the content-opacity contract explicit and
/// to prevent a future content field from silently becoming Debug-printable.
impl fmt::Debug for ActivitySnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActivitySnapshot")
            .field("bytes_total", &self.bytes_total)
            .field("delta_count", &self.delta_count)
            .field("class_counts", &self.class_counts)
            .finish()
    }
}

// NO `impl Display for ActivitySnapshot` — asserted absent (AC-08).

/// Compiles the configured `[transcript_signals]` catalog into ONE shared
/// `regex::bytes::RegexSet` and performs exactly one byte scan per delta
/// (ADR-002, ADR-005).
///
/// The `RegexSet` pattern index == class index == `class_counts` array index.
/// Config order is preserved (`RegexSet` preserves input order; the caller
/// builds the pattern vector in config order over `enabled` entries). v1:
/// index 0 = error, index 1 = refusal.
///
/// BYTES domain: `regex::bytes::RegexSet` matches raw delta bytes without a
/// UTF-8 validation pass (deltas are arbitrary bytes; FR-B3 counts bytes).
///
/// `Debug` is the derived `RegexSet` debug (operator-trusted config patterns,
/// never transcript bytes) — content-opaque by construction.
#[derive(Debug)]
pub struct SignatureScanner {
    /// Compiled from enabled patterns, in config order.
    set: RegexSet,
    /// Number of enabled classes == `set.len()`; `<= MAX_SIGNAL_CLASSES`.
    class_count: usize,
}

impl SignatureScanner {
    /// Build from the validated, enabled signal-class patterns (in config order).
    ///
    /// Called ONCE at startup after config `validate()` (later wave), which has
    /// already guaranteed `<= MAX_SIGNAL_CLASSES` enabled, every pattern compiles,
    /// and no duplicate class name. This is the defense-in-depth second compile of
    /// already-validated patterns; it still returns `Result` and the caller
    /// propagates loudly — no silent "no scanning" fallback (R-10).
    pub fn compile(enabled_patterns: &[String]) -> Result<SignatureScanner, ScannerError> {
        debug_assert!(
            enabled_patterns.len() <= MAX_SIGNAL_CLASSES,
            "enabled patterns must be bounded by validate() to MAX_SIGNAL_CLASSES"
        );
        let set = RegexSet::new(enabled_patterns).map_err(ScannerError::InvalidRegex)?;
        Ok(SignatureScanner {
            set,
            class_count: enabled_patterns.len(),
        })
    }

    /// The empty scanner — used when `[transcript_signals]` is absent/empty.
    /// Matches nothing; the fold still counts bytes/deltas. A legitimate config
    /// (zero signal classes), distinct from "fold not running".
    pub fn empty() -> SignatureScanner {
        SignatureScanner {
            set: RegexSet::empty(),
            class_count: 0,
        }
    }

    /// Number of enabled classes (`<= MAX_SIGNAL_CLASSES`).
    pub fn class_count(&self) -> usize {
        self.class_count
    }

    /// ONE scan per delta. Returns the matched class indices (each `< class_count
    /// <= MAX_SIGNAL_CLASSES`), ascending and deduped. Iterator-based so `fold`
    /// loops without a heap `Vec` on the hot path (NFR-3). `RegexSet` runs all
    /// patterns in a single linear pass (no catastrophic backtracking). Infallible.
    pub(crate) fn scan(&self, bytes: &[u8]) -> impl Iterator<Item = usize> {
        self.set.matches(bytes).into_iter()
    }
}

/// Scanner construction error — surfaced loudly at startup; never a runtime
/// fallback. Scan time is infallible (a `RegexSet` match cannot error); only
/// `compile` returns `Result`.
#[derive(Debug)]
pub enum ScannerError {
    /// A configured pattern failed to compile.
    InvalidRegex(regex::Error),
}

impl fmt::Display for ScannerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScannerError::InvalidRegex(e) => write!(f, "invalid transcript-signal regex: {e}"),
        }
    }
}

impl std::error::Error for ScannerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ScannerError::InvalidRegex(e) => Some(e),
        }
    }
}

#[cfg(test)]
#[path = "transcript_activity_tests.rs"]
mod tests;
