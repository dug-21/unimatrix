# vnc-009 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | UsageService struct exists in services/usage.rs with record_access(entry_ids, source, ctx) | file-check + grep | `grep "pub(crate) fn record_access" crates/unimatrix-server/src/services/usage.rs` | PENDING |
| AC-02 | AccessSource enum has McpTool, HookInjection, Briefing variants | grep | `grep -A5 "enum AccessSource" crates/unimatrix-server/src/services/usage.rs` | PENDING |
| AC-03 | MCP tools call UsageService::record_access with AccessSource::McpTool | grep | `grep "AccessSource::McpTool" crates/unimatrix-server/src/mcp/tools.rs` | PENDING |
| AC-04 | UDS listener calls UsageService::record_access with AccessSource::HookInjection | grep | `grep "AccessSource::HookInjection" crates/unimatrix-server/src/uds/listener.rs` | PENDING |
| AC-05 | HookInjection triggers injection log + co-access + feature entry writes | test | Integration test: HookInjection source triggers all three storage operations | PENDING |
| AC-06 | UsageService is a field on ServiceLayer | grep | `grep "usage:" crates/unimatrix-server/src/services/mod.rs` | PENDING |
| AC-07 | Fire-and-forget preserved: record_access returns immediately | test | Unit test: record_access returns in <1ms regardless of store state | PENDING |
| AC-08 | UsageDedup functions for MCP dedup via McpTool variant | test | Unit test: duplicate McpTool access returns empty dedup filter | PENDING |
| AC-09 | UDS injection dedup via session-scoped mechanisms | test | Integration test: duplicate injection in same session deduped | PENDING |
| AC-10 | Storage writes identical to pre-vnc-009 | test | Regression test: compare store state after UsageService vs old path | PENDING |
| AC-11 | record_usage_for_entries() removed from server.rs | grep | `grep "record_usage_for_entries" crates/unimatrix-server/src/server.rs` returns no matches | PENDING |
| AC-12 | SearchParams has session_id: Option<String> | grep | `grep "session_id" crates/unimatrix-server/src/mcp/tools.rs` in SearchParams | PENDING |
| AC-13 | LookupParams has session_id: Option<String> | grep | `grep "session_id" crates/unimatrix-server/src/mcp/tools.rs` in LookupParams | PENDING |
| AC-14 | GetParams has session_id: Option<String> | grep | `grep "session_id" crates/unimatrix-server/src/mcp/tools.rs` in GetParams | PENDING |
| AC-15 | BriefingParams has session_id: Option<String> | grep | `grep "session_id" crates/unimatrix-server/src/mcp/tools.rs` in BriefingParams | PENDING |
| AC-16 | Provided session_id prefixed with mcp:: | test | Unit test: session_id="abc" -> audit_ctx.session_id = Some("mcp::abc") | PENDING |
| AC-17 | UDS session_id prefixed with uds:: | test | Unit test: UDS session "sess-123" -> audit_ctx.session_id = Some("uds::sess-123") | PENDING |
| AC-18 | AuditContext.session_id populated with prefixed ID | test | Unit test: ToolContext with session_id produces prefixed AuditContext | PENDING |
| AC-19 | CallerId enum with Agent, UdsSession variants | grep | `grep -A3 "enum CallerId" crates/unimatrix-server/src/services/mod.rs` | PENDING |
| AC-20 | check_search_rate enforces 300/hr | test | Unit test: 300 calls Ok, 301st returns RateLimited | PENDING |
| AC-21 | check_write_rate enforces 60/hr | test | Unit test: 60 calls Ok, 61st returns RateLimited | PENDING |
| AC-22 | SearchService calls check_search_rate | grep + test | `grep "check_search_rate" crates/unimatrix-server/src/services/search.rs` | PENDING |
| AC-23 | StoreService calls check_write_rate | grep + test | `grep "check_write_rate" crates/unimatrix-server/src/services/store_ops.rs` | PENDING |
| AC-24 | BriefingService calls check_search_rate when include_semantic=true | grep + test | `grep "check_search_rate" crates/unimatrix-server/src/services/briefing.rs` | PENDING |
| AC-25 | ServiceError::RateLimited has limit, window_secs, retry_after_secs | grep | `grep "RateLimited" crates/unimatrix-server/src/services/mod.rs` | PENDING |
| AC-26 | CallerId::UdsSession exempt from rate limiting | test | Unit test: UdsSession caller unlimited calls Ok | PENDING |
| AC-27 | Internal callers exempt from rate limiting | test | Integration test: internal service calls succeed without CallerId | PENDING |
| AC-28 | Rate limiter state in-memory, resets on restart | manual | No persistence code in RateLimiter. Verify no file/db writes. | PENDING |
| AC-29 | MCP constructs CallerId::Agent, UDS constructs CallerId::UdsSession | grep | `grep "CallerId::Agent" crates/unimatrix-server/src/mcp/` and `grep "CallerId::UdsSession" crates/unimatrix-server/src/uds/` | PENDING |
| AC-30 | Closes finding F-09 | test | Acceptance test: MCP search rate limited at 300/hr | PENDING |
| AC-31 | StatusReport has #[derive(Serialize)] | grep | `grep "derive.*Serialize" crates/unimatrix-server/src/mcp/response/status.rs` | PENDING |
| AC-32 | CoAccessClusterEntry has #[derive(Serialize)] | grep | `grep -B1 "struct CoAccessClusterEntry" crates/unimatrix-server/src/mcp/response/status.rs` | PENDING |
| AC-33 | ContradictionPair, EmbeddingInconsistency have #[derive(Serialize)] | grep | `grep "derive.*Serialize" crates/unimatrix-server/src/infra/contradiction.rs` | PENDING |
| AC-34 | JSON branch uses serde_json via StatusReportJson | grep | `grep "StatusReportJson" crates/unimatrix-server/src/mcp/response/status.rs` | PENDING |
| AC-35 | JSON field names match existing output | test | Snapshot test: known StatusReport -> JSON matches golden file | PENDING |
| AC-36 | Auth failure writes AuditEvent with uds_auth_failure action | grep + test | `grep "uds_auth_failure" crates/unimatrix-server/src/uds/listener.rs` | PENDING |
| AC-37 | Audit event includes error details | grep | `grep "Authentication failed" crates/unimatrix-server/src/uds/listener.rs` in AuditEvent | PENDING |
| AC-38 | Arc<AuditLog> accessible in handle_connection | grep | `grep "audit_log" crates/unimatrix-server/src/uds/listener.rs` in handle_connection signature | PENDING |
| AC-39 | Closes finding F-23 | test | Integration test: auth failure produces AUDIT_LOG entry | PENDING |
| AC-40 | No session_id = identical behavior | test | Regression test: SearchParams without session_id produces same results | PENDING |
| AC-41 | No net test count reduction from 739 | shell | `cargo test --package unimatrix-server -- --list 2>/dev/null \| grep -c "test$"` >= 739 | PENDING |
| AC-42 | All changes in crates/unimatrix-server/ | shell | `git diff --stat` shows only crates/unimatrix-server/ files | PENDING |
| AC-43 | No new tables, no schema version bump | grep | `grep "schema_version\|create_table\|define_table" crates/unimatrix-server/src/` returns no new entries | PENDING |
