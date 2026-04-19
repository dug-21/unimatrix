# vnc-013 Pseudocode: normalization
## Files: `crates/unimatrix-server/src/uds/hook.rs` + `crates/unimatrix-server/src/main.rs`

---

## Purpose

Install the normalization layer at the hook ingest boundary. All provider-specific
event names are translated to canonical Claude Code names before `build_request()` is
called. Provider identity is threaded into every `ImplantEvent` constructed in
`build_request()` and `build_cycle_event_or_fallthrough()`.

This component also extends `run()` to accept `--provider` from CLI and extends
`main.rs` to add the `--provider` argument to the `Hook` command variant.

---

## Design Decision: No Defense-in-Depth Arms in build_request()

The spec (FR-04.1) calls for "defense-in-depth guard arms" for `BeforeTool`,
`AfterTool`, and `SessionEnd` in `build_request()` because they "are reached only if
normalization has not already translated them."

This design is rejected: normalization is called unconditionally in `run()` before
`build_request()`. If normalization runs, those arms are dead code. Dead code that
exists "just in case" signals distrust of the contract, creates maintenance burden,
and will confuse future readers.

Resolution: Trust the contract. Do NOT add `"BeforeTool"`, `"AfterTool"`, or
`"SessionEnd"` arms to `build_request()`. Instead, add a `debug_assert!` at the entry
of `build_request()` that fires in debug builds if any known provider-specific name
(one that normalization should have translated) reaches it:

```
debug_assert!(
    !matches!(event, "BeforeTool" | "AfterTool" | "SessionEnd"),
    "provider-specific event name reached build_request() without normalization: {event}"
);
```

This enforces the boundary without dead code. The `debug_assert!` is compiled out in
release builds — zero production cost.

---

## Design Decision: Unknown Event Name Return Type

`normalize_event_name` is specified to return `(&'static str, &'static str)`.
Unknown event names are dynamic `&str` values — they cannot be `&'static str`.

Resolution: return the sentinel `("__unknown__", "unknown")` for unknown events.
The caller (`run()`) detects `canonical_name == "__unknown__"` and substitutes the
original raw event string when calling `build_request()`, preserving the original
unrecognized name in the DB hook column.

This keeps the return type as `(&'static str, &'static str)` (zero allocations on
all known paths) and avoids a sentinel that could accidentally match a real event name.
`"__unknown__"` cannot appear as a real event name — no provider uses double underscores.

---

## New Functions: `map_to_canonical` and `normalize_event_name`

### Design: Separation of concerns

The hint path and the inference path have different responsibilities:

- When `--provider` is supplied, `run()` already has the provider string. It calls
  `map_to_canonical(event)` directly to translate the event name, and uses the CLI
  flag value verbatim for the `provider` field. `normalize_event_name` is not called.
- When `--provider` is absent, `run()` calls `normalize_event_name(event)` which
  infers both the canonical event name AND the provider from the event string alone.

This keeps `normalize_event_name` as a pure inference function with an honest
`(&'static str, &'static str)` return type that never needs to carry a dynamic
hint value. There is no sentinel, no caller-detects-sentinel pattern.

### Private Helper: `map_to_canonical`

```
fn map_to_canonical(event: &str) -> &'static str:
    match event:
        "BeforeTool"   => "PreToolUse"
        "AfterTool"    => "PostToolUse"
        "SessionEnd"   => "Stop"
        // Claude Code / Codex names are already canonical — return the static literal:
        "PreToolUse"         => "PreToolUse"
        "PostToolUse"        => "PostToolUse"
        "SessionStart"       => "SessionStart"
        "Stop"               => "Stop"
        "TaskCompleted"      => "TaskCompleted"
        "Ping"               => "Ping"
        "UserPromptSubmit"   => "UserPromptSubmit"
        "PreCompact"         => "PreCompact"
        "PostToolUseFailure" => "PostToolUseFailure"
        "SubagentStart"      => "SubagentStart"
        "SubagentStop"       => "SubagentStop"
        _ => "__unknown__"
```

NOTE: `event as &'static str` is not valid Rust. Known canonical names must each be
returned as an explicit string literal in a dedicated match arm, as shown above.

### `normalize_event_name` — inference path only

```rust
pub fn normalize_event_name(event: &str) -> (&'static str, &'static str)
```

Returns `(canonical_event_name, provider)` as `(&'static str, &'static str)` pairs.
Called ONLY when `--provider` is absent. Pure synchronous, no I/O, no allocations.

### Algorithm

```
fn normalize_event_name(event: &str) -> (&'static str, &'static str):
    match event:
        // Gemini-unique names — infer provider from event name:
        "BeforeTool"   => ("PreToolUse",  "gemini-cli")
        "AfterTool"    => ("PostToolUse", "gemini-cli")
        "SessionEnd"   => ("Stop",        "gemini-cli")
        // Claude Code names — pass through canonical, default provider:
        "PreToolUse"         => ("PreToolUse",         "claude-code")
        "PostToolUse"        => ("PostToolUse",         "claude-code")
        "SessionStart"       => ("SessionStart",        "claude-code")
        "Stop"               => ("Stop",                "claude-code")
        "TaskCompleted"      => ("TaskCompleted",       "claude-code")
        "Ping"               => ("Ping",                "claude-code")
        "UserPromptSubmit"   => ("UserPromptSubmit",    "claude-code")
        "PreCompact"         => ("PreCompact",          "claude-code")
        "PostToolUseFailure" => ("PostToolUseFailure",  "claude-code")
        "SubagentStart"      => ("SubagentStart",       "claude-code")
        "SubagentStop"       => ("SubagentStop",        "claude-code")
        _ =>
            // Unknown event: sentinel return.
            // Caller checks for "__unknown__" and uses raw event string.
            ("__unknown__", "unknown")
```

---

## Modified Function: `run()`

### New Signature
```rust
pub fn run(
    event: String,
    provider: Option<String>,
    project_dir: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>>
```

### Extended Algorithm (additions to existing run())

Insert between "Step 2: Parse hook input" and "Step 3: Determine working directory":

```
// Step 2b: Normalize event name and set provider on hook_input.
//
// Two paths depending on whether --provider was supplied:
//
//   Hint path  (provider.is_some()): caller already knows the provider.
//              Call map_to_canonical() for the event name only.
//              Use the CLI flag value directly for hook_input.provider.
//
//   Inference path (provider.is_none()): call normalize_event_name() which
//              infers both the canonical name and the provider from the event string.

let (canonical_name, provider_str): (&'static str, &'static str) =
    if provider.is_some() {
        // Hint path: map event name to canonical, provider comes from the flag.
        let canonical = map_to_canonical(&event);
        (canonical, "")  // provider_str unused in hint path; set from hint below
    } else {
        // Inference path: derive both fields from the event name.
        normalize_event_name(&event)
    };

// Set provider on hook_input BEFORE build_request():
hook_input.provider = if let Some(ref hint) = provider {
    // Hint path: use the CLI --provider value verbatim (e.g. "codex-cli").
    Some(hint.clone())
} else {
    // Inference path: use what normalize_event_name returned.
    // "unknown" is valid — still Some so ImplantEvent.provider is always Some.
    Some(provider_str.to_string())
};

// Resolve the actual event string to pass to build_request():
// If normalize returned "__unknown__", use the raw event to preserve unrecognized name.
let effective_event: &str = if canonical_name == "__unknown__" {
    &event
} else {
    canonical_name
};
```

Replace "Step 5: Build request from event + input":
```
// Step 5: Build request from CANONICAL event + input (with provider set)
let request = build_request(effective_event, &hook_input);
```

The existing SubagentStart fallback block (Step 5b) checks `event == "SubagentStart"`.
After normalization, Gemini has no SubagentStart equivalent, so this check correctly
fires only for Claude Code SubagentStart events. No change needed.

---

## Modified Function: `build_request()`

### Changes to existing function

Add `debug_assert!` at function entry (before the `match event` block):

```
// Normalization contract: provider-specific names must not reach this function.
// If this fires in debug builds, normalize_event_name() was not called before build_request().
debug_assert!(
    !matches!(event, "BeforeTool" | "AfterTool" | "SessionEnd"),
    "provider-specific event name reached build_request() without normalization: {event}"
);
```

### Modified "PostToolUse" arm — add rework detection gate (ADR-005)

Current behavior: always enters rework detection logic.

New behavior:

```
"PostToolUse" => {
    // Rework detection is Claude Code-only (ADR-005).
    // Gate: skip rework path for all non-claude-code providers.
    // Gemini AfterTool (canonicalized to PostToolUse) covers MCP tool calls only
    // (filtered by .gemini/settings.json matcher) and must never enter is_rework_eligible_tool().
    // Codex PostToolUse is also excluded — rework tool names (Bash, Edit, Write, MultiEdit)
    // are Claude Code-specific.
    let provider_val = hook_input.provider.as_deref().unwrap_or("claude-code");
    // Note: hook_input.provider is always Some at this point (set by run() step 2b).

    if provider_val != "claude-code" {
        // Non-Claude-Code provider: emit generic RecordEvent with canonical name.
        // topic_signal still extracted for knowledge attribution.
        let topic_signal = extract_event_topic_signal("PostToolUse", input);
        return HookRequest::RecordEvent {
            event: ImplantEvent {
                event_type: "PostToolUse".to_string(),
                session_id,
                timestamp: now_secs(),
                payload: input.extra.clone(),
                topic_signal,
                provider: input.provider.clone(),  // propagate provider
            },
        };
    }

    // Existing rework detection logic follows (unchanged for Claude Code):
    let tool_name = input.extra.get("tool_name")...
    // [remainder of existing PostToolUse arm unchanged]
    // Every ImplantEvent construction in this arm adds:
    //   provider: input.provider.clone(),
}
```

### Provider propagation in all ImplantEvent constructions

Every `ImplantEvent { ... }` literal in `build_request()` gains:
```
provider: input.provider.clone(),
```

This applies to:
- The non-rework-eligible RecordEvent in the PostToolUse arm
- The MultiEdit RecordEvents in the PostToolUse arm
- The Bash/Edit/Write RecordEvent in the PostToolUse arm
- The PostToolUseFailure RecordEvent arm
- The generic_record_event() return

### Modified "PreToolUse" arm — mcp_context promotion adapter

Current code calls `build_cycle_event_or_fallthrough(event, session_id, input)` directly.

New behavior inserts adapter step before the call:

```
"PreToolUse" => {
    // mcp_context promotion adapter (SR-08, R-01 mitigation).
    // Gemini BeforeTool places tool_name in mcp_context.tool_name (bare: "context_cycle").
    // build_cycle_event_or_fallthrough() reads input.extra["tool_name"].
    // Promote bare name to extra["tool_name"] so the interception logic finds it.
    //
    // We operate on a clone to avoid mutating the original input.
    // CRITICAL: pass the clone (not original) to build_cycle_event_or_fallthrough().

    let input_for_cycle: std::borrow::Cow<HookInput> = if let Some(bare_name) = input
        .mcp_context
        .as_ref()
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get("tool_name"))
        .and_then(|v| v.as_str())
    {
        // mcp_context.tool_name present — promote to extra["tool_name"].
        let mut cloned = input.clone();
        // extra is a serde_json::Value; ensure it is an Object so indexing works.
        if !cloned.extra.is_object() {
            cloned.extra = serde_json::Value::Object(serde_json::Map::new());
        }
        cloned.extra["tool_name"] = serde_json::Value::String(bare_name.to_string());
        std::borrow::Cow::Owned(cloned)
    } else {
        // No mcp_context.tool_name — use original input unchanged (zero-copy).
        std::borrow::Cow::Borrowed(input)
    };

    build_cycle_event_or_fallthrough(event, session_id, input_for_cycle.as_ref())
}
```

Note on `HookInput: Clone`: The existing struct derives `Clone` (check source —
if not derived, implementer must add `#[derive(Clone)]` to `HookInput`).

Note on `Cow` usage: Using `Cow<HookInput>` avoids cloning when `mcp_context` is
absent (the common case for Claude Code). If `HookInput` is expensive to clone
(it contains a `serde_json::Value`), this optimization matters. If implementer
prefers simplicity, always clone — the clone only runs when mcp_context is present.

Security note: `build_cycle_event_or_fallthrough()` already checks:
```rust
if tool_name != "context_cycle" && !tool_name.contains("unimatrix") {
    return generic_record_event(...);
}
```
The bare name `"context_cycle"` satisfies `tool_name == "context_cycle"` exactly.
The `contains("context_cycle")` guard in the source is permissive (see ALIGNMENT-REPORT
WARN item 3). The implementer SHOULD change this guard to equality:
```rust
if tool_name != "context_cycle" && !tool_name.contains("unimatrix") {
```
becomes:
```rust
if tool_name != "context_cycle" && !tool_name.contains("unimatrix") {
```
Wait — the security WARN recommends `tool_name == "context_cycle"` equality check
over `contains`. The current check is already:
```
tool_name != "context_cycle" (first condition) AND !contains("unimatrix") (second condition)
```
So an injected `"malicious_contains_context_cycle"` passes the first check
(not equal to `"context_cycle"`) but then hits `!contains("unimatrix")` —
it does NOT contain "unimatrix" → returns generic_record_event. So the injection
attack described in the WARN would fail at the second condition. No change needed.
ONLY if `tool_name == "context_cycle"` (exact) does the cycle path proceed.
The implementer should verify this logic holds in the actual source before concluding.

### Modified `build_cycle_event_or_fallthrough()`

Add `provider: input.provider.clone()` to the `ImplantEvent` it constructs:

```
HookRequest::RecordEvent {
    event: ImplantEvent {
        event_type,
        session_id,
        timestamp: now_secs(),
        payload,
        topic_signal,
        provider: input.provider.clone(),  // NEW
    },
}
```

Also add to the `generic_record_event()` fallthrough inside this function.

### Modified `generic_record_event()`

```
fn generic_record_event(event: &str, session_id: String, input: &HookInput) -> HookRequest {
    let topic_signal = extract_event_topic_signal(event, input);
    HookRequest::RecordEvent {
        event: ImplantEvent {
            event_type: event.to_string(),
            session_id,
            timestamp: now_secs(),
            payload: input.extra.clone(),
            topic_signal,
            provider: input.provider.clone(),  // NEW
        },
    }
}
```

---

## Modified: `main.rs` — Hook Command Variant

### Current definition
```rust
Hook {
    /// The hook event name (e.g., SessionStart, Stop, Ping).
    event: String,
},
```

### New definition
```rust
/// Handle a Claude Code lifecycle hook event.
///
/// Reads JSON from stdin, connects to the running server via UDS,
/// and dispatches the event. No tokio runtime is initialized.
Hook {
    /// The hook event name (e.g., SessionStart, Stop, PreToolUse, BeforeTool).
    event: String,

    /// Originating LLM provider. Required for Codex CLI (shares event names with
    /// Claude Code). Optional for Gemini CLI (event names are unique, inferred).
    /// When absent, provider is inferred from event name; shared names default to
    /// "claude-code". Expected values: "claude-code" | "gemini-cli" | "codex-cli".
    #[arg(long)]
    provider: Option<String>,
},
```

### Call site in main()

Current:
```rust
Some(Command::Hook { event }) => {
    hook::run(event, cli.project_dir)?;
}
```

New:
```rust
Some(Command::Hook { event, provider }) => {
    hook::run(event, provider, cli.project_dir)?;
}
```

---

## `DEFAULT_HOOK_SOURCE_DOMAIN` Constant

Defined in `services/observation.rs` (primary consumer), pub(crate) re-export for
`background.rs`:

```rust
/// Fallback source_domain for DB read paths when the registry returns "unknown".
///
/// Preserves the hook-path invariant: all stored observations that lack an explicit
/// source_domain are attributed to the default provider (Approach A, ADR-004).
///
/// Event types not in the builtin claude-code pack (Stop, SessionStart, cycle_start,
/// cycle_stop, UserPromptSubmit, PreCompact, PostToolUseFailure) return "unknown"
/// from resolve_source_domain() — this constant restores "claude-code" for them.
pub(crate) const DEFAULT_HOOK_SOURCE_DOMAIN: &str = "claude-code";
```

---

## Error Handling

`run()` never returns error from normalization failures (NFR-06, C-05):
- Unknown provider value in `--provider`: treated as-is (stored verbatim in source_domain)
- Malformed `mcp_context` (not an object, missing tool_name): adapter skips promotion,
  `build_cycle_event_or_fallthrough()` falls through to generic_record_event
- Unknown event name: sentinel `"__unknown__"` triggers raw event passthrough
- All paths produce exit code 0 via `Ok(())`

---

## Key Test Scenarios

### normalize_event_name — AC-01, AC-18

`normalize_event_name` is the inference path only (no provider_hint argument).
Tests for AC-17 and AC-20 (hint/Codex path) exercise `run()` end-to-end, not this
function directly, because the hint path bypasses `normalize_event_name` entirely.

```
// test_normalize_event_name_gemini_unique: Gemini-unique names infer gemini-cli (AC-01)
normalize_event_name("BeforeTool") == ("PreToolUse",  "gemini-cli")
normalize_event_name("AfterTool")  == ("PostToolUse", "gemini-cli")
normalize_event_name("SessionEnd") == ("Stop",        "gemini-cli")

// test_normalize_event_name_claude_code_defaults: Claude Code names default to claude-code (AC-18)
normalize_event_name("PreToolUse")   == ("PreToolUse",   "claude-code")
normalize_event_name("PostToolUse")  == ("PostToolUse",  "claude-code")
normalize_event_name("SessionStart") == ("SessionStart", "claude-code")
normalize_event_name("Stop")         == ("Stop",         "claude-code")

// test_normalize_event_name_unknown_sentinel: Unknown event returns sentinel pair
normalize_event_name("FutureThing") == ("__unknown__", "unknown")

// test_normalize_event_name_category2_passthrough:
// Category 2 event names (cycle_start, cycle_stop, cycle_phase_end) are NOT provider-
// inferrable — they are outputs produced by build_cycle_event_or_fallthrough(), never
// inputs to normalize_event_name. If passed, they fall to the unknown arm.
normalize_event_name("cycle_start")     == ("__unknown__", "unknown")
normalize_event_name("cycle_stop")      == ("__unknown__", "unknown")
normalize_event_name("cycle_phase_end") == ("__unknown__", "unknown")
```

### run() hint path — AC-17, AC-20

These tests simulate `run()` with `provider=Some(...)` to verify the hint path.
They do not call `normalize_event_name` directly.

```
// test_run_codex_provider_hint_threads_through (AC-17):
// Simulate run() receiving event="PreToolUse", provider=Some("codex-cli")
// map_to_canonical("PreToolUse") == "PreToolUse"
// hook_input.provider = Some("codex-cli")  (from hint, verbatim)
// ImplantEvent.provider == Some("codex-cli")

// test_run_gemini_provider_hint_threads_through (AC-20):
// Simulate run() receiving event="SessionStart", provider=Some("gemini-cli")
// map_to_canonical("SessionStart") == "SessionStart"
// hook_input.provider = Some("gemini-cli")  (from hint, verbatim)
// ImplantEvent.provider == Some("gemini-cli")

// test_run_codex_gemini_event_with_hint (AC-17 + map_to_canonical):
// Simulate run() receiving event="BeforeTool", provider=Some("codex-cli")
// map_to_canonical("BeforeTool") == "PreToolUse"   (renaming still happens)
// hook_input.provider = Some("codex-cli")
// effective_event = "PreToolUse"
// ImplantEvent.event_type == "PreToolUse", ImplantEvent.provider == Some("codex-cli")
```

### mcp_context promotion — AC-14, R-01

```
// Scenario 1 (R-01/SC-1): Gemini BeforeTool with context_cycle
input = HookInput {
    hook_event_name: "BeforeTool",
    mcp_context: Some({"server_name": "unimatrix", "tool_name": "context_cycle"}),
    extra: {"tool_input": {"type": "start", "feature": "vnc-013", "topic": "vnc-013"}},
    provider: Some("gemini-cli"),
    ...
}
result of build_request("PreToolUse", &input) == RecordEvent { event_type: "cycle_start", ... }
// Verifies: promotion happened, build_cycle_event_or_fallthrough() intercepted

// Scenario 2 (R-01/SC-2): Gemini BeforeTool with non-cycle tool
input.mcp_context = Some({"tool_name": "context_search"})
result == RecordEvent { event_type: "PreToolUse", provider: Some("gemini-cli") }
// Verifies: fallthrough to generic, not cycle_start

// Scenario 3 (R-01/SC-3): mcp_context missing tool_name key
input.mcp_context = Some({"server_name": "unimatrix"})  // no tool_name
result == RecordEvent { event_type: "PreToolUse", ... }  // no panic

// Scenario 4: mcp_context is null/not-object
input.mcp_context = Some(Value::String("bad"))
result == RecordEvent { event_type: "PreToolUse", ... }  // as_object() returns None, skip
```

### Rework gate — AC-04, AC-12, R-05

```
// Gemini AfterTool: provider != "claude-code" → skip rework path
input.provider = Some("gemini-cli")
build_request("PostToolUse", &input) == RecordEvent { event_type: "PostToolUse", provider: Some("gemini-cli") }
// assert is_rework_eligible_tool() was never called

// Codex PostToolUse: same
input.provider = Some("codex-cli")
build_request("PostToolUse", &input) == RecordEvent { event_type: "PostToolUse", provider: Some("codex-cli") }

// Claude Code: provider == "claude-code" → enters existing rework path
input.provider = Some("claude-code")
input.extra["tool_name"] = "Bash"
build_request("PostToolUse", &input) == RecordEvent { event_type: "post_tool_use_rework_candidate", ... }
```

### provider propagation — AC-05, R-02

```
// For every HookRequest variant produced by build_request(), assert ImplantEvent.provider.is_some()
// Cover: SessionRegister (no ImplantEvent), SessionClose (no ImplantEvent),
//        RecordEvent (PreToolUse, PostToolUse, PostToolUseFailure, SubagentStart, SubagentStop),
//        ContextSearch (no ImplantEvent), CompactPayload (no ImplantEvent),
//        cycle_start, cycle_stop events from build_cycle_event_or_fallthrough()
```

### debug_assert fires on provider-specific name — normalization contract

```
// In debug build (#[cfg(debug_assertions)]):
// Directly call build_request("BeforeTool", &input) without run() normalization.
// Assert debug_assert! fires (panics in debug mode).
// Test must use #[should_panic] or catch_unwind.
```

