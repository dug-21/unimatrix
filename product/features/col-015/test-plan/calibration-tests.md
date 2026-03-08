# Test Plan: calibration-tests

## Calibration (pipeline_calibration.rs)

| ID | Test | Expected | Risk |
|----|------|----------|------|
| T-CAL-01 | standard_ranking scenario | expert > good > auto > stale > quarantined | R-03 |
| T-CAL-02 | trust_source_ordering scenario | human > system > agent > neural > auto | R-03 |
| T-CAL-03 | freshness_dominance scenario | now > 1d > 1w > 1m > 1y | R-03 |
| T-CAL-04 | Weight sensitivity +/-10% | tau > 0.6 for each perturbed weight | R-07 |
| T-CAL-05 | Boundary entries (all-zero, all-max) | confidence in [0.0, 1.0] | R-03 |
| T-ABL-01 | Base signal ablation | tau < 0.9 when signal zeroed | R-07 |
| T-ABL-02 | Usage signal ablation | Measurable tau impact | R-07 |
| T-ABL-03 | Freshness signal ablation | Measurable tau impact | R-07 |
| T-ABL-04 | Helpfulness signal ablation | Measurable tau impact | R-07 |
| T-ABL-05 | Correction signal ablation | Measurable tau impact | R-07 |
| T-ABL-06 | Trust signal ablation | Measurable tau impact | R-07 |

## Retrieval (pipeline_retrieval.rs)

| ID | Test | Expected | Risk |
|----|------|----------|------|
| T-RET-01 | rerank_score blend ordering | Similarity-dominant wins at 0.85 weight | R-03 |
| T-RET-02 | Status penalty ordering | active > deprecated > superseded | R-03 |
| T-RET-03 | Provenance boost effect | Boosted entry > unboosted | R-03 |
| T-RET-04 | Co-access boost monotonic capped | Non-decreasing, <= 0.03 | R-03 |
| T-RET-05 | Combined interaction | Expected ordering with all effects | R-03 |
