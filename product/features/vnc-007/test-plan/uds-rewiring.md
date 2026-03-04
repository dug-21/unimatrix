# Test Plan: UDS Rewiring (uds_listener.rs)

## Test Infrastructure

UDS handler tests in uds_listener.rs use `dispatch_request` with mock dependencies (make_store, make_embed_service, make_dispatch_deps). CompactPayload tests use real SessionRegistry with injection history.

## Test Scenarios

### T-UR-01: CompactPayload delegates to BriefingService (AC-18)
```
Method: grep -n "briefing.assemble\|services.briefing" uds_listener.rs
Assert: handle_compact_payload calls services.briefing.assemble()
```

### T-UR-02: CompactPayload behavioral equivalence — primary path (R-01, AC-19)
```
Setup:
  - Store 3 entries: 1 decision (id=1), 1 pattern/other (id=2), 1 convention (id=3)
  - Register session with injection history containing all 3 entries
Call: dispatch_request(HookRequest::CompactPayload { session_id, ... })
Assert: Response is BriefingContent
Assert: Content contains "## Decisions" section with entry 1
Assert: Content contains "## Key Context" section with entry 2
Assert: Content contains "## Conventions" section with entry 3
Assert: Section ordering preserved (decisions first, then key context, then conventions)
```

### T-UR-03: CompactPayload behavioral equivalence — fallback path (R-01)
```
Setup:
  - Store decision and convention entries
  - Register session WITHOUT injection history
Call: dispatch_request(HookRequest::CompactPayload { session_id, ... })
Assert: Response is BriefingContent
Assert: Content contains decisions from category query
Assert: Content contains conventions from category query
```

### T-UR-04: CompactPayload format preservation (R-09)
```
Setup: Store entries, register session with injection history
Call: dispatch_request(HookRequest::CompactPayload { ... })
Assert: Content starts with "--- Unimatrix Compaction Context ---"
Assert: Content contains "## Decisions" header
Assert: Content contains "## Key Context" header
Assert: Content contains "## Conventions" header
Assert: Entry format matches "[title] (NNN% confidence)\ncontent\n<!-- id:N -->"
```

### T-UR-05: CompactPayload compaction count increment (AC-21)
```
Setup: Register session, verify compaction_count=0
Call: dispatch_request(HookRequest::CompactPayload { ... })
Assert: session.compaction_count is now 1
Call: dispatch_request again
Assert: session.compaction_count is now 2
```

### T-UR-06: HookRequest::Briefing returns BriefingContent (AC-22, AC-24)
```
Setup: Store convention entries with topic matching role
Call: dispatch_request(HookRequest::Briefing {
    role: "dev", task: "test", feature: None, max_tokens: None
})
Assert: Response is HookResponse::BriefingContent (NOT Error)
Assert: content is non-empty
Assert: token_count > 0
```

### T-UR-07: HookRequest::Briefing with conventions + search not ready (AC-23)
```
Setup: Store convention entries. Embed service not started.
Call: dispatch_request(HookRequest::Briefing {
    role: "architect", task: "design", feature: None, max_tokens: None
})
Assert: Response is BriefingContent
Assert: content contains convention entries
```

### T-UR-08: dispatch_unknown_returns_error test update (R-10, AC-36)
```
Current test: sends HookRequest::Briefing, expects ERR_UNKNOWN_REQUEST
After: HookRequest::Briefing is now handled, so this test must change.

Options:
  a) Remove the test (all variants now handled, catch-all unreachable)
  b) If catch-all `_ =>` is retained for future-proofing, test cannot send any existing variant
  c) Test that Briefing returns BriefingContent (not Error) and rename test

Recommended: Option (a) or (c). The test should verify Briefing works, not that unknown returns error.
```

### T-UR-09: Session state resolved before BriefingService call (AC-20)
```
Method: Code inspection
Assert: session_registry.get_state() called before services.briefing.assemble()
Assert: effective_role/feature derived from session state
```

## Updated Existing Tests

Tests in uds_listener.rs that may need updates:
- `dispatch_compact_payload_*` tests — verify they still pass after handle_compact_payload signature change
- `dispatch_briefing_returns_error` — MUST be updated (now returns BriefingContent)
- `format_compaction_payload_*` tests — should still pass (function retained)

## Risk Coverage

| Risk | Test(s) | Status |
|------|---------|--------|
| R-01 (CompactPayload regression) | T-UR-02, T-UR-03, T-UR-04 | Covered |
| R-09 (Format text divergence) | T-UR-04 | Covered |
| R-10 (dispatch_unknown test) | T-UR-08 | Covered |
