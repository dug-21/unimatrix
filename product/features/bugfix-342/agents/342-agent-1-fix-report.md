# Agent Report: 342-agent-1-fix

## Task
Resolve 19 clippy `-D warnings` violations in `crates/unimatrix-store/`.

## Branch
`bugfix/342-clippy-store-warnings` — committed to `main` (worktree branch is `main`).

## Files Modified
- `/workspaces/unimatrix/crates/unimatrix-store/src/write.rs`
- `/workspaces/unimatrix/crates/unimatrix-store/src/write_ext.rs`
- `/workspaces/unimatrix/crates/unimatrix-store/src/db.rs`
- `/workspaces/unimatrix/crates/unimatrix-store/src/observations.rs`
- `/workspaces/unimatrix/crates/unimatrix-store/src/analytics.rs`
- `/workspaces/unimatrix/crates/unimatrix-store/src/read.rs`
- `/workspaces/unimatrix/crates/unimatrix-store/src/migration.rs`

## Fixes Applied

### 1. `explicit_auto_deref` — 13 occurrences (write.rs:7, write_ext.rs:6)
Changed `&mut *txn` → `&mut txn` at counter function call sites only:
- `crate::counters::next_entry_id(&mut *txn)` → `&mut txn`
- `crate::counters::increment_counter(&mut *txn, ...)` → `&mut txn`
- `crate::counters::decrement_counter(&mut *txn, ...)` → `&mut txn`

**Critical finding**: sqlx executor call sites (`.execute(&mut *txn)`, `.fetch_optional(&mut *txn)`) must NOT be changed — the explicit deref is required there because sqlx's `Executor` trait is not implemented for `&mut Transaction`, only for `&mut SqliteConnection`. The clippy lint fires ONLY at the counter call sites where auto-deref applies.

### 2. `too_many_arguments` — 2 occurrences
Added `#[allow(clippy::too_many_arguments)]`:
- `db.rs:307` — `insert_cycle_event` (8 params)
- `observations.rs:81` — `insert_observation` (8 params)

### 3. `while_let_loop` — 1 occurrence (analytics.rs:298)
Rewrote loop as `while let Ok(Some(e)) = tokio::time::timeout_at(deadline, rx.recv()).await` with comment explaining both exit conditions (channel closed and deadline elapsed).

### 4. `collapsible_if` — 2 occurrences (read.rs:393, read.rs:409)
Merged nested `if let / if` into let-chains using Rust 1.93.1 stable let-chain syntax.

### 5. `needless_borrow` — 1 occurrence (migration.rs:864)
Changed `&data` → `data` where auto-ref applies.

## Test Results
- `cargo clippy -p unimatrix-store -- -D warnings`: **PASS** (0 errors)
- `cargo test -p unimatrix-store`: **144 passed, 0 failed**

## New Tests
None — mechanical lint fixes with no logic change.

## Issues
None.

## Knowledge Stewardship
- Queried: `/uni-query-patterns` for `unimatrix-store` — no relevant clippy or transaction patterns found.
- Stored: attempted via `/uni-store-pattern` but agent lacks Write capability. Pattern documented here instead:
  > In unimatrix-store, `&mut *txn` is used at both sqlx executor sites (necessary — do not change) and counter call sites (unnecessary deref — clippy fires here). Fix only the counter call sites: `counters::next_entry_id`, `counters::increment_counter`, `counters::decrement_counter`. Changing `.execute(&mut *txn)` to `.execute(&mut txn)` causes E0277.
