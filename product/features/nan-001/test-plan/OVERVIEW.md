# nan-001 Test Strategy Overview

## Test Approach

nan-001 (Knowledge Export) is a read-only CLI subcommand producing JSONL output. Testing divides into three layers:

1. **Unit tests** (in `export.rs` `#[cfg(test)]` module) -- per-table serialization, type encoding, edge cases. These form the bulk of coverage since the export logic is pure data transformation.
2. **Integration tests** (`crates/unimatrix-server/tests/export_integration.rs`) -- end-to-end: open a real database, populate data, invoke `run_export()`, parse and verify JSONL output. Covers transaction isolation, determinism, empty database, --project-dir, error paths.
3. **CLI integration** -- binary invocation tests verifying exit codes, --output flag, stderr on error. These can be integration tests using `std::process::Command`.

No infra-001 MCP harness tests are needed (see Integration Harness Plan below).

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Test Layer | Test Plan Section |
|---------|----------|-------------|------------|-------------------|
| R-01 | Critical | row-serialization | Unit + Integration | row-serialization.md: T-RS-01, T-RS-02, T-RS-03 |
| R-02 | High | row-serialization | Unit | row-serialization.md: T-RS-04 |
| R-03 | Critical | row-serialization | Unit | row-serialization.md: T-RS-05 |
| R-04 | Critical | row-serialization | Unit | row-serialization.md: T-RS-06 |
| R-05 | Critical | export-module | Integration | export-module.md: T-EM-01, T-EM-02 |
| R-06 | High | row-serialization | Unit + Integration | row-serialization.md: T-RS-07; export-module.md: T-EM-03 |
| R-07 | Medium | export-module | Integration | export-module.md: T-EM-04 |
| R-08 | High | export-module | Integration | export-module.md: T-EM-05 |
| R-09 | Medium | export-module | Integration | export-module.md: T-EM-06 |
| R-10 | High | export-module, cli-extension | Integration | export-module.md: T-EM-07; cli-extension.md: T-CL-04 |
| R-11 | Medium | row-serialization | Integration (regression) | row-serialization.md: T-RS-08 |
| R-12 | Medium | export-module | Integration | export-module.md: T-EM-08 |
| R-13 | Medium | row-serialization | Unit | row-serialization.md: T-RS-09 |
| R-14 | Medium | row-serialization | Unit | row-serialization.md: T-RS-10 |
| R-15 | Medium | cli-extension | Integration | cli-extension.md: T-CL-03 |

## Cross-Component Test Dependencies

- **cli-extension -> export-module**: CLI dispatch tests depend on `run_export` working. CLI tests focus on argument parsing, exit codes, and output routing (file vs stdout). They invoke the binary or call `run_export()` directly.
- **export-module -> row-serialization**: The export module calls per-table functions that perform row serialization. Integration tests in export-module verify end-to-end correctness. Unit tests in row-serialization verify per-column encoding in isolation.
- **All components -> unimatrix-store**: Tests use `Store::open()` on temp databases. No mocking of the store -- real SQLite is used throughout since the export logic is tightly coupled to SQL types.

## Integration Harness Plan (infra-001)

### Suite Applicability

nan-001 adds a CLI subcommand (`export`). It does NOT modify any MCP server tool logic, store/retrieval behavior, confidence system, contradiction detection, security boundaries, or schema/storage behavior visible through MCP.

| Suite | Applicable? | Reason |
|-------|------------|--------|
| `protocol` | No | Export is CLI-only, no MCP protocol changes |
| `tools` | No | No tool parameters or responses changed |
| `lifecycle` | No | No store/search/correction behavior changed |
| `volume` | No | No MCP-visible scale behavior changed |
| `security` | No | No security boundary changes |
| `confidence` | No | No confidence scoring changes |
| `contradiction` | No | No contradiction detection changes |
| `edge_cases` | No | No MCP-visible edge case behavior changed |
| `smoke` | Yes | Mandatory minimum gate -- regression baseline |

### Smoke Tests (Mandatory Gate)

Run `pytest -m smoke` to verify the export subcommand addition (new clap variant, preserve_order feature flag) does not regress MCP server behavior. This is the R-11 regression check.

### New Integration Tests Needed in infra-001

None. The export subcommand is not accessible through the MCP protocol. All export behavior is testable through Rust unit and integration tests. The MCP harness cannot invoke CLI subcommands.

### Feature-Specific Test Coverage

All 37 risk scenarios from RISK-TEST-STRATEGY.md are covered by Rust-native tests:
- 15 unit tests in `export.rs` (row serialization, type encoding, edge cases)
- 12 integration tests in `tests/export_integration.rs` (full export flows, CLI, errors)

## Acceptance Criteria to Test Mapping

| AC-ID | Test(s) | Layer |
|-------|---------|-------|
| AC-01 | T-CL-01 | Integration (binary) |
| AC-02 | T-CL-02 | Integration (binary) |
| AC-03 | T-EM-09 | Integration |
| AC-04 | T-EM-10 | Integration |
| AC-05 | T-EM-11 | Integration |
| AC-06 | T-RS-01 | Unit + Integration |
| AC-07 | T-EM-05 | Integration |
| AC-08 | T-EM-12 | Integration |
| AC-09 | T-RS-06 | Unit |
| AC-10 | T-EM-08 | Integration |
| AC-11 | T-EM-13 | Integration (benchmark) |
| AC-12 | T-CL-01 | Integration |
| AC-13 | T-CL-03 | Integration |
| AC-14 | T-EM-03 | Integration |
| AC-15 | T-CL-04, T-CL-05 | Integration (binary) |
| AC-16 | T-RS-* (all unit tests) | Unit |
| AC-17 | T-EM-11 | Integration |
| AC-18 | T-EM-04 | Integration |
