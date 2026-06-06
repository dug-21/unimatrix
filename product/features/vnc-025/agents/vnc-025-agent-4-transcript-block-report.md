# Agent Report: vnc-025-agent-4-transcript-block

## Task
Implement the transcript-block component (ADR-005): shared extraction core
`uds/transcript_block.rs` moved from `hook.rs`, plus new
`extract_transcript_block_from_bytes` sharing a `block_from_lines` core.

## Files Created
- `crates/unimatrix-server/src/uds/transcript_block.rs` (420 lines) — moved
  verbatim: `ExchangeTurn`, `build_exchange_pairs`, `format_turn`,
  `extract_key_param`, `get_content_array`, `extract_tool_result_snippet`,
  `truncate_utf8` (pub(crate), hook re-imports), `MAX_PRECOMPACT_BYTES = 3000`
  and `TAIL_MULTIPLIER = 4` (pub, pinned), `TOOL_RESULT_SNIPPET_BYTES`,
  `TOOL_KEY_PARAM_BYTES`, `extract_transcript_block(path)` (pub),
  `prepend_transcript` (pub). New: private `block_from_lines` shared core
  (lines → turns → budget loop → header/body/footer) called by both
  front-ends — parity is literal, not parallel; new pub
  `extract_transcript_block_from_bytes` (lossy UTF-8 decode + line split +
  core).
- `crates/unimatrix-server/src/uds/transcript_block_tests.rs` (297 lines) —
  §1 R-14 inventory: all 22 pre-move test names + `make_jsonl_file` helper,
  bodies unmodified (imports aside; `MAX_INJECTION_BYTES` now imported from
  hook — assertion direction flipped per pseudocode, meaning unchanged).
- `crates/unimatrix-server/src/uds/transcript_block_tests_bytes.rs`
  (262 lines) — §2/§4 new tests: `test_constants_pinned`,
  `test_golden_parity_from_path_vs_streamed_from_bytes` (hard gate, #3426 —
  expected computed at test time from the path variant, actual streamed
  shuffled+duplicated through `TranscriptBuffer` → `contiguous_tail(12_000)`
  → `from_bytes`; byte-for-byte equality on a >12 KB fixture so the seek
  lands mid-line), `test_from_bytes_mid_line_tail_start` (path-variant
  comparison on same data), `test_from_bytes_empty_input_returns_none`,
  `test_from_bytes_all_malformed_returns_none`,
  `test_from_bytes_invalid_utf8_lossy_no_panic`,
  `test_from_bytes_hole_truncated_window_well_formed`,
  `test_from_bytes_respects_byte_budget`,
  `test_block_bounded_regardless_of_input_size` (R-13.1, 1 MiB adversarial,
  document-and-accept comment included), `test_block_structurally_wrapped`.

## Files Modified
- `crates/unimatrix-server/src/uds/hook.rs` — extraction internals removed
  (−656 lines, now 4183); call sites (SubagentStart query, PreCompact block,
  BriefingContent prepend) re-import via
  `use crate::uds::transcript_block::{extract_transcript_block, prepend_transcript, truncate_utf8};`
  bodies unchanged. `std::io` import narrowed to `Read` (stdin only).
- `crates/unimatrix-server/src/uds/mod.rs` — `pub mod transcript_block;`

NOT touched (per brief): listener.rs, session.rs, tools.rs, config.rs,
infra/session_transcript* (read-only use of `TranscriptBuffer` in the golden
parity test).

## Tests
- transcript_block module: 32/32 pass (22 moved + 10 new).
- R-14 inventory diff: zero dropped names (verified by comm against the
  pinned 22-name list); constants pinned 3000/4.
- hook.rs suite: 174/174 pass unmodified.
- Full workspace: 5555 passed, 0 failed.
- cargo fmt applied; clippy: no new warnings (the two `redundant_closure`
  lints at hook.rs:207/239 sit on call-site lines character-identical to
  baseline — pre-existing, left untouched for R-14 minimal diff).

## Content Opacity (AC-12)
No `tracing`/`println!`/`Display`/`Debug` in the new module touches input
bytes, lines, turns, or block content. `ExchangeTurn` deliberately does NOT
derive `Debug` (it carries raw transcript content); it is private and never
formatted except through `format_turn` into the intentionally
content-bearing block output, which flows only to `BriefingContent`.

## Issues / Notes for Tester (Stage 3c)
1. The R-14 "bodies unmodified" diff gate must tolerate `cargo fmt` reflow
   (one moved assertion was line-rejoined by fmt) in addition to `use`
   changes.
2. `make_jsonl_file` is `pub(super)` so the sibling `tests_bytes` module
   reuses it (500-line rule forced the test split; cumulative
   infrastructure, no duplication).
3. Empty-buffer byte-identity (R-09.3) is dispatch-wiring's side (AC-11
   prepend path); `prepend_transcript(None, content) == content` is pinned
   here by the moved `prepend_transcript_none_block_writes_briefing*` tests.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced ADR-005 (#4743,
  applied as the binding move/parity spec), #3331 (hook PreCompact pattern),
  ADR-002 (#4740, contiguous_tail trust contract). context_search for
  vnc-025 decisions confirmed ADR set.
- Stored: nothing novel to store — the implementation followed validated
  pseudocode exactly; the one subtle equivalence (path variant filters
  invalid-UTF-8 lines via BufRead Err, bytes variant via lossy-decode →
  JSON parse failure — same observable filtering) is already recorded in
  ADR-005 and the pseudocode, and is now pinned by the golden parity test.
