# Pseudocode: hook-handler

## Purpose

Add PreCompact event handling to the hook subcommand. The hook reads session_id from stdin, sends CompactPayload via UDS, and prints the BriefingContent response to stdout.

## Changes to hook.rs

### 1. build_request() -- Add PreCompact Arm

```
fn build_request(event, input) -> HookRequest:
    match event:
        // ... existing arms unchanged ...

        "PreCompact" =>
            HookRequest::CompactPayload {
                session_id: input.session_id.clone().unwrap_or_else(|| format!("ppid-{}", parent_id())),
                injected_entry_ids: vec![],  // Server has tracked history (ADR-002)
                role: None,      // Server uses session state
                feature: None,   // Server uses session state
                token_limit: None,  // Server uses defaults
            }

        // ... existing fallthrough to RecordEvent ...
```

The `injected_entry_ids` is empty because the server maintains its own injection history (ADR-002). The field exists for future extensibility and as a fallback hint.

### 2. is_fire_and_forget Check -- Exclude CompactPayload

The existing check in `run()` uses a matches! macro. CompactPayload is NOT fire-and-forget -- it is synchronous (we wait for BriefingContent response).

```
let is_fire_and_forget = matches!(
    request,
    HookRequest::SessionRegister { .. }
        | HookRequest::SessionClose { .. }
        | HookRequest::RecordEvent { .. }
        | HookRequest::RecordEvents { .. }
);
```

CompactPayload is already excluded because it's not in the matches! list. But we must verify this is correct after removing `#[allow(dead_code)]` -- the wildcard arm `_ =>` in dispatch_request currently catches CompactPayload and returns Error. After col-008 adds the CompactPayload arm to dispatch, the response will be BriefingContent.

### 3. write_stdout() -- Handle BriefingContent

```
fn write_stdout(response) -> Result<()>:
    match response:
        HookResponse::Entries { items, .. } =>
            // Existing col-007 injection formatting (unchanged)
            if let Some(text) = format_injection(items, MAX_INJECTION_BYTES):
                println!("{text}")
            Ok(())

        HookResponse::BriefingContent { content, .. } =>
            // NEW: col-008 compaction defense output
            if !content.is_empty():
                println!("{content}")
            Ok(())

        _ =>
            // Other responses: serialize as JSON (unchanged)
            let json = serde_json::to_string(response)?
            println!("{json}")
            Ok(())
```

### 4. build_request() -- Pass session_id to ContextSearch

The existing UserPromptSubmit arm builds ContextSearch without session_id. Add it.

```
"UserPromptSubmit" =>
    let query = input.prompt.clone().unwrap_or_default()
    if query.is_empty():
        // ... existing RecordEvent fallback ...
    else:
        HookRequest::ContextSearch {
            query,
            session_id: input.session_id.clone(),  // NEW -- col-008
            role: None,
            task: None,
            feature: None,
            k: None,
            max_tokens: None,
        }
```

## Error Handling

- Transport failure on CompactPayload: print nothing to stdout, exit 0 (graceful degradation)
- BriefingContent with empty content: print nothing to stdout (silent skip)
- Server returns Error response: existing write_stdout serializes as JSON to stdout

## Key Test Scenarios

1. build_request("PreCompact", input_with_session_id) returns CompactPayload with correct session_id
2. build_request("PreCompact", input_without_session_id) returns CompactPayload with ppid fallback
3. CompactPayload is NOT fire-and-forget (matches! returns false)
4. write_stdout(BriefingContent { content: "...", token_count: 10 }) prints content
5. write_stdout(BriefingContent { content: "", token_count: 0 }) prints nothing (silent skip)
6. build_request("UserPromptSubmit", input_with_prompt_and_session) includes session_id in ContextSearch
7. Existing write_stdout behavior for Entries is unchanged
