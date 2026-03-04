# Test Plan Overview: nxs-005

## Test Strategy

Testing operates at two levels:

1. **Unit-level parity**: All 234 existing store tests run against whichever backend is compiled. The feature flag selects the Store implementation; tests are backend-agnostic. Additional risk-specific tests target SQLite-specific edge cases.

2. **System-level parity**: The infra-001 integration harness (157 tests, 8 suites) exercises the compiled binary over MCP stdio. Building with `--features unimatrix-store/backend-sqlite` validates that SQLite produces identical system-level behavior.

## Risk Mapping

| Risk | Severity | Test Coverage |
|------|----------|--------------|
| R-01 (semantic divergence) | High | 234 parity tests + boundary value tests + infra-001 full suite |
| R-02 (mutex deadlock) | High | Concurrent stress test (10 threads x 100 ops) + infra-001 lifecycle/volume |
| R-03 (transaction abstraction leak) | Medium | Full workspace compile-check + server integration tests |
| R-04 (migration chain divergence) | High | Migration chain test (v0->v5) with entry verification |
| R-05 (co_access CHECK) | Medium | Explicit CHECK constraint violation test |
| R-06 (signal eviction order) | Medium | Existing 10K cap test passes on both backends |
| R-07 (WAL checkpoint latency) | Low | PRAGMA verification test + informational benchmark |
| R-08 (cfg gaps) | High | Dual-backend compile-check (store + workspace) |
| R-09 (migration tool corruption) | Medium | Migration test with healthy, empty, and corrupt inputs |
| R-10 (counter atomicity) | High | Concurrent counter test (10 threads, verify uniqueness) |

## Integration Harness Plan

### Suites to Run (ALL -- full harness required for storage backend replacement)

| Suite | Tests | Why |
|-------|-------|-----|
| protocol | 13 | MCP handshake validation with new backend |
| tools | 53 | Every tool parameter, every response format |
| lifecycle | 16 | Multi-step flows, correction chains, restart persistence |
| volume | 11 | Scale to hundreds of entries with SQLite |
| security | 15 | Content scanning and capability enforcement |
| confidence | 13 | 6-factor composite formula end-to-end |
| contradiction | 12 | Detection pipeline through full stack |
| edge_cases | 24 | Unicode, boundary values, empty DB, concurrent ops |

### Harness Run Command

```bash
# Build SQLite-backed binary
cargo build --release --features unimatrix-store/backend-sqlite

# Run full harness
cd product/test/infra-001
python -m pytest suites/ -v --timeout=60
```

### New Integration Tests Needed

None. The existing 157 tests comprehensively cover the system-level behavior. The SQLite backend is transparent to the MCP protocol layer.

### Failure Triage (per USAGE-PROTOCOL.md)

- Feature-caused failure -> fix code/test
- Pre-existing failure -> file GH Issue, mark xfail
- Bad test assertion -> fix test, document

## Test Execution Order

1. `cargo test -p unimatrix-store` (redb regression -- must still pass)
2. `cargo test -p unimatrix-store --features backend-sqlite` (SQLite parity)
3. `cargo test --workspace --features unimatrix-store/backend-sqlite` (workspace integration)
4. Build + run infra-001 smoke tests (`-m smoke`)
5. Build + run infra-001 full suite

## Per-Component Test Plans

| Component | Test Plan File | Key Tests |
|-----------|---------------|-----------|
| C1: Connection Manager | test-plan/C1-connection-manager.md | Table creation, PRAGMAs, compact no-op, Send+Sync |
| C2: Write Operations | test-plan/C2-write-operations.md | Insert, update, delete, index sync, counter atomicity |
| C3: Read Operations | test-plan/C3-read-operations.md | All query methods, vector mappings, co-access |
| C4: Specialized Ops | test-plan/C4-specialized-operations.md | Signal queue, sessions, injection log |
| C5: Migration | test-plan/C5-migration.md | Schema chain v0-v5, fresh DB |
| C6: Parity Testing | test-plan/C6-parity-testing.md | Both-backend pass, migration tool, infra-001 |
