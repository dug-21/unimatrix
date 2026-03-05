# nxs-007: Risk & Test Strategy

## Risk Register

### R-01: Compat Type Path Breakage After Module Flattening
**Severity: HIGH** | **Likelihood: MEDIUM** | **Wave: 3**

When compat files move from `sqlite/` to crate root and are renamed (compat.rs -> tables.rs, compat_handles.rs -> handles.rs, compat_txn.rs -> dispatch.rs), all internal `use super::` paths must change to `use crate::`. Additionally, the moved files reference each other:
- handles.rs imports from compat.rs (now tables.rs)
- dispatch.rs imports from both compat.rs and compat_handles.rs

If any import path is missed, the server (which imports 90+ symbols from these modules via lib.rs re-exports) will fail to compile.

**Mitigation**:
- T-01: After Wave 3, `cargo check -p unimatrix-store` must pass
- T-02: After Wave 3, `cargo check -p unimatrix-server` must pass (verifies re-exports work)
- Grep for `super::compat` and `super::txn` in all moved files; every occurrence must become `crate::`

**Scope Risk Trace**: SR-01 (compat layer depth), SR-02 (module flattening)

### R-02: Shared Module Merge Loses Type Definitions
**Severity: HIGH** | **Likelihood: LOW** | **Wave: 4**

The three shared modules (sessions.rs, injection_log.rs, signal.rs) each contain type definitions that are re-exported from lib.rs. If the merge incorrectly drops a type or serialization helper, downstream crates break.

Key types that must survive:
- `SessionRecord`, `SessionLifecycleStatus`, `GcStats`, `TIMED_OUT_THRESHOLD_SECS`, `DELETE_THRESHOLD_SECS`
- `InjectionLogRecord`, `serialize_injection_log`, `deserialize_injection_log`
- `SignalRecord`, `SignalType`, `SignalSource`, `serialize_signal`, `deserialize_signal`

**Mitigation**:
- T-03: After Wave 4, verify all re-exports in lib.rs resolve: `cargo check -p unimatrix-store`
- T-04: After Wave 4, `cargo check -p unimatrix-server` passes (server imports these types)
- T-05: Run full test suite after merge: `cargo test -p unimatrix-store`

**Scope Risk Trace**: SR-02 (module flattening name collisions)

### R-03: Serialization Helper Duplication
**Severity: MEDIUM** | **Likelihood: MEDIUM** | **Wave: 4**

The `serialize_session` / `deserialize_session` functions exist in both root sessions.rs AND sqlite/sessions.rs with identical implementations. During merge, if both copies are kept, the compiler will emit a duplicate definition error. If neither is kept, serialization breaks.

Same applies to `serialize_injection_log` in injection_log.rs.

**Mitigation**:
- T-06: Keep exactly one copy of each serialization helper per module
- T-07: After merge, `cargo test -p unimatrix-store -- session` passes (verifies serialization roundtrip)

**Scope Risk Trace**: SR-02 (module flattening)

### R-04: Schema.rs Shared Types Mixed with Redb Definitions
**Severity: MEDIUM** | **Likelihood: LOW** | **Wave: 5**

Schema.rs has 19 cfg-gated blocks containing redb table definitions, interspersed with shared types. If a shared type (Status, EntryRecord, etc.) is accidentally deleted when removing cfg-gated blocks, the entire crate breaks.

**Mitigation**:
- T-08: Before deleting any block, verify its contents are only redb table definitions (not shared types)
- T-09: After Wave 5, verify these re-exports still work: `EntryRecord`, `Status`, `NewEntry`, `QueryFilter`, `TimeRange`, `DatabaseConfig`, `CoAccessRecord`, serialization helpers
- T-10: `cargo test -p unimatrix-store -- schema` passes (schema roundtrip tests)

**Scope Risk Trace**: SR-05 (schema shared types)

### R-05: Error Variant Removal Breaks Server Match Arms
**Severity: MEDIUM** | **Likelihood: MEDIUM** | **Wave: 5-6**

The server's main.rs matches on `StoreError::Database(redb::DatabaseError::DatabaseAlreadyOpen)`. When the `Database` variant is removed from StoreError (Wave 5), this match arm becomes unresolvable. The server must remove this arm in Wave 6.

If Wave 5 and Wave 6 are not executed atomically for the server, there is a compilation gap.

**Mitigation**:
- T-11: Waves 5 and 6 for the server crate must be applied together: remove cfg gates from store AND remove redb match arms from server in the same commit
- T-12: Grep for `StoreError::Database`, `StoreError::Transaction`, `StoreError::Table`, `StoreError::Storage`, `StoreError::Commit`, `StoreError::Compaction` across the entire workspace before removing variants

**Scope Risk Trace**: SR-06 (error variant references)

### R-06: DatabaseAlreadyOpen Retry Logic May Be Needed for SQLite
**Severity: LOW** | **Likelihood: LOW** | **Wave: 6**

The `open_with_retries` function in server main.rs retries on `DatabaseAlreadyOpen`. SQLite uses a different locking mechanism (file locks via fs2, PidGuard) and may not need this retry logic. However, removing it entirely could regress the startup reliability improvement from vnc-004.

**Mitigation**:
- T-13: Review the vnc-004 PidGuard implementation to confirm that the fs2-based locking handles concurrent access without needing application-level retries
- T-14: If the SQLite backend already has its own open-with-retry or lock detection, the redb retry block can be safely removed
- Decision: The `open_with_retries` function can be simplified to a single `Store::open()` call if PidGuard handles locking. If not, a SQLite-specific SQLITE_BUSY retry should replace it.

**Scope Risk Trace**: SR-06 (error variants)

### R-07: Cargo.lock Drift After Dependency Removal
**Severity: LOW** | **Likelihood: CERTAIN** | **Wave: 6**

Removing `redb = "3.1"` from workspace dependencies will cause `cargo` to remove redb and its transitive dependencies from Cargo.lock. This is expected behavior but the lock file delta will be large (redb pulls in several crates). The PR reviewer should expect this.

**Mitigation**:
- T-15: After Wave 6, `cargo build --workspace` succeeds (lock file is regenerated correctly)
- T-16: Review Cargo.lock diff to confirm only redb and its transitive deps are removed

**Scope Risk Trace**: None (new risk)

### R-08: nxs-006 Merge Conflict Invalidates Design
**Severity: MEDIUM** | **Likelihood: HIGH** | **Wave: All**

nxs-006 is in-flight. If it modifies files that nxs-007 plans to delete or restructure, the implementation plan may need adjustment.

**Mitigation**:
- T-17: Before starting implementation, re-read the nxs-006 final state and diff against the codebase snapshot used for this design
- T-18: Specific files to re-check: lib.rs, main.rs, Cargo.toml files, sqlite/ directory structure
- T-19: If nxs-006 adds new cfg gates or compat types, update the wave plan accordingly

**Scope Risk Trace**: SR-07 (nxs-006 in-flight)

---

## Scope Risk Traceability

| Scope Risk | Architecture Response | Implementation Risks | Test Strategy |
|------------|----------------------|---------------------|---------------|
| SR-01: Compat layer depth | ADR-001: Retain and relocate | R-01 (path breakage) | T-01, T-02 |
| SR-02: Module flattening | ADR-002: Merge strategy | R-02 (type loss), R-03 (duplication) | T-03, T-04, T-05, T-06, T-07 |
| SR-03: test.redb references | Wave 7 cleanup | No residual risk | Grep verification |
| SR-04: Migrate dependencies | Wave 2 atomic delete | No residual risk | Compilation gate |
| SR-05: Schema shared types | Wave 5 careful deletion | R-04 (accidental deletion) | T-08, T-09, T-10 |
| SR-06: Error variant refs | Wave 5-6 coordinated | R-05 (broken match), R-06 (retry) | T-11, T-12, T-13, T-14 |
| SR-07: nxs-006 in-flight | Prerequisite enforcement | R-08 (invalidation) | T-17, T-18, T-19 |

---

## Test Strategy

### Testing Approach

nxs-007 is a subtractive feature. No new tests are written. The test strategy is:
1. **Existing tests are the regression suite** -- every test that passes before nxs-007 must pass after
2. **redb-only tests are deleted** -- they test deleted code
3. **Compilation gates at each wave** -- `cargo check` after every wave
4. **Full test suite after final wave** -- `cargo test --workspace`
5. **Grep verification** -- no residual references to redb, backend-sqlite, test.redb

### Test Deletion Inventory

Tests that will be deleted (they test redb-only code):

| File | Test Module | Lines | Reason |
|------|-------------|-------|--------|
| `src/sessions.rs` | `#[cfg(not(feature = "backend-sqlite"))] mod tests` | ~340 | Tests redb Store methods |
| `src/injection_log.rs` | Redb-only tests if present | ~130 | Tests redb Store methods |
| `src/db.rs` (deleted in Wave 1) | All tests | ~300 | Entire file is redb |
| `src/write.rs` (deleted in Wave 1) | All tests | ~900 | Entire file is redb |
| `src/migration.rs` (deleted in Wave 1) | All tests | ~200 | Entire file is redb |

Tests in the server crate, core crate, and vector crate are backend-agnostic (they go through the Store API) and will all survive.

### Verification Gates (by Wave)

| Wave | Gate | Command |
|------|------|---------|
| 1 | Store compiles | `cargo check -p unimatrix-store` |
| 2 | Store + Server compile | `cargo check -p unimatrix-store -p unimatrix-server` |
| 3 | Store compiles (new structure) | `cargo check -p unimatrix-store` |
| 3 | Server compiles (re-exports work) | `cargo check -p unimatrix-server` |
| 4 | Store compiles (merged modules) | `cargo check -p unimatrix-store` |
| 4 | Store tests pass | `cargo test -p unimatrix-store` |
| 5 | Store compiles without feature flags | `cargo check -p unimatrix-store` |
| 6 | Workspace compiles | `cargo check --workspace` |
| 6 | All tests pass | `cargo test --workspace` |
| 7 | No residual references | `grep -r "test\.redb\|backend-sqlite\|redb::" crates/` returns empty |

### Risk-to-Test Mapping

| Risk | Primary Test | Secondary Test |
|------|-------------|----------------|
| R-01 | T-01: `cargo check -p unimatrix-store` | T-02: `cargo check -p unimatrix-server` |
| R-02 | T-03: `cargo check -p unimatrix-store` | T-05: `cargo test -p unimatrix-store` |
| R-03 | T-06: One copy per helper | T-07: `cargo test -- session` |
| R-04 | T-08: Manual review | T-10: `cargo test -- schema` |
| R-05 | T-11: Coordinated wave | T-12: Grep verification |
| R-06 | T-13: PidGuard review | T-14: Lock handling review |
| R-07 | T-15: `cargo build` | T-16: Lock file review |
| R-08 | T-17: Re-read nxs-006 | T-18: File diff check |

---

## Top 3 Risks by Severity

1. **R-01 (Compat Type Path Breakage)** -- HIGH severity. Module flattening changes every import path in the compat layer. One missed path breaks the server. Mitigated by compilation gates and systematic path replacement.

2. **R-02 (Shared Module Merge Loses Types)** -- HIGH severity. Three critical modules must be merged carefully to preserve types that are part of the public API. Mitigated by explicit merge plans in ADR-002 and post-merge compilation + test verification.

3. **R-05 (Error Variant Removal Breaks Server)** -- MEDIUM severity. Cross-crate dependency between store error types and server match arms requires coordinated changes. Mitigated by applying store and server changes together.
