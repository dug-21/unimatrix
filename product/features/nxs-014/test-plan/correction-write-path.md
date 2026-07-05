# Test Plan — correction-write-path (`unimatrix-store/src/write_ext.rs`, changed)

> The two-site half-fix zone (R-01, Critical) + empty-predecessor reject (R-08). Tests extend the
> existing `#[cfg(test)]` module in `write_ext.rs` (`open_test_store`, `TestEntry`, `#[tokio::test]`).
> **The load-bearing rule: every assertion reads `previous_hash`/`version` BACK FROM THE DB via a
> fresh `SELECT` — never from the returned in-memory `EntryRecord`.** An in-memory-only assertion
> passes green over the struct-only half-fix that leaves the INSERT binding `""`/`1` (C-04, SR-06).

## R-01 — DB read-back after correction (the false-green killer, Critical)

### `test_correct_persists_previous_hash_from_db` (AC-01)
- Arrange: `open_test_store`; insert an entry via `TestEntry`; capture `original.content_hash`
  (read it back from the DB, or from the insert result) and `original_id`.
- Act: drive a real `context_correct` through `correct_entry(...)`.
- Assert: `SELECT previous_hash FROM entries WHERE id = <new_id>` (fresh query against
  `store.write_pool`/`read_pool()`, via `entry_from_row` or direct column read).
  `persisted_previous_hash == original.content_hash`.
- **Teeth:** MUST FAIL on a struct-only fix — the struct literal at `:539` can be correct while the
  INSERT bind at `:582` still binds `""`. This test reads the persisted column, so a struct-only fix
  yields `""` and the assertion fails. State this explicitly in a test comment.

### `test_correct_persists_version_increment_from_db` (AC-02)
- Act: correct an entry whose persisted `version` is known (genesis = 1).
- Assert: `SELECT version FROM entries WHERE id = <new_id>` reads back `original.version + 1` (== 2).
- **Teeth:** fails on the `:583` `.bind(1_i64)` half-fix (would read 1, not 2).

### `test_correct_multi_hop_chain_db_readback` (AC-02, FR-03)
- Act: build an N=3 chain — correct the original, then correct the successor, then correct again.
- Assert: read ALL three successor rows back from the DB. For each hop assert
  `row.previous_hash == predecessor.content_hash` (predecessor read back from DB too) and versions
  are `1, 2, 3` monotonic in supersession order. `supersedes`/`superseded_by` edges intact.
- **Teeth:** any hop persisting `""`/`1` fails; catches a fix that works for hop 1 but not chained.

### `test_correct_returned_record_not_sole_authority` (R-01 negative control)
- Documents (comment/marker) that the returned in-memory `EntryRecord` reflecting the correct link
  is NOT sufficient — the DB read-back above is the authority. Assert the DB value equals the
  in-memory value on a full fix (they agree), so a divergence (half-fix) is caught by the DB read.
  Do not rely on the in-memory value as the only check.

## R-08 / AC-08 — empty predecessor `content_hash` rejected at correction (FR-04)

### `test_correct_empty_predecessor_content_hash_rejected_names_id`
- Arrange: construct/insert an entry with an empty `content_hash` (force via direct
  `UPDATE entries SET content_hash = '' WHERE id = ?`), keep it `Active`.
- Act: attempt `correct_entry` on it.
- Assert: correction returns `Err(...)` (e.g. `StoreError::InvalidInput { field: "original_id"/"content_hash", .. }`)
  and the error message/field NAMES `original_id`. NOT an empty `previous_hash` persisted.
- Assert: **no row persisted** — post-failure `SELECT COUNT(*)` for the would-be successor is 0, and
  the original is NOT deprecated (correction failed atomically before write). Confirms the check
  fires at correction time (write path), before persistence (R-08 coverage requirement).
- **Boundary:** distinguishes a REAL bad-state (active entry, empty hash → hard error) from
  forward-only legacy tolerance in verify (empty `previous_hash` on a *successor* = skip). The write
  path must NOT launder a bad predecessor into a legacy skip.

## Regression guard (existing behavior preserved)
- Existing correction tests (`test_*correct*` in this module and the deprecate-on-correct path) MUST
  still pass unchanged: original goes `Deprecated`, `superseded_by = new_id`, new entry
  `supersedes = original_id`, `correction_count` increments. The link population is additive to this
  established path (`write_ext.rs:439-620`).

## Assertions summary (concrete)
- `SELECT previous_hash FROM entries WHERE id=<new_id>` == `original.content_hash`
- `SELECT version FROM entries WHERE id=<new_id>` == `original.version + 1`
- N=3: persisted versions `[1,2,3]`, each `previous_hash == predecessor.content_hash`
- empty-predecessor correction → `Err` naming `original_id`, zero rows persisted, original still Active
- No assertion in AC-01/AC-02 tests reads from the returned in-memory record as its sole basis (C-04).
