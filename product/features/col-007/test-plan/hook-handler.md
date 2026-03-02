# Test Plan: hook-handler

## Unit Tests

### build_request() -- UserPromptSubmit arm

| Test | Input | Expected | Risk |
|------|-------|----------|------|
| `test_build_request_user_prompt_submit_with_prompt` | event="UserPromptSubmit", input.prompt=Some("search query") | ContextSearch { query: "search query", role: None, ... } | R-07 |
| `test_build_request_user_prompt_submit_without_prompt` | event="UserPromptSubmit", input.prompt=None | RecordEvent (fallback) | R-07 |
| `test_build_request_user_prompt_submit_empty_prompt` | event="UserPromptSubmit", input.prompt=Some("") | RecordEvent (fallback) | R-07 |
| `test_build_request_user_prompt_submit_long_prompt` | event="UserPromptSubmit", input.prompt=Some("x".repeat(20000)) | ContextSearch with full query string | R-10 |

### is_fire_and_forget classification

| Test | Input | Expected | Risk |
|------|-------|----------|------|
| `test_context_search_is_not_fire_and_forget` | HookRequest::ContextSearch { ... } | is_fire_and_forget == false | R-01 |
| `test_session_register_is_fire_and_forget` | HookRequest::SessionRegister { ... } | is_fire_and_forget == true | -- |

### write_stdout() -- Entries handling

| Test | Input | Expected | Risk |
|------|-------|----------|------|
| `test_write_stdout_entries_with_items` | Entries { items: [entry1, entry2], total_tokens: 100 } | format_injection called, output printed | R-09 |
| `test_write_stdout_entries_empty` | Entries { items: [], total_tokens: 0 } | No stdout output (silent skip) | R-04 |
| `test_write_stdout_pong_unchanged` | Pong { server_version: "0.1.0" } | JSON serialized to stdout | R-02 |

### HookInput.prompt deserialization (wire.rs)

| Test | Input JSON | Expected | Risk |
|------|-----------|----------|------|
| `test_hook_input_with_prompt` | `{"hook_event_name":"UserPromptSubmit","prompt":"test query"}` | prompt == Some("test query") | R-07 |
| `test_hook_input_without_prompt` | `{"hook_event_name":"SessionStart"}` | prompt == None | R-07 |
| `test_hook_input_empty_prompt` | `{"hook_event_name":"UserPromptSubmit","prompt":""}` | prompt == Some("") | R-07 |
| `test_hook_input_prompt_with_unknown_fields` | `{"hook_event_name":"Test","prompt":"q","custom":"val"}` | prompt == Some("q"), extra["custom"] == "val" | R-07 |

## Assertions

- `build_request("UserPromptSubmit", input)` returns `HookRequest::ContextSearch` when prompt is non-empty
- `ContextSearch` variant is NOT matched by the fire-and-forget check
- `write_stdout` with `Entries` prints formatted text (not raw JSON)
- `write_stdout` with empty `Entries` produces no output
- `HookInput.prompt` is `None` when not present in JSON (backward compat)
- `HookInput.prompt` does not appear in `extra` when present as named field

## Edge Cases

- Prompt containing only whitespace: treated as non-empty (sends ContextSearch)
- Prompt containing special characters (newlines, tabs): passed through to query
- Very long prompt (>10KB): accepted, ONNX model truncates internally
