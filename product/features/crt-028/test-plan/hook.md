# crt-028 Test Plan: `uds/hook.rs`

Component covers: `extract_transcript_block`, `build_exchange_pairs`, `prepend_transcript`,
`extract_key_param`, `ExchangeTurn` enum, and the `MAX_PRECOMPACT_BYTES` / `TAIL_MULTIPLIER` /
`TOOL_RESULT_SNIPPET_BYTES` / `TOOL_KEY_PARAM_BYTES` constants.

All tests are `#[test]` (no async — hook.rs has no tokio runtime). Fixture JSONL files are written
to `tempfile::tempdir()` within the test (or use `std::env::temp_dir()` with a unique name).

Existing pattern: tests use `fn test_input() -> HookInput` and `fn test_entry(id, title, content)`.
New tests add a `fn make_jsonl_file(lines: &[&str]) -> (TempDir, PathBuf)` helper.

---

## Constants (AC-10)

### `max_precompact_bytes_constant_defined`
- Assert `MAX_PRECOMPACT_BYTES == 3000`
- Assert `MAX_PRECOMPACT_BYTES != MAX_INJECTION_BYTES` (distinct constants, D-4)
- Assert `TAIL_MULTIPLIER == 4`
- Assert `TOOL_RESULT_SNIPPET_BYTES == 300`
- Assert `TOOL_KEY_PARAM_BYTES == 120`
- **Rationale**: AC-10 compile-time constant check; catches aliasing bugs before they hit tests

---

## `extract_transcript_block` — R-01: Degradation (Critical gate)

### `extract_transcript_block_empty_path_returns_none` (R-01, AC-07, AC-15)
- Call `extract_transcript_block("")`
- Assert: returns `None`
- **Rationale**: Empty string path filtered by `.filter(|p| !p.is_empty())` — no file open attempted

### `extract_transcript_block_missing_file_returns_none` (R-01, AC-07, AC-15 — non-negotiable gate)
- Call `extract_transcript_block("/nonexistent/path/session.jsonl")`
- Assert: returns `None` (not `Err`, not panic)
- **Rationale**: SR-07 explicit test. The function must not propagate `std::io::Error`. This is the
  primary gate for the degradation contract (ADR-003).

### `prepend_transcript_none_block_writes_briefing` (R-01, AC-06)
- Call `prepend_transcript(None, "briefing content")`
- Assert: returns `"briefing content"` (verbatim)
- Assert: output does not contain `"=== Recent conversation"`
- **Rationale**: When transcript extraction fails (returns `None`), briefing must be passed through
  unchanged. This is the critical invariant from Lesson #699.

### `extract_transcript_block_all_malformed_lines_returns_none` (R-01, AC-08)
- Write temp file with lines: `["not json", "also not json", "{broken"]`
- Call `extract_transcript_block(path)`
- Assert: returns `None` (no parseable pairs found → AC-09 path)
- **Rationale**: Pure-failure JSONL must not produce output or panic

---

## `extract_transcript_block` — R-03: Seek Boundary (Critical gate)

### `extract_transcript_block_zero_byte_file_returns_none` (R-03 — non-negotiable gate)
- Write temp file with zero bytes
- Call `extract_transcript_block(path)`
- Assert: returns `None` without panic or error
- **Rationale**: `seek_back = window.min(0) = 0`; no seek issued; BufReader reads nothing; zero lines

### `extract_transcript_block_file_equals_window_reads_from_start` (R-03 — non-negotiable gate)
- Write temp file of exactly `MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER` bytes (12,000 bytes) of valid
  JSONL content containing one user/assistant exchange
- Call `extract_transcript_block(path)`
- Assert: returns `Some(_)` (full file read, exchange found)
- Assert: no first-line truncation artifact (content correct)
- **Rationale**: Boundary case `file_len == window`; must use `SeekFrom::Start(0)` path (not seek-from-end)

### `extract_transcript_block_file_one_byte_over_window_seeks` (R-03)
- Write temp file of `MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER + 1` bytes with one exchange in the last 100 bytes
- Call `extract_transcript_block(path)`
- Assert: returns `Some(_)` with the exchange present (seek-from-end path taken; first line discarded)
- **Rationale**: Boundary case `file_len == window + 1`; triggers seek path; verifies clamp works at N+1

### `extract_transcript_block_window_minus_one_reads_from_start` (R-03)
- Write temp file of `MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER - 1` bytes with one user/assistant exchange
- Call `extract_transcript_block(path)`
- Assert: returns `Some(_)` (read from start; all lines available)
- **Rationale**: `file_len < window`; no seek-from-end; entire file parsed

---

## `extract_transcript_block` — R-02: Tail Multiplier Sufficiency

### `extract_transcript_block_thinking_heavy_session_returns_some_or_none` (R-02)
- Construct JSONL where last 12,000 bytes contains: 3 assistant records with 3,900-byte `thinking`
  blocks each, followed by one normal user/assistant exchange of 200 bytes
- Call `extract_transcript_block(path)`
- Assert: returns `Some(_)` containing at least the one normal exchange (thinking blocks skipped)
- **Rationale**: Validates that thinking block skip (FR-03.7) leaves room in the window for real content

### `extract_transcript_block_file_just_over_window_seeks` (R-02)
- Construct JSONL file of 13,000 bytes: 1,000 bytes of old history (before window) + 12,000 bytes of
  tail containing valid exchanges
- Call `extract_transcript_block(path)`
- Assert: returns `Some(_)` with exchanges from the tail (not the truncated first line)
- **Rationale**: Seek path active; first partial line discarded; tail exchanges recovered

### `extract_transcript_block_system_only_returns_none` (AC-09)
- Write JSONL file with three `{"type": "system", "content": "..."}` lines only
- Call `extract_transcript_block(path)`
- Assert: returns `None` (no user/assistant pairs found → build_exchange_pairs returns empty)
- **Rationale**: AC-09 — no parseable pairs → None, not empty-string Some

---

## `extract_transcript_block` — Budget Enforcement (AC-05)

### `extract_transcript_block_respects_byte_budget` (AC-05, R-05)
- Construct JSONL with 20 user/assistant exchanges of 200 bytes each (total ~4,000 bytes extracted)
- Call `extract_transcript_block(path)`
- Assert: returned `String` byte length ≤ `MAX_PRECOMPACT_BYTES` (3,000)
- Assert: returned string is well-formed (begins with `"=== Recent conversation"`, ends with
  `"=== End recent conversation ==="`)
- **Rationale**: Core budget enforcement — partial pairs not emitted; budget respected exactly

### `extract_transcript_block_budget_exhausted_most_recent_kept` (R-05, AC-02)
- Construct JSONL with exchanges A (old), B (middle), C (most recent), where A + B + C would exceed
  budget but C alone fits
- Call `extract_transcript_block(path)`
- Assert: returned string contains C's content but not A's content
- **Rationale**: Priority ordering — most-recent fills first; oldest dropped when budget exhausted

---

## `build_exchange_pairs` — R-05: Reversal Order

### `build_exchange_pairs_three_exchanges_most_recent_first` (R-05, AC-02 — non-negotiable gate for R-10)
- Input: lines representing exchanges [User:A, Assistant:A], [User:B, Assistant:B], [User:C, Assistant:C]
  in JSONL document order (A oldest, C newest)
- Call `build_exchange_pairs(&lines)`
- Assert: result[0] is `UserText("C")` or `AssistantText("C")` (most recent first)
- Assert: result ordering is C before B before A
- **Rationale**: FR-02.2, AC-02 — reversal is fundamental to budget-priority model

### `build_exchange_pairs_single_exchange_no_reversal_artifact` (R-05)
- Input: one user/assistant exchange
- Call `build_exchange_pairs(&lines)`
- Assert: returns exactly the expected turns (no duplicate, no off-by-one from reversal)
- **Rationale**: Single-element reversal must not produce artifacts

---

## `build_exchange_pairs` — R-04: Adjacent-Record Pairing

### `build_exchange_pairs_tool_pair_format_and_truncation` (AC-03)
- Input: assistant turn with `tool_use` (name: "Read", input: {file_path: "/foo/bar.rs"}),
  followed by user turn with `tool_result` (tool_use_id matching, content: 2,000-byte string)
- Call `build_exchange_pairs(&lines)`
- Find the `ToolPair` entry
- Assert: `name == "Read"`, `key_param == "/foo/bar.rs"`
- Assert: `result_snippet.len() <= 300` (TOOL_RESULT_SNIPPET_BYTES)
- Assert: snippet is valid UTF-8 (no mid-codepoint truncation)
- **Rationale**: AC-03 core contract

### `build_exchange_pairs_system_record_between_tool_use_and_result` (R-04)
- Input: assistant with `tool_use` → system record → user with matching `tool_result`
- Call `build_exchange_pairs(&lines)`
- Find the `ToolPair`
- Assert: `result_snippet == ""` (no result matched — non-adjacent user record)
- **Rationale**: ADR-002 adjacent-record scan; only immediate next record is checked

### `build_exchange_pairs_back_to_back_assistant_no_result` (R-04)
- Input: assistant with `tool_use` → another assistant record (no intervening user)
- Call `build_exchange_pairs(&lines)`
- Find the `ToolPair`
- Assert: `result_snippet == ""` (unmatched tool_use emits empty snippet per FR-03.8)
- **Rationale**: ADR-002 — unmatched tool_use → empty snippet, not dropped

### `build_exchange_pairs_orphaned_tool_result_skipped` (R-04)
- Input: user record containing only `type:"tool_result"` content blocks (no preceding assistant)
- Call `build_exchange_pairs(&lines)`
- Assert: no `UserText` emitted for this record (tool_result not extracted as user text per AC-04, FR-03.6)
- Assert: no `ToolPair` emitted (no corresponding tool_use in scope)
- **Rationale**: AC-04 — tool_result in user turns is not user text; orphaned result is silent skip

### `build_exchange_pairs_multiple_tool_uses_in_one_turn` (R-04)
- Input: assistant turn with two `tool_use` blocks, user turn with matching `tool_result` blocks for both
- Call `build_exchange_pairs(&lines)`
- Assert: two `ToolPair` entries emitted (one per tool_use)
- Assert: each pair has correct name and snippet from the matched result
- **Rationale**: ARCHITECTURE.md — multiple tool_uses in one turn all matched in single pass

### `build_exchange_pairs_user_tool_result_skipped` (AC-04)
- Input: user record with only `type:"tool_result"` content (no `type:"text"` content)
- Call `build_exchange_pairs(&lines)`
- Assert: no `UserText` entry for this record
- **Rationale**: FR-03.6 — tool_result in user turns is not extracted as user text

### `build_exchange_pairs_malformed_lines_skipped` (AC-08)
- Input: mix of valid JSON JSONL records and invalid lines (`"not json"`, `""`, `"{truncated"`)
- Call `build_exchange_pairs(&lines)`
- Assert: does not panic
- Assert: parseable user/assistant records are returned; invalid lines silently skipped
- **Rationale**: AC-08 fail-open contract; SR-01

### `build_exchange_pairs_user_text_budget_fill_utf8_boundary` (R-06)
- Input: user record with text content consisting of 2,990 bytes of ASCII followed by a 4-byte
  CJK codepoint (total 2,994 bytes)
- Run through `extract_transcript_block` with a budget of 3,000 bytes
- Assert: output byte length ≤ 3,000
- Assert: output is valid UTF-8 (not truncated mid-codepoint)
- **Rationale**: R-06 — budget-fill truncation must use `truncate_utf8`; multi-byte boundary

---

## `build_exchange_pairs` — R-10: OQ-SPEC-1 (Non-negotiable gate)

### `build_exchange_pairs_tool_only_assistant_turn_emits_pairs` (R-10 — non-negotiable gate)
- Input: assistant turn with `tool_use` block + `thinking` block, NO `type:"text"` block;
  followed by user turn with matching `tool_result`
- Call `build_exchange_pairs(&lines)`
- Assert: at least one `ToolPair` entry present in result
- Assert: NO `AssistantText` entry for this turn (OQ-SPEC-1: `[Assistant]` header line omitted when no text blocks)
- **Rationale**: OQ-SPEC-1 resolution — tool-only turns emit tool pairs, not suppressed entirely

### `build_exchange_pairs_thinking_only_turn_suppressed` (R-10 — non-negotiable gate)
- Input: assistant turn with ONLY a `type:"thinking"` block (no text, no tool_use)
- Call `build_exchange_pairs(&lines)`
- Assert: no `AssistantText` and no `ToolPair` entries for this turn (suppressed entirely)
- **Rationale**: OQ-SPEC-1 — pure-thinking turn carries no actionable content; suppressed

### `build_exchange_pairs_all_tool_call_session_emits_pairs` (R-10)
- Input: three assistant turns, all with only `tool_use` blocks (no text), each followed by user
  turn with matching `tool_result`
- Call `build_exchange_pairs(&lines)`
- Assert: `ToolPair` entries present (not zero)
- **Rationale**: R-10 scenario 3 — autonomous delivery session with all tool-call turns must emit pairs

---

## `extract_key_param` — R-09

### `extract_key_param_known_tools_correct_field` (R-09)
- For each known tool: `Bash → command`, `Read → file_path`, `Edit → file_path`, `Write → file_path`,
  `Glob → pattern`, `Grep → pattern`, `MultiEdit → file_path`, `Task → description`,
  `WebFetch → url`, `WebSearch → query`
- Call `extract_key_param(tool_name, &input)` with input containing the expected field
- Assert: returned string matches the expected field value
- **Rationale**: Key-param map is hard-coded; all 10 entries must be correct

### `extract_key_param_unknown_tool_first_string_field_fallback` (R-09)
- Call `extract_key_param("UnknownTool", &json!({"api_key": "sk-xxx", "query": "foo"}))`
- Assert: returns `"sk-xxx"` (first string field in iteration order)
- Assert: documented test comment notes this is a known limitation (no denylist yet)
- **Rationale**: R-09 — fallback returns first string field; sensitive fields not filtered (known gap)

### `extract_key_param_unknown_tool_long_first_string_truncated` (R-09, R-06)
- Call `extract_key_param("UnknownTool", &json!({"field": "x".repeat(5000)}))`
- Assert: returned string byte length ≤ 120 (`TOOL_KEY_PARAM_BYTES`)
- Assert: returned string is valid UTF-8
- **Rationale**: Fallback truncation at 120 bytes; R-06 UTF-8 boundary

### `extract_key_param_snippet_truncated_at_utf8_boundary` (R-06)
- Construct tool result content: 299 bytes of ASCII + one 4-byte CJK character
- Via `build_exchange_pairs` with matching tool_use/tool_result
- Assert: `result_snippet` byte length ≤ 300
- Assert: `result_snippet` is valid UTF-8 (not cut mid-character)
- **Rationale**: R-06 per-snippet truncation site at 300 bytes must use `truncate_utf8`

### `extract_key_param_no_string_field_returns_empty` (R-09)
- Call `extract_key_param("UnknownTool", &json!({"count": 5, "flag": true}))`
- Assert: returns `""`
- **Rationale**: When no string field exists in input, empty string returned (not panic)

### `extract_key_param_key_param_truncated_at_utf8_boundary` (R-06)
- Call `extract_key_param("Read", &json!({"file_path": "a".repeat(100) + "\u{4e2d}\u{6587}"}))`
  (100 bytes ASCII + 6 bytes CJK = 106 bytes total, within 120 limit — no truncation needed)
- Also test with 115 bytes of ASCII + 4-byte CJK (119 < 120 — within budget)
- Also test with 119 bytes of ASCII + 4-byte CJK (123 > 120 — must truncate to 119, not 120 mid-char)
- Assert: returned string byte length ≤ 120 and is valid UTF-8
- **Rationale**: R-06 key-param truncation site at 120 bytes must respect UTF-8 boundaries

---

## `prepend_transcript` — R-12, AC-01, AC-06

### `prepend_transcript_both_present_transcript_precedes_briefing` (R-12, AC-01)
- Call `prepend_transcript(Some("=== Recent conversation ===\n[User] foo\n=== End recent conversation ==="), "briefing")`
- Assert: output starts with `"=== Recent conversation"`
- Assert: output contains `"briefing"` after the transcript section
- Assert: transcript footer appears before briefing content
- **Rationale**: AC-01 — transcript precedes briefing (D-5)

### `prepend_transcript_both_present_separator_present` (R-12)
- Call `prepend_transcript(Some("block"), "briefing")`
- Assert: output is `"block\nbriefing"` (single newline separator per FR-05.1)
- **Rationale**: Exact separator format; `\n` between blocks

### `prepend_transcript_transcript_only_has_headers` (R-12)
- Call `prepend_transcript(Some("=== Recent conversation ===\n[User] foo\n=== End recent conversation ==="), "")`
- Assert: output == `"=== Recent conversation ===\n[User] foo\n=== End recent conversation ==="`
  (transcript verbatim, no extra separator)
- Assert: output starts with `"=== Recent conversation"` (header present)
- Assert: output ends with `"=== End recent conversation ==="` (footer present)
- **Rationale**: R-12 scenario 1; SR-04 — transcript-only output includes section header

### `prepend_transcript_both_none_empty_string` (R-12)
- Call `prepend_transcript(None, "")`
- Assert: returns `""`
- **Rationale**: R-12 scenario 2; FR-01.4 invariant — empty stdout when nothing to write

### `prepend_transcript_none_block_writes_briefing` (R-12, AC-06)
- (Also listed under R-01 — dual coverage)
- Call `prepend_transcript(None, "briefing content")`
- Assert: returns `"briefing content"` verbatim
- Assert: no `"=== Recent conversation"` in output
- **Rationale**: R-12 scenario 4; FR-05.2

---

## R-11: Path Outside Expected Directory

### `extract_transcript_block_non_jsonl_path_returns_none` (R-11)
- Call `extract_transcript_block("/etc/passwd")` (or any readable non-JSONL file)
- Assert: returns `None` (no valid user/assistant pairs in non-JSONL content)
- Assert: no panic
- **Rationale**: R-11 — non-JSONL content at arbitrary paths → fail-open → None; no exfiltration

---

## Mixed JSONL Fixture Tests (AC-08 integration within unit)

### `extract_transcript_block_mixed_valid_invalid_jsonl` (AC-08)
- Write temp JSONL file with: 2 valid user/assistant exchanges + 3 malformed lines interspersed
- Call `extract_transcript_block(path)`
- Assert: returns `Some(_)` (valid exchanges extracted despite malformed lines)
- Assert: malformed lines absent from output (not panic-causing)
- **Rationale**: AC-08 requires both skip behavior AND usable result from remaining good lines

---

## Existing Tests (AC-14 regression non-regression)

All existing hook.rs tests must continue to pass. The new functions are purely additive.
The `write_stdout` function is not modified for non-PreCompact events (FR-05.4 invariant).
Verify with: `cargo test -p unimatrix-server -- hook::tests 2>&1 | tail -30`

Key existing tests to verify are unaffected:
- `write_stdout_plain_text_no_json_envelope`
- `write_stdout_subagent_inject_valid_json_envelope`
- `build_request_userpromptsub_*` suite
- `min_query_words_constant_is_five`
