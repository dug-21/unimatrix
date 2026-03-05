# nxs-007: Scope Risk Assessment

## Scope Summary

nxs-007 is a mechanical cleanup feature: delete all redb backend code, remove the compat layer, flatten the sqlite/ module, eliminate cfg gates, and clean up dependencies. The scope is subtractive-only -- no new functionality, no behavioral changes, no schema modifications. The feature removes approximately 7,800 lines across store, server, and engine crates, plus the entire migrate/ directory (export, import, format modules).

---

## Risk Catalog

### SR-01: Compat Layer is the Server's Primary Database API
**Severity: HIGH** | **Likelihood: CERTAIN**

The compat layer (compat.rs, compat_handles.rs, compat_txn.rs) is NOT a thin adapter used only at the boundary. It is the primary API surface for database access in the server crate. Codebase analysis reveals 90+ call sites across 8 server source files that use compat types:

- `open_table()` / `open_multimap_table()` calls: 85+ locations
- Table constants (ENTRIES, COUNTERS, STATUS_INDEX, etc.): used everywhere
- Guard types (BlobGuard, U64Guard, CompositeKeyGuard): used for value extraction
- Handle types (TableU64Blob, TableStrBlob, etc.): used for typed table access
- RangeResult: used for iteration

Deleting the compat layer without replacing these call sites will break the entire server. This is NOT a simple file deletion -- it requires either:
(a) Moving compat types into the store crate root (keeping them but removing the "compat" namespace), OR
(b) Rewriting all 90+ server call sites to use the Store API directly (this is nxs-008's scope).

**Impact**: Server will not compile. Every MCP tool handler will be broken.

**Mitigation**: The architect MUST decide: are compat types retained (just relocated) in nxs-007, or is nxs-007 explicitly NOT deleting the compat layer? Option (a) is the mechanical cleanup approach -- move the types, keep the API. Option (b) is nxs-008's job. The SCOPE.md's goal #3 (delete compat layer) may need to be revised.

### SR-02: Module Flattening Name Collisions
**Severity: MEDIUM** | **Likelihood: CERTAIN**

When flattening sqlite/ to crate root, several files have the same name as files being deleted:
- sqlite/db.rs -> src/db.rs (replaces redb db.rs)
- sqlite/read.rs -> src/read.rs (replaces redb read.rs)
- sqlite/write.rs -> src/write.rs (replaces redb write.rs)
- sqlite/migration.rs -> src/migration.rs (replaces redb migration.rs)

Additionally, three pairs have overlapping concerns:
- sqlite/sessions.rs vs src/sessions.rs (both contain session logic, split by cfg gates)
- sqlite/injection_log.rs vs src/injection_log.rs (both contain injection_log logic)
- sqlite/signal.rs vs src/signal.rs (both contain signal logic)

The shared modules (sessions.rs, injection_log.rs, signal.rs) contain type definitions and serialization code that is used by BOTH backends, plus cfg-gated implementation blocks. Flattening requires merging: keep the type definitions from the current root files, merge in the SQLite implementation from the sqlite/ files, and delete the redb implementation blocks.

**Impact**: Incorrect merging could lose type definitions or create duplicate struct definitions.

**Mitigation**: For each conflicting pair, the architect must specify exactly which sections are retained from each file. A line-by-line merge plan for the three shared modules (sessions, injection_log, signal) is required.

### SR-03: test.redb References Span 6 Crates
**Severity: LOW** | **Likelihood: CERTAIN**

The `test.redb` filename string appears in 40+ test locations across 6 crates: unimatrix-store, unimatrix-server, unimatrix-core, unimatrix-vector, unimatrix-engine (via project.rs), and potentially others. Renaming these is a global search-and-replace, but:
- Some references are in redb-only test blocks that will be deleted (store/db.rs tests) -- no rename needed
- Some are in SQLite test paths (store/test_helpers.rs, server tests) -- rename to test.db
- The server error.rs test has a hardcoded "/tmp/test.redb" path in an assertion string

Missing any reference will not cause compilation failure (the filename is arbitrary for SQLite), but defeats the purpose of the rename.

**Impact**: Cosmetic inconsistency only. No functional impact.

**Mitigation**: Global grep for `test.redb` across the entire workspace after removal, verify zero remaining occurrences.

### SR-04: Migrate Module Has Shared Dependencies
**Severity: MEDIUM** | **Likelihood: MEDIUM**

The migrate/ directory contains three files: export.rs (redb-only), import.rs (SQLite-only), and format.rs (shared between both). The SCOPE resolves this -- all three are deleted. However:
- migrate/mod.rs contains the public module declarations and is referenced from lib.rs (`pub mod migrate;`)
- The server main.rs imports from `unimatrix_store::migrate::export::export` and `unimatrix_store::migrate::import::import`
- If the entire migrate/ directory is deleted, the `pub mod migrate;` declaration in lib.rs must also be removed, and the server's import/export subcommand code must be deleted simultaneously.

**Impact**: Compilation failure if any reference is missed.

**Mitigation**: Delete the entire migrate/ directory, remove `pub mod migrate;` from lib.rs, and remove the Export and Import subcommand variants from the server's CLI enum and handlers -- all in the same wave.

### SR-05: Schema Module Contains Shared Types AND Redb Table Definitions
**Severity: MEDIUM** | **Likelihood: CERTAIN**

The schema.rs file (676 lines) is heavily cfg-gated. It contains:
- Shared types used by both backends: EntryRecord, Status, NewEntry, QueryFilter, TimeRange, DatabaseConfig, CoAccessRecord, serialization functions
- redb-specific table definitions: `TableDefinition<u64, &[u8]>` constants for all 17 tables (gated behind `#[cfg(not(feature = "backend-sqlite"))]`)

Removing the redb table definitions is straightforward (delete the cfg-gated blocks). But the shared types must be preserved exactly. The risk is that a shared type or function sits inside a cfg-gated block that looks like it's redb-only but is actually used by SQLite code too.

**Impact**: Missing shared types cause compilation failures in SQLite code.

**Mitigation**: Before deleting any cfg-gated block from schema.rs, verify that the items it contains are not referenced by any SQLite code path. The shared types (EntryRecord, etc.) are all re-exported from lib.rs without cfg gates, which is a good sign, but the architect should verify.

### SR-06: Error Variants May Be Referenced by SQLite Code
**Severity: LOW** | **Likelihood: LOW**

The error.rs file has 31 cfg gate annotations. Most redb error variants (DatabaseAlreadyOpen, RedbStorage, RedbTransaction, RedbCommit, RedbTable, RedbCompaction) are only constructed by redb code. However, the server's main.rs has explicit matching on `StoreError::DatabaseAlreadyOpen` for the redb lock detection path. When this variant is removed, any match arm referencing it must also be removed.

**Impact**: Compilation failure from unresolved match arm.

**Mitigation**: Grep for each removed error variant across the entire workspace before deleting.

### SR-07: nxs-006 In-Flight Merge Conflict Risk
**Severity: MEDIUM** | **Likelihood: HIGH**

nxs-006 is being implemented in parallel. nxs-007 modifies many of the same files (lib.rs, Cargo.toml, main.rs, server.rs). If nxs-006 lands changes to these files after nxs-007 is designed but before nxs-007 is implemented, the implementation may need adjustment.

Specific concern: if nxs-006 adds more compat layer types or modifies the cfg gate structure, nxs-007's wave plan may be invalidated.

**Impact**: Implementation rework, potential scope re-evaluation.

**Mitigation**: nxs-007 implementation should not start until nxs-006 is fully merged. The implementation brief should note which nxs-006 artifacts it depends on. If nxs-006's final state differs from the current codebase snapshot, nxs-007's architecture doc should be re-validated.

---

## Top 3 Risks for Architect Attention

1. **SR-01 (Compat Layer is the Server's Primary DB API)**: The compat layer cannot simply be deleted -- it is used in 90+ server call sites. The architect must decide whether to retain/relocate the compat types (mechanical approach) or defer compat removal to nxs-008 (server decoupling). This is a scope-defining decision that affects the entire wave plan.

2. **SR-02 (Module Flattening Name Collisions)**: Three shared modules (sessions.rs, injection_log.rs, signal.rs) require careful merging of type definitions and SQLite implementations. The architect must specify merge plans for each.

3. **SR-07 (nxs-006 In-Flight Merge Conflict)**: nxs-007 implementation must wait for nxs-006 to land. The architecture should be designed against nxs-006's expected final state, with explicit notes about which assumptions to re-validate.
