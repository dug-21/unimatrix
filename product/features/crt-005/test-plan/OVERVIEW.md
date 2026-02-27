# Test Plan Overview: crt-005 Coherence Gate

## Test Strategy

### Unit Tests (~60 new)
- Coherence dimension scores (pure functions) in coherence.rs
- Lambda computation + re-normalization in coherence.rs
- Recommendation generation in coherence.rs
- f64 scoring precision in confidence.rs, coaccess.rs
- Weight sum invariants
- StatusReport formatting in response.rs

### Integration Tests (~30 new)
- Schema migration v2->v3 in migration.rs
- VectorIndex::compact in index.rs
- Confidence refresh pipeline in tools.rs
- Maintenance parameter behavior in tools.rs
- End-to-end context_status with coherence in tools.rs/server tests
- Embed service unavailability handling

### Existing Test Updates (~60-80 mechanical updates)
- f32 type annotations -> f64
- f32::EPSILON -> f64::EPSILON
- Hardcoded f32 confidence values -> f64
- Test helpers with f32 signatures -> f64
- StatusReport construction sites gain 10 new fields

## Risk-to-Test Mapping

| Risk | Priority | Component | Test Location |
|------|----------|-----------|---------------|
| R-01 (migration failure) | High | C1 | migration.rs |
| R-02 (residual f32) | Critical | C2 | confidence.rs, coaccess.rs, grep |
| R-03 (compaction corruption) | High | C3 | index.rs |
| R-04 (f64 cast precision) | Med | C2 | index.rs |
| R-05 (lambda re-normalization) | High | C4 | coherence.rs |
| R-06 (VECTOR_MAP ordering) | High | C3 | index.rs |
| R-07 (maintenance opt-out) | Med | C7 | tools.rs |
| R-08 (refresh batch cap) | Med | C5 | tools.rs |
| R-09 (embed unavailable) | Med | C8 | tools.rs |
| R-10 (boundary values) | High | C4 | coherence.rs |
| R-11 (test regression) | High | C2 | cargo test --workspace |
| R-12 (format coherence) | Med | C6 | response.rs |
| R-13 (V2EntryRecord mismatch) | Critical | C1 | migration.rs |
| R-14 (weight sum f64) | High | C2/C4 | confidence.rs, coherence.rs |
| R-15 (search drift) | Med | C3 | index.rs |
| R-16 (staleness detection) | Med | C4 | coherence.rs |
| R-17 (trait object safety) | High | C3 | traits.rs (compile-time) |
| R-18 (empty KB) | Med | C4/C8 | coherence.rs, tools.rs |
| R-19 (concurrent compaction) | Low | C3 | index.rs |
| R-20 (recommendation accuracy) | Low | C4 | coherence.rs |

## Cross-Component Test Dependencies

- C2 tests depend on C1 (schema change) completing first
- C6 tests depend on C4 (coherence types)
- C7/C8 tests depend on C5 (refresh logic) and C3 (compact method)
- Integration tests in tools.rs depend on all components

## Integration Test Scenarios (IT-01 through IT-08)

See RISK-TEST-STRATEGY.md for full descriptions. Key scenarios:
- IT-01: Full coherence pipeline happy path
- IT-02: Schema migration end-to-end
- IT-03: Graph compaction end-to-end
- IT-04: Maintenance opt-out end-to-end
- IT-05: f64 scoring pipeline end-to-end
- IT-06: Empty knowledge base coherence
- IT-07: Embed service unavailable during compaction
- IT-08: Confidence refresh with batch cap

## Regression Verification

1. Before changes: cargo test = 811 passing
2. After Tier 1 (C1+C2+C3+C4+C6): fix type updates, verify >= 811
3. After Tier 2 (C5+C7+C8): verify >= Tier 1 count + new tests
4. Final: no #[ignore] or #[cfg(skip)] added by crt-005
