# Test Plan: C5 — Settings Merge

## Unit Tests (packages/unimatrix/test/merge-settings.test.js)

This is the highest-risk component (R-01, Critical). Tests must be exhaustive.

### R-01 Scenarios (all 7 from Risk Strategy)

- `test_merge_into_empty_file`: Input `{}`. Assert output has `hooks` with all 7 events, each containing one unimatrix hook entry with absolute path command.
- `test_merge_preserves_permissions_block`: Input `{ "permissions": { "allow": ["Read"], "deny": [] } }`. Assert `permissions` is unchanged after merge. Assert `hooks` added with 7 events.
- `test_merge_preserves_non_unimatrix_hooks`: Input has `hooks.PreToolUse` with a custom hook `{ "type": "command", "command": "my-tool pre-check" }`. After merge, assert custom hook still present AND unimatrix hook appended.
- `test_merge_updates_pre_rename_hooks`: Input has hooks with `unimatrix-server hook SessionStart`. After merge, assert command is replaced with `<binary-path> hook SessionStart` (new name, absolute path). Assert no duplicate entry.
- `test_merge_updates_absolute_path_hooks`: Input has hooks with `/old/path/unimatrix hook SessionStart`. After merge, assert path updated to new binary path. No duplicate.
- `test_merge_preserves_extra_top_level_keys`: Input has `{ "customKey": "value", "hooks": {} }`. After merge, assert `customKey` is still `"value"`.
- `test_merge_idempotent_round_trip`: Merge into empty, capture output. Merge again into that output with same config. Assert JSON is identical.

### Hook Event Coverage

- `test_all_7_events_present`: After merge, assert keys `SessionStart`, `Stop`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `SubagentStart`, `SubagentStop` all exist under `hooks`.
- `test_each_event_has_exactly_one_unimatrix_entry`: After merging twice, count entries matching the unimatrix identification patterns per event. Assert exactly 1 per event.

### Identification Patterns (ADR-004)

- `test_identifies_bare_unimatrix_hook`: Command `"unimatrix hook SessionStart"` matches.
- `test_identifies_bare_unimatrix_server_hook`: Command `"unimatrix-server hook SessionStart"` matches.
- `test_identifies_absolute_path_unimatrix`: Command `"/path/to/unimatrix hook SessionStart"` matches.
- `test_identifies_absolute_path_unimatrix_server`: Command `"/old/path/unimatrix-server hook SessionStart"` matches.
- `test_does_not_identify_custom_hook`: Command `"my-tool hook SessionStart"` does NOT match unimatrix pattern.

### R-14 Error Handling

- `test_malformed_json_errors_with_diagnostic`: Input file contains `{invalid json`. Assert function throws/returns error with message mentioning "JSON" and the file path. Assert file is NOT modified.
- `test_empty_file_treated_as_empty_object`: Input file is 0 bytes. Assert merge succeeds as if input were `{}`.
- `test_hooks_key_not_object_errors`: Input `{ "hooks": "string" }`. Assert error with diagnostic.

### Output Format

- `test_output_uses_2_space_indentation`: Assert `JSON.stringify` output uses 2-space indent.
- `test_actions_array_describes_changes`: Assert `mergeSettings()` return value has `actions` array with human-readable strings describing what was added/updated.

### R-04 Dedup Across Multiple Runs

- `test_three_consecutive_merges_no_growth`: Merge 3 times. Assert the number of hook entries per event is still exactly 1 unimatrix entry (no growth).

## Edge Cases

- Hook entry with `matcher` field (e.g., `PreToolUse` with `{ "matcher": { "tool_name": "Write" } }`): must be preserved if it is a non-unimatrix hook.
- Hook entry where `type` is not `"command"`: skip identification, preserve as-is.

## Risk Coverage

| Risk ID | Scenario | Test |
|---------|----------|------|
| R-01 | Merge corrupts existing config | All 7 merge scenarios above |
| R-04 | Duplicate hooks on re-run | `test_merge_idempotent_round_trip`, `test_three_consecutive_merges_no_growth` |
| R-14 | Malformed JSON crash | `test_malformed_json_errors_with_diagnostic`, `test_empty_file_treated_as_empty_object` |
