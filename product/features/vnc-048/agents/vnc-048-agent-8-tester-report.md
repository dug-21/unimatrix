# vnc-048 Agent 8 — Tester (Stage 3c Test Execution)

## Deliverable
`product/features/vnc-048/testing/RISK-COVERAGE-REPORT.md` — full R-01..R-14 mapping, unit +
integration counts, AC-01..13 verification, gate non-negotiable verdicts.
GH #953 comment posted with the summary.

## Gate Non-Negotiables
- **AC-09 / R-01 S1 (disagreement seam): PASS — genuine.** `test_export_slug_emits_slug_store_not_hash_store`
  seeds set A via runtime literal-slug layout + disjoint non-empty B via path-hash store (distinct
  code), drives `run_export_with_base(slug=Some)`, asserts `emitted==A ∧ ∩B==∅`. Divergence guard
  confirms. Not ceremonial.
- **AC-12 / R-03 S2 (served-vector-from-`start`): FAIL — NOT COVERED.** No `register→stop→import→start`
  served-query test exists; only the AC-02 disk-state proxy, which the Risk Strategy declares
  insufficient for SR-10. Blocking Gate 3c gap → rework by import developer.

## Gates run
- Rust unit `-p unimatrix-server --lib`: 4562 pass / 1 pre-existing flake (#790). slug_store units 12/12.
- Rust integration: export_integration 21/21, import_integration 26/26 (7 new slug tests pass).
- infra-001 pytest smoke: 35 passed, 0 failed (non-regression gate — tool surface unchanged).
- clippy `-p unimatrix-server` + `--workspace`: clean (#935 did not surface).
- Link smoke (#878): PASS. `cargo build --release`: clean.

## Triage
- `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` — pre-existing flake → GH #790 (existing).
- `unimatrix-vector index::tests::test_self_search_50_entries` — parallel-load ANN flake → GH #958 (filed).
- Both are cargo unit tests (not infra-001 pytest) → tracked by GH issue, not xfail. No tests deleted/commented.

## Gaps
1. AC-12/R-03 S2 served-vector-from-`start` — uncovered (blocking gate).
2. AC-08 (Med, non-gate) read-only export under WAL — no dedicated test.

## Verdict
AC-09 gate PASS; AC-12 gate FAIL → Stage 3c REWORKABLE.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing (vnc-048 Stage 3c execution) — surfaced #4781 (3c
  pre-existing-failure triage), #2758 (grep non-negotiable test names before PASS), #4202/#4473/#3806
  (named-test-never-implemented Gate 3b/3c family). Applied: #2758 + #4202 family drove the AC-12
  verification that caught the missing served-vector test behind a disk-state proxy.
- Stored: nothing novel — disk-state-proxy-vs-assembled-path (#917/#918/#930) and
  named-test-never-implemented (#4202) already exist; AC-12 gap is a textbook instance. Flakes are
  code defects → GH issues (#790, #958), per "bugs are GH issues, not lessons."
