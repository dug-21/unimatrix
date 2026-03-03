# col-010b Test Plan Overview

## Test Strategy

Tests are organized by component, matching the pseudocode structure. Each component
has unit tests in its source file and integration tests verified in Stage 3c.

## Risk Mapping

| Risk | Priority | Test Coverage |
|------|----------|--------------|
| R-01: Truncation mutates in-memory report | Critical | T-EL-03 (clone-and-truncate verification) |
| R-02: Provenance boost divergence | High | T-PB-03, T-PB-04, T-PB-05, T-PB-06 (both sites) |
| R-03: Fire-and-forget embedding failure | Medium | T-LL-04 (embedding failure path) |
| R-04: Concurrent supersede race | Medium | T-LL-06 (concurrent calls) |
| R-05: Narrative synthesis edge cases | Medium | T-ES-01 through T-ES-06 |
| R-06: evidence_limit breaks tests | Low | T-EL-01 (R-09 audit), T-EL-02 (backward compat) |
| R-07: CategoryAllowlist absent | Low | T-LL-05 (allowlist guard) |
| R-08: recommendations field breaks JSON | Low | T-ES-08 (serde roundtrip) |
| R-09: Empty lesson-learned content | Medium | T-LL-07 (content generation edge cases) |

## Component Test Matrix

| Component | Unit Tests | Integration Tests | AC Coverage |
|-----------|-----------|------------------|-------------|
| 1: Evidence-Limiting | T-EL-01..04 | AC-01, AC-02, AC-10 | AC-01, AC-02, AC-10 |
| 2: Evidence-Synthesis | T-ES-01..09 | AC-03, AC-04, AC-05 | AC-03, AC-04, AC-05 |
| 3: Lesson-Learned | T-LL-01..07 | AC-06, AC-07, AC-08 | AC-06, AC-07, AC-08 |
| 4: Provenance-Boost | T-PB-01..06 | AC-09 | AC-09 |

## Integration Harness Plan

### Suites to Run
- `cargo test --workspace` (all unit tests + workspace integration)
- Existing retrospective integration tests with `evidence_limit = 0` for backward compat

### New Integration Tests Needed
1. Evidence truncation end-to-end (AC-01, AC-02)
2. Lesson-learned auto-persistence + HNSW searchability (AC-06, AC-07, AC-08)
3. Provenance boost ranking verification (AC-09)

### Smoke Tests
- `cargo build --workspace` compiles without errors
- `cargo test --workspace` all tests pass
- No TODOs or stubs in implementation

## Test Execution Order

1. Unit tests per component (cargo test within each crate)
2. Integration tests for cross-component behavior
3. Full workspace build and test pass (AC-10)

## embedding_dim Fix Verification

Both `insert_with_audit` and `correct_with_audit` must set `embedding_dim`
from the actual embedding vector length. Tests must verify:
- New entries created via `insert_with_audit` have `embedding_dim == embedding.len()`
- Corrected entries via `correct_with_audit` have `embedding_dim == embedding.len()`
- Lesson-learned entries have `embedding_dim > 0` after successful embedding
