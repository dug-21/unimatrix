# vnc-007 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | BriefingService struct exists in services/briefing.rs with assemble() accepting BriefingParams and AuditContext | grep | `grep -n "pub(crate) async fn assemble" crates/unimatrix-server/src/services/briefing.rs` | PENDING |
| AC-02 | BriefingService supports convention lookup by role/topic when include_conventions=true | test | Unit test: query with role, verify convention entries returned | PENDING |
| AC-03 | BriefingService performs semantic search when include_semantic=true; NO embedding/vector search when include_semantic=false | test | Unit test: mock SearchService, verify called only when include_semantic=true; panicking SearchService test for include_semantic=false | PENDING |
| AC-04 | BriefingService supports injection history as entry source | test | Unit test: provide injection entries, verify partitioned output (decisions/injections/conventions) | PENDING |
| AC-05 | BriefingService applies token budget allocation respecting max_tokens | test | Unit test: set small budget, verify entries truncated; test proportional allocation with injection history | PENDING |
| AC-06 | Quarantined entries excluded from all assembled results | test | Unit test: include quarantined entry in injection history and conventions, verify excluded | PENDING |
| AC-07 | Input validation via SecurityGateway S3 (role length, task length, max_tokens range) | test | Unit test: oversized role/task rejected; max_tokens out of range rejected | PENDING |
| AC-08 | BriefingService registered in ServiceLayer accessible from both transports | grep | `grep -n "briefing" crates/unimatrix-server/src/services/mod.rs` | PENDING |
| AC-09 | Briefing struct in response.rs has no duties field | grep | `grep -n "duties" crates/unimatrix-server/src/response.rs` returns zero matches in Briefing struct | PENDING |
| AC-10 | context_briefing handler performs no duties lookup | grep | `grep -rn "duties" crates/unimatrix-server/src/tools.rs` returns zero matches for category query | PENDING |
| AC-11 | format_briefing has no duties section in any format | test | Unit test: verify summary/markdown/json output contain no "duties" or "Duties" | PENDING |
| AC-12 | BriefingService has no duties concept in params or results | grep | `grep -n "duties" crates/unimatrix-server/src/services/briefing.rs` returns zero matches | PENDING |
| AC-13 | context_briefing delegates to BriefingService::assemble() | grep | `grep -n "briefing.assemble\|services.briefing" crates/unimatrix-server/src/tools.rs` | PENDING |
| AC-14 | context_briefing produces equivalent output (minus duties) for same inputs | test | Snapshot test: compare pre/post refactoring output for identical knowledge base | PENDING |
| AC-15 | context_briefing retains transport-specific concerns (identity, capability, format, usage) | manual | Code inspection: identity resolution, capability check, format param, usage recording remain in tools.rs | PENDING |
| AC-16 | context_briefing gated behind #[cfg(feature = "mcp-briefing")] | grep | `grep -n 'cfg.*mcp.briefing' crates/unimatrix-server/src/tools.rs` | PENDING |
| AC-17 | mcp-briefing feature defined in Cargo.toml with default on | grep | `grep -A2 'mcp-briefing' crates/unimatrix-server/Cargo.toml` | PENDING |
| AC-18 | handle_compact_payload delegates to BriefingService::assemble() | grep | `grep -n "briefing.assemble\|services.briefing" crates/unimatrix-server/src/uds_listener.rs` | PENDING |
| AC-19 | CompactPayload produces equivalent output for same session state | test | Snapshot test: compare pre/post refactoring output for identical session + entries | PENDING |
| AC-20 | Session state resolved from SessionRegistry before calling BriefingService | manual | Code inspection: SessionRegistry lookup precedes BriefingService call in handle_compact_payload | PENDING |
| AC-21 | Compaction count incremented after assembly | test | Unit test: verify count increases after handle_compact_payload | PENDING |
| AC-22 | dispatch_request handles HookRequest::Briefing via BriefingService | test | Unit test: send Briefing request, verify BriefingContent response | PENDING |
| AC-23 | HookRequest::Briefing returns BriefingContent with conventions + semantic search | test | Integration test: populate knowledge base, send Briefing request, verify content | PENDING |
| AC-24 | HookRequest::Briefing no longer returns ERR_UNKNOWN_REQUEST | test | Unit test: send Briefing request, verify not Error response | PENDING |
| AC-25 | --no-default-features build has no context_briefing tool | shell | `cargo build --no-default-features -p unimatrix-server 2>&1` succeeds; runtime check tool absent | PENDING |
| AC-26 | Default build has functional context_briefing tool | shell | `cargo build -p unimatrix-server 2>&1` succeeds; integration test verifies tool works | PENDING |
| AC-27 | BriefingService always available regardless of feature flag | shell | `cargo build --no-default-features -p unimatrix-server 2>&1` succeeds; BriefingService used by UDS path | PENDING |
| AC-28 | (Deferred to vnc-009) SecurityGateway check_write_rate() | -- | Deferred per ADR-004 | DEFERRED |
| AC-29 | (Deferred to vnc-009) StoreService calls check_write_rate() | -- | Deferred per ADR-004 | DEFERRED |
| AC-30 | (Deferred to vnc-009) 60 writes/hour rate limit | -- | Deferred per ADR-004 | DEFERRED |
| AC-31 | (Deferred to vnc-009) In-memory rate limiter state | -- | Deferred per ADR-004 | DEFERRED |
| AC-32 | (Deferred to vnc-009) AuditSource::Internal exempt from rate limiting | -- | Deferred per ADR-004 | DEFERRED |
| AC-33 | No net reduction in test count | shell | `cargo test -p unimatrix-server -- --list 2>&1 \| grep -c "test$"` before and after | PENDING |
| AC-34 | BriefingService unit tests cover all entry sources and edge cases | test | Test suite inspection: conventions-only, semantic-only, injection-history, mixed, budget overflow, empty, quarantine | PENDING |
| AC-35 | Integration tests verify MCP and UDS produce equivalent results | test | Snapshot comparison tests for MCP and UDS paths with identical data | PENDING |
| AC-36 | dispatch_unknown_returns_error test updated | test | `cargo test dispatch_unknown_returns_error -p unimatrix-server` passes | PENDING |
| AC-37 | No changes outside unimatrix-server and unimatrix-engine | shell | `git diff --stat` confined to crates/unimatrix-server/ and crates/unimatrix-engine/ | PENDING |
