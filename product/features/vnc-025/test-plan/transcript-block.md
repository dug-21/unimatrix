# Test Plan: transcript-block (`uds/transcript_block.rs`)

Covers R-14, R-09 (extraction arms), R-13; AC-11 (golden side), AC-12 (module arm).
The move out of `hook.rs` touches the ONLY production-live transcript path today — the
pre/post inventory below is the regression contract.

## §1 Move fidelity — R-14 (hard gate)

**Pre-move test-name inventory** (captured 2026-06-05 from `hook.rs` test module; these names
MUST all exist and pass after the move — imports aside, bodies unmodified):

```
max_precompact_bytes_constant_defined
extract_transcript_block_empty_path_returns_none
extract_transcript_block_missing_file_returns_none
extract_transcript_block_all_malformed_lines_returns_none
extract_transcript_block_zero_byte_file_returns_none
extract_transcript_block_respects_byte_budget
extract_transcript_block_system_only_returns_none
build_exchange_pairs_three_exchanges_most_recent_first
build_exchange_pairs_user_tool_result_skipped
build_exchange_pairs_tool_only_assistant_turn_emits_pairs
build_exchange_pairs_thinking_only_turn_suppressed
build_exchange_pairs_malformed_lines_skipped
extract_key_param_known_tools_correct_field
extract_key_param_unknown_tool_first_string_field_fallback
extract_key_param_no_string_field_returns_empty
extract_key_param_long_value_truncated
prepend_transcript_none_block_writes_briefing
prepend_transcript_none_block_writes_briefing_verbatim
prepend_transcript_both_present_separator_present
prepend_transcript_both_present_transcript_precedes_briefing
prepend_transcript_transcript_only_has_headers
prepend_transcript_both_none_empty_string
```

(helper `make_jsonl_file` moves with them.)

- Stage 3c verification: `cargo test -p unimatrix-server <name>` inventory diff — zero
  dropped names, zero modified bodies (allow `use` line changes only in the diff).
- `test_constants_pinned` — in the NEW module: `assert_eq!(MAX_PRECOMPACT_BYTES, 3000)`,
  `assert_eq!(TAIL_MULTIPLIER, 4)` (a transposed constant changes hook and server silently).
- Hook call sites (`hook.rs:220/:252/:295`) re-import; the rest of the `hook.rs` suite
  (~68 tests) passes unmodified.

## §2 `from_bytes` + golden parity — R-09.1/.2/.3, AC-11

- `test_golden_parity_from_path_vs_streamed_from_bytes` (hard gate, #3426): fixture JSONL
  transcript file. Expected = `extract_transcript_block(path)` computed at test time — NO
  hand-written or checked-in expectation. Actual = read the same file bytes, split into
  deltas, apply shuffled + duplicated through a `TranscriptBuffer`, then
  `extract_transcript_block_from_bytes(&contiguous_tail(MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER))`.
  Assert byte-for-byte equality.
- `test_from_bytes_mid_line_tail_start` — feed bytes whose first line is partial (as a
  12 KB-window tail will be); assert the partial line is filtered identically to the path
  variant's mid-line seek behavior (construct the path-variant comparison on the same data).
- `test_from_bytes_empty_input_returns_none` and `test_from_bytes_all_malformed_returns_none`
  — mirror the path-variant None cases.
- `test_from_bytes_hole_truncated_window_well_formed` — input shorter than 12 KB (hole inside
  the last 12 KB upstream produces a short tail): block well-formed, never includes pre-hole
  bytes (the buffer guarantees that; here assert short input alone still yields a valid block
  or None — no panic, no garbage).
- `test_from_bytes_respects_byte_budget` — output ≤ `MAX_PRECOMPACT_BYTES` + header/footer
  framing, same budget rule as the path variant.

## §3 Prepend behavior

Covered by the moved `prepend_transcript_*` inventory in §1 — no new cases needed; the
server-side prepend ordering (before `token_count`) is dispatch-wiring.md §3.

## §4 Prompt-injection bound — R-13

- `test_block_bounded_regardless_of_input_size` — adversarial 1 MiB single-"turn" input:
  output ≤ budget; attacker cannot inflate the block (R-13.1).
- `test_block_structurally_wrapped` — output carries the same header/footer framing as the
  local hook (structural wrapping is the only mitigation in scope).
- Document-and-accept (in the test as a comment + in RISK-COVERAGE-REPORT): content is
  untrusted by design — identical exposure to the local hook reading a local file. No
  sanitization is in scope (like-for-like). Recorded so acceptance is deliberate.

## §5 Content opacity — AC-12 module arm

Static gate: no `tracing` call in this module logs line/turn/block content; no `Display`
over raw input; errors carry no input bytes. (The block OUTPUT is intentionally
content-bearing — it flows only to `BriefingContent`, never to logs.)
