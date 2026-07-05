# Gate 3a Report: nxs-014

> Gate: 3a (Component Design Review)
> Date: 2026-07-05
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | Core pure & I/O-free in `unimatrix-store`; import + CLI thin callers; ADR-001/002/003 followed |
| Specification coverage | PASS | FR-01..FR-11 all have pseudocode; AC-01..AC-12 all indexed; no scope additions |
| Risk coverage | PASS | R-01..R-12 each mapped to ≥1 test scenario; Critical/High emphasis reflected |
| Interface consistency | PASS | Shared types defined once; `verify_entries`/`ChainReport` consistent across components |
| Knowledge stewardship | PASS | All 3 design agents have `## Knowledge Stewardship` with Queried + Stored/reason |
| R-01/C-03 two-site + DB read-back | PASS | Both literal sites changed, bound from record; test reads back via fresh SELECT, fails struct-only |
| R-02 Deprecated predecessor / loader | PASS | Both callers load all-status; counted-as-`checked` + `query_all_entries` guard; claim verified vs read.rs:324 |
| R-03/C-02 legacy skip | PASS | Empty `previous_hash` → `skipped_legacy`; mixed clean + mutation-fails-loud-naming-id |
| Counter-semantics adjudication | PASS (1 WARN) | `checked`/`skipped_legacy` definition consistent with R-02 AND R-03; tests assert one definition |
| C-01 frozen hash + AC-10 tripwire | PASS | `compute_content_hash` frozen; hash.rs known-value vectors are the tripwire |
| C-08 README same PR | PASS | Component 5 (readme-integrity) explicitly same PR |

Result: 11/11 checks PASS, 2 WARN (non-blocking).

## Detailed Findings

### Architecture alignment
**Status**: PASS
**Evidence**: `chain-verify-core.md` places `verify_entries(&[EntryRecord]) -> ChainReport` in
`unimatrix-store/src/chain_verify.rs`, "performs no I/O and takes no CLI/MCP types (C-07)" — matches
ARCHITECTURE §Component Breakdown and ADR-001 (leaf-crate placement, no cycle). `import-validation.md`
and `verify-cli.md` both call the same `verify_entries` (single oracle), consuming the shared types
without redefining them. Component decomposition (5 components) maps 1:1 to the ARCHITECTURE table.
`verify_entries` signature and `compute_content_hash` (FROZEN) match the ARCHITECTURE Integration Surface
exactly. Correction path line references (struct :539-540, INSERT bind :582-583, `original` loaded :462)
verified against `write_ext.rs` — all correct.

### Specification coverage
**Status**: PASS
**Evidence**: FR-01 (struct site) → correction-write-path Change 2; FR-02 (INSERT bind) → Change 3;
FR-03 (version monotonicity) → multi-hop data flow; FR-04 (empty-predecessor guard) → Change 1;
FR-05/06 → chain-verify-core; FR-07/10 → import-validation; FR-08 → verify-cli; FR-09 (no MCP) explicit;
FR-11 → readme-integrity. NFR-01..06 applied in OVERVIEW §Cross-cutting constraints. No pseudocode
implements an unrequested feature (Open Q2 `--json` explicitly deferred out of v1). AC-01..AC-12 all
indexed to a component test plan in test-plan/OVERVIEW.md §AC index.

### Risk coverage
**Status**: PASS
**Evidence**: test-plan/OVERVIEW.md §Risk-to-Test Mapping covers R-01..R-12. Critical risks get the
sharpest scenarios: R-01 DB read-back "fails on struct-only fix" (`test_correct_persists_previous_hash_from_db`,
explicit teeth comment); R-02 tested on BOTH callers plus a direct loader guard. Integration and edge
scenarios present (mid-chain Deprecated predecessor, legacy-predecessor+new-successor, long chain,
empty/single corpus). Priority emphasis reflected (Critical/High scenario counts in Coverage Summary).

### Interface consistency
**Status**: PASS
**Evidence**: `ChainReport { checked, skipped_legacy, violations }`, `ChainViolation`, `ViolationKind`
(4 variants) defined ONCE in chain-verify-core.md and OVERVIEW §Shared types; import-validation.md and
verify-cli.md consume `report.is_clean()`/`report.describe()` identically. No contradictory type or
signature across files. Data flow (correction → verify, two callers one core) coherent and matches
ARCHITECTURE §Component Interactions.

### R-01 / C-03 — two-site fix + DB read-back (Critical)
**Status**: PASS
**Evidence**: correction-write-path.md Changes 2 and 3 change BOTH the struct literal (`previous_hash:
original.content_hash.clone()`, `version: original.version + 1`) AND the INSERT binds
(`.bind(&new_rec.previous_hash)`, `.bind(new_rec.version as i64)`), binding from the record; the
"Why both" note names the exact half-fix failure mode. Test `test_correct_persists_previous_hash_from_db`
asserts `SELECT previous_hash FROM entries WHERE id=<new_id>` == `original.content_hash` with an explicit
"MUST FAIL on a struct-only fix" teeth statement, and `test_correct_returned_record_not_sole_authority`
forbids the in-memory-only assertion (C-04). Struct/INSERT line references verified against live code.

### R-02 — Deprecated predecessor / loader coverage (Critical)
**Status**: PASS
**Evidence**: Both loaders load all statuses — CLI via `query_all_entries` (verified at read.rs:324-325:
`SELECT {ENTRY_COLUMNS} FROM entries` with NO `WHERE status`), import via full-row `SELECT {ENTRY_COLUMNS}`
on the in-flight txn connection. The pseudocode claim "no read.rs change needed" is **confirmed true**
against the code. Deprecated-predecessor-counted-as-`checked` asserted on all three surfaces
(`test_verify_deprecated_predecessor_counted_as_checked`, `test_import_deprecated_predecessor_verifies_clean`,
`test_verify_cli_deprecated_predecessor_verifies_clean`), plus the direct loader guard
`test_query_all_entries_returns_deprecated_rows`.

### R-03 / C-02 — legacy tolerance
**Status**: PASS
**Evidence**: chain-verify-core.md step (2): `if e.previous_hash.is_empty()` → `skipped_legacy += 1;
continue` (never a break, matches import/mod.rs:429 convention). `test_verify_mixed_legacy_and_chained_is_clean`
asserts clean + `skipped_legacy == count of empty-prev`. Paired mutation:
`test_roundtrip_then_mutation_fails_loud` and chain-verify content-mismatch tests assert non-clean naming
the offending `entry_id` (AC-04).

### Counter-semantics adjudication (`checked` vs `skipped_legacy`)
**Status**: PASS (1 WARN)
**Evidence / adjudication**: OVERVIEW.md §Counter semantics defines `checked` = incremented once per entry
at top of loop (== `entries.len()`, counts entries examined via content-hash recompute); `skipped_legacy`
= incremented for entries whose CHAIN-LINK step was skipped (`previous_hash == ""`). A legacy/genesis entry
is counted in BOTH. This is **consistent** with:
- R-02: a Deprecated (genesis) predecessor with empty `previous_hash` is examined like any entry → in
  `checked`. `test_verify_deprecated_predecessor_counted_as_checked` requires exactly this.
- R-03: every empty-`previous_hash` entry increments `skipped_legacy`.
The two counters answer different questions and their overlap is intentional and non-contradictory. No test
plan asserts `checked` EXCLUDES legacy — the tension the pseudocode flagged (Open Q1) is resolved in favor
of the definition above. Both R-02 and R-03 scenarios are satisfied under ONE definition. **Not a fail.**
**WARN**: two wordings should be tightened at 3b to remove ambiguity: (a) `test_verify_clean_two_hop_chain_is_clean`
uses `report.checked >= 2` — prefer the exact `checked == corpus.len()` per OVERVIEW Open Q1; (b) the
mixed-corpus test's phrase "counted as skipped, NOT as checked-and-passed" reads as if legacy were excluded
from `checked` — it means the LINK step was not silently passed. Tighten to assert both `checked == corpus.len()`
AND `skipped_legacy == empty_prev_count` so implementer and tester adopt the identical definition.

### C-01 — frozen hash + AC-10 tripwire
**Status**: PASS
**Evidence**: OVERVIEW §Cross-cutting constraints and chain-verify-core.md both pin
`compute_content_hash(title, content)` as recompute-only, "never re-derive/fold `previous_hash`".
readme-integrity.md test plan and chain-verify-core.md §Frozen-hash dependency name the AC-10 tripwire:
existing `hash.rs` known-value vectors (`e3b0c442…` both-empty, `"Test: Content"`) remain byte-identical +
signature grep. Vector tripwire present on the vector path (chain-verify recompute correctness is coupled
to those vectors).

### C-08 — README same PR
**Status**: PASS
**Evidence**: readme-integrity.md is Component 5, "doc-only; no code. It is a required part of the SAME PR
(C-08)"; both overclaim occurrences (README:235 and :722-724) enumerated with replacement intent; AC-07
durability covered via README + ADR-002; grep scan for residual "tamper-evident" specified.

### Knowledge stewardship compliance
**Status**: PASS
**Evidence**: All three design-phase agent reports carry a `## Knowledge Stewardship` block.
- architect (`nxs-014-agent-1-architect-report.md`): `Queried:` context_briefing entries; `Stored:` #5502/#5503/#5504 (ADR-001/002/003) — active-storage agent obligation met.
- spec (`nxs-014-agent-2-spec-report.md`): `Queried:` entries; declined with reason "No storable generalizable pattern (read-only tier)".
- testplan (`nxs-014-agent-2-testplan-report.md`): `Queried:` entries; "nothing novel to store" with a substantive reason (subsumed by #3611/#4177/#4473/#5180).
All present with reasons — no WARN.

## Notes (non-blocking)

- **File-size assessment inaccuracy (WARN, out of scope):** ARCHITECTURE Open Q3 and import-validation.md
  describe `import/mod.rs` as "near the 500-line ceiling" and instruct to "confirm post-edit line count stays
  under 500". The file is actually **1705 lines** — a pre-existing violation outside nxs-014's touch surface
  (this feature only edits `validate_hashes`, lines 396-442). The design is still sound: the refactor deletes
  ~35 / adds ~12 (net shrink) and adds no new oversized file (`chain_verify.rs` ~120, `verify.rs` well under).
  Gate 3b should apply the 500-line rule only to files nxs-014 creates/owns; the pre-existing `import/mod.rs`
  size is a separate cleanup, not an nxs-014 regression.

## Rework Required

None.
