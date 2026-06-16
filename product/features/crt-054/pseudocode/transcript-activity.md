# Component 2 — `transcript_activity` module + `SignatureScanner`

**File**: `crates/unimatrix-server/src/infra/transcript_activity.rs` (new); register `pub(crate) mod transcript_activity;` in `infra/mod.rs`.
**Crate dep**: the `regex` crate (`RegexSet` — linear-time, no catastrophic backtracking).
**ADRs**: ADR-002 (shared `RegexSet`, `[transcript_signals]` config, `validate()`-bounded, v1 = error/refusal only), ADR-005 (content-opaque).

## Purpose

The sibling module that owns the fold logic types (`ActivityCounters`, `ActivitySnapshot`, `MAX_SIGNAL_CLASSES` — see Components 1, 4) and the `SignatureScanner`. The scanner compiles the configured `[transcript_signals]` catalog into ONE shared `RegexSet` and performs exactly one byte scan per delta, returning the set of matched class indices. It exists as a sibling module because `session_transcript.rs` is already near the 500-line cap; the buffer file gains only fields, one fold call, and one accessor.

## `SignatureScanner`

```
struct SignatureScanner {
    set:           RegexSet,        // compiled from enabled patterns, config order
    class_count:   usize,           // number of enabled classes == set.len(); <= MAX_SIGNAL_CLASSES
}
```

The `RegexSet` pattern index == class index == `class_counts` array index. Config order is preserved because Component 9 builds the pattern vector in config order over `enabled` entries, and `RegexSet` preserves input order. v1: index 0 = error, index 1 = refusal (AC-10, FR-C4).

## Functions

```
impl SignatureScanner

    // Build from the validated, enabled signal classes (in config order).
    // Called ONCE at startup after config validate() (Component 9). validate()
    // has already guaranteed: <= MAX_SIGNAL_CLASSES enabled, every pattern compiles,
    // no duplicate class_name. So compile() here is the second compile of already-
    // validated patterns and is not expected to fail — but it still returns Result
    // and the caller propagates loudly (no silent fallback, R-10).
    fn compile(enabled_patterns: &[String]) -> Result<SignatureScanner, ScannerError>
        DEBUG_ASSERT enabled_patterns.len() <= MAX_SIGNAL_CLASSES   // upheld by validate()
        set = RegexSet::new(enabled_patterns)?      // ScannerError::InvalidRegex on failure
        return SignatureScanner { set, class_count: enabled_patterns.len() }

    // The empty scanner — used when [transcript_signals] is absent/empty.
    // Matches nothing; fold still counts bytes/deltas. Keeps Surface B alive with
    // zero signal classes (a legitimate config, distinct from "fold not running").
    fn empty() -> SignatureScanner
        return SignatureScanner { set: RegexSet::empty(), class_count: 0 }

    // ONE scan per delta. Returns the matched class indices (each < class_count
    // <= MAX_SIGNAL_CLASSES). Allocation-discipline: prefer an iterator over
    // set.matches(bytes).into_iter() so fold() loops without a heap Vec on the
    // hot path (NFR-3). RegexSet runs all patterns in a single linear pass.
    fn scan(&self, bytes: &[u8]) -> impl Iterator<Item = usize>
        return self.set.matches(bytes).into_iter()    // yields usize indices, ascending, deduped
```

Notes:
- `RegexSet::matches` over `&[u8]`: use the `regex::bytes::RegexSet` variant so the scanner matches raw delta bytes without a UTF-8 validation pass (deltas are arbitrary bytes; FR-B3 counts bytes). This is the bytes-domain RegexSet — confirm the import is `regex::bytes::RegexSet`, not `regex::RegexSet` (which is `&str`-only).
- `matches(...).into_iter()` yields each matched pattern index at most once — so a delta matching a class twice still increments that class's count by exactly 1 per delta (FR-B3: "per delta per matched class", once per delta).

## Error type

```
enum ScannerError {
    InvalidRegex(regex::Error)    // surfaced loudly at startup; never a runtime fallback
}
```

The scanner never fails at scan time — only at `compile`. Scan is infallible (a `RegexSet` match cannot error). This keeps the fold path (Component 1/3) infallible under the buffer lock.

## State / lifecycle

- One `SignatureScanner` is compiled at startup and shared. It is cloned/`Arc`-shared into every `TranscriptBuffer::new` call (Component 3 threads it through). The `RegexSet` is internally `Arc`-like (cheap clone) — share by `Arc<SignatureScanner>` to avoid recompiling per buffer.
- Immutable after construction.

## Error handling

- `compile` failure → propagate to startup, fail loud (the config `validate()` in Component 9 is the primary gate; this is the defense-in-depth second compile). No degrade to "no scanning."
- Scan: infallible.

## Module re-exports

This file also defines/re-exports the shared types so dependents have one import path:
- `pub(crate) const MAX_SIGNAL_CLASSES: usize = 16;` (Component 1)
- `pub(crate) struct ActivityCounters { ... }` (Component 1)
- `pub(crate) struct ActivitySnapshot { ... }` (Component 4)
- `pub(crate) struct SignatureScanner { ... }` (this component)

## Key test scenarios (hints)

- `compile(["err.*", "refus.*"])` → `class_count == 2`; `scan` over a delta matching both yields `{0, 1}` (AC-09).
- `scan` invoked once per delta (assert via a single-pass instrumentation or by construction — one `set.matches` call).
- `empty()` scanner: `scan` yields nothing; fold still advances bytes/deltas.
- Bytes-domain: a delta of non-UTF-8 bytes is scanned without panic (uses `regex::bytes`).
- `compile` of an invalid pattern returns `Err(InvalidRegex)` (paired with Component 9's loud `validate()`).
- No content stored on the scanner; no `Display`; `Debug` (if any) is metadata-only.
