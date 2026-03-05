# nxs-006: Acceptance Map

## Traceability: Acceptance Criteria -> Implementation -> Verification

| AC | Requirement | Implementation (Wave) | Verification Method | Risk Coverage |
|----|-------------|----------------------|--------------------|----|
| AC-01 | Export reads all 17 tables | Wave 1: migrate/export.rs | T-01: Integration test with populated redb | R-01 |
| AC-02 | Import creates equivalent SQLite | Wave 1: migrate/import.rs | T-01, T-02: Row counts + blob fidelity | R-01 |
| AC-03 | Production migration verified | Wave 4: Manual migration | Manual: context_status comparison | R-01, R-02 |
| AC-04 | SQLite is default backend | Wave 3: Cargo.toml changes | T-06: Compilation matrix test | R-03 |
| AC-05 | redb remains compilable | Wave 3: Feature flags preserved | T-06: Build with --no-default-features | R-03 |
| AC-06 | Store unit tests pass (new defaults) | Wave 3: default = backend-sqlite | CI: cargo test -p unimatrix-store | - |
| AC-07 | Server tests pass (new defaults) | Wave 3: default = backend-sqlite | CI: cargo test -p unimatrix-server | - |
| AC-08 | Integration tests pass (SQLite) | Waves 1-3 | CI: cargo test --workspace | - |
| AC-09 | Import refuses overwrite | Wave 1: import.rs precondition | T-09: Unit test | - |
| AC-10 | Default uses SQLite (db filename) | Wave 3: project.rs cfg-gate | T-04: project path unit test | R-02 |
| AC-11 | redb backend compilable for export | Wave 2: cfg-gated Export subcommand | T-06, T-11: Build + compilation test | R-03 |
| AC-12 | Store tests pass | Wave 1 | CI | - |
| AC-13 | Server tests pass | Wave 2-3 | CI | - |
| AC-14 | Project path returns correct filename | Wave 3: project.rs | T-04: Unit test | R-02 |
| AC-15 | Empty tables handled | Wave 1: export/import | T-14: Empty database round-trip | R-07 |

---

## Gate Checklist

Before declaring nxs-006 complete, all items must be checked:

### Code Gates
- [ ] migrate/ module exists in unimatrix-store with export.rs, import.rs, format.rs, mod.rs
- [ ] Export subcommand compiles under redb backend (--no-default-features)
- [ ] Import subcommand compiles under SQLite backend (default)
- [ ] Default features changed: store (backend-sqlite), server (mcp-briefing, backend-sqlite)
- [ ] Engine has backend-sqlite feature; project.rs cfg-gates db_path
- [ ] `cargo build` (default) produces SQLite binary
- [ ] `cargo build --no-default-features --features mcp-briefing` produces redb binary

### Test Gates
- [ ] All 17 tables round-trip test passes (export redb -> import SQLite)
- [ ] Blob fidelity verified for all record types (EntryRecord, CoAccessRecord, etc.)
- [ ] Multimap tables preserve all (key, value) pairs
- [ ] Counter state correct after import
- [ ] Empty table handling works
- [ ] `cargo test -p unimatrix-store` passes (default features)
- [ ] `cargo test -p unimatrix-server` passes (default features)
- [ ] `cargo test --workspace` passes

### Production Gates
- [ ] Export produces intermediate file from production redb database
- [ ] Import creates production SQLite database from intermediate file
- [ ] Per-table row counts verified (export output matches import output)
- [ ] context_status shows same entry counts, categories, topics
- [ ] MCP tools return correct results against migrated database
- [ ] Old redb file backed up as .redb.bak
- [ ] HNSW vector index rebuilt successfully on first startup

---

## Scope Boundary Verification

These items are explicitly OUT OF SCOPE for nxs-006 (deferred to nxs-007):

- [ ] NOT removing redb code from store crate
- [ ] NOT removing cfg gates from store or server
- [ ] NOT flattening sqlite/ module to crate root
- [ ] NOT removing redb from workspace dependencies
- [ ] NOT making rusqlite unconditional (stays optional but default)
- [ ] NOT removing compat layer (compat.rs, compat_handles.rs, compat_txn.rs)
- [ ] NOT decoupling server from Store internals
- [ ] NOT normalizing schema (no SQL JOINs, no index elimination)
