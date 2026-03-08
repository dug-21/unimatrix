# Agent Report: col-015-researcher

## Agent ID: col-015-researcher
## Feature: col-015 — Intelligence Pipeline End-to-End Validation
## Role: Researcher (Problem Space Explorer)

## Status: COMPLETE

## Deliverable
- `product/features/col-015/SCOPE.md` — full scope document with 10 acceptance criteria

## Key Findings

### Pipeline Architecture
- 7-stage pipeline across 6 crates: observation → extraction (5 rules) → neural enhancement (2 models) → quality gate (6 checks) → storage → confidence evolution (6 factors) → retrieval/re-ranking
- Each stage has unit tests but NO cross-cutting integration tests
- unimatrix-learn has 1 e2e test (retraining_e2e.rs) — good pattern to follow

### Existing Test Infrastructure
- TestDb + TestEntry builder in unimatrix-store (test-support feature flag)
- 45+ unit tests in confidence.rs covering component functions
- No integration tests in unimatrix-engine or unimatrix-server
- make_test_entry() helper in confidence.rs creates EntryRecord with explicit fields

### Crate Dependency Constraints
- unimatrix-engine depends on core + store (good home for confidence/retrieval tests)
- unimatrix-observe depends on core + store + learn (good home for extraction tests)
- unimatrix-server depends on ALL crates (too heavy for intelligence logic tests)
- No circular dependency risk with proposed placement

### 15 Key Constants Identified for Calibration
- 6 confidence weights (sum=0.92), 3 boosts, 2 penalties, 4 extraction thresholds
- All are public constants, testable without production code changes

## Proposed Scope Boundaries
- Pure test infrastructure — no production code changes
- Primary home: unimatrix-engine/tests/ (calibration, retrieval, regression)
- Secondary home: unimatrix-observe/tests/ (extraction pipeline)
- Shared fixtures: unimatrix-engine/src/test_scenarios.rs (test-support feature)

## Open Questions for Design Phase
1. Real ONNX embeddings vs synthetic for test scenarios
2. Ranking metric choice (Kendall tau vs position delta)
3. Test data volume per scenario (recommended 10-20)
4. Whether SearchService-level tests are needed
5. Co-access helper availability in test-support
