# Gate 3b Report: nxs-014

> Gate: 3b (Code Review)
> Date: 2026-07-05
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | `verify_entries` / `ChainReport` / `ViolationKind` match pseudocode + integration surface; algorithm follows ADR-003 (index + single O(n) pass, dual-violation no early `continue`, legacy skip). |
| 2. Architecture compliance | PASS | Core is pure & I/O-free in `unimatrix-store`; import + CLI are thin callers over the single oracle; ADR-001/002/003 followed; frozen hash; no schema migration; no new crate dep. |
| 3. Interface implementation | PASS | `verify_entries(&[EntryRecord]) -> ChainReport`; `compute_content_hash` FROZEN (`hash.rs` unchanged); `open_readonly`, `query_all_entries`, `ensure_data_directory` used as specified. |
| 4. Test case alignment | FAIL | Comprehensive R-01..R-12 coverage, BUT `test_verify_cli_opens_readonly` fails deterministically (mis-specified assertion — see finding). |
| 5. Code quality | FAIL | Build clean, clippy clean, no stubs, no prod `.unwrap()`, prod files <500 lines — but the test suite is RED (check 4), so the deliverable is not green. |
| 6. Security | PASS | Read-only open; content-hash+link recompute at import boundary; `describe()` emits ids/hashes only (no raw content); no new dep; no NEW CVE. |
| 7. Knowledge stewardship | WARN | 3 implementation-agent reports carry `## Knowledge Stewardship` blocks (Queried + Stored/nothing-novel). No agent report exists for the correction-write-path (`write_ext.rs`) component — confirm ownership / file the block. |

**Build**: `cargo build --workspace` clean. **Clippy** (`unimatrix-store` + `unimatrix-server`, all-targets): clean.
**Tests**: `chain_verify` 17/17 PASS; `write_ext` correction 5/5 PASS; import single-oracle/R-02/R-05/R-07 PASS; `verify_integration` **9/10 PASS, 1 FAIL**.

## Detailed Findings

### 4/5. `test_verify_cli_opens_readonly` fails — mis-specified assertion (NOT a production bug)
**Status**: FAIL (reworkable — test only)
**Evidence**: The test captures `before = fs::read(db_path)`, runs `run_verify_with_base`, then asserts `after == before` byte-for-byte (verify_integration.rs:353). Byte analysis of the failure:
- `before` = **4096 bytes** (empty DB shell — schema page + header only)
- `after` = **311296 bytes** (full seeded corpus)

The seed (`seed_clean_chain` → `open_store`, WAL mode → inserts → `drop`) left the committed rows in the `-wal` file; the main DB file was still 4096 bytes when `before` was read. The growth to 311296 is a **WAL checkpoint of already-committed data flushed into the main file** — no row was logically added or changed by verify. `SqlxStore::open_readonly` opens with `.read_only(true)` (db.rs:154-162) and `run_verify_async` issues only a `query_all_entries` read (verify.rs:74-81) — the verify path cannot and does not write rows. The R-10 intent (a mis-pointed path cannot corrupt data; verify does not mutate) is satisfied by the production code.

The assertion is invalid under WAL journaling: a checkpoint reorganizes the physical file without changing logical content, and the checkpoint here is a side effect of the seed's hot WAL, not of verify. The suite is red on a test-design defect, so the gate cannot pass, but the fix is confined to the test.
**Fix**: In the seed, force `PRAGMA wal_checkpoint(TRUNCATE)` (or close with a full checkpoint) before capturing `before`, OR assert logical invariance (entry count / stored content/hashes unchanged) instead of raw file bytes.

### 7. Knowledge stewardship — correction-write-path report absent
**Status**: WARN
**Evidence**: `agents/` contains stewardship-complete reports for chain-verify-core, readme-integrity, and verify-cli. The chain-verify-core report's "Files Modified" is `chain_verify.rs` + `lib.rs` only — it does not cover `write_ext.rs`. No report enumerates the correction-write-path work (FR-01/02, C-03) though the code and 5 passing tests are present. Per rubric a missing block is a rework item; confirm whether one agent owned both and simply under-reported, or file the block.

## Requested Confirmations

- **R-01 / C-03 — both write sites fixed, from the record.** CONFIRMED. Struct: `previous_hash: original.content_hash.clone()` (:555), `version: original.version + 1` (:557). INSERT binds: `.bind(&new_rec.previous_hash)` (:601), `.bind(new_rec.version as i64)` (:602) — no inline `""`/`1`. AC-01/AC-02 assert by **DB read-back** (`SELECT previous_hash/version FROM entries WHERE id=...`) in `write_ext` tests (`test_correct_persists_previous_hash_from_db`, `test_correct_persists_version_increment_from_db`, `test_correct_multi_hop_chain_db_readback`) and in `test_import_clean_corrected_corpus_commits` — not from the in-memory record.
- **R-02 — both loaders all-status; Deprecated predecessor counted.** CONFIRMED. `query_all_entries` = `SELECT {ENTRY_COLUMNS} FROM entries` with **no WHERE** (read.rs:325); import `validate_hashes` loads full `ENTRY_COLUMNS` with no status filter from the in-flight conn (import/mod.rs:410-419). Tests: `test_verify_deprecated_predecessor_counted_as_checked` (core), `test_verify_cli_deprecated_predecessor_verifies_clean` (CLI), `test_import_deprecated_predecessor_verifies_clean` (import), plus the `test_query_all_entries_returns_deprecated_rows` loader guard.
- **R-03 / C-02 — empty previous_hash = skipped_legacy; mutation fails loud naming id.** CONFIRMED. chain_verify.rs:166-170 increments `skipped_legacy` and `continue`s (not a break). `test_verify_mixed_legacy_and_chained_is_clean` + `test_roundtrip_then_mutation_fails_loud` (names entry 1) prove the skip is scoped to empty links.
- **R-04 (SCRUTINIZED) — refactor to thin adapter over the stronger supersedes-keyed check.** CONFIRMED, not loosened tolerance. `validate_hashes` is now a thin adapter over `verify_entries` (import/mod.rs:409-428). All three changed import fixtures are justified by the stronger check:
  - `test_hash_validation_valid_chain`: old entry 2 had a populated `previous_hash` with `supersedes=null` (now → `DanglingPreviousHash`). Fixture adds the real `supersedes=Some(1)` edge so it models a genuine clean chain → still `is_ok`. Justified.
  - `test_hash_validation_broken_chain`: old entry 1 carried `previous_hash="nonexistent_hash"` with `supersedes=null`; under the new check that is `DanglingPreviousHash` whose message would not contain the hash. Rewritten so entry 2 supersedes entry 1 with a wrong link → `ChainLinkMismatch` naming `entry 2` + `nonexistent_hash`. **Still `Err` — the break persists**; only the modeling moved onto the authoritative edge. Justified.
  - `test_format_version_2_import_succeeds`: old entry 2 had populated `previous_hash` + `supersedes=null` (now rejected). This test asserts a derived `graph_edges` count, so adding a real edge would perturb it; instead entry 2 was made genesis (`previous_hash=""`). Removes an artificial populated-without-edge state, does not mask a break. Justified.
  Both AND-halves are additionally proven on the import path by new tests (`test_import_rejects_broken_link_with_good_content_hash`, `test_import_rejects_mutated_content_with_good_link`).
  **Flagged behavioral change**: a populated `previous_hash` with `supersedes == None` is now rejected (`DanglingPreviousHash`). This is intended R-04 strengthening and matches production: `correct_entry` co-populates both (`supersedes: Some(original_id)` :547 and `previous_hash: original.content_hash` :555), so a populated link without an edge is unreachable by construction — failing it loud is correct.
- **C-01 — frozen hash.** CONFIRMED. `git diff main...feature/nxs-014 -- hash.rs` is empty; signature `compute_content_hash(title,&content)->String` unchanged; AC-10 known-value vectors untouched.
- **C-06 — new-file size.** PASS under the Gate 3a production-line convention: `chain_verify.rs` prod = 208 lines (tests 209-619 inline), `verify.rs` = 100, `import/mod.rs` non-test = 495. (Total `chain_verify.rs` = 619 incl. inline `#[cfg(test)]`; consistent with existing crate precedent where inline test modules are excluded from the 500 rule — as directed by the spawn note.)
- **cargo audit / new deps.** No new dependency: `Cargo.lock` and all `Cargo.toml` are unchanged by nxs-014. `cargo audit` reports RUSTSEC-2023-0071 (`rsa` Marvin timing sidechannel, transitive) — **pre-existing on `main`, not introduced here**; no NEW CVE. (Plus pre-existing `bincode`/`number_prefix` unmaintained warnings.)

## Rework Required (REWORKABLE FAIL)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| `test_verify_cli_opens_readonly` fails (byte-identity assertion invalid under WAL; seed left data in a hot WAL, main file grows 4096→311296 on checkpoint) | uni-tester (or uni-rust-dev) | Force `PRAGMA wal_checkpoint(TRUNCATE)` in `seed_clean_chain` before capturing `before`, OR replace raw-file-bytes comparison with a logical-invariance assertion (entry count + stored content/hashes unchanged). Production `open_readonly`/`run_verify` are correct — do not change them. |
| Correction-write-path component has no agent report / stewardship block | uni-scrum-master / owning rust-dev | Confirm which agent implemented `write_ext.rs` and ensure a `## Knowledge Stewardship` block exists for that work. |

## Knowledge Stewardship
- Queried: reviewed the three source docs + pseudocode/test-plan; no Unimatrix write performed (validator is read-only here).
- Stored: nothing novel to store -- the WAL-checkpoint-vs-read-only test trap is a candidate pattern, but it is a test-design lesson best captured on rework via `/uni-store-lesson` by the fixing agent, not a cross-feature validation pattern.
