# Test Plan: shadow

## Risks Covered: R-05, R-08, R-10

### T-SH-01: NeuralEnhancer shadow mode passes entry unchanged (R-05, AC-09)
- Create NeuralEnhancer in Shadow mode with baseline models
- enhance(entry) returns NeuralPrediction
- Entry itself is not modified (shadow mode is observation-only)

### T-SH-02: NeuralEnhancer produces valid prediction
- enhance(valid_entry) returns prediction with:
  - classification.probabilities sums to ~1.0
  - convention_score in [0.0, 1.0]
  - digest.features[0..7] match entry fields

### T-SH-03: ShadowEvaluator tracks evaluations (AC-10)
- Log 5 predictions
- evaluation_count() == 5
- accuracy() returns ShadowAccuracy with total_evaluations == 5

### T-SH-04: ShadowEvaluator accuracy computation
- Log 10 predictions where 7 match rule categories
- accuracy().overall == 0.7
- per_category counts are correct

### T-SH-05: ShadowEvaluator can_promote requires min evaluations (R-05)
- Log 19 predictions (all matching)
- can_promote() == false (need 20)
- Log 1 more
- can_promote() == true

### T-SH-06: ShadowEvaluator should_rollback within tolerance (R-10)
- Set baseline_accuracy = 0.80
- Log 50 predictions with 77% accuracy (3% drop, within 5% threshold)
- should_rollback() == false

### T-SH-07: ShadowEvaluator should_rollback triggers on large drop (R-10)
- Set baseline_accuracy = 0.80
- Log 50 predictions with 74% accuracy (6% drop, exceeds 5% threshold)
- should_rollback() == true

### T-SH-08: ShadowEvaluator should_rollback requires minimum window (R-10)
- Set baseline_accuracy = 0.80
- Log 49 predictions with 0% accuracy
- should_rollback() == false (window is 50)
