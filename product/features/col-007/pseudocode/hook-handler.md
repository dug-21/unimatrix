# Pseudocode: hook-handler

## Purpose

Add `UserPromptSubmit` handling to the hook process. Extract the prompt from stdin JSON, construct a `ContextSearch` request, send it synchronously via UDS, and handle the `Entries` response by formatting and printing to stdout.

## Wire Protocol Changes (wire.rs)

### HookInput -- add prompt field

```
struct HookInput {
    // ... existing fields ...
    #[serde(default)]
    pub prompt: Option<String>,   // NEW
    #[serde(flatten)]
    pub extra: serde_json::Value,
}
```

### Remove dead_code attributes

Remove `#[allow(dead_code)]` from:
- `HookRequest::ContextSearch`
- `HookResponse::Entries`
- `EntryPayload`

### parse_hook_input fallback

Update the error-path fallback `HookInput` construction to include `prompt: None`.

## Modified Functions (hook.rs)

### build_request() -- add UserPromptSubmit arm

```
fn build_request(event: &str, input: &HookInput) -> HookRequest:
    match event:
        "SessionStart" => SessionRegister { ... }   // existing
        "Stop" => SessionClose { ... }               // existing
        "Ping" => Ping                               // existing
        "UserPromptSubmit" =>
            let query = input.prompt.clone().unwrap_or_default()
            if query.is_empty():
                // No prompt text -- fall through to RecordEvent
                // This handles the edge case where UserPromptSubmit fires
                // without a prompt field
                RecordEvent { event: ImplantEvent { ... } }
            else:
                ContextSearch {
                    query,
                    role: None,
                    task: None,
                    feature: None,
                    k: None,
                    max_tokens: None,
                }
        _ => RecordEvent { ... }                     // existing catch-all
```

### is_fire_and_forget classification -- update

Currently the fire-and-forget check uses a `matches!` expression listing the fire-and-forget request types. ContextSearch must NOT be fire-and-forget. The existing code already excludes Ping (which expects Pong). ContextSearch is also excluded because it expects Entries.

No change needed to the matches expression because ContextSearch is not in the list. But verify that UserPromptSubmit does NOT match the fire-and-forget patterns since it now produces ContextSearch, not RecordEvent.

The existing code:
```
let is_fire_and_forget = matches!(
    request,
    HookRequest::SessionRegister { .. }
        | HookRequest::SessionClose { .. }
        | HookRequest::RecordEvent { .. }
        | HookRequest::RecordEvents { .. }
);
```

ContextSearch is not in this list, so it will be treated as synchronous. Correct.

Edge case: if UserPromptSubmit has an empty prompt, it falls back to RecordEvent which IS fire-and-forget. This is intentional -- an empty prompt has nothing to search for.

### write_stdout() -- handle Entries response

```
fn write_stdout(response: &HookResponse) -> Result<(), Box<dyn Error>>:
    match response:
        HookResponse::Entries { items, .. } =>
            if let Some(text) = format_injection(items, MAX_INJECTION_BYTES):
                println!("{text}")
            // else: silent skip (no stdout output)
            Ok(())
        _ =>
            // Existing behavior: serialize as JSON
            let json = serde_json::to_string(response)?
            println!("{json}")
            Ok(())
```

## Error Handling

- Empty prompt: falls back to RecordEvent (fire-and-forget)
- Transport failure: existing error path logs to stderr, exits 0
- Server returns Error instead of Entries: existing path serializes Error as JSON to stdout (harmless -- Claude Code ignores non-recognized hook output)
- Server unavailable: existing Unavailable handler queues nothing (ContextSearch is not fire-and-forget), exits 0 silently

## Key Test Scenarios

1. `build_request("UserPromptSubmit", input_with_prompt)` returns `ContextSearch` with correct query
2. `build_request("UserPromptSubmit", input_without_prompt)` returns `RecordEvent` (fallback)
3. `build_request("UserPromptSubmit", input_with_empty_prompt)` returns `RecordEvent` (fallback)
4. `write_stdout` with `Entries` response containing items calls `format_injection` and prints
5. `write_stdout` with `Entries` response with empty items produces no output
6. ContextSearch request is NOT classified as fire-and-forget
7. HookInput with `prompt` field deserializes correctly
8. HookInput without `prompt` field: `prompt` is `None` (backward compat)
9. HookInput with `prompt` and unknown fields: `prompt` in named field, others in `extra`
