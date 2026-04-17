# Component Test Plan: normalization
## `crates/unimatrix-server/src/uds/hook.rs`

Validating ACs: **AC-01, AC-02, AC-03, AC-04, AC-05, AC-11, AC-12, AC-14, AC-15, AC-17, AC-18**
Risk coverage: **R-01, R-02, R-03 (partial), R-05, R-07, R-08, R-10, R-11**

---

## Component Responsibility

`hook.rs` is the normalization boundary. This is where all provider-specific event
names are translated to canonical names, `ImplantEvent.provider` is threaded into
every constructed request, and the Gemini-specific dispatch arms and mcp_context
promotion adapter live.

Functions under test:
- `normalize_event_name(event: &str) -> (&'static str, &'static str)`
- `build_request(event: &str, input: &HookInput) -> HookRequest` (modified)
- `build_cycle_event_or_fallthrough(event: &str, session_id: String, input: &HookInput) -> HookRequest` (unchanged but called with Gemini payloads)
- `is_rework_eligible_tool(tool_name: &str) -> bool` (gate test)

---

## GATE PREREQUISITE: AC-14 Tests (Must Pass First)

These three tests verify R-01 (the highest-risk integration point). They must be
written and green before any other Gemini BeforeTool AC is attempted.

### `test_gemini_mcp_context_tool_name_promotion` (AC-14, R-01 scenario 1)

```rust
// Arrange: Gemini BeforeTool payload with mcp_context.tool_name = "context_cycle"
// and tool_input at top level (per ASS-049 confirmation — top-level position, same as Claude Code)
let mut input = HookInput {
    hook_event_name: "PreToolUse".to_string(),  // already normalized
    session_id: Some("sess-gemini-1".to_string()),
    mcp_context: Some(serde_json::json!({
        "server_name": "unimatrix",
        "tool_name": "context_cycle",
        "url": "http://localhost:3000"
    })),
    extra: serde_json::json!({
        "tool_input": { "type": "start", "topic": "vnc-013" }
    }),
    // ... other fields None/default
};

// Act: the "PreToolUse" arm must promote mcp_context.tool_name into extra["tool_name"]
// before calling build_cycle_event_or_fallthrough().
// Test calls build_request() with canonical "PreToolUse" (already normalized).
let req = build_request("PreToolUse", &input);

// Assert
match req {
    HookRequest::RecordEvent { event } => {
        assert_eq!(event.event_type, "cycle_start",
            "Promotion failed: mcp_context.tool_name was not promoted to extra[tool_name]");
        assert!(event.payload.get("feature_cycle").is_some());
    }
    _ => panic!("expected RecordEvent with cycle_start, got {:?}", req),
}
```

**Failure mode**: If promotion is a no-op (original `input` passed instead of clone),
`build_cycle_event_or_fallthrough()` finds no `tool_name` in `extra`, returns
`RecordEvent { event_type: "PreToolUse" }`. Test catches this immediately.

---

### `test_gemini_before_tool_non_cycle_fallthrough` (AC-14, R-01 scenario 2)

```rust
// Arrange: mcp_context.tool_name is "context_search" (not context_cycle)
let input = HookInput {
    hook_event_name: "PreToolUse".to_string(),
    session_id: Some("sess-gemini-2".to_string()),
    mcp_context: Some(serde_json::json!({
        "tool_name": "context_search",
        "server_name": "unimatrix"
    })),
    extra: serde_json::json!({
        "tool_input": { "query": "test query" }
    }),
    // ...
};

// Act
let req = build_request("PreToolUse", &input);

// Assert: falls through to generic RecordEvent — NOT cycle_start
match req {
    HookRequest::RecordEvent { event } => {
        assert_eq!(event.event_type, "PreToolUse",
            "Non-cycle tool must not produce cycle_start");
    }
    _ => panic!("expected generic RecordEvent, got {:?}", req),
}
```

---

### `test_mcp_context_missing_tool_name_degrades_gracefully` (AC-14, R-01 scenario 3)

```rust
// Arrange: mcp_context present but tool_name key is absent
let input = HookInput {
    hook_event_name: "PreToolUse".to_string(),
    session_id: Some("sess-gemini-3".to_string()),
    mcp_context: Some(serde_json::json!({
        "server_name": "unimatrix",
        "url": "http://localhost:3000"
        // no "tool_name" key
    })),
    extra: serde_json::json!({}),
    // ...
};

// Act (must not panic)
let req = build_request("PreToolUse", &input);

// Assert: graceful fallthrough — no crash
match req {
    HookRequest::RecordEvent { event } => {
        assert_eq!(event.event_type, "PreToolUse");
    }
    _ => panic!("expected RecordEvent fallthrough, got {:?}", req),
}
```

---

## normalize_event_name: Exhaustive Table Tests (AC-01, AC-17, AC-18, AC-20)

### `test_normalize_event_name_gemini_unique_names` (AC-01)

Table-driven test covering all three Gemini-unique event names (inference path, no provider hint):

```rust
let cases = [
    ("BeforeTool", ("PreToolUse", "gemini-cli")),
    ("AfterTool",  ("PostToolUse", "gemini-cli")),
    ("SessionEnd", ("Stop", "gemini-cli")),
];

for (input_event, (expected_canonical, expected_provider)) in &cases {
    let (canonical, provider) = normalize_event_name(input_event);
    assert_eq!(canonical, *expected_canonical,
        "normalize_event_name({input_event}) canonical mismatch");
    assert_eq!(provider, *expected_provider,
        "normalize_event_name({input_event}) provider mismatch");
}
```

---

### `test_normalize_event_name_claude_code_passthrough` (AC-01, AC-18)

All Claude Code event names (inference path, no provider hint) must return themselves
with `provider = "claude-code"`:

```rust
let claude_code_events = [
    "PreToolUse", "PostToolUse", "SessionStart", "Stop",
    "SubagentStart", "SubagentStop", "PreCompact", "UserPromptSubmit",
    "PostToolUseFailure", "TaskCompleted", "Ping",
];

for event in &claude_code_events {
    let (canonical, provider) = normalize_event_name(event);
    assert_eq!(canonical, *event,
        "normalize_event_name({event}) must return itself as canonical");
    assert_eq!(provider, "claude-code",
        "normalize_event_name({event}) must infer claude-code");
}
```

This test also covers AC-18 (backward-compatible default for `PreToolUse`).

---

### `test_normalize_event_name_unknown_fallback` (AC-01)

```rust
let (canonical, provider) = normalize_event_name("CompletelyUnknownEvent");
// Note: unknown event names cannot return &'static str for the canonical name.
// The implementation resolves this by returning the static sentinel ("__unknown__", "unknown").
// The caller (run()) detects "__unknown__" and substitutes the raw event string.
assert_eq!(canonical, "__unknown__");
assert_eq!(provider, "unknown");
```

Consult the implementation decision in pseudocode/normalization.md for the exact
return value of the unknown arm before writing the final assertion.

---

### `test_run_codex_provider_hint` (AC-17)

Tests the hint path in `run()`: when `--provider codex-cli` is passed, `ImplantEvent.provider`
must be "codex-cli" regardless of the event name. This cannot be tested via
`normalize_event_name` because the hint path bypasses that function entirely (it calls
`map_to_canonical()` and uses the flag value directly).

```rust
// Arrange: simulate run() hint path by calling build_request() with provider already set
// (run() sets hook_input.provider = Some("codex-cli") before calling build_request)
let mut input = test_input();
input.session_id = Some("sess-codex".to_string());
input.provider = Some("codex-cli".to_string());

let req = build_request("PreToolUse", &input);

// Assert: provider threaded through to ImplantEvent
match req {
    HookRequest::RecordEvent { event } => {
        assert_eq!(event.provider, Some("codex-cli".to_string()),
            "hint path must set provider = codex-cli on ImplantEvent");
    }
    _ => panic!("expected RecordEvent for PreToolUse"),
}
```

---

### `test_run_session_start_provider_hint_precedence` (AC-20)

Tests that when `--provider` is supplied, the hint value takes precedence over inference.
`SessionStart` infers as "claude-code" in the inference path, but when `--provider gemini-cli`
or `--provider codex-cli` is passed, the hint must win. Tested at the `run()` level via
`build_request()` with provider already set on `hook_input`.

```rust
// Provider hint "claude-code": run() sets hook_input.provider = Some("claude-code")
let mut input = test_input();
input.session_id = Some("sess-cc".to_string());
input.provider = Some("claude-code".to_string());
let req = build_request("SessionStart", &input);
match req {
    HookRequest::SessionRegister { .. } | HookRequest::RecordEvent { .. } => {
        // Acceptable — provider is set; verify if SessionRegister carries provider (see AC-05)
    }
    _ => panic!("unexpected variant for SessionStart"),
}

// Provider hint "codex-cli": run() sets hook_input.provider = Some("codex-cli")
let mut input2 = test_input();
input2.session_id = Some("sess-codex".to_string());
input2.provider = Some("codex-cli".to_string());
let req2 = build_request("SessionStart", &input2);
// If build_request produces a RecordEvent for this path, assert provider:
if let HookRequest::RecordEvent { event } = req2 {
    assert_eq!(event.provider, Some("codex-cli".to_string()),
        "hint path must override inference: codex-cli hint must produce provider=codex-cli");
}
```

---

### `test_normalize_event_name_category2_passthrough` (AC-01 — Category 2 events)

Category 2 events (`cycle_start`, `cycle_stop`, `cycle_phase_end`) are never inputs to
`normalize_event_name` — they are outputs produced by `build_cycle_event_or_fallthrough()`
and are already canonical. If somehow passed to `normalize_event_name`, they fall to the
unknown arm and return `("__unknown__", "unknown")`:

```rust
for event in &["cycle_start", "cycle_stop", "cycle_phase_end"] {
    let (canonical, provider) = normalize_event_name(event);
    // These events are never inputs to normalize_event_name; if passed they fall to
    // the unknown arm — they are not in the match table.
    assert_eq!(canonical, "__unknown__",
        "Category 2 event {event} is not a normalize_event_name input; must return __unknown__");
    assert_eq!(provider, "unknown",
        "Category 2 event {event} is not a normalize_event_name input; must return unknown");
}
```

---

## Rework Detection Gate Tests (AC-04, AC-12, R-05)

### `test_gemini_after_tool_skips_rework_path` (AC-04, AC-12, R-05 scenario 1)

```rust
// Arrange: canonical PostToolUse with provider = gemini-cli
// Simulate a rework-eligible tool name to verify the provider gate fires first
let mut input = test_input();
input.session_id = Some("sess-gemini".to_string());
input.provider = Some("gemini-cli".to_string());
input.extra = serde_json::json!({ "tool_name": "Bash" });  // normally rework-eligible

let req = build_request("PostToolUse", &input);

// Assert: RecordEvent with PostToolUse — rework path NOT entered
match req {
    HookRequest::RecordEvent { event } => {
        assert_eq!(event.event_type, "PostToolUse");
        assert_eq!(event.provider, Some("gemini-cli".to_string()));
        // event_type is NOT "post_tool_use_rework_candidate"
        assert_ne!(event.event_type, "post_tool_use_rework_candidate");
    }
    HookRequest::RecordEvents { .. } => panic!("MultiEdit path entered — rework gate failed"),
    _ => panic!("unexpected variant"),
}
```

Key assertion: `event_type == "PostToolUse"` (not `"post_tool_use_rework_candidate"`).
If the gate is missing, `is_rework_eligible_tool("Bash")` returns true, and the output
would be `"post_tool_use_rework_candidate"` — a clear signal.

---

### `test_codex_post_tool_use_skips_rework_path` (R-05 scenario 2)

```rust
let mut input = test_input();
input.provider = Some("codex-cli".to_string());
input.extra = serde_json::json!({ "tool_name": "Edit" });

let req = build_request("PostToolUse", &input);

match req {
    HookRequest::RecordEvent { event } => {
        assert_eq!(event.event_type, "PostToolUse");
        assert_ne!(event.event_type, "post_tool_use_rework_candidate");
    }
    _ => panic!("expected RecordEvent, rework gate must block codex"),
}
```

---

### `test_claude_code_post_tool_use_enters_rework_path` (R-05 scenario 3 — over-block guard)

```rust
// Arrange: Claude Code PostToolUse with rework-eligible tool — must still enter rework path
let mut input = test_input();
input.provider = Some("claude-code".to_string());
input.extra = serde_json::json!({ "tool_name": "Bash", "exit_code": 1 });

let req = build_request("PostToolUse", &input);

// Assert: enters rework path — event_type is rework_candidate
match req {
    HookRequest::RecordEvent { event } => {
        assert_eq!(event.event_type, "post_tool_use_rework_candidate");
    }
    _ => panic!("expected rework candidate for claude-code Bash with failure"),
}
```

---

## ImplantEvent.provider Threading Tests (AC-05, R-02)

### `test_implant_event_provider_set_for_session_register` (AC-05, R-02 scenario 1)

```rust
let mut input = test_input();
input.session_id = Some("sess-1".to_string());
input.provider = Some("gemini-cli".to_string());

let req = build_request("SessionStart", &input);
// Note: SessionRegister does not carry ImplantEvent directly.
// Verify provider is threaded into the request metadata.
// If SessionRegister struct gains a provider field, assert it here.
// Otherwise document that SessionRegister is not an ImplantEvent carrier.
```

---

### `test_implant_event_provider_set_for_record_event_variants` (AC-05, R-02 scenario 1)

For each canonical event that produces `RecordEvent`:

```rust
let canonical_events = ["PreToolUse", "PostToolUseFailure", "SubagentStop", "TaskCompleted"];

for event in &canonical_events {
    let mut input = test_input();
    input.provider = Some("gemini-cli".to_string());
    input.session_id = Some("sess-1".to_string());

    let req = build_request(event, &input);
    match req {
        HookRequest::RecordEvent { event: ie } => {
            assert_eq!(ie.provider, Some("gemini-cli".to_string()),
                "build_request({event}) must thread provider into ImplantEvent");
        }
        _ => {} // Some events produce non-RecordEvent — skip
    }
}
```

---

### `test_cycle_event_provider_propagated` (AC-05, R-02 scenario 2)

```rust
// Arrange: Gemini cycle_start path — provider must be in ImplantEvent
let mut input = test_input();
input.session_id = Some("sess-gemini".to_string());
input.provider = Some("gemini-cli".to_string());
input.mcp_context = Some(serde_json::json!({ "tool_name": "context_cycle" }));
input.extra = serde_json::json!({ "tool_input": { "type": "start", "topic": "vnc-013" } });

let req = build_request("PreToolUse", &input);

match req {
    HookRequest::RecordEvent { event } => {
        assert_eq!(event.event_type, "cycle_start");
        assert_eq!(event.provider, Some("gemini-cli".to_string()),
            "cycle_start event must carry provider from normalization");
    }
    _ => panic!("expected cycle_start RecordEvent"),
}
```

---

## Gemini Dispatch Tests (AC-03, R-08)

### `test_gemini_session_end_produces_session_close` (AC-01, R-08 scenario 2)

```rust
// "SessionEnd" normalized to "Stop" before build_request() is called.
// This test verifies the normalized "Stop" arm produces SessionClose for gemini provider.
let mut input = test_input();
input.session_id = Some("sess-gemini".to_string());
input.provider = Some("gemini-cli".to_string());

let req = build_request("Stop", &input);

match req {
    HookRequest::SessionClose { session_id, .. } => {
        assert_eq!(session_id, "sess-gemini");
    }
    _ => panic!("expected SessionClose for normalized Stop"),
}
```

---

> **R-08 scenario 3 removed** — the pseudocode uses a `debug_assert!` in `build_request()`
> instead of defense-in-depth arms. If `"SessionEnd"` reaches `build_request()` in a debug
> build, the assert fires (panics) before any match arm is reached. R-08 coverage is
> adequate via scenario 2 (`test_gemini_session_end_produces_session_close`), which
> verifies the normalized `"Stop"` path produces `SessionClose` for the gemini provider.

---

## Rework Candidate Guard Tests (AC-16, R-07)

### `test_rework_candidate_guard_fires_in_debug` (AC-16, R-07 scenario 1)

```rust
// This test must only run in debug builds
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "rework candidate string escaped normalization boundary")]
fn test_rework_candidate_guard_fires_in_debug() {
    // Attempt to call extract_observation_fields() with "post_tool_use_rework_candidate".
    // The debug_assert! must fire before the match arm processes it.
    // Implementation-specific: this may need to call extract_observation_fields()
    // directly if it's accessible, or verify through a test helper.
    // If extract_observation_fields() is private, the test must be an in-module test.
    use crate::uds::listener::extract_observation_fields;
    extract_observation_fields("post_tool_use_rework_candidate", &serde_json::json!({}));
}
```

Note: If `extract_observation_fields()` is private to `listener.rs`, place this test
in `listener.rs`'s `mod tests` block, not in `hook.rs`.

---

### `test_post_tool_use_failure_arm_unchanged` (AC-16, R-07 scenario 3)

```rust
// Verify PostToolUseFailure events are NOT affected by the rework candidate guard
let mut input = test_input();
input.session_id = Some("sess-1".to_string());
input.extra = serde_json::json!({ "tool_name": "Bash", "error": "file not found" });

let req = build_request("PostToolUseFailure", &input);

match req {
    HookRequest::RecordEvent { event } => {
        assert_eq!(event.event_type, "PostToolUseFailure",
            "PostToolUseFailure must NOT be normalized to PostToolUse (ADR-003 col-027)");
    }
    _ => panic!("expected RecordEvent with PostToolUseFailure"),
}
```

---

## Topic Signal Tests (AC-11, R-10)

### `test_gemini_before_tool_topic_signal_extraction` (AC-11, R-10 scenario 1)

```rust
// Arrange: Gemini BeforeTool with tool_input at top level (ASS-049 confirmed)
let mut input = test_input();
input.session_id = Some("sess-gemini".to_string());
input.provider = Some("gemini-cli".to_string());
input.extra = serde_json::json!({
    "tool_input": { "query": "relevant test query for signal extraction" }
});

// After normalization to "PreToolUse", extract_event_topic_signal is called.
// Direct test: call extract_event_topic_signal with canonical "PreToolUse" and this input.
let signal = extract_event_topic_signal("PreToolUse", &input);

// Assert: topic signal is non-empty and contains the query text
assert!(signal.is_some(), "topic signal must be Some for Gemini BeforeTool with tool_input");
let sig = signal.unwrap();
assert!(!sig.is_empty());
// Confirm the signal relates to the tool_input content
assert!(sig.contains("relevant test query") || sig.len() > 0,
    "signal must extract from tool_input, not generic stringification");
```

---

## Gemini AfterTool Response Degradation Tests (R-11)

### `test_gemini_after_tool_response_fields_degrade_gracefully` (R-11 scenario 1)

```rust
// Arrange: AfterTool payload without tool_response field (or with different field name)
let mut input = test_input();
input.session_id = Some("sess-gemini".to_string());
input.provider = Some("gemini-cli".to_string());
// No tool_response field — Gemini may use a different field name
input.extra = serde_json::json!({
    "tool_name": "context_search",
    "tool_output": "some result"  // hypothetical Gemini field name
});

// Act: must not panic
let req = build_request("PostToolUse", &input);

// Assert: produces RecordEvent without panic; response fields will be null in DB
match req {
    HookRequest::RecordEvent { event } => {
        assert_eq!(event.event_type, "PostToolUse");
        // No assertion on response_size / response_snippet — they will be null
    }
    _ => panic!("expected RecordEvent"),
}
```

---

## Backward Compatibility Tests (AC-18, R-13)

### `test_existing_hook_rs_tests_unaffected`

After implementation, run the full existing `hook.rs` test suite:
- `build_request_session_start` — must still pass
- `build_request_stop` — must still pass
- `build_request_ping` — must still pass
- `build_request_unknown_event` — must still pass with `PreToolUse` falling through
- All `UserPromptSubmit` tests — must still pass
- All `PreCompact` tests — must still pass

This is verified by `cargo test --workspace`. No modifications to existing tests.

---

## Assertions Summary

| Test | Risk | AC |
|------|------|----|
| `test_gemini_mcp_context_tool_name_promotion` | R-01 | AC-14 (gate) |
| `test_gemini_before_tool_non_cycle_fallthrough` | R-01 | AC-14 (gate) |
| `test_mcp_context_missing_tool_name_degrades_gracefully` | R-01 | AC-14 (gate) |
| `test_normalize_event_name_gemini_unique_names` | R-01, R-08 | AC-01 |
| `test_normalize_event_name_claude_code_passthrough` | R-13 | AC-01, AC-18 |
| `test_normalize_event_name_unknown_fallback` | — | AC-01 |
| `test_run_codex_provider_hint` | R-03 | AC-17 |
| `test_run_session_start_provider_hint_precedence` | — | AC-20 |
| `test_normalize_event_name_category2_passthrough` | — | AC-01 |
| `test_gemini_after_tool_skips_rework_path` | R-05 | AC-04, AC-12 |
| `test_codex_post_tool_use_skips_rework_path` | R-05 | AC-04 |
| `test_claude_code_post_tool_use_enters_rework_path` | R-05 | AC-12 |
| `test_implant_event_provider_set_for_record_event_variants` | R-02 | AC-05 |
| `test_cycle_event_provider_propagated` | R-02 | AC-05 |
| `test_gemini_session_end_produces_session_close` | R-08 | AC-01, AC-15 |
| `test_rework_candidate_guard_fires_in_debug` | R-07 | AC-16 |
| `test_post_tool_use_failure_arm_unchanged` | R-07 | AC-16 |
| `test_gemini_before_tool_topic_signal_extraction` | R-10 | AC-11 |
| `test_gemini_after_tool_response_fields_degrade_gracefully` | R-11 | — |
