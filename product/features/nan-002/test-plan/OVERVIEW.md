# nan-002: Knowledge Import -- Test Strategy Overview

## Test Approach

Three test levels cover the import feature:

1. **Unit tests** -- Deserialization, header validation, hash logic, format struct correctness. Run via `cargo test` inside `unimatrix-server`.
2. **Integration tests** -- Full pipeline execution: export-import round-trips, --force behavior, atomicity, counter restoration, embedding reconstruction. Run via `cargo test` (Rust integration tests in `tests/` or inline).
3. **MCP integration tests** -- Post-import server behavior through the MCP JSON-RPC interface. Run via infra-001 harness.

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Test Level | Key Scenarios |
|---------|----------|---------------|------------|---------------|
| R-01 | Critical | format-types, import-pipeline | Unit + Integration | Column-by-column entry verification (AC-08), round-trip comparison (AC-15), PRAGMA table_info column count check |
| R-02 | Critical | format-types | Unit | Null optionals, empty strings, unicode, i64::MAX, JSON-in-TEXT columns, malformed line with line number (AC-21, AC-23) |
| R-03 | High | import-pipeline | Integration | Post-import insert ID > max imported ID (AC-09), counter name/value verification, --force then insert |
| R-04 | High | cli-registration, import-pipeline | Integration | --force drops + imports (AC-27), stderr warning with count, rejection without --force (AC-06), --force on empty DB |
| R-05 | High | embedding-reconstruction | Integration | VectorIndex file presence, entry count in index (AC-10), semantic search post-import (AC-11) |
| R-06 | Med | import-pipeline | Integration | Misordered JSONL produces FK error, valid dependency-ordered file succeeds |
| R-07 | Med | import-pipeline | Unit + Integration | Valid chain passes, broken chain fails, content hash mismatch fails (AC-12, AC-13), empty previous_hash skipped, --skip-hash-validation bypass (AC-14) |
| R-08 | Med | import-pipeline | Integration | PID file warning emitted, no PID file no warning |
| R-09 | Med | embedding-reconstruction | Integration | ONNX unavailable error message clarity, DB valid after embedding failure |
| R-10 | Med | format-types | Unit | f64 confidence precision through JSON round-trip, boundary values 0.0 and 1.0 |
| R-11 | Low | format-types | Unit | Unknown `_table` discriminator produces error with table name and line number |
| R-12 | Low | embedding-reconstruction | Perf | 500-entry import under 60s (AC-17) |
| R-13 | Med | import-pipeline | Integration | Audit provenance event_id > max imported audit event_id (AC-26) |
| R-14 | Low | cli-registration | Integration | --project-dir resolution matches export behavior (AC-19) |
| R-15 | Med | import-pipeline | Unit | SQL injection in string fields prevented by parameterized queries, large strings handled |

## Cross-Component Test Dependencies

- **format-types --> import-pipeline**: Import pipeline depends on correct deserialization. Format unit tests must pass before pipeline integration tests are meaningful.
- **import-pipeline --> embedding-reconstruction**: Embedding runs after DB commit. Tests must verify the DB state independently of embedding success.
- **cli-registration --> import-pipeline**: CLI dispatches to `run_import()`. CLI tests verify argument parsing; pipeline tests verify behavior.

## Integration Harness Plan (infra-001)

### Existing Suites to Run

nan-002 is a CLI-only feature (no MCP tool changes), so existing MCP tool behavior should be unaffected. The import modifies the database that the MCP server reads, so post-import server behavior matters.

| Suite | Relevance | Reason |
|-------|-----------|--------|
| `smoke` | MANDATORY | Minimum gate -- ensures no regression |
| `lifecycle` | Run | Restart persistence tests validate that imported data survives server restart |
| `tools` | Run | Verifies stored data is queryable through MCP tools after import |
| `edge_cases` | Run | Unicode and boundary value behavior on imported data |

### Gaps in Existing Suites

The infra-001 harness tests the MCP server, not CLI subcommands. The import subcommand is invoked directly (not through MCP), so the harness cannot test the import pipeline itself. All import-specific testing must be Rust integration tests.

**No new infra-001 tests are needed** because:
- Import is a CLI subcommand, not an MCP tool -- the harness cannot invoke it.
- Post-import MCP behavior (search, get, status) is already covered by existing `tools`, `lifecycle`, and `smoke` suites.
- AC-11 (server starts and serves queries after import) is validated by running existing suites against a post-import database, but this requires a custom test harness step, not a new pytest test.

### Feature-Specific Integration Tests (Rust)

These are the key integration test scenarios that must be implemented as Rust tests in `crates/unimatrix-server/`:

| Test | Validates | ACs | Risks |
|------|-----------|-----|-------|
| Round-trip (export -> import -> re-export, compare) | Full fidelity across all 8 tables | AC-15, AC-24 | R-01, R-10 |
| Force-import (populated DB -> --force -> verify) | Destructive restore works | AC-02, AC-06, AC-27 | R-04 |
| Hash validation (broken chains, tampered content) | Integrity checking | AC-12, AC-13 | R-07 |
| --skip-hash-validation bypass | Bypass works with warning | AC-14 | R-07 |
| Empty import (header + counters only) | Edge case handling | AC-16 | R-02 |
| Atomicity (mid-import failure -> rollback) | Transaction safety | AC-22 | R-06 |
| Audit provenance (event_id non-collision) | Post-import audit trail | AC-26 | R-13 |
| Counter restoration (post-import insert ID) | ID continuity | AC-09 | R-03 |
| --project-dir resolution | Path handling | AC-19 | R-14 |
| Exit codes (success and failure cases) | Error handling | AC-20 | -- |
| Malformed JSONL with line number | Error reporting | AC-21 | R-02 |

### Test Infrastructure Reuse

Import tests should reuse patterns from `export.rs` tests:
- `tempfile::TempDir` for isolated database directories
- `Store::open()` + direct data population for test fixtures
- Helper functions to create entries with known field values
