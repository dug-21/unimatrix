# Acceptance Map: col-015

## AC to Implementation Tracing

| AC | Description | Implementation | Test IDs | Wave |
|----|-------------|---------------|----------|------|
| AC-01 | test_scenarios module with 3+ scenario builders, 3+ assertion helpers | `unimatrix-engine/src/test_scenarios.rs` | T-KT-01..05, T-PROF-01..02 | 1 |
| AC-02 | Confidence calibration tests, 8+ ranking assertions | `unimatrix-engine/tests/pipeline_calibration.rs` | T-CAL-01..05 | 2 |
| AC-03 | Signal ablation with Kendall tau, 6 signals | `unimatrix-engine/tests/pipeline_calibration.rs` | T-ABL-01..06 | 2 |
| AC-04 | Extraction pipeline tests, 5+ scenarios | `unimatrix-observe/tests/extraction_pipeline.rs` | T-EXT-01..04 | 3 |
| AC-05 | Neural enhancer shadow/active mode tests | `unimatrix-observe/tests/extraction_pipeline.rs` | T-EXT-05..06 | 3 |
| AC-06 | Retrieval quality tests (re-rank, penalties, boosts) | `unimatrix-engine/tests/pipeline_retrieval.rs` | T-RET-01..05 | 2 |
| AC-07 | Supersession injection in full pipeline | `unimatrix-server/tests/pipeline_e2e.rs` | T-E2E-02 | 4 |
| AC-08 | Co-access boost in full pipeline | `unimatrix-server/tests/pipeline_e2e.rs` | T-E2E-04 | 4 |
| AC-09 | Provenance boost in full pipeline | `unimatrix-server/tests/pipeline_e2e.rs` | T-E2E-03 | 4 |
| AC-10 | Documented scenario fixtures | Module docs in `test_scenarios.rs` | -- | 1 |
| AC-11 | Server-level SearchService tests with real ONNX | `unimatrix-server/tests/pipeline_e2e.rs` | T-E2E-01..05 | 4 |
| AC-12 | Formal Kendall tau in ablation tests | `test_scenarios::kendall_tau()` | T-ABL-01..06 | 1, 2 |
| AC-13 | Usage guide in module docs | `test_scenarios.rs` module-level docs | -- | 1 |
| AC-14 | Unimatrix procedures stored | `/store-procedure` calls | -- | 5 |

## Verification Checklist

- [ ] `cargo test -p unimatrix-engine --features test-support` passes
- [ ] `cargo test -p unimatrix-observe` passes (includes extraction_pipeline)
- [ ] `cargo test -p unimatrix-server --features test-support` passes (or skips without ONNX)
- [ ] `cargo test --workspace` passes
- [ ] Total test execution time < 60 seconds
- [ ] Module-level docs present in test_scenarios.rs
- [ ] At least 3 procedures stored in Unimatrix
- [ ] No production code changes (only test files and feature-gated modules)
