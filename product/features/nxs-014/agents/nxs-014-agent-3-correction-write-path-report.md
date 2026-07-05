# Agent Report — correction-write-path (nxs-014)

**Agent:** nxs-014-agent-3-correction-write-path
**Component:** correction-write-path
**File:** `crates/unimatrix-store/src/write_ext.rs` (`correct_entry`)
**Traces:** FR-01, FR-02, FR-03, FR-04; AC-01, AC-02, AC-08; R-01/SR-06, R-08; C-03, C-04.

## Work Done

Wired weak-mode cross-version hash-chain population into `correct_entry`, changing all three
hardcoded sites (the two-site half-fix zone plus the FR-04 guard).

### 1. FR-04 guard — reject empty predecessor content_hash
Inserted after the existing Quarantined status check and **before** the Deprecate UPDATE (`~:489`),
so nothing is persisted on rejection (the txn has only read the original at this point). Rejects a
correction whose loaded `original.content_hash` is empty, returning
`StoreError::InvalidInput { field: "original_id", reason }` naming `original_id`. This stops a real
bad-state from being laundered into an unverifiable "legacy" empty `previous_hash` (R-08).

### 2. Struct literal site (FR-01) — was `write_ext.rs:539-540`
- `previous_hash: String::new()` → `previous_hash: original.content_hash.clone()`
- `version: 1` → `version: original.version + 1`

### 3. INSERT bind site (FR-02) — was `write_ext.rs:582-583`
- `.bind("")` → `.bind(&new_rec.previous_hash)`
- `.bind(1_i64)` → `.bind(new_rec.version as i64)`

**Why both (C-03 / R-01):** the struct literal and the INSERT binds are independent. The INSERT
binds are what hit the DB; fixing only the struct compiles clean and returns a correct-looking
record while the persisted row stays `""`/`1`. Both sites now derive from the record fields (single
source of truth) — no inline duplicate literal.

`u32 -> i64` bind for `version` is lossless (widening); `entry_from_row` reads it back into `u32`.

### 4. Tests — 5 new, all DB-read-back
Added to the existing `#[cfg(test)] mod tests` in `write_ext.rs`, using `open_test_store` / `TestEntry`.
Every `previous_hash`/`version` assertion reads BACK FROM THE DB via a fresh `SELECT` on the new id
(helpers `select_previous_hash` / `select_version`) — never from the in-memory returned record (C-04).

- `test_correct_persists_previous_hash_from_db` (AC-01) — persisted `previous_hash == original.content_hash`; fails a struct-only half-fix.
- `test_correct_persists_version_increment_from_db` (AC-02) — persisted `version == 2`; fails the `.bind(1_i64)` half-fix.
- `test_correct_multi_hop_chain_db_readback` (AC-02/FR-03) — N=3 A→B→C chain, all hops read from DB, versions `[1,2,3]`, supersedes edges intact.
- `test_correct_returned_record_agrees_with_db` (R-01 negative control) — in-memory record asserted equal to the DB authority.
- `test_correct_empty_predecessor_content_hash_rejected_names_id` (AC-08/R-08) — `Err(InvalidInput)` naming `original_id`, zero rows persisted, original still Active (Deprecate UPDATE did not run).

## Test Results
`cargo test -p unimatrix-store --lib write_ext`: **8 passed, 0 failed** (5 new + 3 pre-existing).
`cargo clippy -p unimatrix-store --lib --tests`: no warnings.

## Guardrails Honored
Edited only `crates/unimatrix-store/src/write_ext.rs` (source) and this report file. No touch to
chain_verify.rs, lib.rs, read.rs, hash.rs, README, or any server file. Ran no git commands and no
crate-wide `cargo fmt`.

## Issues / Adjacent Breakage
None observed.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern, "sqlx INSERT bind inline literal overrides
  struct field two-site write path") — no strongly relevant prior entry (top hit #4219 struct-rename
  two-error-class at 0.38; nothing on the inline-bind-vs-struct write-path trap).
- Stored: entry **#5506** "correct_entry persists chain fields at TWO independent sites — struct
  literal AND inline INSERT binds; fixing one is false-green" via `/uni-store-pattern`
  (pattern, topic `unimatrix-store`). Captures the runtime-invisible trap: the INSERT binds win at
  write time over the struct, so bind from the record field and never inline a duplicate literal;
  and any guard test must read the value back from the DB, since an in-memory assertion is false-green
  over a struct-only fix.
