# Test Plan: hook-handler

## Component Scope

Changes to `crates/unimatrix-server/src/hook.rs`: PreCompact arm, fire-and-forget check, BriefingContent stdout, session_id in ContextSearch.

## Risk Coverage

| Risk | Test |
|------|------|
| R-10 (fire-and-forget classification) | CompactPayload excluded from fire-and-forget |

## Unit Tests

### build_request Tests

#### test_build_request_precompact_with_session_id
```
Arrange: input with session_id = Some("sess-1")
Act: build_request("PreCompact", &input)
Assert: matches HookRequest::CompactPayload { session_id: "sess-1", injected_entry_ids: vec![], role: None, feature: None, token_limit: None }
```

#### test_build_request_precompact_without_session_id
```
Arrange: input with session_id = None
Act: build_request("PreCompact", &input)
Assert: matches HookRequest::CompactPayload { session_id } where session_id starts with "ppid-"
```

#### test_compact_payload_not_fire_and_forget
```
Arrange: HookRequest::CompactPayload { session_id: "s1", injected_entry_ids: vec![], ... }
Act: evaluate matches! for fire-and-forget classification
Assert: returns false (CompactPayload is synchronous)
```

### write_stdout Tests

#### test_write_stdout_briefing_content_with_content
```
Arrange: HookResponse::BriefingContent { content: "compaction data", token_count: 10 }
Act: write_stdout(&response)
Assert: Ok(()) -- content printed to stdout
```

#### test_write_stdout_briefing_content_empty
```
Arrange: HookResponse::BriefingContent { content: "", token_count: 0 }
Act: write_stdout(&response)
Assert: Ok(()) -- nothing printed (silent skip)
```

#### test_write_stdout_entries_unchanged
```
Arrange: HookResponse::Entries { items: vec![entry], total_tokens: 10 }
Act: write_stdout(&response)
Assert: Ok(()) -- existing behavior preserved
```

### ContextSearch session_id Passthrough

#### test_build_request_user_prompt_passes_session_id
```
Arrange: input with prompt = Some("query"), session_id = Some("sess-1")
Act: build_request("UserPromptSubmit", &input)
Assert: matches HookRequest::ContextSearch { session_id: Some("sess-1"), query: "query", ... }
```

#### test_build_request_user_prompt_no_session_id
```
Arrange: input with prompt = Some("query"), session_id = None
Act: build_request("UserPromptSubmit", &input)
Assert: matches HookRequest::ContextSearch { session_id: None, ... }
```
