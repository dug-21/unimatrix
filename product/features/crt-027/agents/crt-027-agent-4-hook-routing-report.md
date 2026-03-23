# Agent Report: crt-027-agent-4-hook-routing

## Task

Implement hook.rs routing changes for crt-027 WA-4a (Proactive Knowledge Delivery).

## Files Modified

- `crates/unimatrix-server/src/uds/hook.rs`

## Changes Implemented

1. **`MIN_QUERY_WORDS: usize = 5`** constant added at module level alongside `HOOK_TIMEOUT` and `MAX_INJECTION_BYTES`.

2. **`write_stdout_subagent_inject(entries_text: &str) -> io::Result<()>`** added alongside existing `write_stdout`. Writes the `hookSpecificOutput` JSON envelope using `serde_json::json!` and `stdout().lock()`.

3. **`write_stdout_subagent_inject_response(response: &HookResponse) -> Result<(), Box<dyn std::error::Error>>`** added as dispatch helper. Routes `HookResponse::Entries` to the JSON envelope path; non-Entries falls back to plain-text `write_stdout` for graceful degradation (FR-06, C-01).

4. **`run()` updated** to extract `req_source: Option<String>` from `HookRequest::ContextSearch { source, .. }` before consuming the request. After `transport.request()`, branches on `req_source.as_deref() == Some("SubagentStart")` to call `write_stdout_subagent_inject_response` vs `write_stdout`.

5. **`SubagentStart` arm added** in `build_request` before the `_ =>` fallthrough. Extracts `prompt_snippet` from `input.extra`, applies `.trim().is_empty()` guard (EC-01), routes to `HookRequest::ContextSearch { source: Some("SubagentStart"), session_id: input.session_id.clone(), ... }`. No `MIN_QUERY_WORDS` guard on SubagentStart per ADR-002.

6. **`UserPromptSubmit` arm updated**: replaced `query.is_empty()` with `query.trim().is_empty()` (Guard 1) and added `query.split_whitespace().count() < MIN_QUERY_WORDS` (Guard 2). Query value itself is not trimmed.

7. **Existing tests updated** to use ≥5-word prompts where they previously used short strings ("query", "search query", single word "x" repeat) that would now fail the word-count guard. Four tests updated: `build_request_user_prompt_submit_with_prompt`, `build_request_user_prompt_submit_long_prompt`, `build_request_user_prompt_passes_session_id`, `build_request_user_prompt_no_session_id`.

## Tests

**137 passed / 0 failed** in `cargo test -p unimatrix-server --lib -- uds::hook`.

20 new tests added covering all mandatory test names from the test plan:

| Test | AC |
|------|-----|
| `build_request_subagentstart_with_prompt_snippet` | AC-01 |
| `build_request_subagentstart_empty_prompt_snippet` | AC-02 (a+b) |
| `build_request_subagentstart_session_id_from_input` | AC-03 |
| `build_request_subagentstart_one_word_routes_to_context_search` | AC-23 |
| `build_request_subagentstart_whitespace_only_prompt_snippet` | AC-23b / EC-01 |
| `build_request_subagentstart_null_prompt_snippet_record_event` | EC-02 |
| `build_request_userpromptsub_four_words_record_event` | AC-22 |
| `build_request_userpromptsub_five_words_context_search` | AC-22 |
| `build_request_userpromptsub_six_words_context_search` | AC-02b |
| `build_request_userpromptsub_one_word_record_event` | AC-02b |
| `build_request_userpromptsub_three_words_record_event` | AC-02b |
| `build_request_userpromptsub_whitespace_padded_one_word` | AC-23c |
| `build_request_userpromptsub_source_is_none` | AC-05 |
| `write_stdout_subagent_inject_valid_json_envelope` | AC-SR02 |
| `write_stdout_plain_text_no_json_envelope` | AC-SR03 |
| `write_stdout_subagent_inject_returns_ok` | AC-SR01 (smoke) |
| `write_stdout_subagent_inject_response_entries_returns_ok` | AC-SR01 |
| `write_stdout_subagent_inject_response_empty_entries_returns_ok` | AC-SR01 |
| `min_query_words_constant_is_five` | constant value guard |

Note: AC-SR01 (SubagentStart actually injects at runtime) is confirmed via documentation review per ADR-006 but cannot be asserted as a unit test without a live Claude Code process.

`cargo build --workspace` passes (zero errors). `cargo test --workspace` passes (zero new failures).

## Issues Encountered

None. Implementation followed pseudocode exactly.

One clippy lint addressed: `trim_split_whitespace` on `.trim().split_whitespace().count()` — removed the redundant `.trim()` since `split_whitespace()` already handles leading/trailing whitespace. Behavior is identical.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `hook routing SubagentStart injection patterns` -- found entry #3230 (SubagentStart hook routing to ContextSearch — implementation pattern, 0.60 score). Confirmed alignment with the pattern before implementing.
- Stored: entry #3297 "SubagentStart hook routing: input.session_id vs ppid fallback session_id" via `/uni-store-pattern` — captures the gotcha that `ContextSearch` must use `input.session_id.clone()` (raw Option), NOT the resolved `session_id` local variable (which has ppid fallback applied). Using the fallback silently breaks WA-2 histogram boost for SubagentStart events.
