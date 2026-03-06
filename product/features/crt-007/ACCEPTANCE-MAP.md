# Acceptance Map: crt-007 Neural Extraction Pipeline

## Wave 1: Shared Training Infrastructure

| AC-ID | Criterion | Wave | Test Type | Risk |
|-------|-----------|------|-----------|------|
| AC-01 | unimatrix-learn crate with TrainingReservoir<T>, EwcState, ModelRegistry, persistence | W1 | cargo build + unit | R-01 |
| AC-02 | unimatrix-adapt uses shared implementations (no duplication) | W1 | grep + compilation | R-01 |
| AC-03 | All 174+ unimatrix-adapt tests pass | W1 | cargo test -p unimatrix-adapt | R-01, R-02 |

## Wave 2: Neural Models

| AC-ID | Criterion | Wave | Test Type | Risk |
|-------|-----------|------|-----------|------|
| AC-04 | Signal Classifier MLP with baseline weights via burn | W2 | unit: construct, verify output shape | R-04 |
| AC-05 | Convention Scorer MLP with baseline weights via burn | W2 | unit: construct, verify [0,1] output | R-04 |
| AC-06 | SignalDigest defined with all features | W2 | unit: from_proposed -> valid 32-element vector | -- |
| AC-07 | Classifier inference < 50ms | W2 | benchmark: 1000 inferences, p99 | R-07 |
| AC-08 | Scorer inference < 10ms | W2 | benchmark: 1000 inferences, p99 | R-07 |
| AC-11 | ModelRegistry manages slots with promotion/rollback | W2 | unit: state transitions | R-05, R-10 |
| AC-12 | Auto-rollback on >5% accuracy drop | W2 | unit: simulate accuracy drop | R-10 |
| AC-13 | Models stored at correct path with versioned filenames | W2 | unit: save/load round-trip | R-06 |
| AC-14 | Baseline weights bias classifier->noise, scorer->low | W2 | unit: all-zero digest output | R-04, R-09 |
| AC-17 | Unit tests for classifier, scorer, shadow, registry | W2 | cargo test -p unimatrix-learn | -- |

## Wave 3: Shadow Mode + Integration

| AC-ID | Criterion | Wave | Test Type | Risk |
|-------|-----------|------|-----------|------|
| AC-09 | Shadow mode: no effect on stored entries | W3 | integration: compare shadow vs rule-only output | R-05 |
| AC-10 | Shadow evaluation logs persist in SQLite | W3 | unit: write + query shadow_evaluations | R-08 |
| AC-15 | "neural" -> 0.40 in confidence scoring | W3 | unit: neural entry scored correctly | -- |
| AC-16 | Neural entries use trust_source "neural" | W3 | integration: active-mode entry in store | -- |
| AC-18 | E2E shadow mode: rules -> digest -> classify -> log -> no store impact | W3 | integration | R-05 |

## Traceability Matrix

| AC-ID | FR | NFR | Risk | ADR |
|-------|-----|-----|------|-----|
| AC-01 | FR-01 | -- | R-01 | -- |
| AC-02 | FR-02 | NFR-04.1 | R-01 | ADR-002 |
| AC-03 | FR-02 | NFR-04.1 | R-01, R-02 | -- |
| AC-04 | FR-03 | -- | R-04 | ADR-001 |
| AC-05 | FR-04 | -- | R-04 | ADR-001 |
| AC-06 | FR-05 | -- | -- | ADR-003 |
| AC-07 | FR-03 | NFR-01.1 | R-07 | -- |
| AC-08 | FR-04 | NFR-01.2 | R-07 | -- |
| AC-09 | FR-06 | -- | R-05 | -- |
| AC-10 | FR-06 | NFR-04.4 | R-08 | ADR-004 |
| AC-11 | FR-07 | -- | R-05, R-10 | ADR-005 |
| AC-12 | FR-07 | -- | R-10 | ADR-005 |
| AC-13 | FR-07 | -- | R-06 | ADR-005 |
| AC-14 | FR-03, FR-04 | -- | R-04, R-09 | ADR-003 |
| AC-15 | FR-08 | -- | -- | -- |
| AC-16 | FR-08 | -- | -- | -- |
| AC-17 | FR-03, FR-04, FR-06, FR-07 | -- | -- | -- |
| AC-18 | FR-06, FR-08 | -- | R-05 | -- |

## Coverage Gaps

None identified. All 18 acceptance criteria mapped to functional requirements, risks, and test types. All 10 risks from RISK-TEST-STRATEGY.md are covered by at least one AC.
