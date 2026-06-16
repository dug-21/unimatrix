# Agent Report — crt-054-agent-5-fold-wiring

**Wave**: Stage 3b — Surface B in-memory fold wiring (Components 3, 4, 5, 10; Component 9 validate call).
**Outcome**: COMPLETE. The fold is wired through the buffer lifecycle and startup; the crate compiles and all component-level tests pass.

## Files modified (production)
- `crates/unimatrix-server/src/infra/session_transcript.rs` — Components 3+4: `activity`/`scanner` fields on `TranscriptBuffer`; `new(max_bytes, scanner)`; fold call after merge on the accepted path (zero-length folds count-only; clipped/overflow not folded); `clear()` preserves the accumulator (ADR-006 comment); `pub(crate) activity_snapshot()`; Debug extended with activity scalars.
- `crates/unimatrix-server/src/infra/session.rs` — Component 3 threading (`signature_scanner` field + `with_signature_scanner` builder, threaded into both registered-route `TranscriptBuffer::new` sites); Component 5 collector `activity_snapshots_for_feature` (exact mirror of `take_transcripts_for_feature`); Component 10 accessor `has_transcript_hold`.
- `crates/unimatrix-server/src/main.rs` — Component 10: `build_signature_scanner` (calls `transcript_signals.validate()` then `SignatureScanner::compile`) + `assert_wave_b_precondition` helpers; both called on BOTH construction paths (daemon ~750, mirror ~1290); scanner threaded into the registry via `with_signature_scanner` on both paths.
- `crates/unimatrix-server/src/infra/transcript_activity.rs` — visibility widening only (no logic): `SignatureScanner`/`ScannerError`/`ActivitySnapshot` (+fields/ctors) `pub(crate)`→`pub` so the bin (main.rs, separate crate) can build/pass the scanner; removed the now-obsolete `#![allow(dead_code)]` (types are consumed).
- `crates/unimatrix-server/src/infra/mod.rs` — `pub(crate) mod transcript_activity` → `pub mod` (bin reachability).
- `crates/unimatrix-server/src/services/index_briefing.rs` — test-only call site updated to pass an empty scanner.

**transcript_hold.rs production code unchanged by design**: it never constructs `TranscriptBuffer::new` — it receives the buffer `Arc` from the registry at `hold_on_drain`, so the held route inherits the registry's scanner + embedded accumulator by construction (ADR-001). Only its TEST file was touched.

## Files modified (tests)
- `session_transcript_tests.rs` (+ `test_scanner()` helper, fold/clipped/zero-length/clear tests), `session_transcript_tests_overflow.rs`, `session_transcript_tests_snapshot.rs` (snapshot shape/Copy/Debug tests), `transcript_hold_tests.rs`, `uds/transcript_block_tests_bytes.rs`, `main_tests.rs` (wave-b precondition tests), and the `session.rs` test module (collector + has_transcript_hold tests).

## Construction sites updated: 51 total
- Production (3): `session.rs` register_session ×2; main.rs threads the scanner into both registry builds.
- Tests (48): `session_transcript_tests.rs` ×15, `_overflow.rs` ×23, `_snapshot.rs` ×6, `session.rs` test mod ×2, `transcript_hold_tests.rs` ×2, `index_briefing.rs` ×1 (test), `transcript_block_tests_bytes.rs` ×1.
- 100% of `TranscriptBuffer::new` call sites in the crate now pass a scanner; crate compiles.

## Tests: pass / fail
- `cargo build -p unimatrix-server`: 0 errors.
- `cargo test -p unimatrix-server --lib`: **4103 passed, 0 failed**, 1 ignored.
- `cargo test -p unimatrix-server --bin unimatrix`: **62 passed, 0 failed** (incl. 3 wave-b precondition tests).
- New crt-054 component tests all green: registered-route fold (AC-05), class-count fold, clipped-not-folded, overflow-not-folded, zero-length count-only, clear() preserves accumulator, fold-continues-after-clear; ActivitySnapshot Copy/shape/metadata-Debug; collector registered∪held + Arc dedup + feature filter + undeclared-no-entry (AC-12) + absence-vs-measured-zero; has_transcript_hold true/false; wave-b passes-when-wired / Err-unwired / Err-disabled.
- clippy on `--lib --bin unimatrix`: no warnings on any crt-054 symbol.

## Stage 3c basis (not implemented here, by instruction)
Held-route believable-zero guard (AC-06) and read-before-purge (AC-07) are Stage 3c integration tests. The structural basis they need is in place and verified: single embedded accumulator in `TranscriptBuffer`, fold on the accepted path (both routes share the same `Arc`), `clear()` preserves the accumulator, and the collector mirrors `take_transcripts_for_feature` verbatim (registered∪held, Arc dedup).

## Process notes
- Did NOT run `cargo fmt`; did NOT `git add`/`commit` (left in working tree for the Delivery Leader).
- No forbidden files touched (eval/runner/sweep_tests.rs, projects/tests.rs, tests/project_routing_integration.rs are unmodified).
- The `cargo error.rs` startup error type: used the existing `ServerError::Config(String)` convention (matching `InferencePoolInit`'s map pattern) rather than adding new `StartupError` enum variants — keeps the change within scope and out of `error.rs`. The pseudocode permits folding into the existing startup/config error enum.

## Issues / blockers
None. One transient `cc` linker failure occurred when building ALL integration-test binaries at once (memory pressure under `--no-run`); per-crate `--lib`/`--bin` runs link and pass cleanly. Not a code defect.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision, topic crt-054) — surfaced ADR-001/004/006/009/010 (#5026/#5031/#5033/#5034) and the vnc-025 transcript-buffer content-opacity pattern (#4740, #4860); applied the metadata-only Debug + embedded-accumulator-both-routes posture.
- Stored: entry #5056 "Wiring a pub(crate) infra type across the server lib/bin boundary requires widening to pub" via /uni-store-pattern (topic unimatrix-server). This is the non-obvious trap this wave hit: a Wave-1 `pub(crate)` type compiled fine in the lib but broke the bin once main.rs needed to construct/pass it.
