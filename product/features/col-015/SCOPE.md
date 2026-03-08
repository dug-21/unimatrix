# col-015: Intelligence Pipeline End-to-End Validation

## Problem Statement

Unimatrix has built a sophisticated self-learning intelligence pipeline across 6+ crates: observation recording (unimatrix-observe), neural extraction (unimatrix-learn), adaptive embeddings (unimatrix-adapt), confidence scoring and co-access boosting (unimatrix-engine), storage (unimatrix-store), and retrieval with re-ranking (unimatrix-server). Each component has unit tests validating its individual behavior, but there is no cross-cutting test infrastructure that validates the pipeline end-to-end.

This creates five concrete gaps:

1. **No confidence calibration testing.** The 6-factor confidence formula (W_BASE through W_TRUST, sum=0.92) has unit tests for each component function, but no test validates that the composite formula produces correct _relative rankings_ across realistic entry populations. If weights are changed, nothing verifies the ranking impact.

2. **No extraction quality validation.** The col-013 extraction pipeline (5 rules + 6-check quality gate) and crt-007/008 neural enhancement (SignalClassifier + ConventionScorer) auto-store entries with `trust_source: "auto"` or `"neural"`. No test validates that these auto-stored entries are actually useful — i.e., that they rank appropriately in retrieval and don't pollute the knowledge base.

3. **No regression detection.** Changes to confidence weights, extraction thresholds, retrieval penalties, or neural model parameters could degrade pipeline quality. No test catches regressions because no test measures end-to-end behavior.

4. **No retrieval quality measurement.** The search pipeline (embed → HNSW → quarantine filter → status penalty → supersession injection → re-rank → co-access boost → floors) is complex. No test validates that for a given knowledge base state, the right entries surface at the top for given queries.

5. **No signal effectiveness measurement.** Six confidence signals (base, usage, freshness, helpfulness, correction, trust) plus co-access boost and provenance boost all contribute to ranking. No test quantifies which signals actually improve retrieval quality vs. add noise.

This is Wave 4 of Intelligence Sharpening — all predecessor features (crt-011, vnc-010, col-014, crt-012, nxs-009, crt-013) must be complete, ensuring the pipeline is in a known-good state before validation infrastructure is built.

## Goals

1. **Build a deterministic scenario-based test framework** that exercises the full intelligence pipeline from observation → extraction → quality gate → stored entries → retrieval → ranking, with expected outcomes defined per scenario.

2. **Enable confidence calibration testing** — validate that the 6-factor formula produces correct relative rankings for known entry populations, with weight sensitivity analysis.

3. **Enable extraction quality validation** — validate that auto-extracted entries (trust_source: "auto"/"neural") pass quality gates and rank appropriately when mixed with human-authored entries.

4. **Support regression detection** — any change to confidence weights, extraction thresholds, neural model parameters, or retrieval logic is caught by deterministic test scenarios that assert specific ranking properties.

5. **Provide signal effectiveness metrics** — measure each confidence signal's contribution to retrieval quality via ablation-style tests (toggle signals on/off, measure ranking impact).

## Non-Goals

- **No changes to the confidence formula.** This feature validates the formula, not changes it. Weight tuning is a follow-up after validation reveals what needs adjustment.
- **No changes to extraction rules.** Extraction rules are tested as-is. If validation reveals poor extraction quality, rule changes are a separate feature.
- **No changes to neural model architecture.** SignalClassifier and ConventionScorer are tested as-is.
- **No production runtime changes.** This is pure test infrastructure. No new MCP tools, no schema changes, no server behavior changes.
- **No UI or reporting dashboard.** Test results are Rust test output, not a visualization layer.
- **No benchmark harness or latency testing.** #70 (latency benchmarks) is a separate concern in Platform Hardening.
- **No fuzzing or property-based testing.** Focused on deterministic scenario-based validation, not random input generation.
- **No live/production validation.** Tests use synthetic scenarios with known expected outcomes, not production data.

## Background Research

### Existing Test Infrastructure

- **unimatrix-store**: `TestDb` (temp dir + fresh DB), `TestEntry` builder, `seed_entries()`, `assert_index_consistent()`. Available via `test-support` feature flag. 2 integration test files (sqlite_parity, sqlite_parity_specialized).
- **unimatrix-engine**: Extensive unit tests for each confidence component (45+ tests in confidence.rs, tests in coaccess.rs). No integration tests.
- **unimatrix-observe**: Unit tests in every extraction rule module and detection module. No integration tests exercising the full extraction→quality_gate pipeline with a real store.
- **unimatrix-learn**: 1 integration test (retraining_e2e.rs) covering feedback→label→reservoir→retrain→shadow. Good pattern to follow.
- **unimatrix-server**: No integration test directory. Service tests are inline unit tests. SearchService, BriefingService, ConfidenceService have no cross-service integration tests.

### Intelligence Pipeline Architecture (Current State)

The full pipeline has 7 stages across 6 crates:

```
[1] Observation Recording (unimatrix-observe via unimatrix-server hooks)
    → Tool calls, session events recorded to SQLite observations/sessions tables
[2] Extraction (unimatrix-observe::extraction)
    → 5 rules (knowledge-gap, implicit-convention, dead-knowledge, recurring-friction, file-dependency)
    → ProposedEntry with extraction_confidence, source_features, source_rule
[3] Neural Enhancement (unimatrix-observe::extraction::neural + unimatrix-learn::models)
    → SignalClassifier (signal vs noise) + ConventionScorer (convention quality)
    → Shadow mode (log only) or Active mode (suppress noise, override confidence)
[4] Quality Gate (unimatrix-observe::extraction::quality_gate)
    → 6 checks: rate_limit, content_validation, cross_feature, confidence_floor,
      near_duplicate (server-level), contradiction (server-level)
[5] Storage (unimatrix-store)
    → Entry stored with trust_source "auto" or "neural", status Proposed
    → Embedding computed, vector indexed
[6] Confidence Evolution (unimatrix-engine::confidence)
    → 6-factor composite: base(0.18) + usage(0.14) + freshness(0.18) +
      helpfulness(0.14) + correction(0.14) + trust(0.14) = 0.92
    → Recomputed by ConfidenceService on access/vote events
[7] Retrieval & Re-ranking (unimatrix-server::services::search)
    → embed → HNSW(ef=32) → quarantine filter → status penalty(0.7/0.5) →
      supersession injection → rerank(0.85*sim + 0.15*conf) →
      co-access boost(max 0.03) → provenance boost(0.02 lesson-learned) → floors
```

### Key Constants for Calibration Testing

| Constant | Value | Location |
|----------|-------|----------|
| W_BASE | 0.18 | unimatrix-engine/src/confidence.rs |
| W_USAGE | 0.14 | unimatrix-engine/src/confidence.rs |
| W_FRESH | 0.18 | unimatrix-engine/src/confidence.rs |
| W_HELP | 0.14 | unimatrix-engine/src/confidence.rs |
| W_CORR | 0.14 | unimatrix-engine/src/confidence.rs |
| W_TRUST | 0.14 | unimatrix-engine/src/confidence.rs |
| SEARCH_SIMILARITY_WEIGHT | 0.85 | unimatrix-engine/src/confidence.rs |
| MAX_CO_ACCESS_BOOST | 0.03 | unimatrix-engine/src/coaccess.rs |
| PROVENANCE_BOOST | 0.02 | unimatrix-engine/src/confidence.rs |
| DEPRECATED_PENALTY | 0.7 | unimatrix-engine/src/confidence.rs |
| SUPERSEDED_PENALTY | 0.5 | unimatrix-engine/src/confidence.rs |
| Extraction confidence floor | 0.2 | unimatrix-observe/src/extraction/mod.rs |
| Extraction rate limit | 10/hour | unimatrix-observe/src/extraction/mod.rs |
| Min features per rule | 2-5 | unimatrix-observe/src/extraction/mod.rs |
| trust_score("human") | 1.0 | unimatrix-engine/src/confidence.rs |
| trust_score("auto") | 0.35 | unimatrix-engine/src/confidence.rs |
| trust_score("neural") | 0.40 | unimatrix-engine/src/confidence.rs |

### Crate Dependency Structure

```
unimatrix-server depends on ALL crates (integration point)
unimatrix-engine depends on unimatrix-core, unimatrix-store
unimatrix-observe depends on unimatrix-core, unimatrix-store, unimatrix-learn
unimatrix-adapt depends on (standalone ML, no store dependency)
unimatrix-learn depends on (standalone ML, no store dependency)
```

### Test Patterns from Existing Code

- `unimatrix-learn/tests/retraining_e2e.rs`: Uses `tempfile::TempDir`, constructs service with config overrides, feeds synthetic signals, waits for async training, asserts model state changes. Good template for pipeline e2e tests.
- `unimatrix-engine::confidence::tests::make_test_entry()`: Constructs `EntryRecord` with explicit field values. Useful for confidence calibration scenarios.
- `unimatrix-store::test_helpers::TestDb`: Provides temp store for integration tests. All pipeline tests will need this.

## Proposed Approach

### Architecture: Scenario-Based Test Framework

Create a `tests/` directory in `unimatrix-engine` (the natural home for cross-cutting intelligence logic) with integration tests that compose components from multiple crates. The framework has three layers:

**Layer 1: Scenario Fixtures** — Deterministic functions that construct known knowledge base states (entries with specific confidence profiles, co-access patterns, trust sources). These are the "given" in given-when-then tests. Reusable across all test categories.

**Layer 2: Pipeline Exercisers** — Functions that exercise specific pipeline stages or combinations: confidence computation for an entry population, extraction+quality_gate for synthetic observations, retrieval+ranking for a seeded knowledge base. These are the "when".

**Layer 3: Ranking Assertions** — Custom assertion helpers that check ranking properties: "entry A ranks above entry B", "top-K results contain entries X,Y,Z", "signal S increases score by at least D". These are the "then".

### Test Categories

1. **Confidence Calibration Tests** (unimatrix-engine integration tests)
   - Seed a population of entries with varying signal profiles
   - Assert relative ranking properties (e.g., "high-usage human entry ranks above low-usage auto entry")
   - Weight sensitivity: verify that each signal contributes meaningful differentiation
   - Boundary tests: entries at confidence extremes, all-zero signals, all-max signals

2. **Extraction Pipeline Tests** (unimatrix-observe integration tests)
   - Seed a store with entries and synthetic observations
   - Run extraction rules → quality gate → assert proposals pass/fail correctly
   - Neural enhancement: run SignalClassifier + ConventionScorer on proposals, verify shadow/active mode behavior
   - Cross-feature validation: verify minimum-features thresholds work across rules

3. **Retrieval Quality Tests** (unimatrix-engine integration tests)
   - Seed a store with entries of known content, embeddings, and confidence
   - Verify re-ranking produces correct ordering for semantic similarity + confidence blend
   - Verify status penalties push deprecated/superseded entries down
   - Verify co-access boost lifts co-accessed entries
   - Verify provenance boost lifts lesson-learned entries

4. **Signal Ablation Tests** (unimatrix-engine integration tests)
   - For each confidence signal: compute rankings with signal at 0 vs. at max
   - Measure ranking distance (Kendall tau or simple position delta)
   - Assert each signal produces measurable ranking change

5. **Full Pipeline Regression Tests** (unimatrix-engine integration tests)
   - End-to-end scenario: create entries → record usage → compute confidence → search → verify ranking
   - Captures known-good rankings as golden test expectations
   - Any formula/threshold/weight change that shifts rankings will fail these tests (intentional friction)

### Crate Placement

- Primary test code: `crates/unimatrix-engine/tests/` (integration tests)
  - `pipeline_calibration.rs` — confidence calibration and signal ablation
  - `pipeline_retrieval.rs` — retrieval quality and ranking assertions
  - `pipeline_regression.rs` — golden ranking regression tests
- Secondary test code: `crates/unimatrix-observe/tests/` (integration tests)
  - `extraction_pipeline.rs` — extraction + quality gate + neural enhancement
- Server-level test code: `crates/unimatrix-server/tests/` (integration tests)
  - `pipeline_e2e.rs` — full SearchService pipeline with real ONNX embeddings
- Shared fixtures: `crates/unimatrix-engine/src/test_scenarios.rs` (behind `test-support` feature flag)
  - Scenario builders, ranking assertions (including Kendall tau), deterministic entry populations
  - Module-level doc comments serving as usage guide

### Server-Level Integration Tests

Full SearchService pipeline tests are in scope. Server-level integration tests in `crates/unimatrix-server/tests/` exercise the complete pipeline (embed -> HNSW -> filter -> re-rank -> boost) using real ONNX embeddings. These complement the pure-function tests in unimatrix-engine by validating the actual application behavior end-to-end, including async execution, embedding adaptation, and service layer interactions.

## Acceptance Criteria

- AC-01: A `test_scenarios` module exists in unimatrix-engine (feature-gated `test-support`) providing at least 3 deterministic scenario builders that create entry populations with known confidence profiles, and at least 3 ranking assertion helpers.
- AC-02: Confidence calibration integration tests exist in `crates/unimatrix-engine/tests/pipeline_calibration.rs` covering at least 8 ranking property assertions (e.g., "human-authored high-usage entry ranks above auto-extracted low-usage entry").
- AC-03: Signal ablation tests exist that toggle each of the 6 confidence signals independently and assert measurable ranking impact, confirming no signal is dead weight.
- AC-04: Extraction pipeline integration tests exist in `crates/unimatrix-observe/tests/extraction_pipeline.rs` that seed a store with synthetic observations, run extraction rules through the quality gate, and assert correct accept/reject behavior for at least 5 scenarios.
- AC-05: Neural enhancement integration tests exist that run SignalClassifier and ConventionScorer on extracted proposals and verify shadow vs. active mode produces expected behavior differences.
- AC-06: Retrieval quality integration tests exist in `crates/unimatrix-engine/tests/pipeline_retrieval.rs` that verify re-ranking (similarity + confidence blend), status penalties (deprecated/superseded ranking below active), and co-access/provenance boosts produce correct relative ordering.
- AC-07: Golden regression tests exist in `crates/unimatrix-engine/tests/pipeline_regression.rs` with at least 3 scenarios that capture expected rankings for specific entry populations. Changing any confidence constant breaks at least one regression test.
- AC-08: All new test infrastructure extends existing patterns — uses `TestDb`, `TestEntry`, `test-support` feature flags. No isolated test scaffolding.
- AC-09: All new tests pass in CI (`cargo test --workspace`).
- AC-10: Test scenario fixtures are documented with comments explaining what each scenario validates and why the expected ranking is correct.
- AC-11: Server-level integration tests exist in `crates/unimatrix-server/tests/` that exercise the full SearchService pipeline with real ONNX embeddings, validating end-to-end retrieval behavior.
- AC-12: Signal ablation tests use formal rank correlation (Kendall tau) to measure each signal's impact, with descriptive failure messages.
- AC-13: A usage guide exists (as module-level doc comments in test_scenarios) documenting when to add new scenarios, how to interpret results, and what to do when tests fail after changes.
- AC-14: Procedures stored in Unimatrix describing when pipeline validation should be invoked (after weight changes, extraction rule changes, new signals).

## Constraints

1. **Crate boundaries.** unimatrix-engine cannot depend on unimatrix-observe or unimatrix-learn (no circular deps). Extraction pipeline tests must live in unimatrix-observe's integration tests. Server-level integration tests that exercise the full SearchService pipeline live in unimatrix-server's integration tests.
2. **ONNX model in server tests.** Server-level integration tests require the all-MiniLM-L6-v2 ONNX model. Tests should handle model absence gracefully (skip with descriptive message). Pure-function tests in unimatrix-engine use synthetic embeddings and do not require the model.
3. **Determinism.** All test scenarios must be fully deterministic. No reliance on wall-clock time for freshness scoring (inject `now` parameter). No random seeds without explicit configuration.
4. **Test infrastructure is cumulative.** Per CLAUDE.md rules, extend `TestDb`, `TestEntry`, and existing `test-support` features. No parallel test scaffolding.
5. **Predecessor dependency.** This feature assumes crt-011 (session count fix), crt-013 (co-access cleanup, status penalty validation), and nxs-009 (observation metrics normalization) are complete. Test scenarios should use the post-fix behavior.
6. **No production code changes.** This feature adds only test code and test-support infrastructure. No changes to production logic, schemas, or tool behavior.

## Resolved Questions

1. **Embedding in tests** (RESOLVED): Blend approach. Synthetic embeddings for the multitude of calibration/scenario tests (fast, deterministic). Real ONNX embeddings for a subset of server-level integration tests validating production-like behavior. Both are in scope.
2. **Ranking metric** (RESOLVED): Formal rank correlation (Kendall tau or equivalent). More rigorous is preferred over simple position-delta assertions. Test failure messages should explain the correlation breakdown.
3. **Test data volume**: 10-20 entries per scenario (enough for ranking differentiation, fast enough for CI).
4. **SearchService testing** (RESOLVED): Full SearchService pipeline is in scope. Add server-level integration tests in `crates/unimatrix-server/tests/` that exercise the complete SearchService pipeline (embed -> HNSW -> filter -> re-rank -> boost), not just pure function composition. Tests should exercise the real application as closely as possible.
5. **Co-access scenario construction**: Co-access pairs require `Store::record_co_access()`. Existing test-support helpers may need extension.

## Additional Requirements

1. **Usage guide**: Document when and how to use this test infrastructure -- when to add new scenarios, how to interpret results, what to do when a test fails after a weight change. Lives as a doc comment block in the test_scenarios module.
2. **Unimatrix procedures**: Store procedures in Unimatrix for when pipeline validation testing should be invoked (e.g., after weight changes, after extraction rule changes, after new signals are added).

## Tracking

Feature: col-015
Phase: Collective
Milestone: Intelligence Sharpening (Wave 4)
Dependencies: crt-011, vnc-010, col-014, crt-012, nxs-009, crt-013
GH Issue: TBD (created during Session 1 synthesis)
