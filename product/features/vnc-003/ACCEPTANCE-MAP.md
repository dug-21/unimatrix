# vnc-003 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | context_correct accepts required + optional params, returns corrected entry | test | Param deserialization tests + handler integration test | PENDING |
| AC-02 | Original entry: status=Deprecated, superseded_by=new_id, correction_count+=1 | test | Verify original entry fields after correction | PENDING |
| AC-03 | New entry: supersedes=original_id. Topic/category/tags inherited when not provided | test | Verify inheritance and override behavior | PENDING |
| AC-04 | Content scanning on new content + title. Injection/PII rejected. | test | Trigger scan rejection on correction content | PENDING |
| AC-05 | Category validation on new category. Inherited category not re-validated. | test | Validate new category, skip inherited | PENDING |
| AC-06 | Correction embedded and indexed. Discoverable via context_search. | test | Search for correction after insert (requires embed model) | PENDING |
| AC-07 | Original + correction in single write transaction (atomicity) | test | Verify both entries present after commit | PENDING |
| AC-08 | EntryNotFound when original_id does not exist | test | context_correct with non-existent ID | PENDING |
| AC-09 | Error when original is already deprecated | test | context_correct on deprecated entry | PENDING |
| AC-10 | Write capability required. MCP error -32003 if denied. | test | Capability denial test for context_correct | PENDING |
| AC-11 | context_deprecate accepts id + optional params. Returns confirmation. | test | Param deserialization + handler test | PENDING |
| AC-12 | Status=Deprecated. Counters/indexes updated atomically. | test | Verify status + counters after deprecation | PENDING |
| AC-13 | Already-deprecated: no-op returning success (idempotent) | test | Double deprecation test | PENDING |
| AC-14 | EntryNotFound when ID does not exist | test | context_deprecate with non-existent ID | PENDING |
| AC-15 | Write capability required. MCP error -32003 if denied. | test | Capability denial test for context_deprecate | PENDING |
| AC-16 | Audit event with reason logged for deprecation | test | Verify audit log entry after deprecation | PENDING |
| AC-17 | context_status accepts optional params. Returns health report. | test | Param deserialization + handler test | PENDING |
| AC-18 | Report includes entry counts by status | test | Verify status counts match actual entries | PENDING |
| AC-19 | Category/topic distribution, filtered when params provided | test | Distribution with and without filters | PENDING |
| AC-20 | Correction chain metrics: supersedes/superseded_by counts, correction_count sum | test | Verify metrics after corrections | PENDING |
| AC-21 | Security metrics: trust_source distribution, entries without created_by | test | Verify trust_source grouping + attribution gaps | PENDING |
| AC-22 | Admin capability required. MCP error -32003 if denied. | test | Capability denial test for context_status | PENDING |
| AC-23 | context_briefing accepts role + task (required), optional feature/max_tokens/agent_id/format | test | Param deserialization test | PENDING |
| AC-24 | Briefing includes conventions for role | test | Store conventions, verify in briefing | PENDING |
| AC-25 | Briefing includes duties for role | test | Store duties, verify in briefing | PENDING |
| AC-26 | Task-relevant context from semantic search. Feature entries boosted. | test | With embedding model (model-dependent test) | PENDING |
| AC-27 | max_tokens budget respected. Truncation from least-relevant first. | test | Verify output within budget limits | PENDING |
| AC-28 | Embed not ready: falls back to lookup-only | test | Briefing without embedding model loaded | PENDING |
| AC-29 | Read capability required. MCP error -32003 if denied. | test | Capability denial test for context_briefing | PENDING |
| AC-30 | VECTOR_MAP write in same txn as entry insert + audit | test | Verify VECTOR_MAP present after insert_with_audit | PENDING |
| AC-31 | Crash after commit: VECTOR_MAP mapping present | test | Verify mapping after simulated crash | PENDING |
| AC-32 | HNSW insert after transaction commits | test | Verify HNSW point count after insert | PENDING |
| AC-33 | Fix applies to both insert_with_audit and correct_with_audit | test | Verify VECTOR_MAP for both paths | PENDING |
| AC-34 | Initial allowlist: 8 categories including "duties" and "reference" | test | Validate both new categories | PENDING |
| AC-35 | All 4 new tools accept format parameter | test | Each tool with each format variant | PENDING |
| AC-36 | Correct response shows deprecated original + new correction | test | Verify response content structure | PENDING |
| AC-37 | Deprecate response confirms with title and ID | test | Verify response content | PENDING |
| AC-38 | Status response structured: counts, distributions, chains, security | test | Parse JSON response, verify all sections | PENDING |
| AC-39 | Briefing response: Conventions/Duties/Relevant Context sections | test | Verify section presence in output | PENDING |
| AC-40 | All existing tests (506) continue to pass | shell | cargo test --workspace | PENDING |
| AC-41 | Workspace conventions: forbid(unsafe_code), edition 2024, MSRV 1.89 | grep | grep for forbid(unsafe_code) in lib.rs; check edition in Cargo.toml | PENDING |
| AC-42 | No new crate dependencies | file-check | Compare Cargo.toml before/after | PENDING |
| AC-43 | All 4 tools log audit events. Combined txn for mutations. | test | Verify audit log entries for each tool | PENDING |
| AC-44 | Correction audit: target_ids includes both original and new IDs | test | Verify target_ids in correction audit event | PENDING |
| AC-45 | Audit IDs sequential across all tools (v0.1 + v0.2) | test | Mixed operations, verify monotonic IDs | PENDING |
