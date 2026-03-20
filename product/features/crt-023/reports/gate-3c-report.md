# Gate 3c Report: crt-023

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-20
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | FAIL | R-09 non-negotiable test functions do not exist in codebase despite being claimed PASS |
| Test coverage completeness | WARN | R-01, R-08, R-16, R-19 partial (CI model absent — documented and acceptable) |
| Specification compliance | PASS | 23/25 ACs pass; AC-09 PARTIAL (eval gate, ADR-006 path — pending human sign-off); AC-16 PENDING (CLI download, CI network constraint) |
| Architecture compliance | PASS | All 8 components implemented per spec; ADRs 001-007 followed |
| Knowledge stewardship | PASS | Tester report has Queried + Stored block with reason |

---

## Detailed Findings

### Check 1: Risk Mitigation Proof

**Status**: FAIL

**Evidence**: The RISK-COVERAGE-REPORT.md claims the two non-negotiable R-09 tests pass:

> `test_circuit_breaker_counts_all_edge_types`, `test_circuit_breaker_stops_at_cap` (unit, services/nli_detection) | PASS | Full

An exhaustive filesystem search across `/workspaces/unimatrix/crates/` and `/workspaces/unimatrix/product/` finds zero occurrences of these function names. The actual test functions in `nli_detection.rs` are:

```
fn test_format_nli_metadata_contains_required_keys
fn test_format_nli_metadata_is_valid_json
async fn test_empty_embedding_skips_nli
async fn test_nli_not_ready_exits_immediately
async fn test_bootstrap_promotion_zero_rows_sets_marker
async fn test_maybe_bootstrap_promotion_skips_if_marker_present
async fn test_maybe_bootstrap_promotion_defers_when_nli_not_ready
async fn test_bootstrap_promotion_confirms_above_threshold
async fn test_bootstrap_promotion_refutes_below_threshold
async fn test_bootstrap_promotion_idempotent_second_run_no_duplicates
async fn test_bootstrap_promotion_nli_inference_runs_on_rayon_thread
```

The test plan pseudocode in `product/features/crt-023/test-plan/post-store-detection.md` defined these functions as `test_circuit_breaker_counts_supports_and_contradicts_combined` and `test_circuit_breaker_stops_at_cap_mixed_types`. Neither the pseudocode names nor the report-claimed names were implemented.

**R-09 is rated Critical and is a non-negotiable test** per the Risk Strategy:

> Non-negotiable tests (feature must not ship without them):
> 4. R-09: Cap enforcement across both Supports and Contradicts edge types

The implementation code in `nli_detection.rs` at lines 164-233 is correctly structured — the cap logic at line 165 explicitly comments "Cap counts BOTH Supports AND Contradicts edges combined (not just Contradicts)." The implementation is likely correct, but the R-09 tests that would prove this are absent.

**Issue**: The RISK-COVERAGE-REPORT falsely claims these tests pass. This is a reporting error combined with an implementation gap (tests were not written).

**Fix**: Add the missing circuit breaker unit tests to `nli_detection.rs`. The test plan pseudocode in `post-store-detection.md` provides the complete implementation blueprint.

---

### Check 2: Test Coverage Completeness

**Status**: WARN

All 22 risks are mapped in the RISK-COVERAGE-REPORT. The partial-coverage risks (R-01, R-08, R-16, R-19) are documented as model-absent CI constraints — acceptable per ADR-006 design. Specific observations:

- **R-01 (Critical)**: Pool floor >= 6 verified by `test_pool_floor_raised_when_nli_enabled`. Concurrent ONNX load test requires model — documented. Non-negotiable test #1 (`test_pool_floor_raised_when_nli_enabled` + `test_concurrent_search_stability`) exists and passes.
- **R-03 (Critical)**: `test_nli_sort_stable_identical_scores_preserves_original_order` confirmed present in `search.rs` line 1524. Passes.
- **R-05 (Critical)**: `test_hash_mismatch_transitions_to_failed` confirmed present in `nli_handle.rs` line 745. Passes. Integration security test `test_nli_hash_mismatch_graceful_degradation` confirmed present.
- **R-10 (Critical)**: `test_nli_edges_below_auto_quarantine_threshold_no_quarantine` confirmed in `background.rs` line 2970. Passes.
- **R-13 (Critical)**: `test_mutex_poison_detected_at_get_provider` confirmed in `nli_handle.rs` line 828. Passes.
- **R-09 (Critical)**: See Check 1. Non-negotiable test MISSING. FAIL.

Integration test counts match actual suite files:
- `test_tools.py`: 75 test functions (confirmed by count)
- `test_lifecycle.py`: 28 test functions (confirmed)
- `test_security.py`: 19 test functions (confirmed)
- `test_contradiction.py`: 13 test functions (confirmed)

All xfail markers have corresponding pre-existing GH Issues: GH#305 (tools), GH#291 (lifecycle), GH#111 (edge_cases). No crt-023 issues were filed as xfail, which is correct.

---

### Check 3: Specification Compliance (25 ACs)

**Status**: PASS

23 of 25 ACs have test coverage. The two exceptions are correct per the spawn prompt instructions:

**AC-09 (PARTIAL — known pending action)**:
- 1582 scenarios extracted — AC-22 waiver NOT applicable (non-zero scenario count confirmed in tester report).
- Baseline profile ran: P@K=0.329, MRR=0.449, 0 regressions.
- Candidate profile SKIPPED per ADR-006 (NLI model not cached in CI — correct behavior).
- SKIPPED annotation present in `skipped.json` per eval report.
- Human sign-off with model present required before final deliverability marking. This is documented correctly as a pending action, not a gate defect.

**AC-16 (PENDING)**:
- `unimatrix model-download --nli` requires network access to HuggingFace Hub, not available in CI. Documented as pending manual smoke test in delivery report.
- This is acceptable per the specification's "CLI test (or manual smoke test in delivery report)" wording.

**AC-22**: NOT a waiver scenario. The RISK-COVERAGE-REPORT correctly notes the 1582-scenario count and that the ADR-006 SKIPPED path applies, not the D-01 zero-scenario waiver.

**AC-01 independent verification**: 12 model-independent tests in `cross_encoder_tests.rs` pass (softmax invariant, truncation, trait bounds). 8 model-dependent tests are correctly ignored with `#[ignore = "Requires NliMiniLM2L6H768 model on disk..."]`. The trait bound tests (`test_nli_provider_send_sync`, `test_cross_encoder_provider_object_safe`) pass at compile time.

All 10 NLI config fields confirmed present in `config.rs` with correct `#[serde(default)]` attributes (AC-07 verified).

---

### Check 4: Architecture Compliance

**Status**: PASS

All architecture-specified new files and structures exist:

| Architecture Spec | Implementation | Status |
|---|---|---|
| `unimatrix-embed/src/cross_encoder.rs` | Present | PASS |
| `unimatrix-server/src/infra/nli_handle.rs` | Present | PASS |
| `unimatrix-server/src/services/nli_detection.rs` | Present | PASS |
| `NliModel` enum in `model.rs` | Present | PASS |
| `ensure_nli_model` in `download.rs` | Present (`download.rs` exists) | PASS |
| 10 NLI fields in `InferenceConfig` | All 10 confirmed in `config.rs` | PASS |
| Pool floor = 6 when `nli_enabled=true` | Confirmed in `config.rs` line 196+ | PASS |
| `write_pool_server()` for NLI edge writes (SR-02) | Confirmed at `nli_detection.rs` line 209 | PASS |
| No schema migration | Schema v13 used as-is | PASS |
| `COUNTERS` key `bootstrap_nli_promotion_done` | Confirmed at `nli_detection.rs` line 1229 | PASS |

ADR-001 through ADR-007 implementation was validated in Gate 3b and no architectural drift is observed in the test stage.

No `write_pool_server()` bypass found; NLI edges go direct per SR-02 constraint.

---

### Check 5: Knowledge Stewardship Compliance

**Status**: PASS

Tester agent report (`crt-023-agent-9-tester-report.md`) contains:

```markdown
## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for testing procedures — found entry #840 (harness how-to), #487 (workspace tests), #750 (pipeline validation)
- Stored: nothing novel to store — NLI-absent degradation test pattern is crt-023-specific; will revisit after feature ships to confirm as reusable convention
```

Both `Queried:` and `Stored:` (with reason) entries are present. Stewardship block is complete.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| R-09 non-negotiable tests absent: `test_circuit_breaker_counts_all_edge_types` and `test_circuit_breaker_stops_at_cap` claimed in RISK-COVERAGE-REPORT do not exist in `nli_detection.rs` or anywhere in the codebase | `uni-rust-dev` (re-spawn for `nli_detection.rs`) | Add the two circuit breaker unit tests to `nli_detection.rs`. The test plan pseudocode in `product/features/crt-023/test-plan/post-store-detection.md` lines 102-158 provides the full implementation blueprint. Rename to match the report's claimed names or update the report to use the actual names. Once tests exist, re-run `cargo test` and update the RISK-COVERAGE-REPORT with the verified count. |

---

## Pending Actions (Not Gate-Blocking)

These items are correctly documented as pending and do not constitute gate failures:

1. **AC-09 eval gate (human sign-off required)**: Candidate NLI profile was SKIPPED per ADR-006 (model absent in CI). Baseline metrics captured (P@K=0.329, MRR=0.449). Human review with model present is required before the feature is marked fully deliverable. This is a known pending action per the spawn prompt instructions.

2. **AC-16 CLI smoke test**: `unimatrix model-download --nli` requires network access. Manual smoke test in delivery report satisfies the specification's verification method.

3. **R-01/R-08/R-16/R-19 partial ONNX coverage**: All four risks are correctly documented as partial due to CI model absence. The degradation paths (NLI absent, fire-and-forget decoupled) are verified by integration tests. Full ONNX inference coverage requires the model cached.

---

## Knowledge Stewardship

- Queried: context_search for "validation gate 3c non-negotiable test missing coverage report discrepancy" — no prior stored patterns found; this is a novel instance.
- Stored: nothing novel to store — the R-09 gap (test functions claimed PASS in report but absent in code) is a crt-023-specific tester reporting error. If this pattern recurs (report claiming test names that differ from pseudocode plan names), it may warrant a lesson-learned entry after the rework confirms the root cause.
