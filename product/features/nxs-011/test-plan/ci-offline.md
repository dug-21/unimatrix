# Test Plan: sqlx-data.json + CI Enforcement (ci-offline.md)

**Component**: `sqlx-data.json` (workspace root), `.github/workflows/release.yml`
**Risks**: R-05 (sqlx-data.json drift), R-15 (rusqlite re-introduction)
**ACs**: AC-01, AC-03, AC-05, AC-12, AC-13

---

## Overview

This component has no Rust unit or integration tests. Verification is via grep checks, file existence checks, and CI log review. All checks below are executed in Stage 3c before the delivery gate.

---

## File Existence Checks

### CI-F-01: `sqlx-data.json` exists at workspace root — (AC-12)
- **Check**: `test -f /workspaces/unimatrix/sqlx-data.json`
- **Assert**: File exists; is non-empty; contains valid JSON with a `db` object
- **Risk**: R-05

### CI-F-02: `sqlx-data.json` covers all `sqlx::query!()` call sites
- **Check**: `cargo sqlx check --workspace` exits with status 0 (with `DATABASE_URL` pointed at a schema-v12 DB, or using `SQLX_OFFLINE=false` and a live DB)
- **Assert**: No "query not found in offline data" errors
- **Risk**: R-05

---

## CI Configuration Checks

### CI-C-01: `SQLX_OFFLINE=true` in release.yml — (AC-12, NF-07)
- **Check**: `grep -n "SQLX_OFFLINE" .github/workflows/release.yml` returns one or more matches
- **Assert**: All `cargo build` and `cargo test` steps in `release.yml` have `SQLX_OFFLINE=true` set (either as env at job level or step level)
- **Risk**: R-05

### CI-C-02: `cargo sqlx check` step present in release.yml — (CI-02)
- **Check**: `grep -n "sqlx check\|sqlx prepare" .github/workflows/release.yml` returns a match before the build step
- **Assert**: The step is ordered before `cargo build` (pre-build gate)
- **Assert**: Step is not conditional (`if:` guard must not skip it on PRs)
- **Risk**: R-05

---

## Dependency Removal Checks

### CI-D-01: No rusqlite in `unimatrix-store` Cargo.toml — (AC-01)
- **Check**: `grep -n "rusqlite" crates/unimatrix-store/Cargo.toml` returns zero matches
- **Assert**: Neither `rusqlite` nor `rusqlite-bundled` appears as direct or dev dependency
- **Risk**: R-05, R-15

### CI-D-02: No rusqlite in `unimatrix-server` Cargo.toml — (AC-13)
- **Check**: `grep -n "rusqlite" crates/unimatrix-server/Cargo.toml` returns zero matches
- **Risk**: R-05

### CI-D-03: No rusqlite in `unimatrix-observe` Cargo.toml — (AC-13, SR-07)
- **Check**: `grep -n "rusqlite" crates/unimatrix-observe/Cargo.toml` returns zero matches
- **Risk**: SR-07

### CI-D-04: sqlx listed in `unimatrix-store` Cargo.toml with correct features — (AC-01)
- **Check**: `grep -A3 'sqlx' crates/unimatrix-store/Cargo.toml` shows features including `sqlite`, `runtime-tokio`, `macros`
- **Risk**: AC-01

---

## Source Code Cleanness Checks

### CI-G-01: No `spawn_blocking.*store` in server crate — (AC-05)
- **Check**: `grep -rn "spawn_blocking.*store\." crates/unimatrix-server/src/` returns zero matches
- **Risk**: R-15

### CI-G-02: No `Mutex::lock`, `lock_conn`, or `spawn_blocking` in store src — (AC-03)
- **Check**: `grep -rn "Mutex::lock\|lock_conn\|spawn_blocking" crates/unimatrix-store/src/` returns zero matches
- **Risk**: R-15 (AC-03)

### CI-G-03: No `unimatrix_store::rusqlite` anywhere — (AC-13)
- **Check**: `grep -rn "unimatrix_store::rusqlite" crates/` returns zero matches
- **Risk**: R-05

### CI-G-04: No `pub use rusqlite` in store lib.rs — (AC-13)
- **Check**: `grep -n "pub use rusqlite" crates/unimatrix-store/src/lib.rs` returns zero matches
- **Risk**: R-05

### CI-G-05: No `AsyncEntryStore` anywhere — (AC-04)
- **Check**: `grep -rn "AsyncEntryStore" crates/` returns zero matches
- **Risk**: R-15

### CI-G-06: No `SqliteWriteTransaction` or `MutexGuard` in production code — (AC-16)
- **Check**: `grep -rn "SqliteWriteTransaction\|MutexGuard" crates/unimatrix-server/src/ crates/unimatrix-store/src/ crates/unimatrix-observe/src/` returns zero matches
- **Risk**: R-09

### CI-G-07: txn.rs file deleted — (AC-16)
- **Check**: `test -f crates/unimatrix-store/src/txn.rs` returns failure
- **Risk**: R-09

### CI-G-08: No SQL injection via `format!` string interpolation in store crate
- **Check**: `grep -rn 'format!.*SELECT\|format!.*INSERT\|format!.*UPDATE\|format!.*DELETE' crates/unimatrix-store/src/` returns zero matches
- **Risk**: Security requirement from IMPLEMENTATION-BRIEF.md

---

## Offline Build Test

### CI-B-01: `cargo build` succeeds with `SQLX_OFFLINE=true` and no live DB
- **Check**: `SQLX_OFFLINE=true cargo build --workspace` exits with status 0
- **Assert**: Build completes using only `sqlx-data.json`; no `DATABASE_URL` environment variable needed
- **Risk**: R-05 (AC-12)

### CI-B-02: `cargo test --workspace` succeeds with `SQLX_OFFLINE=true`
- **Check**: `SQLX_OFFLINE=true cargo test --workspace 2>&1 | tail -30`
- **Assert**: All tests pass; total count ≥ 1,649 (AC-14)
- **Risk**: R-14

---

## Summary: Stage 3c Execution Sequence

Run all checks in this order in Stage 3c:

1. CI-F-01 (sqlx-data.json exists)
2. CI-D-01 through CI-D-04 (dependency checks)
3. CI-G-01 through CI-G-08 (source cleanness)
4. CI-G-07 (txn.rs deleted)
5. CI-C-01 and CI-C-02 (release.yml configuration)
6. CI-B-01 (offline build)
7. CI-B-02 (offline test run + AC-14 count gate)
8. CI-F-02 (sqlx check — requires live DB or prior run)

Any single check failure is a delivery blocker. No partial credit.
