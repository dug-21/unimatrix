# Test Plan: response component

## Unit Tests

### test_format_enroll_success_summary_created (R-10)
- EnrollResult { created: true, agent with Internal + [Read, Write, Search] }
- Format: Summary
- Assert: output contains "Enrolled", agent_id, "internal", capabilities

### test_format_enroll_success_summary_updated (R-10)
- EnrollResult { created: false, agent with Privileged + [Read, Write, Search, Admin] }
- Format: Summary
- Assert: output contains "Updated", agent_id, "privileged", capabilities

### test_format_enroll_success_markdown (R-10)
- Format: Markdown
- Assert: output contains "---BEGIN UNIMATRIX RESPONSE---"
- Assert: output contains "---END UNIMATRIX RESPONSE---"
- Assert: output contains agent ID, trust level, capabilities in table format

### test_format_enroll_success_json (R-10)
- Format: Json
- Assert: output is valid JSON (serde_json::from_str succeeds)
- Assert: JSON contains "action" field ("enrolled" or "updated")
- Assert: JSON contains "agent_id", "trust_level", "capabilities" fields

### test_format_enroll_success_json_updated
- EnrollResult { created: false, ... }
- Assert: JSON "action" field is "updated"

### test_trust_level_str_all_variants
- Verify trust_level_str returns correct lowercase string for each variant

### test_capability_str_all_variants
- Verify capability_str returns correct lowercase string for each variant

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-10 | test_format_enroll_success_summary_created, _updated, _markdown, _json |
