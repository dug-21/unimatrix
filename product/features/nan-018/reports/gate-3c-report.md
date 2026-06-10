# Gate 3c Report: nan-018

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-09
> Result: PASS
> Validator agent: nan-018-gate-3c

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof | PASS | R-01…R-18 each map to ≥1 passing test; RISK-COVERAGE-REPORT maps results to risks |
| 2. Test coverage completeness | PASS | All Phase-2 risk-to-scenario mappings exercised; integration backstop run; no risk uncovered |
| 3. Specification compliance | PASS | AC-01…AC-14 verified; Wave-1 FRs/NFRs implemented and tested; Wave-2 doc gates flagged as such |
| 4. Architecture compliance | PASS | Two penalty sites threaded (search.rs:727/:729), engine-const source-of-truth, ADR-002 ordered manifest, eval-in-server boundary held |
| 5. Knowledge stewardship | PASS | Tester report carries `## Knowledge Stewardship` with Queried + Stored("nothing novel -- {reason}") |
| INT. Integration smoke (23/23) | PASS | Smoke gate green; protocol/lifecycle/tools backstop 266 passed, 0 failed; no MCP surface added |
| INT. xfail hygiene | PASS | 6 reported xfails all pre-existing with documented reasons (GH#406/#405/#111, sandbox tick/ONNX); none added by nan-018 |
| INT. No tests deleted | PASS | `git diff origin/main...feature/nan-018 -- suites/` empty — zero integration changes |
| Red unit test pre-existence | PASS | `http/token.rs` diff vs main EMPTY; `test_concurrent_creation_no_corruption` genuinely pre-existing/flaky, not masking a feature bug |
| Backstop R-09 corpus audit | PASS | `test_primary_corpus_audit_zero_literal_id_zero_null` scans shipped fixtures, asserts zero literal-id / zero null, ≥4 scenarios |
| Backstop R-04 hash-sensitivity matrix | PASS | Each declared manifest input moves the hash; display-only insensitivity + migration-number-not-hashed negative halves present |
| Backstop R-15 non-vacuous AC-14 | PASS | All 5 conditions assert against NON-EMPTY result sets; cond.1 requires rank_below(A,B) with BOTH anchors present |
| R-04 named-human completeness gate | FLAGGED (not closed) | Correctly surfaced as OUTSTANDING delivery obligation owned by leader/human — as required |

## Detailed Findings

### Check 1 — Risk Mitigation Proof
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md §Coverage Summary maps every R-01…R-18 to named passing tests. Independently confirmed the existence and PASS status of the load-bearing tests: ran `cargo test -p unimatrix-server --lib eval::` → **274 passed, 0 failed**; `cargo test -p unimatrix-engine --lib graph_penalty` → **31 passed, 0 failed**. Spot-checked named tests resolve to real test bodies (`test_drift_guard_fires_on_mismatch_primary_aborts`, `test_or_composition_trust_holds_mrr_regresses_flagged`, `test_cost_empty_set_is_zero`, `test_eval_report_exit_code_unchanged_with_trust_regression`). No risk lacks a passing automated test within what tests can prove.

### Check 2 — Test Coverage Completeness
**Status**: PASS
**Evidence**: The Critical risks (R-01 bit-for-bit, R-03 hash determinism) carry their full scenario sets (default-equivalence + enumerated-site + empty-TOML; N≥100 + permuted + cross-process). High risks carry triangulation/sensitivity-matrix/truth-table coverage. Integration backstop (protocol+lifecycle+tools) run as the MCP face of AC-01 bit-for-bit — 266 passed, 0 failed. The two declared residuals (R-04 human column-manifest review, NFR-08 Band-2 doc-review) are by-design non-test items, correctly flagged, not coverage gaps.

### Check 3 — Specification Compliance
**Status**: PASS
**Evidence**: AC-01…AC-14 each verified in RISK-COVERAGE-REPORT §Acceptance Criteria Verification with named test evidence, cross-checked against SPECIFICATION verification table and ACCEPTANCE-MAP. AC-13 hard gate independently re-verified: `git diff --name-only origin/main...feature/nan-018 -- .claude/protocols/` → **EMPTY**. Wave-2 ACs (AC-10/11/12) correctly marked as manual doc-review gates, not blocking Wave-1 exit (NFR-04 wave independence). FR-12a cost-advisory and FR-22 corpus-dependent fail/warn semantics tested (`test_cost_growth_blocks_nothing_advisory_only`, `test_drift_guard_warns_on_mismatch_snapshot_continues`).

### Check 4 — Architecture Compliance
**Status**: PASS
**Evidence**: ADR-001 engine-const-as-source-of-truth honored (`test_graph_penalty_params_default_references_consts`, dual-default triangulation across 7 levers). ADR-002 ordered/versioned manifest tested for determinism and per-input sensitivity. Penalty threading confined to the two LOCKED sites in `services/search.rs`; `background.rs:583` correctly excluded as a log-string non-site (`test_background_rs_not_a_penalty_site`). Eval-in-server boundary (ADR-004) held — all new code under `eval/{corpus,shape,runner,report}`. No architectural drift.

### Check 5 — Knowledge Stewardship
**Status**: PASS
**Evidence**: `agents/nan-018-agent-4-tester-report.md` and RISK-COVERAGE-REPORT both carry a `## Knowledge Stewardship` block. `Queried:` cites `context_briefing` (delivery-process lessons #4202/#3935/#4515/#2656, #3548, ADR-001 #4897). `Stored:` gives an explicit "nothing novel to store -- {reason}" justification (single-feature instances of #4070/#2610/#703/#3548; the "instrument-measures-not-executes" lens deferred to retro as a one-instance observation). Block present with a stated reason — full PASS, not WARN.

### Integration Test Validation
**Status**: PASS
- **Smoke (mandatory gate)**: 23/23 reported PASS; smoke markers present in suites (`product/test/infra-001/suites`).
- **Backstop suites**: protocol+lifecycle (77 passed, 5 xfailed, 2 xpassed, 0 failed) + tools (189 passed, 1 xfailed, 0 failed) = 266 passed, 0 failed. nan-018 adds no MCP tool/parameter/client surface, so these prove default-config retrieval is bit-for-bit unperturbed (the MCP face of AC-01).
- **xfail hygiene**: 11 xfail markers in-suite, all carrying documented pre-existing reasons (GH#406 find_terminal_active, GH#405 confidence timing, GH#111 rate-limit, sandbox tick/ONNX env). The 6 xfails in the run subset are all pre-existing; none reference nan-018. The 2 xpassed are env-dependent tick tests, correctly flagged for marker owners to re-baseline (not a nan-018 obligation).
- **No deletions/comment-outs**: `git diff --name-status origin/main...feature/nan-018 -- suites/` → empty. Zero integration test files touched.
- **RISK-COVERAGE-REPORT integration counts**: present (§Integration Tests table with per-suite passed/xfailed/xpassed/failed).
- **Red unit test**: `http/token.rs` diff vs origin/main is EMPTY (verified). `test_concurrent_creation_no_corruption` is genuinely pre-existing/flaky (passes in isolation per report), file untouched by nan-018, not masking a feature defect.

### Three Non-Negotiable Wave-1 Backstops
**Status**: PASS (all three independently inspected at source + run green)
- **R-09 corpus audit** (`fixtures_tests.rs:95`): scans every shipped fixture scenario, asserts `!has_literal_expected()` AND `!is_null_ground_truth()` AND ≥1 property assertion each, ≥4 scenarios. Genuine static audit, not self-consistent.
- **R-04 hash-sensitivity matrix** (`shape/tests.rs`): every declared manifest input (entry columns add/remove/rename, edge types, each confidence dim, embedding dim, model-id, manifest version) asserted to MOVE the hash; negative halves `test_shape_hash_insensitive_to_display_only_column` + `test_migration_number_not_hashed` present.
- **R-15 non-vacuous AC-14** (`sweep_tests.rs:104`): all 5 conditions assert against NON-EMPTY result sets — cond.1 requires a `rank_below(A,B)` with BOTH anchors resolved and PRESENT (never the vacuous A-absent arm); cond.2 each of 4 shapes ≥1 evaluated assertion; cond.3 observable non-zero final_score delta on a shared deprecated entry (lever proven live); cond.4 baseline reproduces bit-for-bit across two runs AND differs from steep; cond.5 live drift guard `HardAbort` on stamp mismatch. Ran green individually (`test_ac14_correlated_sweep_non_vacuous ... ok`).

### R-04 Named-Human Column-Manifest Completeness Gate
**Status**: FLAGGED (correctly, as a delivery obligation — not a test, not closed)
**Evidence**: RISK-COVERAGE-REPORT §Gaps item 1 and the tester report both surface this as **OUTSTANDING — owned by the leader/human, not closable by the tester** (ARCHITECTURE §7.3 LOCKED). The automated sensitivity matrix proves the hash is sensitive to every *declared* input but cannot prove the *declared set is complete*. The spawn prompt requires confirming this is flagged, not closed — it is correctly flagged. The leader/human must certify before delivery acceptance that the manifest's `entries` column list covers every column the live retrieval/ranking path reads.

## Rework Required

None.

## Outstanding Delivery Obligations (for leader/human — not gate blockers)

1. **R-04 named-human column-manifest completeness certification** (LOCKED delivery gate, ARCHITECTURE §7.3). A named human must certify the declared manifest column set is complete before delivery acceptance. Flagged correctly; remains the human's to close.
2. **NFR-08 Band-2 cost-proxy error-bar doc-review** — Wave-2, deferrable; does not gate the Wave-1 exit.
