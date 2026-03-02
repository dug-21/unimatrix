# Pseudocode: hook-posttooluse

## Purpose

Extend `hook.rs` to parse PostToolUse hook events and dispatch rework-candidate `RecordEvent`s. Also updates the `Stop` arm to set `outcome = "success"`. Adds helper functions for field extraction.

## Files

- MODIFY `crates/unimatrix-server/src/hook.rs`
- MODIFY `.claude/settings.json` — register PostToolUse hook

## New helpers (add to `hook.rs`)

```rust
/// Returns true if the tool_name is rework-eligible (file-mutating tools).
/// Rework-eligible: Bash, Edit, Write, MultiEdit. All others return false.
fn is_rework_eligible_tool(tool_name: &str) -> bool {
    matches!(tool_name, "Bash" | "Edit" | "Write" | "MultiEdit")
}

/// Returns true if the Bash tool call had a failure.
/// Failure = exit_code is non-zero integer, OR interrupted is true.
/// All other cases (missing fields, non-integer exit_code) return false.
fn is_bash_failure(extra: &serde_json::Value) -> bool {
    // Check exit_code: non-zero integer
    if let Some(exit_code) = extra.get("exit_code").and_then(|v| v.as_i64()) {
        if exit_code != 0 {
            return true;
        }
    }
    // Check interrupted: boolean true
    if extra.get("interrupted").and_then(|v| v.as_bool()).unwrap_or(false) {
        return true;
    }
    false
}

/// Extract file_path for Edit or Write tools.
/// Edit: extra["tool_input"]["path"]
/// Write: extra["tool_input"]["file_path"]
/// Returns None if the field is absent or not a string.
fn extract_file_path(extra: &serde_json::Value, tool_name: &str) -> Option<String> {
    match tool_name {
        "Edit" => extra
            .get("tool_input")
            .and_then(|ti| ti.get("path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "Write" => extra
            .get("tool_input")
            .and_then(|ti| ti.get("file_path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        _ => None,
    }
}

/// Extract (file_path, had_failure) pairs for MultiEdit.
/// MultiEdit has extra["tool_input"]["edits"] = array of {path, ...}
/// Each distinct path produces one entry with had_failure=false (Edit tools can't fail).
/// Empty edits array → empty Vec. Missing fields → empty Vec (no panic).
fn extract_rework_events_for_multiedit(extra: &serde_json::Value) -> Vec<(Option<String>, bool)> {
    let edits = match extra.get("tool_input").and_then(|ti| ti.get("edits")) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let arr = match edits.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };

    let mut results = Vec::new();
    for edit in arr {
        let path = edit.get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        results.push((path, false)); // Edit tools can't fail (ADR-002)
    }
    results
}
```

## Modified: `build_request` — new `"PostToolUse"` arm

Add BEFORE the catch-all / generic RecordEvent arm:

```rust
"PostToolUse" => {
    let tool_name = hook_input.extra
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if !is_rework_eligible_tool(&tool_name) {
        // Non-rework tool: fall through to generic RecordEvent
        return generic_record_event(event_type, session_id, hook_input);
    }

    // MultiEdit: generate one RecordEvent per path
    if tool_name == "MultiEdit" {
        let pairs = extract_rework_events_for_multiedit(&hook_input.extra);
        if pairs.is_empty() {
            // No edits in MultiEdit — safe to return generic RecordEvent
            return generic_record_event(event_type, session_id, hook_input);
        }
        // For MultiEdit with multiple paths, generate multiple RecordEvents.
        // Since HookRequest doesn't currently batch, produce RecordEvents array.
        // Use HookRequest::RecordEvents if available, or the first path as RecordEvent.
        // Architecture specifies: "generate one ReworkEvent per distinct path in edits array"
        // RecordEvents variant exists (wire.rs): use it when pairs.len() > 1
        // For simplicity and compatibility, use RecordEvents for all MultiEdit cases.
        let events: Vec<ImplantEvent> = pairs.into_iter().map(|(file_path, had_failure)| {
            ImplantEvent {
                event_type: "post_tool_use_rework_candidate".to_string(),
                session_id: session_id.clone(),
                timestamp: now_secs(),
                payload: serde_json::json!({
                    "tool_name": "MultiEdit",
                    "file_path": file_path,
                    "had_failure": had_failure,
                }),
            }
        }).collect();
        return HookRequest::RecordEvents { events };
    }

    // Bash, Edit, Write: single RecordEvent
    let had_failure = match tool_name.as_str() {
        "Bash" => is_bash_failure(&hook_input.extra),
        _ => false, // Edit, Write cannot fail (ADR-002)
    };
    let file_path = extract_file_path(&hook_input.extra, &tool_name);

    HookRequest::RecordEvent {
        event: ImplantEvent {
            event_type: "post_tool_use_rework_candidate".to_string(),
            session_id,
            timestamp: now_secs(),
            payload: serde_json::json!({
                "tool_name": tool_name,
                "file_path": file_path,
                "had_failure": had_failure,
            }),
        },
    }
}
```

## Modified: `build_request` — `"Stop"` arm

Change to set `outcome = Some("success")`:

```rust
"Stop" => HookRequest::SessionClose {
    session_id,
    outcome: Some("success".to_string()),  // Server overrides to "rework" if threshold crossed
},
```

If `"TaskCompleted"` is handled, treat it identically to `"Stop"` (FR-08.2).

## Modified: `is_fire_and_forget`

`RecordEvent` and `RecordEvents` are already fire-and-forget in the existing match. No change needed for PostToolUse rework events — they remain fire-and-forget (FR-07.8).

Verify: the existing `is_fire_and_forget` check includes `HookRequest::RecordEvent` and `HookRequest::RecordEvents`. If `RecordEvents` is missing, add it.

## Modification: `.claude/settings.json`

Add PostToolUse hook registration alongside existing hooks. The exact format depends on the existing hook registration format in `settings.json`.

Typical Claude Code settings.json hook format:
```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "unimatrix hook PostToolUse"
          }
        ]
      }
    ]
  }
}
```

Add `"PostToolUse"` entry following the same pattern as existing `"Stop"` / `"UserPromptSubmit"` entries.

## Error Handling

- All field extractions use `.get()` / `.as_str()` / `.as_i64()` / `.as_bool()` — return `None`/`false` on missing or wrong-type fields (never panic)
- Missing `tool_name` → treated as non-rework tool → generic RecordEvent
- `hook_input.extra` is `serde_json::Value::Null` → all `.get()` calls return `None` → safe defaults

## Key Test Scenarios

1. `test_posttooluse_bash_failure_exit_code_nonzero` — exit_code=1 → had_failure=true
2. `test_posttooluse_bash_success_exit_code_zero` — exit_code=0 → had_failure=false
3. `test_posttooluse_bash_missing_exit_code` — no exit_code field → had_failure=false
4. `test_posttooluse_bash_interrupted_true` — interrupted=true → had_failure=true
5. `test_posttooluse_edit_extracts_path` — tool_input.path="src/foo.rs" → file_path=Some("src/foo.rs")
6. `test_posttooluse_write_extracts_file_path` — tool_input.file_path="src/bar.rs" → file_path=Some("src/bar.rs")
7. `test_posttooluse_multiedit_two_paths` — edits=[{path:"a.rs"},{path:"b.rs"}] → RecordEvents with 2 events
8. `test_posttooluse_multiedit_empty_edits` — edits=[] → generic RecordEvent
9. `test_posttooluse_non_rework_tool` — tool_name="Read" → generic RecordEvent (not rework-candidate)
10. `test_posttooluse_missing_tool_name` — no tool_name field → generic RecordEvent, no panic
11. `test_posttooluse_null_extra` — extra is Value::Null → generic RecordEvent, no panic
12. `test_stop_sets_outcome_success` — build_request("Stop", ...) → HookRequest::SessionClose { outcome: Some("success") }
