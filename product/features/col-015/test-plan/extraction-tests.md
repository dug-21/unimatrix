# Test Plan: extraction-tests

## Extraction Pipeline (extraction_pipeline.rs)

| ID | Test | Expected | Risk |
|----|------|----------|------|
| T-EXT-01 | Rules fire with seeded store | At least one proposal produced | R-05 |
| T-EXT-02 | Quality gate accepts valid entry | QualityGateResult::Accept | R-05 |
| T-EXT-03 | Quality gate rejects short title | Reject with content_validation | R-05 |
| T-EXT-04 | Quality gate rejects insufficient features | Reject with cross_feature | R-05 |
| T-EXT-05 | Neural enhancer shadow mode | Prediction produced, entry unchanged | R-05 |
| T-EXT-06 | Cross-rule feature minimums | knowledge-gap=2, implicit-convention=3, recurring-friction=3, file-dependency=3, dead-knowledge=5 | R-05 |

## Seeding Strategy

- T-EXT-01: Construct ObservationRecord instances with tool_name, input, feature_cycle fields matching knowledge-gap detection pattern
- T-EXT-02..04: Construct ProposedEntry directly (no store needed)
- T-EXT-05: Construct NeuralEnhancer with baseline models
