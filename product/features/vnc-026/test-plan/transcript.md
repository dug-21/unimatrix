# Test Plan: transcript.js (JSONL tail-parse)

Oracle: `transcript_block.rs` (entire module — `build_exchange_pairs`, `format_turn`,
`block_from_lines`, `truncate_utf8`). Constants: tail window 12,000 B; MAX_PRECOMPACT_BYTES 3000.
Risk: R-01 scenario 2 (Critical). Suite: `test/hook-client/transcript.test.js` + corpus cases
with `transcript.jsonl` fixtures (goldens authoritative).

## Parity Cases (corpus-backed; SubagentStart query derivation, RQ-6)

- `tail_nominal` — well-formed JSONL larger than 12,000 B → window is the last 12,000 bytes; first partial line discarded (window starting mid-line).
- `tail_malformed_lines` — interleaved unparseable JSONL lines skipped exactly as Rust skips them; derivation continues.
- `tail_multibyte_split_at_window_edge` — multi-byte char split at the 12,000-byte boundary → byte-safe handling identical to Rust (no replacement-char divergence in derived query).
- `tail_thinking_only_turns` — thinking-turn suppression matches Rust (`build_exchange_pairs`); thinking-only transcript → fallback behavior per golden.
- `tail_tool_use_result_pairing` — adjacent tool_use/tool_result records paired; orphaned tool_result unpaired per Rust.
- `tail_missing_file` — `transcript_path` points at nonexistent file → no throw; fallback (RecordEvent) per golden.
- `tail_empty_transcript_path` — `""` → no read attempted (fs spy), fallback per golden.
- `tail_file_smaller_than_window` — whole file read; no negative seek.
- `tail_zero_length_file` — empty file → fallback, no throw.
- `tail_directory_path` — `transcript_path` is a directory → read error swallowed, fallback.

## Unit Tests (locality)

### truncate_utf8 (byte-boundary-safe)
- `test_truncate_at_exact_boundary` — limit lands between chars → identity truncation.
- `test_truncate_mid_2byte / mid_3byte / mid_4byte` — backs off to last complete char; result length ≤ limit; valid UTF-8; equals Rust golden for the same input.
- `test_truncate_limit_zero_and_tiny` — 0/1/2/3-byte limits on multi-byte content → empty or valid prefix, never throws.

### MAX_PRECOMPACT_BYTES = 3000
- `test_block_capped_at_3000_bytes` — derived block ≤3000 bytes, truncated byte-safely; golden-compared.

### Window mechanics
- `test_window_exactly_12000` / `test_window_12001` — inclusion boundary matches Rust.
- `test_read_is_single_tail_read` — fs spy: exactly one open+read of the tail region (sync-path budget, C-03 — this is the ONLY permitted sync-spawn file I/O besides config).

## Concrete Assertions

- `deriveQuery(transcriptPath, agentType) -> string|null` pure aside from the single tail read; null → caller falls back to RecordEvent (build-request.md).
- For every corpus case with a `transcript.jsonl`, the derived ContextSearch `query`/`role`/`source`
  fields structurally equal `expected-request.json`.
