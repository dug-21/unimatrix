# nxs-007: Acceptance Map

## AC-to-Wave-to-Test Mapping

| AC | Description | Wave | Verification Method | Status |
|----|-------------|------|---------------------|--------|
| AC-01 | redb impl files deleted (db.rs, read.rs, write.rs, migration.rs, query.rs, counter.rs) | 1 | Files do not exist; `cargo check -p unimatrix-store` passes | Pending |
| AC-02 | migrate/ deleted (export.rs, import.rs, format.rs) | 2 | Directory does not exist; `cargo check -p unimatrix-store` passes | Pending |
| AC-03 | Compat layer handled (relocated and renamed per ADR-001, not deleted) | 3 | tables.rs, handles.rs, dispatch.rs exist at crate root; server compiles | Pending |
| AC-04 | All cfg gates for `backend-sqlite` removed | 4-6 | `grep -r "cfg.*backend.sqlite" crates/` returns 0 results | Pending |
| AC-05 | sqlite/ submodule flattened to crate root | 3-4 | `crates/unimatrix-store/src/sqlite/` directory does not exist | Pending |
| AC-06 | `backend-sqlite` feature removed from all Cargo.toml | 6 | `grep -r "backend-sqlite" crates/*/Cargo.toml Cargo.toml` returns 0 | Pending |
| AC-07 | `redb` removed from workspace dependencies and all Cargo.toml | 6 | `grep -r "redb" Cargo.toml crates/*/Cargo.toml` returns 0 | Pending |
| AC-08 | `rusqlite` unconditional in unimatrix-store | 6 | `rusqlite` in store Cargo.toml has no `optional = true` | Pending |
| AC-09 | Export subcommand removed from server | 2 | `cargo run -p unimatrix-server -- export` fails with "unknown subcommand" | Pending |
| AC-10 | `cargo build --workspace` succeeds | 6 | Build command exits 0 | Pending |
| AC-11 | `cargo test --workspace` passes | 6 | All tests pass | Pending |
| AC-12 | No functional cfg gates referencing redb/backend-sqlite remain | 6 | Grep verification | Pending |
| AC-13 | All 10 MCP tools produce identical results | 6 | Existing test suite passes (behavioral parity) | Pending |
| AC-14 | `test.redb` renamed to `test.db` | 7 | `grep -r "test\.redb" crates/` returns 0 | Pending |
| AC-15 | Import subcommand removed from server | 2 | `cargo run -p unimatrix-server -- import` fails with "unknown subcommand" | Pending |

## Risk-to-AC Trace

| Risk | Affects ACs | Mitigation |
|------|-------------|------------|
| R-01: Path breakage after flattening | AC-03, AC-05 | Compilation gates at Wave 3 |
| R-02: Type loss during merge | AC-04 | Post-merge compilation + tests at Wave 4 |
| R-03: Serialization helper duplication | AC-04, AC-11 | Keep one copy, verify tests pass |
| R-04: Schema type deletion | AC-04, AC-10 | Manual review before deleting cfg blocks |
| R-05: Error variant breaks server | AC-04, AC-10 | Coordinated Wave 5+6 |
| R-06: Retry logic removal | AC-10, AC-13 | PidGuard review confirms safety |
| R-07: Cargo.lock drift | AC-10 | Expected; review lock diff |
| R-08: nxs-006 merge conflict | All | Re-validate before starting |

## ADR Impact on Acceptance Criteria

| ADR | AC Impact |
|-----|-----------|
| ADR-001 (Retain compat) | AC-03 revised: "handled" means relocated, not deleted |
| ADR-002 (Merge strategy) | AC-04, AC-05 depend on correct merge execution |
| ADR-003 (Wave order) | All ACs organized by wave for incremental verification |

## Verification Sequence

1. Pre-flight: confirm nxs-006 merged, re-validate codebase
2. Wave 1: AC-01 (delete redb files)
3. Wave 2: AC-02, AC-09, AC-15 (delete migrate/, remove subcommands)
4. Wave 3: AC-03, AC-05 (flatten sqlite/, relocate compat)
5. Wave 4: AC-04 partial (merge shared modules)
6. Wave 5: AC-04 partial (store cfg gates)
7. Wave 6: AC-04, AC-06, AC-07, AC-08, AC-10, AC-11, AC-12, AC-13 (final cleanup + verification)
8. Wave 7: AC-14 (cosmetic rename)
