# crt-011: Acceptance Map — Confidence Signal Integrity

## AC-to-Test Traceability

| AC | Description | FR | Test(s) | Status |
|----|-------------|----|---------|----- --|
| AC-01 | success_session_count dedup per (session_id, entry_id) | FR-01 | T-CON-01, T-CON-02 | Pending |
| AC-02 | rework_session_count dedup per (session_id, entry_id) | FR-02 | T-CON-03 | Pending |
| AC-03 | helpful_count dedup preserved (no regression) | FR-03 | Existing tests | Pending |
| AC-04 | Unit test reproduces over-counting and verifies fix | FR-01, FR-02 | T-CON-01, T-CON-03 | Pending |
| AC-05 | rework_flag_count NOT deduped, documented | FR-02 | T-CON-04, ADR-002 | Pending |
| AC-06 | Integration test: context_search handler path | FR-04 | T-INT-01 or T-INT-03 | Pending |
| AC-07 | Integration test: context_get handler path | FR-04 | T-INT-01 or T-INT-03 | Pending |
| AC-08 | Integration test: UsageDedup prevents double access | FR-04 | T-INT-02 or T-INT-04 | Pending |
| AC-09 | All existing tests pass | FR-03 | cargo test --workspace | Pending |
| AC-10 | Multi-session overlapping entry_ids handled correctly | FR-01 | T-CON-02 | Pending |

## Test-to-File Mapping

| Test ID | File | Test Function |
|---------|------|---------------|
| T-CON-01 | crates/unimatrix-server/src/uds/listener.rs | test_confidence_consumer_dedup_same_session |
| T-CON-02 | crates/unimatrix-server/src/uds/listener.rs | test_confidence_consumer_different_sessions_count_separately |
| T-CON-03 | crates/unimatrix-server/src/uds/listener.rs | test_retrospective_consumer_rework_session_dedup |
| T-CON-04 | crates/unimatrix-server/src/uds/listener.rs | test_retrospective_consumer_flag_count_not_deduped |
| T-INT-01 | crates/unimatrix-server/src/services/usage.rs | test_mcp_usage_confidence_recomputed |
| T-INT-02 | crates/unimatrix-server/src/services/usage.rs | test_mcp_usage_dedup_prevents_double_access |
| T-INT-03 | crates/unimatrix-server/src/server.rs | test_confidence_path_search_to_store (or existing test_confidence_updated_on_retrieval) |
| T-INT-04 | crates/unimatrix-server/src/server.rs | test_confidence_path_dedup_across_calls (or existing test_record_usage_for_entries_access_dedup) |

## ADR Coverage

| ADR | AC | Verified By |
|-----|-----|-------------|
| ADR-001: Per-session dedup | AC-01, AC-02, AC-10 | T-CON-01, T-CON-02, T-CON-03 |
| ADR-002: rework_flag_count no dedup | AC-05 | T-CON-04 |
| ADR-003: UsageService-level tests | AC-06, AC-07, AC-08 | T-INT-01..04 |

## Risk Coverage

| Risk | Test Coverage |
|------|---------------|
| R-01: Three-pass race | T-CON-01, T-CON-02 |
| R-02: Integration test gap | T-INT-01..04 |
| R-03: Semantic confusion | T-CON-04, code comments |
| R-04: Queue backlog | T-CON-02 |

## Completion Gate

All ACs marked "Pending" above must be verified during implementation. The implementation is complete when:
- [ ] All T-CON-* tests pass
- [ ] All T-INT-* tests pass (or mapped to existing passing tests)
- [ ] `cargo test --workspace` passes with no regressions
- [ ] Code comments document rework_flag_count vs rework_session_count distinction
