# crt-011: Test Plan — integration-tests

## Handler-Level Integration Tests

### T-INT-01: test_mcp_usage_confidence_recomputed (usage.rs)

**Target:** `UsageService::record_access` with `AccessSource::McpTool`
**Scenario:** Record usage for an entry via MCP path, verify confidence is recomputed
**Setup:**
1. Create UsageService via make_usage_service()
2. Insert test entry
3. Call record_access with helpful=Some(true)
4. Wait for spawn_blocking

**Assertions:**
- `entry.confidence > 0.0` (confidence was recomputed)
- `entry.access_count >= 1`
- `entry.helpful_count == 1`

**Risk covered:** R-02 (integration test gap)

### T-INT-02: test_mcp_usage_dedup_prevents_double_access (usage.rs)

**Target:** `UsageService::record_access` with `AccessSource::McpTool`
**Scenario:** Same agent+entry called twice, verify UsageDedup prevents double access_count
**Setup:**
1. Create UsageService
2. Insert test entry
3. Call record_access twice with same agent_id + entry_id

**Assertions:**
- `entry.access_count == 1` (deduped by UsageDedup)

**Risk covered:** R-02

### T-INT-03: Check existing test_confidence_updated_on_retrieval (server.rs)

**Target:** `UnimatrixServer::record_usage_for_entries`
**Action:** Verify existing test covers: insert entry -> record usage -> access_count + confidence updated
**If covered:** Document mapping, no new test needed
**If not covered:** Add test following make_server() pattern

### T-INT-04: Check existing test_record_usage_for_entries_access_dedup (server.rs)

**Target:** `UnimatrixServer::record_usage_for_entries`
**Action:** Verify existing test covers: two calls same agent+entry -> access_count stays 1
**If covered:** Document mapping, no new test needed
**If not covered:** Add test following make_server() pattern
