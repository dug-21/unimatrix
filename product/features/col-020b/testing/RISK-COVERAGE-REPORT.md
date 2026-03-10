# Risk Coverage Report: col-020b

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | normalize_tool_name misses edge-case prefixes | test_normalize_tool_name_standard_prefix, test_normalize_tool_name_passthrough_bare, test_normalize_tool_name_passthrough_claude_native, test_normalize_tool_name_double_prefix, test_normalize_tool_name_prefix_only, test_normalize_tool_name_empty_string, test_normalize_tool_name_case_sensitive, test_normalize_tool_name_different_server | PASS | Full (8/8 scenarios) |
| R-02 | Serde alias silently drops fields on deserialization | test_session_summary_deserialize_pre_col020b, test_feature_knowledge_reuse_deserialize_from_old, test_retrospective_report_deserialize_old_knowledge_reuse_field, test_retrospective_report_partial_new_fields, test_session_summary_serde_roundtrip, test_feature_knowledge_reuse_serde_roundtrip | PASS | Full (6/6 scenarios) |
| R-03 | Serde default produces incorrect zero | test_session_summary_knowledge_curated_default, test_session_summary_knowledge_curated_present, test_feature_knowledge_reuse_deserialize_from_old (cross_session_count=0 default) | PASS | Full (3/3 scenarios) |
| R-04 | delivery_count semantic change miscounts | test_knowledge_reuse_single_session_delivery, test_knowledge_reuse_delivery_vs_cross_session, test_knowledge_reuse_single_session_not_cross_session, test_knowledge_reuse_dedup_across_query_and_injection_same_session, test_knowledge_reuse_deduplication_across_sources | PASS | Full (5/5 scenarios) |
| R-05 | by_category and category_gaps computed against wrong entry set | test_knowledge_reuse_by_category_includes_single_session, test_knowledge_reuse_category_gaps_delivery_based, test_knowledge_reuse_no_gaps_all_reused, test_knowledge_reuse_both_sources_empty | PASS | Full (4/4 scenarios) |
| R-06 | #193 data flow returns empty slices silently | Code review: 4 tracing::debug! log points at lines 1687, 1701, 1715, 1725 plus result summary at 1744. Error propagation via `??` confirmed. Caller handles Err with tracing::warn at line 1339. | PASS (code review) | Partial (debug tracing verified; end-to-end data flow not unit-testable per ADR-002) |
| R-07 | Re-export rename missed at import site | cargo build --workspace, cargo build --release both succeed. FeatureKnowledgeReuse re-exported in lib.rs line 31. | PASS | Full (compile gate) |
| R-08 | Existing tests never exercise MCP-prefixed tool names | test_classify_tool_mcp_prefixed (7 assertions), test_session_summaries_mcp_prefixed_knowledge_flow, test_session_summaries_mixed_bare_and_prefixed, test_session_summaries_curate_in_tool_distribution | PASS | Full (7/7 scenarios) |
| R-09 | classify_tool curate category mapping error | test_classify_tool_all_categories (exhaustive bare names including curate), test_classify_tool_admin_tools_are_other (6 assertions) | PASS | Full (2/2 scenarios) |
| R-10 | Inconsistent normalization across counters | test_session_summaries_mcp_prefixed_knowledge_flow (all 3 counters non-zero with MCP-prefixed input), test_session_summaries_mixed_bare_and_prefixed (both forms contribute) | PASS | Full (2/2 scenarios) |
| R-11 | tool_distribution curate key breaks consumers | test_session_summaries_curate_in_tool_distribution (curate key present), test_session_summaries_no_curate_without_curation_tools (curate key absent), test_classify_tool_all_categories (HashMap<String,u64> verified) | PASS | Full (3/3 scenarios) |
| R-12 | spawn_blocking error swallowing | Code review: compute_knowledge_reuse_for_sessions returns Result (line 1678-1680). Caller at line 1337-1339 logs warn and sets feature_knowledge_reuse to None (not Some(zeroed)). No unwrap() on spawn_blocking JoinHandle -- uses `??` for propagation. | PASS (code review) | Full |
| R-13 | New field names in serialized output | test_retrospective_report_roundtrip_with_new_fields (asserts JSON contains "feature_knowledge_reuse", "delivery_count", "knowledge_served"), test_retrospective_report_serialize_none_fields_omitted | PASS | Full (2/2 scenarios) |

## Test Results

### Unit Tests -- unimatrix-observe
- Total: 359
- Passed: 359
- Failed: 0

### Unit Tests -- unimatrix-server
- Total: 915
- Passed: 915
- Failed: 0

### Integration Tests (infra-001 smoke gate)
- Total: 19 (18 selected + 1 xfail)
- Passed: 18
- Failed: 0
- XFail: 1 (test_store_1000_entries -- pre-existing GH#111, rate limit blocks volume test)

## Gaps

No risk coverage gaps. All 13 risks (R-01 through R-13) have test coverage:
- 11 risks covered by unit tests (Full coverage)
- 2 risks covered by code review (R-06 partial -- end-to-end data flow accepted gap per ADR-002; R-12 full)

**Accepted gap (ADR-002):** The end-to-end data flow for R-06 (#193) cannot be validated by unit tests. Debug tracing has been added at all 4 data flow boundaries plus the result summary. If debug logs show zero counts in production, a follow-up issue is required per ADR-005.

**Deferred integration tests (ADR-002):** Three integration tests were identified in the test plan OVERVIEW.md for follow-up:
- test_retrospective_mcp_prefixed_knowledge_counters
- test_retrospective_knowledge_curated_counter
- test_retrospective_feature_knowledge_reuse_single_session

These validate the full stack (Store -> observe -> server -> MCP JSON) which unit tests cannot cover. Not in scope for col-020b.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | test_normalize_tool_name_standard_prefix, test_normalize_tool_name_passthrough_bare, test_normalize_tool_name_passthrough_claude_native |
| AC-02 | PASS | test_classify_tool_all_categories (13 bare-name assertions including curate), test_classify_tool_mcp_prefixed (7 MCP-prefixed assertions) |
| AC-03 | PASS | test_session_summaries_knowledge_served_stored (knowledge_served=8), test_session_summaries_mcp_prefixed_knowledge_flow (knowledge_served=4 from MCP-prefixed) |
| AC-04 | PASS | test_session_summaries_knowledge_served_stored (knowledge_stored=3), test_session_summaries_mcp_prefixed_knowledge_flow (knowledge_stored=2 from MCP-prefixed) |
| AC-05 | PASS | test_session_summaries_mcp_prefixed_knowledge_flow (knowledge_curated=3 from context_correct, context_deprecate, context_quarantine), test_session_summaries_mixed_bare_and_prefixed (knowledge_curated=2 from mixed) |
| AC-06 | PASS | test_session_summary_deserialize_pre_col020b (knowledge_in:5 -> knowledge_served=5, knowledge_out:3 -> knowledge_stored=3) |
| AC-07 | PASS | test_knowledge_reuse_single_session_delivery (delivery_count=3, cross_session_count=0 for single-session data) |
| AC-08 | PASS | test_knowledge_reuse_delivery_vs_cross_session (delivery_count=3, cross_session_count=1 for mixed data) |
| AC-09 | PASS | test_knowledge_reuse_by_category_includes_single_session (by_category non-empty for single-session entries) |
| AC-10 | PASS | test_knowledge_reuse_category_gaps_delivery_based (category_gaps excludes convention with single-session delivery, includes pattern and procedure with zero delivery) |
| AC-11 | PASS | test_retrospective_report_deserialize_old_knowledge_reuse_field (knowledge_reuse alias -> feature_knowledge_reuse populated, delivery_count=3) |
| AC-12 | PASS | test_session_summary_knowledge_curated_default (missing field -> knowledge_curated=0), test_session_summary_knowledge_curated_present (present field -> knowledge_curated=5) |
| AC-13 | PASS | cargo test -p unimatrix-observe: 359 passed, 0 failed. cargo test -p unimatrix-server: 915 passed, 0 failed. |
| AC-14 | PASS | test_classify_tool_mcp_prefixed, test_session_summaries_mcp_prefixed_knowledge_flow, test_session_summaries_mixed_bare_and_prefixed, test_session_summaries_curate_in_tool_distribution |
| AC-15 | PASS | test_knowledge_reuse_single_session_delivery (delivery_count=3, cross_session_count=0 -- regression test for 2+ sessions filter bug) |
| AC-16 | PASS | grep of tools.rs confirms 5 tracing::debug! calls containing "knowledge_reuse" at data flow boundaries: session IDs (line 1687), query_log records (line 1701), injection_log records (line 1715), active categories (line 1725), result summary (line 1744) |
