# Implementation Brief: col-015

## Overview

Build cross-cutting test infrastructure validating the full intelligence pipeline. 4 test files across 3 crates, 1 shared fixtures module, 1 test-support module. ~39 tests. No production code changes.

## Implementation Order

### Wave 1: Shared Fixtures + Kendall Tau (Foundation)

**Crate**: unimatrix-engine
**Files**: `src/test_scenarios.rs`, `Cargo.toml`
**Effort**: Small

1. Add `test-support` feature to `Cargo.toml`
2. Create `src/test_scenarios.rs` behind `#[cfg(any(test, feature = "test-support"))]`
3. Implement types: `EntryProfile`, `CalibrationScenario`, `RetrievalScenario`, `RetrievalEntry`
4. Implement `CANONICAL_NOW` constant (1_700_000_000u64)
5. Implement `profile_to_entry_record()` conversion
6. Implement 5 standard profiles: `expert_human_fresh`, `good_agent_entry`, `auto_extracted_new`, `stale_deprecated`, `quarantined_bad`
7. Implement 3 standard scenarios: `standard_ranking`, `trust_source_ordering`, `freshness_dominance`
8. Implement `kendall_tau()` pure function
9. Implement assertion helpers: `assert_ranked_above`, `assert_in_top_k`, `assert_tau_above`, `assert_confidence_ordering`
10. Write module-level doc comment (usage guide)
11. Add `pub mod test_scenarios;` to `lib.rs` with feature gate

**Verification**: `cargo test -p unimatrix-engine` passes. Unit tests for Kendall tau (T-KT-01 through T-KT-05) pass.

### Wave 2: Calibration + Ablation + Regression Tests

**Crate**: unimatrix-engine
**Files**: `tests/pipeline_calibration.rs`, `tests/pipeline_regression.rs`
**Depends on**: Wave 1
**Effort**: Medium

1. Create `tests/pipeline_calibration.rs`
   - Import test_scenarios via `unimatrix_engine::test_scenarios`
   - Test all 3 standard scenarios (T-CAL-01 through T-CAL-03)
   - Weight sensitivity test (T-CAL-04): manually recompute confidence with perturbed weights
   - Signal ablation tests (T-ABL-01 through T-ABL-06): create contrasting entry pairs per signal
   - Boundary tests (T-CAL-05)

2. Create `tests/pipeline_retrieval.rs`
   - Re-rank blend ordering (T-RET-01)
   - Status penalty ordering (T-RET-02)
   - Provenance boost (T-RET-03)
   - Co-access boost arithmetic (T-RET-04)
   - Combined interaction (T-RET-05)

3. Create `tests/pipeline_regression.rs`
   - Golden confidence values (T-REG-01): hardcode expected confidence for 3 profiles
   - Weight change detection (T-REG-02): assert constant values match
   - Ranking stability (T-REG-03): assert tau = 1.0 against expected ordering

**Verification**: `cargo test -p unimatrix-engine` passes all new tests.

### Wave 3: Extraction Pipeline Tests

**Crate**: unimatrix-observe
**Files**: `tests/extraction_pipeline.rs`
**Depends on**: None (independent of Wave 1/2)
**Effort**: Medium

1. Create `tests/extraction_pipeline.rs`
2. Construct `TestDb` and seed with observations
3. Test rule firing (T-EXT-01): run `default_extraction_rules()` on seeded observations
4. Test quality gate accept/reject paths (T-EXT-02 through T-EXT-04)
5. Test neural enhancer shadow/active modes (T-EXT-05, T-EXT-06)
6. Test cross-rule feature minimums (T-EXT-06 extended)

**Verification**: `cargo test -p unimatrix-observe` passes all new tests.

### Wave 4: Server-Level Pipeline Tests

**Crate**: unimatrix-server
**Files**: `src/test_support.rs`, `tests/pipeline_e2e.rs`, `Cargo.toml`
**Depends on**: Wave 1 (for test_scenarios)
**Effort**: Large (most complex wave)

1. Add `test-support` feature to `Cargo.toml`
2. Create `src/test_support.rs` behind feature gate:
   - Re-export necessary types (ServiceLayer, service types)
   - Implement `TestServiceLayer::new(store_path)` builder
   - Implement `skip_if_no_model()` helper
3. Add `pub mod test_support;` to `lib.rs` with feature gate
4. Create `tests/pipeline_e2e.rs`
   - Active above deprecated (T-E2E-01)
   - Supersession injection (T-E2E-02)
   - Provenance boost (T-E2E-03)
   - Co-access boost (T-E2E-04)
   - Golden regression (T-E2E-05)
   - Model absence handling (T-E2E-skip)
5. All tests use `#[tokio::test]` runtime

**Verification**: `cargo test -p unimatrix-server --features test-support` passes (or skips gracefully without ONNX model).

### Wave 5: Unimatrix Procedures + Final Verification

**Effort**: Small

1. Store procedure in Unimatrix: "Pipeline validation after weight changes"
2. Store procedure in Unimatrix: "Pipeline validation after extraction rule changes"
3. Store procedure in Unimatrix: "Pipeline validation after new signal addition"
4. Run `cargo test --workspace` to verify all tests pass together
5. Verify test execution time < 60 seconds

## Key Implementation Notes

### Kendall Tau Implementation

```
fn kendall_tau(a: &[u64], b: &[u64]) -> f64 {
    // Build position maps for both rankings
    // Count concordant and discordant pairs
    // Return (C - D) / (n * (n-1) / 2)
}
```

O(n^2) is fine for n <= 20 entries per scenario.

### TestServiceLayer Construction

The main challenge is wiring up all 8 dependencies of `ServiceLayer::new()`. Key decisions:
- `Store`: open at provided path
- `VectorIndex`: create empty, populate after inserting entries
- `AsyncVectorStore`/`AsyncEntryStore`: wrap Store/VectorIndex adapters
- `EmbedServiceHandle`: load ONNX model from standard path
- `AdaptationService`: default config (no adaptation for tests)
- `AuditLog`: create with store reference
- `UsageDedup`: create default

The constructor should match production `ServiceLayer::new()` as closely as possible (SR-10/R-06 mitigation).

### Weight Sensitivity Testing

Cannot modify constants at runtime. Instead, manually recompute confidence with adjusted weights:

```rust
fn confidence_with_adjusted_weight(entry: &EntryRecord, now: u64, weight_index: usize, delta: f64) -> f64 {
    let weights = [W_BASE, W_USAGE, W_FRESH, W_HELP, W_CORR, W_TRUST];
    let scores = [base_score(entry.status), usage_score(entry.access_count), ...];
    let mut adjusted = weights.to_vec();
    adjusted[weight_index] *= (1.0 + delta);
    adjusted.iter().zip(scores.iter()).map(|(w, s)| w * s).sum::<f64>().clamp(0.0, 1.0)
}
```

### Observation Seeding for Extraction Tests

Extraction rules need ObservationRecord data. The extraction tests must construct realistic observation patterns:
- Knowledge gap rule: entries accessed but no knowledge stored for the topic
- Implicit convention: same patterns repeated across 3+ feature cycles
- Dead knowledge: entries never accessed in recent observations

This requires understanding each rule's detection logic. Read the rule implementations during Wave 3.

## Files Changed Summary

| File | Change Type | Wave |
|------|------------|------|
| `crates/unimatrix-engine/Cargo.toml` | Modify (add feature) | 1 |
| `crates/unimatrix-engine/src/lib.rs` | Modify (add module) | 1 |
| `crates/unimatrix-engine/src/test_scenarios.rs` | New | 1 |
| `crates/unimatrix-engine/tests/pipeline_calibration.rs` | New | 2 |
| `crates/unimatrix-engine/tests/pipeline_retrieval.rs` | New | 2 |
| `crates/unimatrix-engine/tests/pipeline_regression.rs` | New | 2 |
| `crates/unimatrix-observe/tests/extraction_pipeline.rs` | New | 3 |
| `crates/unimatrix-server/Cargo.toml` | Modify (add feature) | 4 |
| `crates/unimatrix-server/src/lib.rs` | Modify (add module) | 4 |
| `crates/unimatrix-server/src/test_support.rs` | New | 4 |
| `crates/unimatrix-server/tests/pipeline_e2e.rs` | New | 4 |
