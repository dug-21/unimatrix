# Hook Routing — Pseudocode
# File: crates/unimatrix-server/src/uds/hook.rs

## Purpose

Three changes to `hook.rs`:

1. Add `"SubagentStart"` match arm in `build_request` (before `_` fallthrough) that routes
   non-empty `prompt_snippet` to `HookRequest::ContextSearch` with `source: Some("SubagentStart")`.
2. Add `MIN_QUERY_WORDS: usize = 5` constant and word-count guard to the `"UserPromptSubmit"` arm.
3. Add `write_stdout_subagent_inject` helper function that writes the `hookSpecificOutput` JSON
   envelope required by Claude Code for SubagentStart context injection.
4. Update `run()` to branch on `source` when writing response to stdout.

---

## Constants

Add at module level (alongside existing `HOOK_TIMEOUT`, `MAX_INJECTION_BYTES`):

```
/// Minimum word count for UserPromptSubmit to route to ContextSearch.
/// Prompts shorter than this threshold fall through to generic_record_event.
/// Evaluated on query.trim().split_whitespace().count() (leading/trailing
/// whitespace is NOT counted). See ADR-002 crt-027.
const MIN_QUERY_WORDS: usize = 5;
```

---

## Modified Function: `build_request`

### Existing signature (unchanged):
```
fn build_request(event: &str, input: &HookInput) -> HookRequest
```

### Change 1: Add SubagentStart arm before `_` fallthrough

Location: Inside the `match event` block in `build_request`, before the `_ =>` arm.
The arm must be placed BEFORE `_ => generic_record_event(event, session_id, input)`.

```
"SubagentStart" => {
    // Extract prompt_snippet from extra (col-017 also reads this for topic signal)
    let query = input.extra
        .get("prompt_snippet")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Guard: empty or whitespace-only prompt_snippet falls through to RecordEvent
    // EC-01: .trim().is_empty() catches "   " (whitespace-only), not just ""
    if query.trim().is_empty() {
        return generic_record_event(event, session_id, input);
    }

    // Route to ContextSearch with source="SubagentStart"
    // session_id = input.session_id (parent session, not ppid fallback)
    // SubagentStart fires in parent session context before subagent starts
    HookRequest::ContextSearch {
        query,
        session_id: input.session_id.clone(),  // parent session → WA-2 boost applies
        source: Some("SubagentStart".to_string()),
        role: None,
        task: None,
        feature: None,
        k: None,
        max_tokens: None,
    }
}
```

Note on session_id: The existing `session_id` local variable at top of `build_request` uses
`input.session_id.clone().unwrap_or_else(|| format!("ppid-{}", ...))`. The `ContextSearch`
variant requires the raw `input.session_id` (which may be `None`), NOT the ppid fallback.
Use `input.session_id.clone()` directly in the ContextSearch struct, not the resolved
`session_id` variable. This matches how the `"UserPromptSubmit"` arm already works (line 261:
`session_id: input.session_id.clone()`).

### Change 2: Add word-count guard to UserPromptSubmit arm

**Before** (current logic at line ~254):
```
"UserPromptSubmit" => {
    let query = input.prompt.clone().unwrap_or_default();
    if query.is_empty() {
        return generic_record_event(event, session_id, input);
    } else {
        HookRequest::ContextSearch {
            query,
            session_id: input.session_id.clone(),
            role: None,
            task: None,
            feature: None,
            k: None,
            max_tokens: None,
        }
    }
}
```

**After**:
```
"UserPromptSubmit" => {
    let query = input.prompt.clone().unwrap_or_default();

    // Guard 1: empty or whitespace-only → RecordEvent (existing behavior, but now uses .trim())
    if query.trim().is_empty() {
        return generic_record_event(event, session_id, input);
    }

    // Guard 2: word-count threshold (ADR-002, FR-05)
    // Trims before counting so "  approve  " counts as 1 word, not 1 word with padding
    let word_count = query.trim().split_whitespace().count();
    if word_count < MIN_QUERY_WORDS {
        return generic_record_event(event, session_id, input);
    }

    // Route to ContextSearch: source=None (backward compat, treated as UserPromptSubmit)
    HookRequest::ContextSearch {
        query,
        session_id: input.session_id.clone(),
        source: None,   // ADR-001: None → "UserPromptSubmit" at server
        role: None,
        task: None,
        feature: None,
        k: None,
        max_tokens: None,
    }
}
```

Note: `query` is the original (untrimmed) string passed to SearchService. Trimming is
evaluation-only for the guards, not modification of the query string itself (ADR-002).

---

## New Function: `write_stdout_subagent_inject`

Add alongside existing `write_stdout` (after it, not replacing it):

```
fn write_stdout_subagent_inject(entries_text: &str) -> io::Result<()> {
    use std::io::Write;
    // Build the hookSpecificOutput JSON envelope required by Claude Code
    // for SubagentStart context injection (ADR-006 crt-027)
    let envelope = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SubagentStart",
            "additionalContext": entries_text
        }
    });
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    // writeln appends a newline after the JSON object
    writeln!(handle, "{}", envelope)
}
```

Visibility: `fn write_stdout_subagent_inject` (module-private, same as `write_stdout`).
Import: `use std::io` is already present at top of file.
`serde_json` is already a dependency (used in `write_stdout` via `serde_json::to_string`).

---

## Modified Function: `run`

The `run` function currently calls `write_stdout(&response)` for all synchronous responses.
For SubagentStart, `write_stdout_subagent_inject` must be called instead.

The source value is only in the request, not the response. The request must be retained
or the source extracted before the `transport.request()` call.

**Algorithm**:

```
fn run(event: String, project_dir: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    // ... existing steps 1-5 unchanged ...

    // Step 5: Build request from event + input
    let request = build_request(&event, &hook_input);

    // Step 5b: Extract source BEFORE consuming the request (needed for response routing)
    // Only ContextSearch carries a source; extract it now for use after transport.request()
    let req_source: Option<String> = match &request {
        HookRequest::ContextSearch { source, .. } => source.clone(),
        _ => None,
    };

    // Step 6: Determine if fire-and-forget or synchronous (unchanged)
    let is_fire_and_forget = matches!(
        request,
        HookRequest::SessionRegister { .. }
            | HookRequest::SessionClose { .. }
            | HookRequest::RecordEvent { .. }
            | HookRequest::RecordEvents { .. }
    );

    // Step 7: Connect and send (unchanged connection logic)
    let mut transport = LocalTransport::new(socket_path, HOOK_TIMEOUT);

    match transport.connect() {
        Ok(()) => {
            // ... existing replay logic unchanged ...

            if is_fire_and_forget {
                // ... existing fire-and-forget logic unchanged ...
            } else {
                match transport.request(&request, HOOK_TIMEOUT) {
                    Ok(response) => {
                        // Route stdout writing based on source
                        let write_result = if req_source.as_deref() == Some("SubagentStart") {
                            // SubagentStart: write hookSpecificOutput JSON envelope
                            write_stdout_subagent_inject_from_response(&response)
                        } else {
                            // All other sources (UserPromptSubmit, etc.): plain text
                            write_stdout(&response)
                        };
                        if let Err(e) = write_result {
                            eprintln!("unimatrix: stdout write failed: {e}");
                        }
                    }
                    Err(e) => {
                        eprintln!("unimatrix: request failed: {e}");
                    }
                }
            }
        }
        // ... existing error handling unchanged ...
    }

    Ok(())
}
```

The `write_stdout_subagent_inject_from_response` helper extracts the formatted text from
the `HookResponse::Entries` variant and then calls `write_stdout_subagent_inject`. If the
response is not `Entries`, fall through to plain-text write (graceful degradation):

```
fn write_stdout_subagent_inject_from_response(
    response: &HookResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    match response {
        HookResponse::Entries { items, .. } => {
            // Format entries using existing format_injection (same content as write_stdout)
            if let Some(text) = format_injection(items, MAX_INJECTION_BYTES) {
                write_stdout_subagent_inject(&text)?;
            }
            // Empty items: silent skip (no stdout written — same as write_stdout behavior)
            Ok(())
        }
        // For non-Entries responses from SubagentStart ContextSearch (unexpected but safe):
        // fall through to plain-text write
        other => write_stdout(other),
    }
}
```

Alternative simpler approach: instead of a wrapper function, inline the logic directly
in the `run` match. Either approach is acceptable — prefer whichever minimizes duplication
with the existing `write_stdout` implementation.

---

## Error Handling

- `write_stdout_subagent_inject` returns `io::Result<()>`. Callers must handle errors.
- `run` already handles `write_stdout` errors with `eprintln!` and continues. Apply the
  same pattern to `write_stdout_subagent_inject_from_response`.
- Exit code is always 0 (FR-06, C-01). No error in this function produces a non-zero exit.
- If `write_stdout_subagent_inject` fails (e.g., stdout closed), log via `eprintln!` and
  return `Ok(())` from `run` (the hook must not exit non-zero).

---

## Key Test Scenarios

All in `hook.rs` `#[cfg(test)]` block.

**T-HR-01** `build_request_subagentstart_with_prompt_snippet` (AC-01, non-negotiable):
- Input: event="SubagentStart", extra={"prompt_snippet": "implement the spec writer agent"}
  session_id=Some("parent-sid")
- Assert: returns HookRequest::ContextSearch
- Assert: query == "implement the spec writer agent"
- Assert: source == Some("SubagentStart")
- Assert: session_id == Some("parent-sid")  (NOT ppid fallback)
- Assert: role/task/feature/k/max_tokens == None

**T-HR-02** `build_request_subagentstart_empty_prompt_snippet` (AC-02, non-negotiable):
- Input: event="SubagentStart", extra={"prompt_snippet": ""}
- Assert: returns HookRequest::RecordEvent (not ContextSearch)

**T-HR-03** `build_request_subagentstart_absent_prompt_snippet` (AC-02):
- Input: event="SubagentStart", extra={} (no prompt_snippet key)
- Assert: returns HookRequest::RecordEvent

**T-HR-04** `build_request_subagentstart_whitespace_only` (AC-23b, EC-01):
- Input: event="SubagentStart", extra={"prompt_snippet": "   "}
- Assert: returns HookRequest::RecordEvent (whitespace-only treated as empty)

**T-HR-05** `build_request_subagentstart_one_word_routes_to_context_search` (AC-23):
- Input: event="SubagentStart", extra={"prompt_snippet": "implement"}
- Assert: returns HookRequest::ContextSearch
  (SubagentStart is NOT subject to MIN_QUERY_WORDS guard)

**T-HR-06** `build_request_userpromptsub_four_words_record_event` (AC-22, non-negotiable):
- Input: event="UserPromptSubmit", prompt=Some("yes ok thanks friend")
- Assert: returns HookRequest::RecordEvent (4 words < MIN_QUERY_WORDS=5)

**T-HR-07** `build_request_userpromptsub_five_words_context_search` (AC-22, non-negotiable):
- Input: event="UserPromptSubmit", prompt=Some("implement the spec writer agent")
- Assert: returns HookRequest::ContextSearch (5 words == MIN_QUERY_WORDS=5)

**T-HR-08** `build_request_userpromptsub_six_words_context_search` (AC-02b):
- Input: event="UserPromptSubmit", prompt=Some("implement the spec writer agent today")
- Assert: returns HookRequest::ContextSearch (6 words > MIN_QUERY_WORDS)

**T-HR-09** `build_request_userpromptsub_one_word_record_event` (AC-02b):
- Input: event="UserPromptSubmit", prompt=Some("ok")
- Assert: returns HookRequest::RecordEvent

**T-HR-10** `build_request_userpromptsub_whitespace_padding_not_counted` (AC-23c):
- Input: event="UserPromptSubmit", prompt=Some("  approve  ")
- Assert: returns HookRequest::RecordEvent (1 real word, < MIN_QUERY_WORDS)

**T-HR-11** `build_request_userpromptsub_source_is_none` (AC-05):
- Input: event="UserPromptSubmit", prompt=Some("implement the spec writer agent today")
- Assert: ContextSearch returned with source == None

**T-HR-12** `write_stdout_subagent_inject_produces_valid_json_envelope` (AC-SR02):
- Call: write_stdout_subagent_inject("some entries text")
- Capture stdout bytes (via a test helper that redirects stdout, or inspect serde_json output)
- Assert: output is valid JSON
- Assert: JSON has field `hookSpecificOutput.hookEventName == "SubagentStart"`
- Assert: JSON has field `hookSpecificOutput.additionalContext == "some entries text"`

**T-HR-13** `write_stdout_plain_text_for_userpromptsub` (AC-SR03):
- Call: write_stdout(&HookResponse::Entries { items: vec![...], total_tokens: 0 })
- Assert: output does NOT contain "hookSpecificOutput"
- Assert: output does NOT start with "{"  (it's plain text, not JSON envelope)

Note: Tests T-HR-12 and T-HR-13 can be unit tests on the formatting functions themselves
rather than full stdout-capture integration tests, which are harder to write in Rust without
test infrastructure. The format can be verified by constructing the `serde_json::json!` value
directly and asserting fields.
