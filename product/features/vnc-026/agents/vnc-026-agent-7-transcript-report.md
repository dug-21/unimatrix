# Agent Report — vnc-026-agent-7-transcript

Component: transcript (JSONL tail-parse, RQ-6) | Commit: 06d5f9d9

## Files

- `packages/unimatrix/lib/hook-client/transcript.js` (created, 349 lines)
- `packages/unimatrix/test/hook-client/transcript.test.js` (created, 33 tests)

## Implementation

Exact port of `uds/transcript_block.rs` path-variant: `extractTranscriptBlock(path) -> string|null`,
`truncateUtf8(s, maxBytes)` (exported for build-request.js / delta.js). Constants pinned:
MAX_PRECOMPACT_BYTES=3000, TAIL_MULTIPLIER=4 (12,000-byte window), TOOL_RESULT_SNIPPET_BYTES=300,
TOOL_KEY_PARAM_BYTES=120, 10-tool KEY_PARAM_FIELDS map. All byte budgets via `Buffer.byteLength`.
Single open + single positioned tail read (C-03). Null on any failure; empty/non-string path
short-circuits with no fs call. `splitLinesLikeBufRead` implements BufRead::lines() parity
(byte-level split, UTF-8 round-trip drop, \r stripped only on \n-terminated lines).

## Tests

- Component suite: **33/33 pass** (`node --test test/hook-client/transcript.test.js`).
- Full package suite: 222 pass / 6 fail — all 6 failures pre-existing in `mergeSettings`/
  `writeMcpJson` suites (committed code untouched by this agent; `LD_LIBRARY_PATH` prefix
  expectation mismatch in init command strings). Not introduced by this work.
- Covers all test-plan items: window 12000/12001 inclusion boundary, multibyte split at window
  edge (no U+FFFD divergence), malformed/invalid-UTF-8 line drops, thinking-only suppression,
  tool_use/tool_result pairing (string + array shapes, missing tool_use_id, non-adjacent
  unpaired), key-param fallback, budget break-not-continue, zero-fitting-turns → null,
  exchange count = user turns only, byte-not-char budget, fs-spy single-read + empty-path
  no-read, missing file/dir/zero-length/whitespace degradation.
- Corpus-backed golden comparison deferred to the Layer 1 parity suite (parity-corpus agent).

## Issues / Blockers

None. One pseudocode nuance resolved in the oracle's favor: pseudocode said "strip one trailing
0x0D per line"; Rust read_line strips \r only when \n was popped first — implemented Rust
behavior (the pseudocode's stated intent was "BufRead::lines() parity").

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — surfaced #3348 (tail-bytes seek_back=min(window,file_len) pattern, applied) and vnc-025/vnc-026 ADRs; context_search for prior line-split parity patterns: no match.
- Stored: entry #4767 "Porting Rust BufRead::lines() to JS: split bytes before decode, drop lines failing UTF-8 round-trip, strip \r only on newline-terminated lines" via /uni-store-pattern.
