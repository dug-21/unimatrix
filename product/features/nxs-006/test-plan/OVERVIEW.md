# nxs-006 Test Plan Overview

## Test Strategy

Tests are organized by risk priority from the RISK-TEST-STRATEGY.md. Each risk maps to specific test cases.

## Risk Mapping

| Risk | Severity | Tests | Component |
|------|----------|-------|-----------|
| R-01 (Data Loss) | CRITICAL | T-01, T-02, T-03 | migrate-module |
| R-02 (Filename) | HIGH | T-04 | feature-flag-flip |
| R-03 (Feature Flags) | HIGH | T-06, T-07 | feature-flag-flip |
| R-04 (Multimap) | HIGH | T-08, T-09 | migrate-module |
| R-05 (Counters) | HIGH | T-10, T-11 | migrate-module |
| R-06 (u64/i64) | MEDIUM | T-12, T-13 | migrate-module |
| R-07 (Empty Tables) | LOW | T-14 | migrate-module |
| R-08 (PID File) | LOW | T-15 | cli-subcommands |

## Test Infrastructure

### Unit Tests (inline in source files)

Located in `crates/unimatrix-store/src/migrate/format.rs`:
- T-03: Base64 round-trip tests
- TableHeader/DataRow serde round-trip tests

Located in `crates/unimatrix-engine/src/project.rs`:
- T-04: Project path db filename cfg-gate test

### Integration Tests (separate test files)

Located in `crates/unimatrix-store/tests/`:

**`migrate_export.rs`** (compiled under redb: `#[cfg(not(feature = "backend-sqlite"))]`)
- Creates a redb database with test data in all 17 tables
- Exports to a temp JSON-lines file
- Verifies file format: 17 table headers, correct row counts
- Verifies key/value encoding: base64 blobs, composite keys, multimap pairs
- Writes fixture file to a well-known path for the import test

**`migrate_import.rs`** (compiled under SQLite: `#[cfg(feature = "backend-sqlite")]`)
- Reads fixture file from the export test (or generates one in-memory)
- Imports into a temp SQLite database
- Verifies row counts for all 17 tables
- Verifies data fidelity: deserializes blobs, checks composite keys, validates counters
- Verifies co_access ordering invariant
- Verifies multimap associations

**Important note on test infrastructure**: Since both backends cannot be compiled simultaneously, the round-trip test requires TWO separate compilation passes. However, for practical CI testing, the import test can also include a self-contained test that generates a known intermediate file programmatically (without needing an actual redb export).

### Build Tests (CI verification)

- T-06: `cargo build -p unimatrix-server` (default features) must succeed
- T-06: `cargo build -p unimatrix-server --no-default-features --features mcp-briefing` must succeed
- T-07: `cargo check -p unimatrix-store` (default) and `cargo check -p unimatrix-store --no-default-features` must succeed

### Integration Harness Plan

From `product/test/infra-001/`:
1. **Smoke tests**: Existing MCP tool smoke tests continue to pass. No changes to tool behavior.
2. **No new integration harness suites needed**: nxs-006 does not change MCP tool behavior, only adds migration tooling and flips defaults.
3. **Existing parity tests**: `crates/unimatrix-store/tests/sqlite_parity.rs` and `sqlite_parity_specialized.rs` continue to validate SQLite backend correctness.

## Test Execution Order

1. Unit tests: `cargo test -p unimatrix-store` (default = SQLite)
2. Unit tests: `cargo test -p unimatrix-engine`
3. Export integration test: `cargo test -p unimatrix-store --no-default-features --test migrate_export`
4. Import integration test: `cargo test -p unimatrix-store --test migrate_import`
5. Workspace tests: `cargo test --workspace`
6. Build matrix: compile both feature flag configurations
