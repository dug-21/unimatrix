# Specification: col-015 Intelligence Pipeline End-to-End Validation

## Domain Model

### Core Types

#### EntryProfile

A deterministic description of a knowledge entry's signal state for test scenarios. Maps 1:1 to the fields consumed by `compute_confidence()`.

| Field | Type | Description |
|-------|------|-------------|
| label | `&'static str` | Human-readable label for test output (e.g., "expert-human-fresh") |
| status | `Status` | Entry lifecycle status (Active, Proposed, Deprecated, Quarantined) |
| access_count | `u32` | Number of times accessed |
| last_accessed_at | `u64` | Epoch seconds of last access |
| created_at | `u64` | Epoch seconds of creation |
| helpful_count | `u32` | Helpful vote count |
| unhelpful_count | `u32` | Unhelpful vote count |
| correction_count | `u32` | Number of corrections applied |
| trust_source | `&'static str` | Creator trust level ("human", "system", "agent", "neural", "auto") |
| category | `&'static str` | Entry category (for provenance boost testing) |

#### CalibrationScenario

A complete test scenario for confidence calibration.

| Field | Type | Description |
|-------|------|-------------|
| name | `&'static str` | Test name for output |
| description | `&'static str` | Why this ordering is correct |
| entries | `Vec<EntryProfile>` | Entry population |
| now | `u64` | Fixed timestamp for computation |
| expected_ordering | `Vec<usize>` | Indices into entries, highest confidence first |

#### RetrievalScenario

A test scenario for retrieval ranking validation.

| Field | Type | Description |
|-------|------|-------------|
| name | `&'static str` | Test name |
| description | `&'static str` | Why this ranking is correct |
| entries | `Vec<RetrievalEntry>` | Entries with content and embeddings |
| query | `&'static str` | Search query text |
| expected_top_k | `Vec<usize>` | Expected top-K entry indices |
| pairwise_assertions | `Vec<(usize, usize)>` | (higher, lower) index pairs that must hold |

#### RetrievalEntry

Extends EntryProfile with content and optional embedding for retrieval tests.

| Field | Type | Description |
|-------|------|-------------|
| profile | `EntryProfile` | Confidence signal profile |
| title | `&'static str` | Entry title |
| content | `&'static str` | Entry content (for embedding) |
| embedding | `Option<Vec<f32>>` | Pre-computed embedding (synthetic tests) |
| superseded_by | `Option<usize>` | Index of successor entry (for supersession tests) |

### Ranking Metrics

#### Kendall Tau

Formal rank correlation coefficient. Input: two permutations of the same element set. Output: correlation in [-1.0, 1.0].

```
tau = (concordant_pairs - discordant_pairs) / (n * (n - 1) / 2)
```

Where:
- `concordant_pairs`: pairs (i, j) where both rankings agree on relative order
- `discordant_pairs`: pairs (i, j) where rankings disagree
- `n`: number of elements

Implementation must handle:
- Identical rankings -> tau = 1.0
- Reversed rankings -> tau = -1.0
- Single element -> tau = 1.0 (trivially ordered)
- Empty input -> tau = 1.0 (trivially ordered)

## Functional Requirements

### FR-01: Shared Test Fixtures Module

**Module**: `unimatrix-engine/src/test_scenarios.rs`
**Feature gate**: `#[cfg(any(test, feature = "test-support"))]`

FR-01.1: Export `EntryProfile`, `CalibrationScenario`, `RetrievalScenario`, `RetrievalEntry` types.

FR-01.2: Export `profile_to_entry_record(profile: &EntryProfile, id: u64, now: u64) -> EntryRecord` that converts a profile to a full `EntryRecord` with deterministic defaults for non-signal fields.

FR-01.3: Export canonical timestamp constant `CANONICAL_NOW: u64 = 1_700_000_000`.

FR-01.4: Export at least 5 standard entry profiles:
- `expert_human_fresh()` — Active, high access, recent, many helpful votes, human-authored
- `good_agent_entry()` — Active, moderate access, moderately fresh, some helpful votes, agent-authored
- `auto_extracted_new()` — Proposed, low access, very recent, no votes, trust_source="auto"
- `stale_deprecated()` — Deprecated, moderate access, very stale, mixed votes, human-authored
- `quarantined_bad()` — Quarantined, low access, stale, mostly unhelpful, unknown source

FR-01.5: Export at least 3 standard calibration scenarios:
- `standard_ranking()` — 5 profiles, expected order: expert > good_agent > auto_new > stale_deprecated > quarantined
- `trust_source_ordering()` — Same signals except trust_source varies: human > system > agent > neural > auto
- `freshness_dominance()` — Same signals except freshness varies: just_now > 1_day > 1_week > 1_month > 1_year

FR-01.6: Module-level doc comment serving as usage guide (AC-13). Must cover:
- When to add new scenarios (after adding signals, changing weights, discovering edge cases)
- How to interpret test failures (what a failing calibration test means)
- How to update golden regression values (what to change and what to verify)
- Procedure for weight change validation

### FR-02: Ranking Assertion Helpers

**Module**: `unimatrix-engine/src/test_scenarios.rs`

FR-02.1: `kendall_tau(ranking_a: &[u64], ranking_b: &[u64]) -> f64` — Compute Kendall tau rank correlation. Both inputs must contain the same elements (panics otherwise). Returns f64 in [-1.0, 1.0].

FR-02.2: `assert_ranked_above(results: &[(u64, f64)], higher_id: u64, lower_id: u64)` — Assert that `higher_id` appears before `lower_id` in the results list. Panics with descriptive message including both scores and positions.

FR-02.3: `assert_in_top_k(results: &[(u64, f64)], entry_id: u64, k: usize)` — Assert that `entry_id` appears in the first `k` results. Panics with descriptive message.

FR-02.4: `assert_tau_above(ranking_a: &[u64], ranking_b: &[u64], min_tau: f64)` — Assert that Kendall tau between two rankings is at least `min_tau`. Panics with the actual tau value and a human-readable explanation.

FR-02.5: `assert_confidence_ordering(entries: &[EntryRecord], expected_order: &[u64], now: u64)` — Compute confidence for each entry at `now`, verify ordering matches `expected_order`. Panics with actual vs. expected scores.

### FR-03: Confidence Calibration Tests

**File**: `crates/unimatrix-engine/tests/pipeline_calibration.rs`

FR-03.1: Test `standard_ranking` scenario — compute confidence for all 5 standard profiles, assert expected ordering holds. Uses `assert_confidence_ordering`.

FR-03.2: Test `trust_source_ordering` scenario — verify trust source alone differentiates otherwise-identical entries.

FR-03.3: Test `freshness_dominance` scenario — verify freshness decay produces correct temporal ordering.

FR-03.4: Weight sensitivity test — for each of the 6 weights, perturb by +/-10%. Compute rankings before and after perturbation. Assert Kendall tau > 0.6 (perturbation changes rankings but does not completely invert them).

FR-03.5: Signal ablation tests — for each of the 6 confidence signals, create two entry populations: one where the signal is maximized for entry A and minimized for entry B (all other signals equal). Assert A ranks above B. Measure Kendall tau of full population with signal present vs. zeroed.

FR-03.6: Boundary tests — all-zero entry, all-max entry, entry with only one non-zero signal. Assert all produce confidence in [0.0, 1.0] and ordering is sensible.

### FR-04: Retrieval Quality Tests (Pure Function)

**File**: `crates/unimatrix-engine/tests/pipeline_retrieval.rs` (Note: this was originally scoped here but given OQ-3 resolution, these tests validate the re-ranking arithmetic using pure functions. Full SearchService tests are in FR-06.)

FR-04.1: Re-rank blend — create entries with varying similarity and confidence. Assert that `rerank_score` produces expected ordering (high similarity + moderate confidence > moderate similarity + high confidence, given SEARCH_SIMILARITY_WEIGHT=0.85).

FR-04.2: Status penalty ordering — for entries with identical similarity and confidence, assert: active(penalty=1.0) > deprecated(penalty=0.7) > superseded(penalty=0.5).

FR-04.3: Provenance boost — for entries with identical scores, assert lesson-learned entry (PROVENANCE_BOOST=0.02) ranks above non-lesson entry.

FR-04.4: Co-access boost arithmetic — compute `co_access_boost()` for varying counts, assert monotonically increasing, capped at MAX_CO_ACCESS_BOOST.

FR-04.5: Combined interaction — create a scenario where co-access boost, provenance boost, and status penalty all interact. Assert the combined effect produces expected ordering. Document why.

### FR-05: Extraction Pipeline Tests

**File**: `crates/unimatrix-observe/tests/extraction_pipeline.rs`

FR-05.1: Seed a Store with observations from at least 3 feature cycles. Run `default_extraction_rules()`. Assert that at least one rule produces proposals.

FR-05.2: Quality gate accept path — create a `ProposedEntry` that passes all 4 in-memory checks (rate limit, content validation, cross-feature, confidence floor). Assert `quality_gate()` returns `Accept`.

FR-05.3: Quality gate rejection scenarios (at least 5):
- Short title (< 10 chars)
- Short content (< 20 chars)
- Invalid category
- Insufficient source features (below rule minimum)
- Low extraction confidence (< 0.2)

FR-05.4: Neural enhancer shadow mode — create `NeuralEnhancer` in Shadow mode, run on a proposed entry. Assert prediction is produced but entry is unchanged.

FR-05.5: Neural enhancer active mode — create `NeuralEnhancer` in Active mode, run on a proposed entry classified as noise. Assert entry is suppressed or confidence is overridden.

FR-05.6: Cross-rule validation — verify each rule's minimum feature count matches documented values (knowledge-gap=2, implicit-convention=3, recurring-friction=3, file-dependency=3, dead-knowledge=5).

### FR-06: Full SearchService Pipeline Tests

**File**: `crates/unimatrix-server/tests/pipeline_e2e.rs`

FR-06.1: Provide `TestServiceLayer` in `unimatrix-server/src/test_support.rs` behind `test-support` feature. Constructor accepts a store path and wires up `ServiceLayer` with:
- Real ONNX embedding service
- In-memory HNSW vector index
- Default AdaptationService
- Permissive SecurityGateway (no rate limiting for tests)
- Noop AuditLog

FR-06.2: Test active-above-deprecated — store entries with natural language content (one Active about "error handling in Rust", one Deprecated about "error handling patterns"). Search for "error handling". Assert Active entry ranks above Deprecated.

FR-06.3: Test supersession injection — store entry A (deprecated, superseded_by=B) and entry B (Active). Search for content matching A. Assert B appears in results even though it was not in the original HNSW result set.

FR-06.4: Test co-access boost — store 3 entries. Record co-access pairs between entries 1 and 2. Search for content matching entry 1. Assert entry 2 ranks higher than entry 3 (which has no co-access relationship).

FR-06.5: Test provenance boost — store two entries with identical content relevance, one with category "lesson-learned" and one with category "convention". Assert lesson-learned ranks above convention.

FR-06.6: Golden regression test — a fixed scenario with 10+ entries and a specific query. Assert exact top-3 result IDs. This test intentionally breaks when weights change, forcing the developer to re-evaluate and update the golden value.

FR-06.7: Skip gracefully if ONNX model is not available. Log `"ONNX model not found at {path}, skipping pipeline_e2e tests"`.

### FR-07: Regression Tests

**File**: `crates/unimatrix-engine/tests/pipeline_regression.rs`

FR-07.1: Golden confidence values — compute confidence for 3 standard profiles at CANONICAL_NOW. Assert exact f64 values to 6 decimal places. Any weight change breaks these tests.

FR-07.2: Weight change detection — hardcode the expected weight sum (0.92) and individual weight values. Assert they match the constants in `unimatrix_engine::confidence`. Any constant change fails the test with a message explaining what to update.

FR-07.3: Ranking stability — compute rankings for the `standard_ranking` scenario. Assert Kendall tau = 1.0 against the hardcoded expected ordering. Any formula change that alters the standard ranking fails with the actual new ordering.

### FR-08: Usage Guide Documentation

**Location**: Module-level doc comment in `unimatrix-engine/src/test_scenarios.rs`

FR-08.1: Section "When to Add Scenarios" — list triggers: new confidence signal, weight change, new extraction rule, new boost/penalty, bug discovered in ranking.

FR-08.2: Section "How to Interpret Failures" — explain what each test category failure means and what to investigate.

FR-08.3: Section "Updating Golden Values" — step-by-step procedure for updating regression baselines after intentional changes.

FR-08.4: Section "Running Pipeline Tests" — commands for running specific test subsets.

## Acceptance Criteria Traceability

| AC | Functional Requirement | Test Location |
|----|----------------------|---------------|
| AC-01 | FR-01 (test_scenarios module) | `unimatrix-engine/src/test_scenarios.rs` |
| AC-02 | FR-03.1, FR-03.2, FR-03.3 | `unimatrix-engine/tests/pipeline_calibration.rs` |
| AC-03 | FR-03.5 | `unimatrix-engine/tests/pipeline_calibration.rs` |
| AC-04 | FR-05.1, FR-05.2, FR-05.3 | `unimatrix-observe/tests/extraction_pipeline.rs` |
| AC-05 | FR-05.4, FR-05.5 | `unimatrix-observe/tests/extraction_pipeline.rs` |
| AC-06 | FR-04.1, FR-04.2, FR-06.2 | `pipeline_retrieval.rs`, `pipeline_e2e.rs` |
| AC-07 | FR-06.3 | `unimatrix-server/tests/pipeline_e2e.rs` |
| AC-08 | FR-06.4 | `unimatrix-server/tests/pipeline_e2e.rs` |
| AC-09 | FR-06.5 | `unimatrix-server/tests/pipeline_e2e.rs` |
| AC-10 | FR-01 (self-documenting scenarios) | All scenario definitions |
| AC-11 | FR-06.2-FR-06.6 | `unimatrix-server/tests/pipeline_e2e.rs` |
| AC-12 | FR-03.5, FR-02.1, FR-02.4 | `pipeline_calibration.rs` |
| AC-13 | FR-08 | Module-level docs in `test_scenarios.rs` |
| AC-14 | Unimatrix procedures | Stored via `/store-procedure` |

## Constraints

- C-01: `test_scenarios.rs` must be feature-gated with `#[cfg(any(test, feature = "test-support"))]`
- C-02: No `SystemTime::now()` in any test code. All timestamps use `CANONICAL_NOW` or explicit values.
- C-03: Extend existing `TestDb`, `TestEntry` from unimatrix-store. No parallel scaffolding.
- C-04: Server-level tests must skip gracefully when ONNX model is absent.
- C-05: All test files must compile and pass with `cargo test --workspace`.
- C-06: Kendall tau implementation must be a pure function with no external dependencies.
