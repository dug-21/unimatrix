# nxs-010: Activity Schema Evolution -- Test Strategy Overview

## Test Approach

Three test layers, prioritized by risk:

1. **Unit tests** (unimatrix-store) -- DDL validation, Store API methods for topic_deliveries and query_log, JSON serialization round-trips, counter arithmetic, error paths.
2. **Integration tests** (unimatrix-store) -- Migration v10->v11 with seeded session data, backfill correctness, idempotency, edge cases (NULL ended_at, empty feature_cycle, zero sessions).
3. **Integration harness tests** (infra-001) -- MCP search produces query_log rows, lifecycle restart preserves new tables.

All tests follow Arrange/Act/Assert. All use `TestDb` or direct `Store::open()` with tempdir fixtures. Migration tests seed v10 databases by opening at v10, inserting session data via raw SQL, then re-opening with v11 code.

## Risk-to-Test Mapping

| Risk ID | Priority | Component Test Plan | Test Type | Scenario Count |
|---------|----------|-------------------|-----------|---------------|
| R-01 | High | migration.md | Integration | 3 |
| R-02 | Critical | migration.md | Integration | 4 |
| R-03 | Med | query-log.md | Unit | 2 |
| R-04 | Critical | search-pipeline-integration.md | Integration (harness) | 4 |
| R-05 | High | search-pipeline-integration.md | Integration (harness) | 2 |
| R-06 | High | query-log.md | Unit | 3 |
| R-07 | High | topic-deliveries.md | Unit | 3 |
| R-08 | Med | migration.md | Integration | 1 |
| R-09 | Med | migration.md | Note (accepted) | 0 |
| R-10 | Critical | topic-deliveries.md | Unit | 2 |
| R-11 | Med | -- | Note (accepted) | 0 |
| R-12 | Med | query-log.md | Unit | 2 |
| R-13 | Low | -- | Accepted risk | 0 |
| R-14 | High | migration.md | Integration | 2 |

## Cross-Component Test Dependencies

- **migration depends on schema-ddl**: Migration creates the same tables as `create_tables()`. Migration tests implicitly validate DDL correctness.
- **search-pipeline-integration depends on query-log**: Pipeline tests write via `insert_query_log` and read via `scan_query_log_by_session`. query-log unit tests must pass first.
- **migration backfill depends on sessions table**: Migration tests must seed sessions via raw SQL at v10 schema level before triggering v11 migration.

## Integration Harness Plan (infra-001)

### Suites to Run

| Suite | Reason |
|-------|--------|
| `smoke` | Mandatory minimum gate -- any change at all |
| `tools` | context_search tool logic modified (query_log write added) |
| `lifecycle` | Schema/storage change -- restart persistence of new tables |
| `edge_cases` | Storage changes -- verify no regressions on boundary values |

### Existing Coverage Assessment

- **`tools` suite**: Tests `context_search` end-to-end. Existing tests validate search results are returned correctly. They do NOT verify query_log side-effects (new behavior).
- **`lifecycle` suite**: Tests restart persistence (store, search, re-find). New tables are transparent to lifecycle -- existing tests pass if DDL is correct.
- **`edge_cases` suite**: Tests empty DB operations, Unicode, boundary values. New tables should not affect existing edge cases.

### Coverage Gaps -- New Integration Tests Needed

1. **`test_search_writes_query_log`** (add to `suites/test_tools.py`)
   - Fixture: `server`
   - Store an entry, invoke `context_search`, then invoke `context_status` or a diagnostic query to verify query_log row exists.
   - Problem: query_log is not exposed via any MCP tool. The fire-and-forget write is an internal side-effect not visible through the MCP interface. **This test cannot be written in infra-001** -- it requires direct DB access.
   - Resolution: Cover via Rust integration test in unimatrix-store/unimatrix-server crate tests instead. The harness validates that search still works correctly (no regression from the added write).

2. **`test_restart_preserves_topic_deliveries`** (potential addition to `suites/test_lifecycle.py`)
   - Fixture: `shared_server`
   - Problem: topic_deliveries is not populated via any MCP tool (populated by migration backfill and future col-017/col-020 paths). No MCP-visible behavior to test.
   - Resolution: Not needed in infra-001. Covered by Rust integration tests that verify Store::open() on v11 databases preserves data.

### Conclusion

No new infra-001 integration tests are needed for nxs-010. The new behavior (query_log writes, topic_deliveries CRUD) is not exposed through the MCP tool interface. Existing suites validate that search still works correctly after the code change. New behavior is tested via Rust unit and integration tests.

### Harness Execution Plan (Stage 3c)

```bash
# Mandatory smoke gate
cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60

# Relevant suites
python -m pytest suites/test_tools.py -v --timeout=60
python -m pytest suites/test_lifecycle.py -v --timeout=60
python -m pytest suites/test_edge_cases.py -v --timeout=60
```
