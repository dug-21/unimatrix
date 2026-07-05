# Gate 3b Report: nxs-014

> Gate: 3b (Code Review) — RE-VALIDATION after rework iteration 1
> Date: 2026-07-05
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | `verify_entries` / `ChainReport` / `ViolationKind` match pseudocode + integration surface; ADR-003 single O(n) pass, dual-violation no early `continue`, legacy skip. Unchanged by rework. |
| 2. Architecture compliance | PASS | Core pure & I/O-free in `unimatrix-store`; import + CLI thin callers over the single oracle; ADR-001/002/003 followed; frozen hash; no schema migration; no new crate dep. Unchanged by rework. |
| 3. Interface implementation | PASS | `verify_entries(&[EntryRecord]) -> ChainReport`; `compute_content_hash` FROZEN; `open_readonly`/`query_all_entries`/`ensure_data_directory` used as specified. Unchanged by rework. |
| 4. Test case alignment | PASS | R-01..R-12 coverage; `test_verify_cli_opens_readonly` now asserts a **logical row-set invariant** (id/status/content_hash unchanged) via `query_all_entries` — no raw-byte compare. `verify_integration` **10/10 PASS**. |
| 5. Code quality | PASS | Build clean, clippy clean, no stubs, no prod `.unwrap()`, prod files <500-line convention; full suite GREEN. |
| 6. Security | PASS | Read-only open; hash+link recompute at import boundary; `describe()` emits ids/hashes only; no new dep; no NEW CVE. |
| 7. Knowledge stewardship | PASS | Prior WARN resolved — `nxs-014-agent-3-correction-write-path-report.md` now present with a `## Knowledge Stewardship` block (Queried #4219 miss + Stored #5506). All four impl-agent reports carry stewardship blocks. |

**Tests (this run):** `verify_integration` **10/10 PASS**; `unimatrix-store --lib` **402/402 PASS** (incl. all 5 new `write_ext` correction tests). Both rc=0.

## Rework Verification

### Issue 1 (was FAIL) — `test_verify_cli_opens_readonly` byte-identity assertion
**Status**: RESOLVED — PASS
**Evidence**: Fix commit `44c42030` replaces the raw `fs::read(&db_path)` before/after byte comparison with a `snapshot()` helper that queries the row set via `store.query_all_entries()` and captures `(id, status, content_hash)` per row, sorted by id, asserting `before == after`. This is a logical read-only invariant robust to SQLite WAL checkpointing — exactly what R-10 guarantees (verify cannot mutate any row), without depending on physical file layout.
**Production untouched confirmed**: `git show 44c42030 --stat` contains only `verify_integration.rs` + report files. `db.rs` (`open_readonly`), `verify.rs` (`run_verify*`), and `chain_verify.rs` are NOT in the rework commit — production open-read-only + read-only-query path was not altered to make the test pass. The test was corrected; the code was not weakened.

### Issue 2 (was WARN) — correction-write-path agent report absent
**Status**: RESOLVED — PASS
**Evidence**: `product/features/nxs-014/agents/nxs-014-agent-3-correction-write-path-report.md` now exists. It enumerates the FR-01/FR-02/FR-04 write-path work on `write_ext.rs` (both struct-literal and INSERT-bind sites, C-03), the 5 DB-read-back tests, and a complete `## Knowledge Stewardship` block: Queried (`context_search`, top hit #4219 at 0.38, no relevant prior) + Stored (entry **#5506** via `/uni-store-pattern` on the two-site false-green trap).

## Prior PASS Findings — Regression Re-check (all still hold)

Production non-test source is byte-identical to the pre-rework state (rework commit touched only the test file + reports), so the prior evidence stands verbatim:

- **R-01 / C-03** — both write sites derive from the record (`previous_hash: original.content_hash.clone()`, `version: original.version + 1`; INSERT binds `&new_rec.previous_hash` / `new_rec.version as i64`); AC-01/AC-02 proven by DB read-back tests — all 5 green this run. CONFIRMED.
- **R-02** — `query_all_entries` = `SELECT … FROM entries` no WHERE (read.rs:325); import `validate_hashes` loads full columns no status filter; Deprecated-predecessor counted-as-checked tests pass. CONFIRMED.
- **R-03 / C-02** — empty `previous_hash` → `skipped_legacy` + `continue` (not break); mutation fails loud naming id. CONFIRMED.
- **R-04** — `validate_hashes` thin adapter over `verify_entries`; all three changed import fixtures justified (models moved onto the authoritative `supersedes` edge; breaks persist); populated-link-without-edge now `DanglingPreviousHash` (intended strengthening, unreachable by `correct_entry` construction). CONFIRMED.
- **C-01** — frozen hash: `hash.rs` diff empty; signature + known-value vectors untouched. CONFIRMED.
- **No stubs / no prod `.unwrap()` / new-file size** — unchanged; PASS. The test's `snapshot()` uses `.expect(...)` inside a `#[cfg(test)]` integration test (permitted in test code).
- **No new dependency** — `Cargo.lock` / `Cargo.toml` unchanged; only pre-existing RUSTSEC-2023-0071 (transitive `rsa`, present on `main`); no NEW CVE. CONFIRMED.

## Rework Required

None.

## Knowledge Stewardship
- Queried: prior gate-3b report, source docs, branch diff; validator is read-only here — no Unimatrix write performed.
- Stored: nothing novel to store -- the WAL-checkpoint-vs-read-only test trap was already captured by the fixing agent's pattern #5506; no cross-feature validation pattern to add.
