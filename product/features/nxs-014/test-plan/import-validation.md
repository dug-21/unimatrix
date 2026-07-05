# Test Plan — import-validation (`unimatrix-server/src/import/mod.rs` `validate_hashes`, refactored)

> `validate_hashes` becomes a thin adapter over `verify_entries` (one oracle, ADR-001), loading rows
> from its in-flight `BEGIN IMMEDIATE` connection. Covers R-04 (behavior-changing refactor), R-05
> (atomicity), R-07 (round-trip), and the R-02 import-path half (Deprecated predecessor on the txn
> connection). Tests extend the existing import test module in `unimatrix-server`.

## R-04 — refactor is behavior-changing; existing tests are the tripwire (High)

### `test_existing_import_hash_validation_tests_unchanged`
- Run the pre-existing `validate_hashes` / import test suite **unchanged FIRST**. Any diff to an
  existing test must be justified as a documented consequence of the strictly-stronger link check
  (keyed on `supersedes` edge), NOT loosened tolerance. A test that had to be weakened is a red flag.
- Preserve the two prior behaviors: content-hash recompute (old `:421`) and empty-`previous_hash`
  tolerance (old `:429`) still hold.

### `test_import_rejects_broken_link_with_good_content_hash` (AND-half 1, AC-04)
- Corpus where every `content_hash` recomputes correctly BUT one successor's `previous_hash` does not
  match its `supersedes` predecessor's `content_hash`. Assert import returns `Err` (the old existence
  check "references *some* known hash" would have passed this — proves the stronger edge-keyed check).

### `test_import_rejects_mutated_content_with_good_link` (AND-half 2, AC-04)
- Corpus where the chain links are internally consistent BUT one entry's `content` was mutated so its
  stored `content_hash` is stale. Assert import returns `Err` (content-hash recompute half runs).
- Together these two prove the single oracle runs BOTH halves on the import path (not just the CLI).

## R-02 (import half) — Deprecated predecessor visible on the txn connection (Critical)

### `test_import_deprecated_predecessor_verifies_clean`
- Import an export whose predecessors are `Deprecated` (superseded) and successors `Active` with
  populated `previous_hash`. Assert import SUCCEEDS (COMMIT) and chain-verify passed — proving the
  import loader reads ALL statuses from the in-flight transaction, not `Active`-only.
- **Teeth:** if the import load filters to `Active`, every successor's predecessor is absent →
  `MissingPredecessor` → false rejection of a clean corrected export. This test fails loud in that case.
- Assert the load happens on the **in-flight `BEGIN IMMEDIATE` connection** (sees uncommitted rows),
  not a committed snapshot — a committed-snapshot read would see zero rows mid-import.

## R-05 — atomicity on a tampered corpus (High)

### `test_import_tampered_corpus_rollback_no_rows`
- Arrange: capture pre-import `SELECT COUNT(*) FROM entries`.
- Act: import an export with a broken chain link (tampered `previous_hash`).
- Assert: import returns `Err`; transaction ROLLBACKs; post-failure `COUNT(*)` == pre-import count
  (NO rows from the failed import present). Post-failure DB state — not just the `Err` — proves the
  ROLLBACK-before-COMMIT branch (R-05 coverage requirement).

### `test_import_clean_corrected_corpus_commits`
- Import a clean corrected export. Assert COMMIT succeeds and rows are present with intact
  `previous_hash`/`version` (read back). Positive control for the atomicity pair.

## R-07 — export/import round-trip lossless (Medium, AC-05, SR-07)

### `test_roundtrip_multihop_including_legacy_byte_identical`
- Build a corpus mixing a legacy entry (`previous_hash = ""`) AND a multi-hop corrected chain (via
  real corrections so `previous_hash`/`version` are populated). Export → import into a fresh DB.
- Assert every entry's `previous_hash` and `version` are byte-identical after re-import: empty stays
  empty (not coerced to NULL or vice-versa), populated stays populated, `version` not reset.
- Assert import-time chain-verify PASSES on the clean re-import (AC-05 positive).

### `test_roundtrip_version_large_value_survives_u32_i64_bind`
- Entry with a large `version` (near `u32` range) survives the `u32`↔`i64` bind round-trip without
  truncation (R-07 boundary).

### `test_roundtrip_then_mutation_fails_loud` (AC-05 paired negative)
- After the clean round-trip, mutate a superseded entry's content in the imported DB; run verify;
  assert non-clean AND the violation names the offending `entry_id`. Proves the skip is scoped to
  legacy, not a blanket pass (paired positive/negative on one corpus).

## Assertions summary (concrete)
- broken-link corpus → import `Err`; mutated-content corpus → import `Err` (both AND-halves)
- Deprecated-predecessor export → import COMMIT clean; predecessor counted as checked
- tampered import → `Err` AND `COUNT(*)` unchanged (ROLLBACK proven by DB state)
- round-trip: `previous_hash`/`version` byte-identical incl. empty-vs-empty and large-version
- existing import tests pass unmodified (or diffs justified by the stronger check, documented)
