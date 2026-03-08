# crt-011: Test Plan Overview — Confidence Signal Integrity

## Test Strategy

This feature has two test dimensions:

1. **Consumer dedup unit tests** (T-CON-01 through T-CON-04) — directly test the fixed consumer functions with crafted signal inputs
2. **Handler-level integration tests** (T-INT-01 through T-INT-04) — test the handler-to-service-to-store chain for confidence path

## Risk Mapping

| Risk | Tests | Coverage |
|------|-------|----------|
| R-01: Three-pass race in run_confidence_consumer | T-CON-01, T-CON-02 | HashSet persists across all passes |
| R-02: Integration test gap | T-INT-01..04 | Service-level chain tests |
| R-03: Semantic confusion (flag vs session count) | T-CON-04 | Explicitly verifies flag_count NOT deduped |
| R-04: Queue backlog amplification | T-CON-02 | Multiple sessions with overlapping entries |

## Integration Harness Plan

No external integration harness (product/test/infra-001/) is applicable. All tests are Rust-native:

- **Consumer tests**: Use `Store::insert_signal()` to inject signals, call consumer function directly, inspect `PendingEntriesAnalysis`
- **Usage service tests**: Use `make_usage_service()` helper, call `record_access()`, inspect store state
- **Server tests**: Use `make_server()` helper, call `record_usage_for_entries()`, inspect store state

Existing tests that may already cover T-INT-03/T-INT-04:
- `test_confidence_updated_on_retrieval` in server.rs — likely covers T-INT-03
- `test_record_usage_for_entries_access_dedup` in server.rs — likely covers T-INT-04

These will be verified during implementation; if covered, document the mapping rather than adding duplicates.

## Test Execution Order

1. `cargo test --workspace` (baseline — all existing tests pass)
2. Implement consumer dedup fixes
3. Add T-CON-01..04 tests
4. `cargo test -p unimatrix-server` (verify fixes + new tests)
5. Add/verify T-INT-01..04 tests
6. `cargo test --workspace` (full regression)

## Components

| Component | Test Plan | Count |
|-----------|-----------|-------|
| consumer-dedup | test-plan/consumer-dedup.md | 4 unit tests |
| integration-tests | test-plan/integration-tests.md | 2-4 integration tests (some may map to existing) |
