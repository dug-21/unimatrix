# Test Plan: hook-posttooluse

## Component Scope

`crates/unimatrix-server/src/hook.rs` — PostToolUse arm, helper functions, Stop outcome

## Unit Tests

### is_rework_eligible_tool (FR-07.2)

**`test_is_rework_eligible_bash`** — "Bash" → true
**`test_is_rework_eligible_edit`** — "Edit" → true
**`test_is_rework_eligible_write`** — "Write" → true
**`test_is_rework_eligible_multiedit`** — "MultiEdit" → true
**`test_is_rework_eligible_read`** — "Read" → false
**`test_is_rework_eligible_glob`** — "Glob" → false
**`test_is_rework_eligible_empty`** — "" → false

### is_bash_failure (FR-07.3, R-09)

**`test_bash_failure_exit_code_nonzero`** (R-09 scenario 1)
- extra = `{"exit_code": 1}` → true

**`test_bash_failure_exit_code_zero`** (R-09 scenario 2)
- extra = `{"exit_code": 0}` → false

**`test_bash_failure_missing_exit_code`** (R-09 scenario 3)
- extra = `{}` → false

**`test_bash_failure_interrupted_true`** (R-09 scenario 4)
- extra = `{"interrupted": true}` → true

**`test_bash_failure_interrupted_false`**
- extra = `{"interrupted": false}` → false

**`test_bash_failure_exit_code_string_type`**
- extra = `{"exit_code": "1"}` → false (string, not integer — safe default)

**`test_bash_failure_exit_code_null`**
- extra = `{"exit_code": null}` → false

**`test_bash_failure_exit_code_negative`**
- extra = `{"exit_code": -1}` → true (non-zero)

### extract_file_path (FR-07.5, R-09)

**`test_extract_file_path_edit`** (R-09 scenario 5)
- extra = `{"tool_input": {"path": "src/foo.rs"}}`, tool_name = "Edit"
- Returns `Some("src/foo.rs")`

**`test_extract_file_path_write`** (R-09 scenario 6)
- extra = `{"tool_input": {"file_path": "src/bar.rs"}}`, tool_name = "Write"
- Returns `Some("src/bar.rs")`

**`test_extract_file_path_bash_returns_none`**
- tool_name = "Bash" → None (Bash has no file_path concept)

**`test_extract_file_path_missing_tool_input`**
- extra = `{}`, tool_name = "Edit" → None

**`test_extract_file_path_missing_path_field`**
- extra = `{"tool_input": {}}`, tool_name = "Edit" → None

**`test_extract_file_path_null_extra`** (R-09 scenario 10)
- extra = `null` → None (no panic)

### extract_rework_events_for_multiedit (FR-07.5)

**`test_multiedit_two_paths`** (R-09 scenario 7)
- extra = `{"tool_input": {"edits": [{"path": "a.rs"}, {"path": "b.rs"}]}}`
- Returns vec![(Some("a.rs"), false), (Some("b.rs"), false)]

**`test_multiedit_empty_edits`** (EC-03)
- extra = `{"tool_input": {"edits": []}}` → empty Vec

**`test_multiedit_missing_edits_field`**
- extra = `{"tool_input": {}}` → empty Vec

**`test_multiedit_missing_tool_input`**
- extra = `{}` → empty Vec

**`test_multiedit_edit_with_null_path`**
- extra = `{"tool_input": {"edits": [{"path": null}]}}` → vec![(None, false)]

### build_request PostToolUse arm (FR-07.1, FR-07.6)

**`test_build_request_posttooluse_bash_failure`**
- Input: event="PostToolUse", extra = `{"tool_name": "Bash", "exit_code": 1}`
- Returns: HookRequest::RecordEvent with event_type="post_tool_use_rework_candidate", had_failure=true

**`test_build_request_posttooluse_bash_success`**
- extra = `{"tool_name": "Bash", "exit_code": 0}`
- Returns RecordEvent with had_failure=false

**`test_build_request_posttooluse_edit`**
- extra = `{"tool_name": "Edit", "tool_input": {"path": "src/foo.rs"}}`
- Returns RecordEvent with event_type="post_tool_use_rework_candidate", file_path=Some("src/foo.rs")

**`test_build_request_posttooluse_multiedit_two_paths`**
- extra = `{"tool_name": "MultiEdit", "tool_input": {"edits": [{"path": "a.rs"}, {"path": "b.rs"}]}}`
- Returns HookRequest::RecordEvents with 2 events

**`test_build_request_posttooluse_non_rework_tool`** (FR-07.2)
- extra = `{"tool_name": "Read"}`
- Returns generic RecordEvent (NOT rework-candidate event)

**`test_build_request_posttooluse_missing_tool_name`** (R-09 scenario 9)
- extra = `{}` — no tool_name field
- Returns generic RecordEvent, no panic

**`test_build_request_posttooluse_null_extra`** (R-09 scenario 10)
- extra is serde_json::Value::Null
- Returns generic RecordEvent, no panic

### build_request Stop arm (FR-08.1)

**`test_build_request_stop_sets_outcome_success`** (AC related, FR-08.1)
- event="Stop"
- Returns HookRequest::SessionClose { outcome: Some("success") }

**`test_build_request_taskcompleted_sets_outcome_success`** (FR-08.2)
- event="TaskCompleted"
- Returns HookRequest::SessionClose { outcome: Some("success") }

### is_fire_and_forget

**`test_posttooluse_record_event_is_fire_and_forget`** (FR-07.8)
- HookRequest::RecordEvent { event_type: "post_tool_use_rework_candidate" }
- is_fire_and_forget returns true

## Integration Tests

**`test_settings_json_has_posttooluse_hook`**
- Read .claude/settings.json
- Assert: PostToolUse hook entry exists with command "unimatrix hook PostToolUse"

## Edge Cases

- Extra fields with unexpected types: `{"exit_code": "not_a_number"}` → false, no panic (SEC-01)
- Very long file_path string: extracts correctly, stored as-is
- MultiEdit with 100 paths: produces 100 RecordEvents without memory issues
