# Agent Report — C2 Snapshot Types & Primitive (crt-052)

Agent: crt-052-agent-3-snapshot-types | Wave A

## Summary

Implemented `TranscriptSnapshot`, `HoleInfo`, and the `snapshot()` /
`snapshot_block()` primitive on `TranscriptBuffer`, plus full unit coverage.
Naming pin honored (`TranscriptSnapshot`, not `SessionTranscriptSnapshot`).
Zero compile-time reference to `transcript_hold.rs` (R-11). All files <500 lines.

## Files Modified

- `crates/unimatrix-server/src/infra/session_transcript.rs` — added `HoleInfo`
  (`derive(Debug)`, offsets only), `TranscriptSnapshot` (manual metadata-only
  `Debug`), `snapshot()`, private `snapshot_block(start_rel)` + `contiguous_run_start_rel()`;
  refactored `contiguous_tail` to route its copy through `snapshot_block` (single
  span-copy path → single-reader invariant by construction). 469 lines.
- `crates/unimatrix-server/src/infra/session_transcript_tests.rs` — registered
  `snapshot` test submodule. 417 lines.
- `crates/unimatrix-server/src/infra/session_transcript_tests_overflow.rs` —
  overflow/poison snapshot tests (§5). 455 lines.
- `crates/unimatrix-server/src/infra/session_transcript_tests_snapshot.rs` — NEW.
  Below-cap correctness + Debug content-opacity tests. 178 lines.

## Tests

`cargo test -p unimatrix-server --lib infra::session_transcript`: **47 passed, 0 failed**
(12 new C2 tests + poison recovery). Full lib suite: **3686 passed, 0 failed, 1 ignored**.

Coverage vs test plan: contiguous-span no-hole-cross, metadata match, whole-span
(not windowed), empty buffer, truncated tail, all-four-fields, #700 reuse proof,
Debug metadata-only, HoleInfo Debug, base_offset advance under overflow, high_water
survives clipping, holes reported, exact-cap boundary, 4 MiB copy <50ms, poison
treat-as-empty + clear_poison + post-recovery write survives (#4748).

`cargo fmt` applied. `cargo clippy` clean on these files.

## Design Notes

- `snapshot()` is `&self`, does NO locking, NO parse, NO I/O (AC-01). The seam
  caller (C1) holds the buffer lock and does poison recovery; the poison test here
  mirrors that lock-acquisition pattern.
- The "contiguous readable span" = bytes from the post-hole floor to span end
  (matches `contiguous_tail`'s tail-floor logic), never crossing a hole, no zero-fill.
- `TranscriptSnapshot` has a hand-written metadata-only `Debug` (no `derive`) —
  AC-06 leak gate concern. `HoleInfo` derives `Debug` (offsets only).

## Issues / Blockers

- None for C2. Note: a transient build break during work was from a parallel
  agent's in-flight `config.rs` (C9) edit (missing default fns), which resolved
  when their edit completed — not my scope.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing + context_search — surfaced ADR-002
  (#4848, snapshot shape / single reader), poison-recovery pattern (#4748 pair
  clear_poison with into_inner), #4764 (treat-as-empty recovery). Applied all.
- Stored: entry #4860 "Route both TranscriptBuffer content readers through one
  private snapshot_block primitive; metadata-only Debug on content-bearing types"
  via /uni-store-pattern.
