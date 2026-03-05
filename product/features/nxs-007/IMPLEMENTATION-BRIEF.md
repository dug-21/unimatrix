# nxs-007: Implementation Brief -- redb Removal

## Summary

Remove the redb storage backend from the Unimatrix workspace after nxs-006 confirms SQLite stability. This is a mechanical cleanup: delete ~6,500 lines of dead redb code, relocate and rename the compat layer, flatten the sqlite/ module to crate root, remove cfg gates, and clean up dependencies. No behavioral changes. 7 waves, each producing a compilable state.

**Prerequisite**: nxs-006 must be fully merged before implementation begins.

---

## Wave Plan

### Wave 1: Delete redb Implementation Files
**Scope**: AC-01
**Files to delete**: `src/db.rs`, `src/read.rs`, `src/write.rs`, `src/migration.rs`, `src/query.rs`, `src/counter.rs` (all in unimatrix-store)
**Modify**: `src/lib.rs` -- remove 6 cfg-gated module declarations and redb re-export block (lines 9-21, 46-57)
**Gate**: `cargo check -p unimatrix-store`
**Estimated effort**: Small. Pure deletion + lib.rs edit.

### Wave 2: Delete migrate/ Directory
**Scope**: AC-02, AC-09, AC-15
**Files to delete**: Entire `src/migrate/` directory (export.rs, import.rs, format.rs, mod.rs)
**Modify**: `src/lib.rs` -- remove `pub mod migrate;`
**Modify**: `crates/unimatrix-server/src/main.rs` -- remove Export/Import command variants, handler functions (`run_export`, `run_import`), and `use unimatrix_store::migrate` import
**Gate**: `cargo check -p unimatrix-store -p unimatrix-server`
**Estimated effort**: Small. Deletion + server CLI cleanup.

### Wave 3: Flatten sqlite/ Module to Crate Root
**Scope**: AC-03 (revised: relocate, not delete), AC-05
**File moves**: 9 files from `sqlite/` to `src/` root (see ARCHITECTURE.md ADR-001, ADR-003)
**Key renames**: compat.rs -> tables.rs, compat_handles.rs -> handles.rs, compat_txn.rs -> dispatch.rs
**Modify**: All moved files -- change `use super::` to `use crate::`
**Modify**: `src/lib.rs` -- replace `mod sqlite;` with individual module declarations, update re-exports
**Gate**: `cargo check -p unimatrix-store -p unimatrix-server`
**Estimated effort**: Medium. Path rewriting requires careful attention.
**Risk**: R-01 (highest risk wave). Systematic grep-and-replace for `super::` paths.

### Wave 4: Merge Shared Modules
**Scope**: AC-04 (partial)
**Merge**: sessions.rs (root types + sqlite/sessions.rs impl)
**Merge**: injection_log.rs (root types + sqlite/injection_log.rs impl)
**Merge**: signal.rs (root types + sqlite/signal.rs impl)
**Delete**: Remaining sqlite/ files and directory
**Procedure per file**: (1) Remove redb imports and cfg-gated redb impl blocks from root file, (2) Remove cfg gates from remaining code, (3) Append SQLite impl Store block from sqlite/ file, (4) Deduplicate serialization helpers, (5) Update import paths
**Gate**: `cargo check -p unimatrix-store` then `cargo test -p unimatrix-store`
**Estimated effort**: Medium. Merge requires understanding what each section does.
**Risk**: R-02 (type loss), R-03 (helper duplication).

### Wave 5: Remove cfg Gates from Store Crate
**Scope**: AC-04, AC-12
**Modify**: `error.rs` -- delete 6 redb error variants + their Display/Error/From impls, remove cfg gate from Sqlite variant
**Modify**: `schema.rs` -- delete lines 1-91 (redb table definitions + `use redb` import)
**Modify**: `test_helpers.rs` -- remove cfg gates
**Modify**: `lib.rs` -- remove any remaining cfg gates
**Gate**: `cargo check -p unimatrix-store`
**Estimated effort**: Small. Systematic cfg removal.

### Wave 6: Remove cfg Gates + Dependencies from Engine, Server, Workspace
**Scope**: AC-04, AC-06, AC-07, AC-08, AC-10, AC-11, AC-12, AC-13
**Modify**: `unimatrix-engine/src/project.rs` -- remove cfg gates, keep SQLite path
**Modify**: `unimatrix-engine/Cargo.toml` -- remove `[features]` section
**Modify**: `unimatrix-server/src/main.rs` -- remove `DatabaseAlreadyOpen` retry block
**Modify**: `unimatrix-server/Cargo.toml` -- remove redb dep, remove backend-sqlite feature
**Modify**: `unimatrix-store/Cargo.toml` -- remove redb dep, make rusqlite unconditional, remove backend-sqlite feature
**Modify**: `Cargo.toml` (workspace root) -- remove `redb = "3.1"` from workspace deps
**Gate**: `cargo check --workspace` then `cargo test --workspace`
**Estimated effort**: Small. Cargo.toml edits + cfg removal.
**Risk**: R-05 (error variant mismatch). Apply store error changes and server match arm removal together.

### Wave 7: Cosmetic Cleanup
**Scope**: AC-14
**Search-replace**: `test.redb` -> `test.db` across all crates (~40 occurrences, ~15 files)
**Also**: `reopen.redb` -> `reopen.db` in sessions.rs test
**Also**: `unimatrix.redb` -> `unimatrix.db` in server error.rs test assertions
**Update comments**: Remove redb references, update "TEMPORARY" labels on renamed compat files
**Gate**: `grep -r "test\.redb" crates/` returns empty. `cargo test --workspace` passes.
**Estimated effort**: Small. Global search-and-replace.

---

## File Inventory Summary

| Action | Count | Lines |
|--------|-------|-------|
| Files deleted (redb impl) | 6 | ~5,190 |
| Files deleted (migrate/) | 4 | ~1,250 |
| Files moved/renamed (sqlite/ -> root) | 9 | ~2,245 (moved, not net change) |
| Files merged (shared modules) | 3 | redb blocks deleted, SQLite blocks absorbed |
| Files deleted (sqlite/ after merge) | 4 | ~0 (content already merged) |
| Files modified (cfg removal) | ~15 | cfg blocks removed |
| Files modified (test.redb rename) | ~15 | string replacement |
| Cargo.toml files modified | 4 | dependency/feature changes |

**Estimated net line removal**: ~5,600 lines (delete ~7,840, move ~2,245 up)

---

## Implementation Constraints

1. **No git commits until each wave is verified** -- each wave must pass its compilation gate before moving to the next
2. **nxs-006 must be merged first** -- re-validate the codebase snapshot before starting
3. **No behavioral changes** -- if any test fails for a non-obvious reason, investigate before proceeding
4. **Test infrastructure is cumulative** -- do not create new test files or scaffolding
5. **Waves 5 and 6 must be coordinated for server crate** -- error variant removal in store (Wave 5) and match arm removal in server (Wave 6) should be applied in the same commit to avoid intermediate compilation failure

---

## ADR Summary

| ADR | Decision | Impact |
|-----|----------|--------|
| ADR-001 | Retain compat types, relocate to root, rename | Server requires zero DB code changes |
| ADR-002 | Merge shared modules per explicit merge plan | Types preserved, redb impl deleted |
| ADR-003 | 7-wave execution with compilation gates | Each wave independently verifiable |

---

## Vision Variance

**V-01**: Compat layer relocated rather than deleted (see ALIGNMENT-REPORT.md). Accepted per ADR-001. Compat deletion is nxs-008.
