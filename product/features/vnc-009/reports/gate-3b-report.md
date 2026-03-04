# Gate 3b Report: Code Review -- vnc-009

**Result: PASS**

## Validation Checklist

### Code-Pseudocode Alignment
- [x] usage-service: UsageService with AccessSource enum, record_access dispatch, fire-and-forget spawn_blocking. Matches pseudocode/usage-service.md.
- [x] rate-limiter: SlidingWindow + RateLimiter struct on SecurityGateway, check_search_rate/check_write_rate, UDS exemption via CallerId::UdsSession. Matches pseudocode/rate-limiter.md.
- [x] session-aware-mcp: session_id: Option<String> on SearchParams, LookupParams, GetParams, BriefingParams with #[serde(default)]. prefix_session_id/strip_session_prefix helpers. CallerId enum. Matches pseudocode/session-aware-mcp.md.
- [x] status-serialize: StatusReportJson intermediate struct with derive(Serialize), From<&StatusReport> impl. ContradictionPair and EmbeddingInconsistency derive(Serialize). JSON branch reduced from ~130 lines to 4 lines. Matches pseudocode/status-serialize.md.
- [x] uds-auth-audit: AuditLog threaded through start_uds_listener -> accept_loop -> handle_connection. Auth failure writes AuditEvent with Outcome::Error via fire-and-forget spawn_blocking. Matches pseudocode/uds-auth-audit.md.

### Architecture Alignment
- [x] All changes confined to crates/unimatrix-server/
- [x] ServiceLayer extended with UsageService (transport-agnostic)
- [x] SecurityGateway extended with rate limiting (S2 invariant)
- [x] ToolContext extended with caller_id (transport-to-service bridging)
- [x] No new crate dependencies

### Interface Contracts
- [x] SearchService::search now requires caller_id: &CallerId -- all callers updated (tools.rs, briefing.rs, listener.rs)
- [x] StoreService::insert/correct now require caller_id: &CallerId -- all callers updated
- [x] BriefingService::assemble now accepts caller_id: Option<&CallerId> -- all callers updated (tools.rs, listener.rs, 20 tests)
- [x] start_uds_listener now accepts audit_log: Arc<AuditLog> -- caller in main.rs updated
- [x] ServiceLayer::new now accepts usage_dedup: Arc<UsageDedup> -- callers in main.rs and listener.rs tests updated

### Compilation
- [x] `cargo build --workspace` succeeds
- [x] 3 warnings in unimatrix-server (pre-existing dead code: correct_with_audit, record_usage_for_entries on UnimatrixServer, and vector_store/usage_dedup fields)
- [x] No new warnings introduced

### Stubs
- [x] No todo!(), unimplemented!(), TODO, FIXME, or HACK in any modified file

### .unwrap() in Non-Test Code
- [x] No .unwrap() in non-test production code. Single .unwrap() in test code (usage_tests) which is acceptable.

### File Size
- [x] New file usage.rs: 473 lines (within 500-line limit)
- [x] Pre-existing files (tools.rs, server.rs, listener.rs, briefing.rs, gateway.rs) already exceeded 500 lines before vnc-009. No file was created above 500 lines by this feature.

### Clippy
- [x] `cargo clippy --package unimatrix-server -- -D warnings` produces zero errors in unimatrix-server
- [x] Pre-existing clippy issues in unimatrix-adapt and unimatrix-embed (not this feature's scope)

### Test Results
- [x] 759 server tests passing (baseline: 739, +20 new)
- [x] 1693 workspace tests passing (baseline: 1673, +20 new)
- [x] Zero failures

## New Tests (20)
- services::usage: 10 tests (basic access, helpful vote, duplicate vote, access dedup, fire-and-forget timing, feature recording, restricted ignored, briefing access, hook injection)
- services::gateway: 10 tests (rate limiter boundary, UDS exemption, lazy eviction, different callers independent, error display/conversion, new_permissive permissive rates)

## Files Created
- `crates/unimatrix-server/src/services/usage.rs` (NEW)

## Files Modified (14)
- `crates/unimatrix-server/src/services/mod.rs`
- `crates/unimatrix-server/src/services/gateway.rs`
- `crates/unimatrix-server/src/services/briefing.rs`
- `crates/unimatrix-server/src/services/search.rs`
- `crates/unimatrix-server/src/services/store_ops.rs`
- `crates/unimatrix-server/src/services/store_correct.rs`
- `crates/unimatrix-server/src/mcp/context.rs`
- `crates/unimatrix-server/src/mcp/tools.rs`
- `crates/unimatrix-server/src/mcp/response/status.rs`
- `crates/unimatrix-server/src/infra/contradiction.rs`
- `crates/unimatrix-server/src/infra/validation.rs`
- `crates/unimatrix-server/src/uds/listener.rs`
- `crates/unimatrix-server/src/server.rs`
- `crates/unimatrix-server/src/main.rs`
