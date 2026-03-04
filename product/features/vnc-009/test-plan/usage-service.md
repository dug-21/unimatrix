# Test Plan: usage-service

## Risk Coverage

| Risk | Scenarios | Priority |
|------|----------|----------|
| R-01 | Vote semantics preservation | High |
| R-10 | Spawn safety | Medium |
| R-11 | ServiceLayer constructor | High |

## Unit Tests (services/usage.rs)

### R-01: Vote Semantics Preservation

1. **test_record_access_mcp_first_helpful_vote**
   - Create entry, call `record_access(McpTool)` with `helpful: Some(true)`
   - Verify `helpful_count` incremented by 1
   - Covers: R-01 scenario 1

2. **test_record_access_mcp_vote_correction**
   - Create entry, vote unhelpful, then vote helpful
   - Verify helpful_count=1, unhelpful_count=0 (correction applied)
   - Covers: R-01 scenario 2

3. **test_record_access_mcp_duplicate_vote_noop**
   - Create entry, vote helpful twice with same agent
   - Verify helpful_count stays at 1
   - Covers: R-01 scenario 3

4. **test_record_access_mcp_multi_agent_votes**
   - Create entry, two different agents vote helpful
   - Verify helpful_count=2
   - Covers: R-01 scenario 4

5. **test_record_access_mcp_triggers_confidence**
   - Create entry, call record_access(McpTool) with helpful
   - Wait for spawn_blocking to complete (tokio::time::sleep brief)
   - Verify confidence was recomputed (not zero)
   - Covers: R-01 scenario 5

6. **test_record_access_hook_no_votes**
   - Create entry, call record_access(HookInjection) with helpful: None
   - Verify helpful_count stays at 0
   - Covers: R-01 scenario 6

7. **test_record_access_mcp_access_dedup**
   - Create entry, call record_access(McpTool) twice with same agent
   - Verify access_count incremented only once
   - Regression for existing dedup behavior

8. **test_record_access_mcp_feature_recording**
   - Create entry, call record_access(McpTool) with feature_cycle and Internal trust
   - Verify FEATURE_ENTRIES written
   - Regression for feature entry recording

9. **test_record_access_mcp_feature_restricted_ignored**
   - Create entry, call record_access(McpTool) with feature_cycle and Restricted trust
   - Verify FEATURE_ENTRIES NOT written
   - Regression for trust gating

### R-10: Spawn Safety

10. **test_record_access_empty_ids_returns_immediately**
    - Call record_access with empty entry_ids
    - Verify returns without spawning (no panic, no error)

11. **test_record_access_fire_and_forget_returns_quickly**
    - Call record_access, measure wall time
    - Assert returns in < 5ms (fire-and-forget, no await)

12. **test_record_access_concurrent_calls**
    - Spawn 10 concurrent record_access calls
    - Verify no panics, no data races
    - Covers: R-10 scenario 3

### R-11: ServiceLayer Constructor

13. **test_usage_service_on_service_layer**
    - Construct ServiceLayer with all required args including UsageDedup
    - Verify `services.usage` field is accessible
    - Covers: R-11 scenario 1

### Briefing Variant

14. **test_record_access_briefing_access_count_only**
    - Create entry, call record_access(Briefing)
    - Verify access_count incremented, helpful_count stays 0

15. **test_record_access_briefing_dedup**
    - Call record_access(Briefing) twice with same agent and entry
    - Verify access_count incremented only once

## Test Setup Pattern

```
fn make_usage_service() -> (UsageService, Arc<Store>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path().join("test.redb")).unwrap());
    let usage_dedup = Arc::new(UsageDedup::new());
    let service = UsageService::new(Arc::clone(&store), usage_dedup);
    (service, store, dir)
}
```

Uses tokio::test attribute for async tests that need spawn_blocking to complete.
