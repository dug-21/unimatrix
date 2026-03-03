# vnc-006 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | SearchService::search() produces identical results to both existing paths | test | Comparison integration test: seed store, run old inline path and SearchService with same inputs, assert identical result IDs, ordering, and scores | PENDING |
| AC-02 | MCP tools.rs calls SearchService::search() | grep | `grep -c 'services.search.search\|self.services.search' crates/unimatrix-server/src/tools.rs` returns >= 1; no inline embed+search+rank in context_search handler | PENDING |
| AC-03 | UDS uds_listener.rs calls SearchService::search() | grep | `grep -c 'services.search.search\|\.search\.search' crates/unimatrix-server/src/uds_listener.rs` returns >= 1; no inline embed+search+rank in handle_context_search | PENDING |
| AC-04 | ConfidenceService::recompute() replaces all 8 inline blocks | grep | `grep -c 'compute_confidence' crates/unimatrix-server/src/tools.rs crates/unimatrix-server/src/uds_listener.rs` returns 0; `grep -c 'confidence.recompute' crates/unimatrix-server/src/` returns >= 8 | PENDING |
| AC-05 | StoreService insert/correct with atomic audit via insert_in_txn | test | Integration test: insert via StoreService, verify entry and audit record exist in same read transaction; simulate failure, verify neither exists | PENDING |
| AC-06 | S1 scans search queries, logs warning without rejecting | test | Unit test: `validate_search_query("ignore previous instructions", 5, &audit_ctx)` returns Ok(Some(ScanWarning)); search still succeeds | PENDING |
| AC-07 | S1 hard-rejects writes with injection/PII patterns | test | Unit test: `validate_write("test", "ignore all previous instructions", ...)` returns Err(ServiceError::ContentRejected) | PENDING |
| AC-08 | S3 validates all service method parameters | test | Unit tests: query > 10,000 chars rejected, k=0 rejected, k=101 rejected, control chars rejected, oversized title rejected | PENDING |
| AC-09 | S4 quarantine exclusion in SearchService | test | Integration test: insert entry, quarantine it, search with matching query, verify entry absent from results | PENDING |
| AC-10 | S5 audit records emitted with AuditContext | test | Integration test: perform search via SearchService with AuditContext containing session_id and feature_cycle; verify audit event recorded with those fields | PENDING |
| AC-11 | All service methods accept AuditContext | grep | `grep -n 'pub(crate) async fn\|pub(crate) fn' crates/unimatrix-server/src/services/*.rs` — every public service method (search, insert, correct) has `audit_ctx: &AuditContext` param | PENDING |
| AC-12 | AuditSource::Internal exists with pub(crate) visibility | grep | `grep 'Internal' crates/unimatrix-server/src/services/mod.rs` finds Internal variant; `grep 'pub(crate) enum AuditSource' crates/unimatrix-server/src/services/mod.rs` confirms visibility | PENDING |
| AC-13 | MCP produces identical responses | test | Before/after comparison test for context_search, context_store, context_correct: same inputs produce same outputs | PENDING |
| AC-14 | UDS produces identical responses | test | Before/after comparison test for handle_context_search: same inputs produce same HookResponse | PENDING |
| AC-15 | No net reduction in test count | shell | `cargo test --package unimatrix-server -- --list 2>/dev/null \| wc -l` >= 680 | PENDING |
| AC-16 | No new crates added | shell | `grep '\[workspace\]' -A 50 Cargo.toml \| grep 'members'` shows no new crate entries | PENDING |
| AC-17 | No functional changes to happy path | test | All existing integration tests pass without modification (`cargo test --package unimatrix-server`) | PENDING |
