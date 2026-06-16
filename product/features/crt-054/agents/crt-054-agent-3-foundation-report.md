# Agent Report — crt-054-agent-3-foundation

**Task**: Wave-1 foundational type module for crt-054 Surface B.
**Commit**: 5236ec1a — `impl(transcript_activity): foundational fold types + SignatureScanner (#752)`

## Files modified
- `crates/unimatrix-server/src/infra/transcript_activity.rs` (new, 244 lines)
- `crates/unimatrix-server/src/infra/transcript_activity_tests.rs` (new, sibling test file)
- `crates/unimatrix-server/src/infra/mod.rs` (registered `pub(crate) mod transcript_activity;`)

## What landed
- `MAX_SIGNAL_CLASSES: usize = 16` (pinned) + compile-time `const _: () = assert!(... == 16)` and a unit assertion (AC-11).
- `ActivityCounters { bytes_total: u64, delta_count: u32, class_counts: [u32; 16] }` — `new()`, `fold(bytes, scanner)` (saturating adds; one scan per delta; per-class +1), `snapshot()`. derive(Debug,Clone,Copy,PartialEq,Eq,Default); scalars only.
- `ActivitySnapshot` — `#[derive(Clone, Copy, PartialEq, Eq)]`, `empty()`, hand-written metadata-only `Debug` mirroring `TranscriptSnapshot`, NO `Display`, no byte-bearing field (AC-08).
- `SignatureScanner { set: regex::bytes::RegexSet, class_count }` — `compile()`/`empty()`/`scan()`/`class_count()`. Bytes domain (no UTF-8 validation). `scan` infallible; only `compile` returns Result.
- `enum ScannerError { InvalidRegex(regex::Error) }` + Display/Error impls.

## Tests
18 passed, 0 failed (`cargo test -p unimatrix-server --lib transcript_activity`). Covers AC-05 (fold arithmetic), AC-08 (Copy/metadata-Debug/no-Display), AC-09 (single scan, multi-class, per-delta-not-per-occurrence), AC-11 (==16), AC-14 (u64 un-narrowed, saturating), empty scanner, non-UTF-8 no-panic, invalid-regex Err.

## Decisions / notes
- `regex` crate already a dependency (`regex = "1"`); `regex::bytes` is in the default feature set — no Cargo.toml change needed.
- Added module-level `#![allow(dead_code)]` with a "wired in crt-054 wave 2/3" comment — these `pub(crate)` types are consumed by later waves; expected dead-code flags this commit. Targeted, not blanket-suppressing real issues.
- Added `#[derive(Debug)]` to `SignatureScanner` (derived RegexSet debug = operator-trusted config patterns, never transcript bytes; content-opaque) — required so `compile()`'s `Result` is Debug-printable in the invalid-regex test.
- `cargo fmt` reordered the mod.rs declaration alphabetically (cosmetic).
- `build -p unimatrix-server` clean (warnings are pre-existing, unrelated crates/files).

## Issues / blockers
None.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-003 (#5028, Copy counter struct read surface), ADR-001 (#5026, fold inside TranscriptBuffer), ADR-005 envelope; all already reflected in pseudocode. context_search(pattern) returned no directly-applicable foundation pattern.
- Stored: nothing novel to store -- this is a straight 1:1 implementation of validated pseudocode (scalar fold types + RegexSet scanner). The one non-obvious item (use `regex::bytes::RegexSet` not `regex::RegexSet` for the byte domain) is already documented explicitly in the pseudocode and ADR-002; storing it would duplicate existing design knowledge rather than add a gotcha.
