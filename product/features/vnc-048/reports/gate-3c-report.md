# Gate 3c Report: vnc-048

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-07-19
> Result: PASS

Validated from artifacts (RISK-COVERAGE-REPORT.md + tester's posted GH #953 results, two passes) plus
ground-truth code reads of the two gate non-negotiable tests. Full cargo/clippy/pytest suites NOT
re-executed per spawn instruction — the Stage 3c tester ran them twice (initial + post-rework re-verify).

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Behavioral-outcome proof (scope lens) | PASS | All 7 SCOPE entry-point outcomes map to passing tests driving the operator's real invocation |
| 2. Risk mitigation proof | PASS | R-01..R-14 all mapped to passing tests; RISK-COVERAGE-REPORT full coverage |
| 3. Test coverage completeness | PASS | All Phase-2 risk→scenario mappings exercised; integration + edge cases covered |
| 4. Specification compliance | PASS (1 accepted gap) | AC-01..AC-13 verified; AC-08 (Med, non-gate) uncovered — accepted non-blocking |
| 5. Architecture compliance | PASS | Single `resolve_slug_store` funnel; ADR-001..006 honored |
| 6. Knowledge stewardship | PASS | RISK-COVERAGE-REPORT has `## Knowledge Stewardship` with Queried + Stored(nothing-novel + reason) |
| Gate NN #1 — AC-09 disagreement seam | PASS (genuine) | Verified in code, not just report |
| Gate NN #2 — AC-12 served vector from `start` | PASS (genuine) | Verified in code, real served query, not disk-state |
| Integration smoke (non-regression) | PASS | 35 passed / 0 failed; no tests deleted/commented/xfail'd |

## Detailed Findings

### Gate Non-Negotiable #1 — AC-09 / R-01 S1 disagreement seam
**Status**: PASS (genuine, verified in code)
**Evidence**: `export.rs:2798 test_export_slug_emits_slug_store_not_hash_store`. Seeds set A `{101,102,103}`
via `seed_slug_store` (runtime literal-slug layout) and disjoint NON-EMPTY set B `{201,202,203}` via
`seed_store_at(&paths.db_path)` (path-hash layout) — two distinct seed code paths. Drives the CLI resolver
via `run_export_with_base(slug=Some("alpha"))`, asserts `emitted == A` and every B id absent. Paired guard
`test_export_no_slug_emits_hash_store_divergence_guard` (export.rs:2831) proves no-slug emits exactly B —
so the fixture is not aliasing one store onto both. Not a ceremonial N=1 test (#4974). Satisfies SR-01/SR-09.

### Gate Non-Negotiable #2 — AC-12 / R-03 S2 served vector from `start`
**Status**: PASS (genuine, verified in code)
**Evidence**: `import_integration.rs:2166 test_restore_sequence_serves_vector_search_from_start` (commit
`18c50cdb`). Drives assembled `register A → seed 3-entry semantic corpus → export --slug A →
register fresh B → import --slug B`. Simulates `start` via the daemon's exact boot path: `SqlxStore::open`
(Arc) → probe `unimatrix-vector.meta` → `VectorIndex::load(store, VectorConfig::default(), &{bslug}/vector)`
against the POST-IMPORT on-disk state — no pre-import in-memory index exists to reuse. Asserts
`boot_index.point_count() == 3`, then embeds a real query with the same `OnnxProvider`/`EmbedConfig::default()`
model and asserts the SERVED result `boot_index.search(...)[0].entry_id == 1` (restored async-runtime entry
ranks top over sourdough/hiking distractors). The single `file_count(&paths.vector_dir)==0` is a negative
precondition guard (path-hash vector dir untouched), NOT the outcome. This is an assembled-path served-query
proof — breaks the #917/#918/#930 disk-state/in-memory proxy family for SR-10. Tester independently verified
the boot path matches `http_provision.rs:186-224` verbatim.

**Two-pass provenance (GH #953)**: initial Stage 3c reported AC-12 FAIL (only the AC-02 disk-state proxy
existed); rework commit `18c50cdb` added the served-query test; tester v2 re-verified genuine. Correct
gate discipline — the gap was caught and closed, not waved through.

### Check 1 — Behavioral-outcome proof (scope behavioral lens)
**Status**: PASS
**Evidence**: Every entry point + outcome in SCOPE-RISK-ASSESSMENT.md drives the operator's real CLI path:
- `export --slug` corpus (not sibling/hash) → AC-09 seam via `run_export_with_base`.
- `import --slug` serves restored corpus incl. vector after restart → AC-12 served-query test.
- no-`--slug` byte-for-byte → R-09 (`test_import_no_slug_writes_to_path_hash_data_dir`, divergence guard) + suites unchanged.
- missing store fails loud, creates nothing → R-02/R-14 (`test_export/import_slug_missing_store_fails_loud_fs_unchanged`).
- charset-invalid/reserved rejected at CLI edge → R-08 (`_invalid_rejected_no_fs_touch` + unit reject set).
- register→stop→import→start serves vector → AC-12.
- live daemon PID hard-errors → R-11/R-03 S1 (`test_import_slug_live_pid_hard_errors_no_vector_write`).
No outcome proven only by a seam one layer beneath the entry point.

### Check 2 — Risk mitigation proof
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md maps R-01..R-14 to named passing tests, all marked Full. R-03
(the only Critical with two scenarios) has S1 (live-PID unreachability) and S2 (served-vector-from-start)
both covered.

### Check 3 — Test coverage completeness
**Status**: PASS
**Evidence**: All Phase-2 risk→scenario mappings exercised (funnel units 12/12, export integration 21/21,
import integration 27/27). Cross-component risks (vector rebuild redirect, PID base-scoping, funnel↔ensure_data_directory
coupling) covered by integration tests. Edge cases (base `parent()==None` fallback, boundary slug lengths,
stray-hash-dir boundary) covered.

### Check 4 — Specification compliance
**Status**: PASS (1 accepted non-gate gap)
**Evidence**: AC-01..AC-07, AC-09..AC-13 all PASS with named evidence. AC-08 (export against a live daemon's
slug store, read-only under WAL — `test_export_slug_readonly_under_wal_writer` not implemented) is NOT COVERED.
**Accepted**: AC-08 is Med weight and explicitly NOT a gate non-negotiable (Risk Strategy §"Non-negotiable for
gate" names only R-01 S1 and R-03 S2). It is the read-side of R-03; R-03's blocking scenarios (S1 clobber
unreachability, S2 served vector) are both covered, so its absence hides no blocking risk. Recommend adding
for completeness or explicit descope at retro. (The `AC-08` strings in `export_integration.rs` belong to
nxs-012's numbering, not this feature.)

### Check 5 — Architecture compliance
**Status**: PASS
**Evidence**: Implementation uses the single `resolve_slug_store` funnel (validate → derive base → join →
existence gate) per ADR-001/002. ADR-003 (live-PID-only refusal) verified by R-11 tests; ADR-004 (vector
rebuild into `slug_dir/vector`, base-scoped PID) verified by AC-02/AC-12 tests; ADR-005 (non-empty-audit
pre-flight refusal) by R-07; ADR-006 (fail-loud + export summary) by R-10/AC-03. No architectural drift.

### Check 6 — Knowledge stewardship
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md `## Knowledge Stewardship` (lines 144-153) has `Queried:`
(context_briefing surfacing #4781/#2758/#4202 family, applied to the AC-12 verification) and
`Stored: nothing novel -- {reason}` (the disk-state-proxy and named-test-never-implemented lessons already
exist; flakes routed to GH issues per "bugs are GH issues, not lessons"). Reason present — not a WARN.

## Integration Test Validation

- **Smoke gate**: `pytest -m smoke` → 35 passed / 0 failed / 667 deselected. Feature is CLI-only with no
  MCP surface, so Stage 3a correctly planned NO new pytest tests; smoke runs as a pure non-regression gate.
- **No integration tests deleted, commented out, or xfail-marked** (RISK-COVERAGE-REPORT §"Pre-Existing").
- **Integration test counts present** in RISK-COVERAGE-REPORT (export 21, import 27, smoke 35).
- **Smoke carry-forward** after `18c50cdb` justified: the only change was one added Rust test; no server
  source, tool surface, or signature touched — tool-count assertion (15, #942) stays green.

## Pre-Existing Flakes (triaged — NOT vnc-048)

Confirmed vnc-048's diff touches only `crates/unimatrix-server` and no eval or unimatrix-vector files:
- `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` — eval-odometer harness on the ass-098
  branch base (commit 515d1710); empty vnc-048 diff; passes on re-run. Tracked **GH #790**. Unrelated.
- `unimatrix-vector index::tests::test_self_search_50_entries` — ANN-recall parallel-load flake in an
  untouched crate; 113/113 in isolation. Tracked **GH #958**. Unrelated.
- clippy #935 `verbosity.rs manual_repeat_n` — untouched file; did not surface on this toolchain. Not attributable.

Both flakes are cargo unit tests (not pytest), correctly tracked by GH issue rather than pytest xfail. Neither
masks a feature bug — they are in code vnc-048 does not modify.

## Accepted Non-Blocking Gap

**AC-08** (Med, non-gate): read-only export under WAL has no dedicated test. Genuinely non-gate per the Risk
Strategy (gate non-negotiables are AC-09 and AC-12 only). Does not hide a blocking risk — R-03's blocking
scenarios are both proven. Recommend adding or explicitly descoping at retro.

## Verdict

Both gate non-negotiables PASS with genuine, code-verified tests driving the operator's real entry points.
All Critical/High/Med risks mitigated. Architecture and Specification honored. The one uncovered AC (AC-08)
is Med-weight, non-gate, and hides no blocking risk. Stewardship compliant. Integration smoke green with no
test removal. **PASS.**
