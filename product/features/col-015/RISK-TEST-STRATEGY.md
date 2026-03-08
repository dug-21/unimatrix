# Risk-Test Strategy: col-015

## Risk Registry

| Risk ID | Risk | Severity | Likelihood | Mitigation | Test Coverage |
|---------|------|----------|------------|------------|---------------|
| R-01 | Kendall tau implementation error produces incorrect correlation values, leading to false pass/fail on ablation tests | High | Low | Dedicated unit tests for Kendall tau with known reference values (identical, reversed, partially correlated) | T-KT-01 through T-KT-05 |
| R-02 | TestServiceLayer construction fails due to ONNX model absence, blocking all server-level tests | Med | Med | ADR-005: skip-on-absence pattern. Each test checks model availability before constructing the service | T-E2E-skip |
| R-03 | Golden regression tests are too brittle — any minor refactor (e.g., f64 precision change) breaks them even when ranking is unchanged | Med | Med | Golden tests assert confidence values to 4 decimal places (not 15). Ranking assertions use pairwise ordering, not exact scores | T-REG-01 through T-REG-03 |
| R-04 | EntryProfile -> EntryRecord conversion loses signal information (e.g., forgets to set trust_source), causing calibration tests to validate wrong inputs | High | Low | Explicit unit test that round-trips profile -> record -> compute_confidence and verifies each signal component matches expected value | T-PROF-01 |
| R-05 | Extraction pipeline tests seed Store incorrectly, causing rules to fire/not-fire for wrong reasons | Med | Med | Each extraction test documents exactly what observation pattern it seeds and why the rule should/shouldn't fire | T-EXT-01 through T-EXT-06 |
| R-06 | SearchService test constructs services with different configuration than production, invalidating e2e results | High | Med | TestServiceLayer uses same defaults as production ServiceLayer::new(). Diff test construction against production constructor | T-TSL-01 |
| R-07 | Signal ablation test measures tau but sets threshold too low, allowing a truly dead signal to pass | Med | Low | Ablation threshold set at tau < 0.9 (signal removal should noticeably change rankings). Document threshold rationale | T-ABL-01 through T-ABL-06 |
| R-08 | Co-access boost test is non-deterministic due to SystemTime::now() in SearchService co-access staleness computation | Med | Med | Seed co-access pairs with timestamp = 0 (always fresh) or timestamp far in the past (always stale). Avoid boundary cases | T-E2E-04 |

## Scope Risk Traceability

| Scope Risk | Architecture Decision | Test Coverage |
|-----------|----------------------|---------------|
| SR-01 (ONNX model availability) | ADR-005 (skip on absence) | T-E2E-skip: verify skip message printed when model absent |
| SR-03 (SearchService constructor complexity) | ADR-002 (TestServiceLayer builder) | T-TSL-01: verify TestServiceLayer constructs successfully |
| SR-04 (Scope creep into weight tuning) | Relative ordering assertions, not exact scores | T-REG-01 through T-REG-03 use pairwise ordering |
| SR-05 (Ambiguous "correct ranking") | ADR-006 (description field on scenarios) | AC-10: all scenarios have rationale |
| SR-06 (Distributed test infrastructure) | ADR-001 (shared fixtures in test_scenarios) | All test files import from test_scenarios |
| SR-07 (Predecessor features incomplete) | Not mitigated architecturally | Tests validate post-fix behavior; known issues documented |
| SR-08 (Feature flag coordination) | ADR-002 (test-support feature) | Build verification in CI |
| SR-09 (Store schema assumptions) | ADR-006 (profile_to_entry_record builder) | T-PROF-01 validates conversion |
| SR-10 (Pure function vs real behavior) | ADR-002 (server-level tests) | T-E2E-01 through T-E2E-06 validate real SearchService |
| SR-11 (Synthetic vs real embeddings) | ADR-005 (blend approach) | Engine tests use synthetic; server tests use real ONNX |

## Test Plan

### Tier 1: Kendall Tau Implementation (unimatrix-engine unit tests)

| Test ID | Description | Risk |
|---------|-------------|------|
| T-KT-01 | Identical rankings -> tau = 1.0 | R-01 |
| T-KT-02 | Reversed rankings -> tau = -1.0 | R-01 |
| T-KT-03 | Known partial correlation (reference value from stats textbook) | R-01 |
| T-KT-04 | Single element -> tau = 1.0 | R-01 |
| T-KT-05 | Two elements, both orderings | R-01 |

### Tier 2: Profile Conversion (unimatrix-engine unit tests)

| Test ID | Description | Risk |
|---------|-------------|------|
| T-PROF-01 | Round-trip: profile -> EntryRecord -> compute_confidence. Verify each sub-score matches expected value for the profile's signals | R-04 |
| T-PROF-02 | All standard profiles produce distinct confidence values at CANONICAL_NOW | R-04 |

### Tier 3: Confidence Calibration (unimatrix-engine integration tests)

| Test ID | Description | Risk |
|---------|-------------|------|
| T-CAL-01 | standard_ranking scenario ordering holds | R-03 |
| T-CAL-02 | trust_source_ordering scenario ordering holds | R-03 |
| T-CAL-03 | freshness_dominance scenario ordering holds | R-03 |
| T-CAL-04 | Weight sensitivity: +/-10% perturbation, tau > 0.6 | R-07 |
| T-CAL-05 | Boundary entries (all-zero, all-max) in valid range | R-03 |

### Tier 4: Signal Ablation (unimatrix-engine integration tests)

| Test ID | Description | Risk |
|---------|-------------|------|
| T-ABL-01 | Base signal ablation: tau impact measurable | R-07 |
| T-ABL-02 | Usage signal ablation: tau impact measurable | R-07 |
| T-ABL-03 | Freshness signal ablation: tau impact measurable | R-07 |
| T-ABL-04 | Helpfulness signal ablation: tau impact measurable | R-07 |
| T-ABL-05 | Correction signal ablation: tau impact measurable | R-07 |
| T-ABL-06 | Trust signal ablation: tau impact measurable | R-07 |

### Tier 5: Retrieval Arithmetic (unimatrix-engine integration tests)

| Test ID | Description | Risk |
|---------|-------------|------|
| T-RET-01 | rerank_score blend ordering correct | R-03 |
| T-RET-02 | Status penalty ordering: active > deprecated > superseded | R-03 |
| T-RET-03 | Provenance boost effect measurable | R-03 |
| T-RET-04 | Co-access boost monotonic and capped | R-03 |
| T-RET-05 | Combined interaction ordering correct | R-03 |

### Tier 6: Extraction Pipeline (unimatrix-observe integration tests)

| Test ID | Description | Risk |
|---------|-------------|------|
| T-EXT-01 | Seeded store produces rule proposals | R-05 |
| T-EXT-02 | Quality gate accepts valid entry | R-05 |
| T-EXT-03 | Quality gate rejects short title | R-05 |
| T-EXT-04 | Quality gate rejects insufficient features | R-05 |
| T-EXT-05 | Neural enhancer shadow mode produces prediction without mutation | R-05 |
| T-EXT-06 | Neural enhancer active mode suppresses noise | R-05 |

### Tier 7: Full Pipeline (unimatrix-server integration tests)

| Test ID | Description | Risk |
|---------|-------------|------|
| T-E2E-skip | Skip with message when ONNX model absent | R-02 |
| T-E2E-01 | Active entry ranks above deprecated for same query | R-06 |
| T-E2E-02 | Supersession injection: successor appears in results | R-06 |
| T-E2E-03 | Provenance boost: lesson-learned ranks above convention | R-06 |
| T-E2E-04 | Co-access boost: co-accessed entry ranks higher | R-06, R-08 |
| T-E2E-05 | Golden regression: exact top-3 for known scenario | R-03 |
| T-TSL-01 | TestServiceLayer constructs successfully with valid model | R-06 |

### Tier 8: Regression (unimatrix-engine integration tests)

| Test ID | Description | Risk |
|---------|-------------|------|
| T-REG-01 | Golden confidence values match to 4 decimal places | R-03 |
| T-REG-02 | Weight constants match hardcoded expected values | R-03 |
| T-REG-03 | standard_ranking tau = 1.0 against hardcoded ordering | R-03 |

## Coverage Summary

| Category | Test Count | Risks Covered |
|----------|-----------|---------------|
| Kendall tau | 5 | R-01 |
| Profile conversion | 2 | R-04 |
| Calibration | 5 | R-03, R-07 |
| Signal ablation | 6 | R-07 |
| Retrieval arithmetic | 5 | R-03 |
| Extraction pipeline | 6 | R-05 |
| Full pipeline (server) | 7 | R-02, R-06, R-08 |
| Regression | 3 | R-03 |
| **Total** | **39** | **8 risks** |

## Top 3 Risks by Severity

1. **R-01 (Kendall tau implementation error)**: High severity because all ablation tests depend on correct tau computation. Mitigated by 5 dedicated unit tests with known reference values.
2. **R-04 (Profile conversion data loss)**: High severity because incorrect profiles invalidate all calibration tests. Mitigated by round-trip validation test.
3. **R-06 (TestServiceLayer divergence from production)**: High severity because server-level tests become meaningless if construction differs. Mitigated by structural similarity to production constructor and T-TSL-01 verification.
