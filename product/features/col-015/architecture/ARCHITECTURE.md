# Architecture: col-015 Intelligence Pipeline End-to-End Validation

## Overview

col-015 introduces cross-cutting test infrastructure that validates the full intelligence pipeline. The architecture addresses three challenges: (1) composing tests across crate boundaries without circular dependencies, (2) providing full SearchService-level testing despite `pub(crate)` visibility, and (3) maintaining determinism across async and embedding-dependent code paths.

## Architecture Decisions

### ADR-001: Test Infrastructure Placement — Three-Crate Distribution

**Context**: The intelligence pipeline spans 6 crates. No single crate can host all tests without creating circular dependencies (e.g., unimatrix-engine cannot depend on unimatrix-observe).

**Decision**: Distribute tests across three crates based on what they validate:

| Crate | Test Files | What They Validate |
|-------|-----------|-------------------|
| `unimatrix-engine` | `tests/pipeline_calibration.rs`, `tests/pipeline_regression.rs` | Confidence formula, signal ablation, golden regressions (pure functions) |
| `unimatrix-observe` | `tests/extraction_pipeline.rs` | Extraction rules + quality gate + neural enhancement (store-backed) |
| `unimatrix-server` | `tests/pipeline_e2e.rs` | Full SearchService pipeline with real embeddings (async, ONNX) |

Shared fixtures live in `unimatrix-engine/src/test_scenarios.rs` behind `test-support` feature flag.

**Rationale**: Follows the existing crate dependency graph. unimatrix-engine is the natural home for confidence/ranking logic. unimatrix-observe already depends on unimatrix-store for extraction. unimatrix-server depends on everything, so it hosts the full pipeline tests.

**Risk addressed**: SR-06 (distributed test infrastructure). Mitigated by the shared fixtures module providing a single source of truth.

### ADR-002: SearchService Test Access — ServiceLayer pub + test-support Feature

**Context**: `SearchService`, `ServiceSearchParams`, `ScoredEntry`, `AuditContext`, `CallerId`, and `RetrievalMode` are all `pub(crate)` in unimatrix-server. Integration tests in `crates/unimatrix-server/tests/` cannot access `pub(crate)` items.

**Decision**: Add a `test-support` feature to unimatrix-server that re-exports the necessary types and provides a `TestServiceLayer` builder:

1. Add `#[cfg(any(test, feature = "test-support"))]` module `test_support` in `unimatrix-server/src/lib.rs`
2. This module re-exports: `ServiceLayer`, `SearchService`, `ServiceSearchParams`, `ScoredEntry`, `RetrievalMode`, `AuditContext`, `AuditSource`, `CallerId`, `SecurityGateway`
3. Provide a `TestServiceLayer::new(store_path: &Path) -> ServiceLayer` constructor that wires up all services with default configs, in-memory HNSW, and real ONNX embeddings

**Rationale**: This follows the existing pattern (unimatrix-store's `test-support` feature). The alternative (making types `pub`) would expose internal APIs to production consumers. The `test-support` feature flag keeps the API surface clean while enabling full pipeline testing.

**Risk addressed**: SR-03 (SearchService constructor complexity). The `TestServiceLayer` builder encapsulates the 8-argument constructor.

### ADR-003: Ranking Assertions — Kendall Tau + Pairwise Helpers

**Context**: Signal ablation tests need formal rank correlation metrics. Direct position comparison is fragile (tied scores produce unstable orderings).

**Decision**: Implement Kendall tau rank correlation as a pure function in `test_scenarios.rs`:

```
kendall_tau(ranking_a: &[u64], ranking_b: &[u64]) -> f64
```

Returns a value in [-1.0, 1.0] where 1.0 = identical ordering, -1.0 = reversed, 0.0 = uncorrelated. Additionally provide pairwise assertion helpers:

```
assert_ranked_above(results: &[(u64, f64)], higher_id: u64, lower_id: u64)
assert_in_top_k(results: &[(u64, f64)], entry_id: u64, k: usize)
assert_tau_above(ranking_a: &[u64], ranking_b: &[u64], min_tau: f64)
```

**Rationale**: Kendall tau is the standard non-parametric rank correlation measure. It handles ties gracefully and has clear interpretation. The pairwise helpers provide simpler assertions for individual ranking properties.

### ADR-004: Deterministic Timestamps — Injected `now` Parameter

**Context**: `compute_confidence()` accepts a `now: u64` parameter. `freshness_score()` depends on `now`. Test determinism requires fixed timestamps.

**Decision**: All test scenarios define a canonical `NOW` constant (e.g., `1_700_000_000u64` -- approximately 2023-11-14). Entry timestamps are expressed as offsets from `NOW` (e.g., `created_at: NOW - 3600` for "created 1 hour ago"). No test ever calls `SystemTime::now()`.

For server-level tests that go through `SearchService::search()`, the co-access staleness computation inside the service uses real time. This is acceptable because co-access tests seed pairs with timestamps that are either clearly stale or clearly fresh relative to any reasonable wall clock.

**Risk addressed**: AC-10 (determinism).

### ADR-005: ONNX Model Handling — Skip on Absence

**Context**: Server-level tests require the all-MiniLM-L6-v2 ONNX model. Not all environments have it.

**Decision**: Server-level tests detect model absence at the start of each test function and skip with a descriptive message:

```rust
fn skip_if_no_model() -> bool {
    // Check standard model paths
    // If absent, eprintln!("ONNX model not found, skipping pipeline_e2e test");
    // return true (should skip)
}
```

Tests use `if skip_if_no_model() { return; }` rather than `#[ignore]` so they run automatically when the model is present.

Pure-function tests (unimatrix-engine, unimatrix-observe) never need the ONNX model.

**Risk addressed**: SR-01 (ONNX model availability).

### ADR-006: Scenario Data Format — Builder Structs

**Context**: Test scenarios need to be extensible (AC-11) and self-documenting (AC-10).

**Decision**: Define scenario types as builder structs:

```rust
pub struct CalibrationScenario {
    pub name: &'static str,
    pub description: &'static str,
    pub entries: Vec<EntryProfile>,
    pub now: u64,
    pub expected_ordering: Vec<usize>,  // indices into entries, best-to-worst
}

pub struct EntryProfile {
    pub label: &'static str,
    pub status: Status,
    pub access_count: u32,
    pub last_accessed_at: u64,
    pub created_at: u64,
    pub helpful_count: u32,
    pub unhelpful_count: u32,
    pub correction_count: u32,
    pub trust_source: &'static str,
}
```

Adding a new scenario means adding a new function that returns a `CalibrationScenario`. The test runner iterates all scenarios generically.

**Rationale**: Struct-based scenarios are self-documenting, type-checked, and trivially extensible. The `description` field serves as the "why" documentation (AC-10).

## Component Architecture

### Shared Fixtures (`unimatrix-engine/src/test_scenarios.rs`)

```
test_scenarios (feature-gated: test-support)
  |-- EntryProfile         (deterministic entry signal profile)
  |-- CalibrationScenario  (entries + expected confidence ordering)
  |-- RetrievalScenario    (entries + query + expected result ordering)
  |-- RankingMetrics        (Kendall tau, position delta, NDCG)
  |-- Assertion helpers     (assert_ranked_above, assert_in_top_k, assert_tau_above)
  |-- Scenario catalog      (functions returning standard scenarios)
  |-- Module-level docs     (usage guide: when to add scenarios, how to interpret)
```

### Test File Architecture

```
crates/unimatrix-engine/tests/
  pipeline_calibration.rs
    - test_confidence_ordering_standard_profiles()
    - test_weight_sensitivity_per_signal()
    - test_signal_ablation_kendall_tau()
    - test_boundary_entries()

  pipeline_regression.rs
    - test_golden_ranking_active_vs_deprecated()
    - test_golden_ranking_human_vs_auto()
    - test_golden_ranking_fresh_vs_stale()
    - test_weight_change_detection()

crates/unimatrix-observe/tests/
  extraction_pipeline.rs
    - test_knowledge_gap_rule_fires()
    - test_implicit_convention_rule_fires()
    - test_quality_gate_rejects_low_quality()
    - test_quality_gate_accepts_valid()
    - test_neural_enhancer_shadow_mode()
    - test_neural_enhancer_active_mode()

crates/unimatrix-server/tests/
  pipeline_e2e.rs
    - test_search_ranking_active_above_deprecated()
    - test_search_supersession_injection()
    - test_search_co_access_boost()
    - test_search_provenance_boost()
    - test_search_full_pipeline_golden()
```

### Dependency Graph (Test Infrastructure)

```
unimatrix-engine (test-support feature)
  |-- test_scenarios.rs           <-- shared fixtures, Kendall tau, assertions
  |-- tests/pipeline_calibration.rs   <-- uses test_scenarios
  |-- tests/pipeline_regression.rs    <-- uses test_scenarios

unimatrix-observe (dev-dependencies)
  |-- tests/extraction_pipeline.rs    <-- uses unimatrix-store test-support

unimatrix-server (test-support feature, dev-dependencies)
  |-- src/test_support.rs         <-- TestServiceLayer builder
  |-- tests/pipeline_e2e.rs       <-- uses TestServiceLayer + test_scenarios
```

## Integration Surface

### Cargo.toml Changes

**unimatrix-engine/Cargo.toml**:
```toml
[features]
test-support = []

[dev-dependencies]
tempfile = "3"
unimatrix-engine = { path = ".", features = ["test-support"] }
```

**unimatrix-server/Cargo.toml**:
```toml
[features]
default = ["mcp-briefing"]
mcp-briefing = []
test-support = ["unimatrix-store/test-support", "unimatrix-engine/test-support"]

[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["full", "test-util"] }
unimatrix-server = { path = ".", features = ["test-support"] }
```

### No Production Code Changes

All changes are either:
- New test files (`tests/*.rs`)
- New feature-gated modules (`#[cfg(any(test, feature = "test-support"))]`)
- Cargo.toml feature flag additions

No changes to production logic, schemas, MCP tools, or server behavior.
