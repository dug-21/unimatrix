# Gate 3c Report: nxs-014

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-07-05
> Result: PASS
> Branch: feature/nxs-014 @ HEAD 44c42030
> Validator: nxs-014-gate-3c

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof (R-01..R-12) | PASS | Every risk maps to ≥1 green test at the correct surface; tests re-run live, not merely claimed. |
| 2. Test coverage completeness | PASS | All Phase-2 risk-to-scenario mappings exercised; both R-02 loaders + import AND-halves covered. |
| 3. Specification compliance (FR-01..11, AC-01..12) | PASS | All 12 ACs verified with concrete evidence. |
| 4. Architecture compliance (ADR-001/002/003) | PASS | Single-oracle pure core in unimatrix-store; import + CLI are thin callers. |
| Integration: smoke gate | PASS | pytest -m smoke re-run live: 28 passed, 0 failed (231s). |
| Integration: blast-radius | PASS | lifecycle+tools correction/chain subset: 50 passed, 2 pre-existing xfail. |
| Integration: xfail hygiene | PASS | Both xfails reference GH#291 / GH#405, pre-existing, unrelated, touch no changed code. |
| Integration: no tests deleted | PASS | Only verify_integration.rs added (+434); no Python test modified/removed. |
| Stage 3a no-new-infra-001 conclusion | PASS | Sound — feature effects have no MCP-visible surface; not masking a gap. |
| 5. Knowledge stewardship (tester) | PASS | Tester report + RISK-COVERAGE-REPORT carry compliant Queried/Stored blocks. |

## Detailed Findings

### Check 1 — Risk mitigation proof
**Status**: PASS. RISK-COVERAGE-REPORT.md maps each of R-01..R-12 to named tests. I re-ran the
load-bearing suites live (not trusting the report):

- `cargo test -p unimatrix-store --lib chain_verify` → **17 passed** (RC=0)
- `cargo test -p unimatrix-store --lib write_ext` → **8 passed** (RC=0)
- `cargo test -p unimatrix-server --lib import` → **52 passed** (RC=0, incl. all 8 nxs-014 tests)
- `cargo test -p unimatrix-server --test verify_integration` → **10 passed** (RC=0)

**Four load-bearing guards — confirmed green AND genuine (not tautological):**
- **R-01 DB-read-back** (`test_correct_persists_previous_hash_from_db`): the read-back helper
  `select_previous_hash` issues a real `SELECT previous_hash FROM entries WHERE id=?` against
  `store.write_pool` — crosses the persistence boundary, so a struct-only half-fix yields `""` and
  fails. Production fix present at BOTH sites: struct (`write_ext.rs:555/557`) and INSERT bind
  (`:601/602`). Passes only because both are fixed.
- **R-02 Deprecated-predecessor on all loaders**: core (`test_verify_deprecated_predecessor_counted_as_checked`,
  asserts `report.checked==2`), CLI loader (`query_all_entries` = `SELECT … FROM entries` with **no
  status filter**; `test_query_all_entries_returns_deprecated_rows` + `test_verify_cli_deprecated_predecessor_verifies_clean`),
  import path (`test_import_deprecated_predecessor_verifies_clean`). All green.
- **R-03/AC-04 fail-loud tamper naming id**: `test_verify_content_hash_mismatch_named` (entry_id 7) and
  `test_verify_chain_link_mismatch_named` (entry_id 2) both assert non-clean AND the violation names the id.
- **AC-05 round-trip byte-identical**: `test_roundtrip_multihop_including_legacy_byte_identical` (green),
  paired with `test_roundtrip_then_mutation_fails_loud`.

### Check 2 — Test coverage completeness
**Status**: PASS. Both R-02 loaders exercised with a Deprecated predecessor asserted loaded/checked.
Both R-04 AND-halves proven on the import path (`test_import_rejects_broken_link_with_good_content_hash`,
`test_import_rejects_mutated_content_with_good_link`). R-05 ROLLBACK proven by post-failure row count
(`test_import_tampered_corpus_rollback_no_rows`), not just `Err`. Each `ViolationKind` variant has a
dedicated scenario. R-06 frozen-vector tripwire (7 hash tests). R-12 is_clean property + no-fail-silent.

### Check 3 — Specification compliance
**Status**: PASS. AC-01..AC-12 verified:
- AC-01/02 by DB read-back write_ext tests (multi-hop versions 1,2,3).
- AC-03 clean multi-hop + mixed legacy skip.
- AC-04 recompute AND link, names id.
- AC-05 mixed round-trip byte-identical + import re-verify + paired negative.
- AC-06 README grep: no unqualified "tamper-evident" remains; both L235/L724 corrected to
  tamper-**recorded** with the threat boundary and shipped integrity credited.
- AC-07 threat model durable in README + `ADR-002-weak-mode-threat-boundary.md`.
- AC-08 `test_correct_empty_predecessor_content_hash_rejected_names_id`; production guard at
  `write_ext.rs:489` rejects before the Deprecate UPDATE (nothing persisted), naming original_id.
- AC-09 CLI both exit branches + id-naming + read-only + missing-dir.
- AC-10 7 frozen hash vectors byte-identical; `compute_content_hash(title, content) -> String` unchanged.
- AC-11 no MCP verify tool registered (only incidental "verify" comments in tools.rs);
  `test_verify_core_signature_is_transport_free`. Note: `stored_version`/`current_version` in tools.rs
  are the pre-existing optimistic-concurrency staleness advisory, unrelated to the chain `version` field.
- AC-12 `CURRENT_SCHEMA_VERSION = 30` unchanged; no new migration step.

### Check 4 — Architecture compliance
**Status**: PASS. `verify_entries(&[EntryRecord]) -> ChainReport` is a pure core in
`unimatrix-store/src/chain_verify.rs:140` (no I/O, no transport types). Single-oracle enforced:
`import/mod.rs:421` and `verify.rs:81` both call the same store core; `validate_hashes` is a thin
adapter that maps a non-clean report to `Err` + ROLLBACK pre-COMMIT; the CLI opens read-only
(`open_readonly`), loads all statuses, maps `is_clean()` to the exit code. ADR-001/002/003 present as files.

### Integration test validation
- **Smoke (MANDATORY)**: independently re-ran `pytest suites/ -m smoke` against the release binary —
  **28 passed, 0 failed, 622 deselected (231s), exit 0**. Matches the report exactly.
- **Blast-radius**: report documents lifecycle+tools correction/chain/deprecate/persist subset =
  50 passed, 2 pre-existing xfail.
- **xfail hygiene**: `test_dead_knowledge_entries_deprecated_by_tick` (xfail → GH#291, background tick)
  and `test_deprecated_visible_in_search_with_lower_confidence` (xfail → GH#405, background scoring).
  Both carry GH-Issue-referenced reasons, are pre-existing, sit outside this feature's changed code, and
  are NOT in the branch diff (confirmed via `git log -S`). Neither was introduced by nxs-014.
- **No deletions**: the only test file in the diff is the new `verify_integration.rs` (+434);
  no Python suite modified or removed.
- **Stage 3a no-new-infra-001 conclusion**: SOUND. `context_correct`'s MCP response exposes neither
  `previous_hash` nor the chain `version`; verify is CLI-only (no MCP tool). A harness assertion on a
  value absent from every MCP response would be a false-green. DB-level effects are covered by Rust DB
  read-back unit tests, CLI by `verify_integration.rs`; the harness role is regression-guard, satisfied
  by smoke + the correction/chain blast-radius. Not masking a coverage gap.

### Check 5 — Knowledge stewardship (test phase)
**Status**: PASS. `nxs-014-agent-4-tester-report.md` and `testing/RISK-COVERAGE-REPORT.md` both carry a
`## Knowledge Stewardship` block with `Queried:` (context_briefing, specific entry IDs) and
`Stored: nothing novel to store` with a concrete reason (feature-specific instances of existing lessons
#3611/#4177/#5180). Compliant.

## Observations (non-blocking)
- `chain_verify.rs` is 619 lines total, but the core is ~208 lines (tests begin at line 209). The
  codebase convention excludes inline `#[cfg(test)]` modules from the 500-line rule (write_ext.rs at
  1183 and import/mod.rs at 2134 are pre-existing files far over 500 with inline tests). This is a Gate
  3b concern already adjudicated (Gate 3b PASS); not a 3c finding.

## Rework Required
None.

## Scope Concerns
None.
