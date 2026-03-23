# Test Plan: hook-routing (uds/hook.rs)

## Component

`crates/unimatrix-server/src/uds/hook.rs`

Changes: SubagentStart arm in `build_request`, `MIN_QUERY_WORDS` constant, word-count guard
on UserPromptSubmit, `write_stdout_subagent_inject` helper function.

## Risks Covered

R-01 (source field compile surface), R-04 (MIN_QUERY_WORDS boundary), R-07 (SubagentStart
stdout injection), EC-01 (whitespace-only prompt_snippet), EC-02 (JSON null prompt_snippet)

## ACs Covered

AC-01, AC-02, AC-02b, AC-03, AC-04, AC-05 (a), AC-SR01 (manual), AC-SR02, AC-SR03,
AC-22, AC-23, AC-23b, AC-23c, AC-25

---

## Unit Test Expectations

All tests live in `crates/unimatrix-server/src/uds/hook.rs` `#[cfg(test)]` block,
using the existing `test_input()` and `test_entry()` helpers as a base.

### Non-Negotiable Test Names (mandatory — grep at Gate 3c)

#### `build_request_subagentstart_with_prompt_snippet` (AC-01)
**Arrange**: `HookInput { session_id: Some("sess-parent"), extra: json!({ "prompt_snippet": "implement the spec writer agent" }), ... }`
**Act**: `build_request("SubagentStart", &input)`
**Assert**:
- Result is `HookRequest::ContextSearch { ... }`
- `query == "implement the spec writer agent"`
- `source == Some("SubagentStart".to_string())`
- `session_id == Some("sess-parent")` (from `input.session_id`, NOT ppid fallback)
- `role == None`, `task == None`, `feature == None`, `k == None`, `max_tokens == None`

#### `build_request_subagentstart_empty_prompt_snippet` (AC-02)
**Arrange**: Two sub-cases:
  (a) `extra: json!({})` (key absent)
  (b) `extra: json!({ "prompt_snippet": "" })` (key present, empty string)
**Act**: `build_request("SubagentStart", &input)` for each
**Assert**: Both return `HookRequest::RecordEvent { .. }` (not `ContextSearch`)

#### `build_request_userpromptsub_four_words_record_event` (AC-22)
**Arrange**: `input.prompt = Some("implement the spec writer".to_string())` (exactly 4 words)
**Act**: `build_request("UserPromptSubmit", &input)`
**Assert**: `HookRequest::RecordEvent { .. }` (word count 4 < MIN_QUERY_WORDS)

#### `build_request_userpromptsub_five_words_context_search` (AC-22)
**Arrange**: `input.prompt = Some("implement the spec writer agent".to_string())` (exactly 5 words)
**Act**: `build_request("UserPromptSubmit", &input)`
**Assert**: `HookRequest::ContextSearch { query: "implement the spec writer agent", .. }`

### Additional Tests Required

#### `build_request_subagentstart_session_id_from_input` (AC-03)
**Arrange**: `HookInput { session_id: Some("parent-sess-42"), extra: json!({ "prompt_snippet": "design the architecture" }), ... }`
**Act**: `build_request("SubagentStart", &input)`
**Assert**: `session_id == Some("parent-sess-42")` — taken from `input.session_id`, not
`format!("ppid-{}", parent_id())`
**Note**: SubagentStart fires in parent session context; WA-2 histogram lookup requires the
correct session ID.

#### `context_search_is_not_fire_and_forget` (AC-04) — existing test, must remain
The existing test at line 818 of hook.rs constructs `HookRequest::ContextSearch` without
the `source` field. After the `source` field is added to the struct, this test **must be
updated** to include `source: None` (or `..` spread syntax) to compile.
**Assert**: `is_faf` is `false` for `HookRequest::ContextSearch`.

#### `build_request_subagentstart_one_word_routes_to_context_search` (AC-23)
**Arrange**: `extra: json!({ "prompt_snippet": "implement" })` (1 word, non-empty)
**Act**: `build_request("SubagentStart", &input)`
**Assert**: `HookRequest::ContextSearch` — SubagentStart does NOT use `MIN_QUERY_WORDS`

#### `build_request_subagentstart_whitespace_only_prompt_snippet` (AC-23b)
**Arrange**: `extra: json!({ "prompt_snippet": "   " })` (whitespace-only)
**Act**: `build_request("SubagentStart", &input)`
**Assert**: `HookRequest::RecordEvent { .. }` — `.trim().is_empty()` catches whitespace-only

#### `build_request_userpromptsub_whitespace_padded_one_word` (AC-23c)
**Arrange**: `input.prompt = Some("  approve  ".to_string())` (1 real word, surrounding whitespace)
**Act**: `build_request("UserPromptSubmit", &input)`
**Assert**: `HookRequest::RecordEvent { .. }` — `.trim()` strips whitespace before counting

#### `build_request_userpromptsub_one_word` (AC-02b scenario)
**Arrange**: `input.prompt = Some("ok".to_string())`
**Act**: `build_request("UserPromptSubmit", &input)`
**Assert**: `HookRequest::RecordEvent { .. }`

#### `build_request_userpromptsub_three_words` (AC-02b scenario)
**Arrange**: `input.prompt = Some("yes ok thanks".to_string())`
**Act**: `build_request("UserPromptSubmit", &input)`
**Assert**: `HookRequest::RecordEvent { .. }` (3 < 5)

#### `build_request_userpromptsub_six_words` (AC-02b scenario)
**Arrange**: `input.prompt = Some("implement the spec writer agent today".to_string())`
**Act**: `build_request("UserPromptSubmit", &input)`
**Assert**: `HookRequest::ContextSearch { .. }` (6 >= 5)

### Stdout Writing Tests (AC-SR01/SR02/SR03)

These tests must capture what is written to stdout by `write_stdout_subagent_inject` and
the regular `write_stdout`. Because these functions write to `stdout()`, tests should either:
- Test `write_stdout_subagent_inject` directly by calling it and capturing stdout via
  a test-level pipe, or
- Test the helper in isolation using a String-buffer approach if the function signature
  can accept a `Write` impl.

If the function writes to global stdout (current pattern), prefer testing the helper
function's output deterministically by redirecting in a controlled test environment.

#### `write_stdout_subagent_inject_valid_json_envelope` (AC-SR02)
**Arrange**: `entries_text = "1  42   crt-027/hook  decision  0.85  Unimatrix routes SubagentStart..."`
**Act**: call `write_stdout_subagent_inject(entries_text)` capturing stdout
**Assert**:
- Output parses as valid JSON
- `json["hookSpecificOutput"]["hookEventName"] == "SubagentStart"`
- `json["hookSpecificOutput"]["additionalContext"] == entries_text`

#### `write_stdout_plain_text_no_json_envelope` (AC-SR03)
**Arrange**: Construct a `HookResponse::Entries { items: [...] }` and call `write_stdout`
**Act**: capture stdout
**Assert**: Output does NOT start with `{`; does NOT contain `"hookSpecificOutput"`

### EC-02: JSON null prompt_snippet
**Arrange**: `extra: json!({ "prompt_snippet": null })`
**Act**: `build_request("SubagentStart", &input)`
**Assert**: `HookRequest::RecordEvent { .. }` — `v.as_str()` returns `None` for `Null`, falls through

### Existing Tests That Must Be Updated (AC-25)

The following existing tests construct `HookRequest::ContextSearch` via struct literal and
must be updated to add `source: None` after the field is added:

- `context_search_is_not_fire_and_forget` (line ~818) — add `source: None`
- `build_request_user_prompt_submit_with_prompt` (line ~777) — match arm, no struct literal update needed but confirm it compiles
- Any test that directly constructs `HookRequest::ContextSearch { query, session_id, role, task, feature, k, max_tokens }` — must add `source: None`

**Verification**: `cargo build --release` emits zero `non_exhaustive` errors after change.

---

## Integration Test Expectations

The SubagentStart hook path involves the external hook process binary, which is not
exercised by infra-001 (MCP-only harness). Integration coverage for hook.rs is provided
by the `listener.rs` integration tests in the same crate, which submit
`HookRequest::ContextSearch { source: Some("SubagentStart") }` directly to `dispatch_request`.

See `listener-dispatch.md` for `dispatch_request` source-tagging integration tests (AC-05b).

---

## Edge Cases

| Edge Case | Test | Expected Result |
|-----------|------|-----------------|
| EC-01: whitespace-only `prompt_snippet` | `build_request_subagentstart_whitespace_only_prompt_snippet` | `RecordEvent` |
| EC-02: `null` JSON value for `prompt_snippet` | `build_request_subagentstart_null_prompt_snippet_record_event` | `RecordEvent` |
| 5-word exact boundary | `build_request_userpromptsub_five_words_context_search` | `ContextSearch` |
| 4-word just-below boundary | `build_request_userpromptsub_four_words_record_event` | `RecordEvent` |
| SubagentStart 1-word non-empty | `build_request_subagentstart_one_word_routes_to_context_search` | `ContextSearch` |
