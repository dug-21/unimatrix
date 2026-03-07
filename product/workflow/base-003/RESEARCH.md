# base-003: Intelligence & Confidence Validation Harness — Research

## Problem Statement

Unimatrix has a fully self-training system (confidence scoring, co-access boosting, coherence monitoring, contradiction detection, EWC-based retraining) but no way to validate it works correctly or is tuned properly. Individual unit tests exist for each component, but no harness validates:
- Confidence converges correctly under realistic usage
- Search results actually improve as the system learns
- Contradictions are caught without excessive false positives
- Training doesn't cause catastrophic forgetting
- All components work together end-to-end

---

## System Inventory

### Component Map

| Component | Location | Unit Tests | Integration Tests | Coverage Gap |
|-----------|----------|-----------|------------------|-------------|
| **Confidence Formula** | `unimatrix-engine/src/confidence.rs` | 60+ | 10 (basic) | No convergence, no concurrent updates, no realistic simulation |
| **Co-Access System** | `unimatrix-server/src/infra/usage_dedup.rs` | 24 | 5 | No ranking improvement validation, no staleness expiration |
| **Coherence / Lambda** | `unimatrix-server/src/infra/coherence.rs` | 35+ | 5 | No lambda-search quality correlation, no maintenance outcome |
| **Contradiction Detection** | `unimatrix-server/src/infra/contradiction.rs` | 30+ | 14 | No adversarial testing, no weight tuning validation |
| **Usage Tracking** | `unimatrix-server/src/services/usage.rs` | 12+ | Implicit | No session isolation proof, no vote symmetry validation |
| **EWC++ Training** | `unimatrix-learn/` | 5 | **0** | **CRITICAL**: No convergence, no forgetting test, no model quality |
| **Training Pipeline** | `unimatrix-learn/src/service.rs` | 5 | **0** | **CRITICAL**: No end-to-end training, no signal→model feedback |

### Confidence Formula (6 Stored Factors + 1 Query-Time)

| Factor | Weight | Formula | Saturation |
|--------|--------|---------|-----------|
| Base Score | 0.18 | Active=0.5, Deprecated=0.2, Quarantined=0.1 | Status-dependent |
| Usage Score | 0.14 | `log(1+count)/log(1+50)` | 50 accesses |
| Freshness | 0.18 | Exponential decay, half-life 168h | 1 week |
| Helpfulness | 0.14 | Wilson lower bound (z=1.96, 95% CI, min 5 votes) | ~100 votes |
| Correction Chain | 0.14 | Piecewise: 0→0.5, 1-2→0.8, 3-5→0.6, 6+→0.3 | N/A |
| Trust Score | 0.14 | human=1.0, system=0.7, agent=0.5, neural=0.4, auto=0.35 | N/A |
| Co-Access (query-time) | 0.08 | `log(1+partners)/log(1+10) * avg_confidence` | 10 partners |

**Search re-ranking**: `0.85*similarity + 0.15*confidence + co_access_boost(max 0.03) + provenance_boost(0.02 for lessons)`

### Coherence Lambda (4 Dimensions)

| Dimension | Weight | Measures |
|-----------|--------|---------|
| Confidence Freshness | 0.35 | Fraction of entries with non-stale confidence (<24h) |
| Graph Quality | 0.30 | Fraction of non-stale HNSW nodes |
| Embedding Consistency | 0.15 | Entries as own top-1 match with similarity >= 0.99 |
| Contradiction Density | 0.20 | Inverse of quarantined/active ratio |

Maintenance triggers at lambda < 0.8: batch confidence refresh (100), graph compaction, co-access cleanup (30-day window).

---

## Proposed Validation Harness

### Architecture

```
product/test/base-003/
├── conftest.py                         # Shared fixtures, synthetic data generators
├── scenarios/
│   ├── test_confidence_convergence.py  # Scenario 1
│   ├── test_coaccess_ranking.py        # Scenario 2
│   ├── test_coherence_maintenance.py   # Scenario 3
│   ├── test_contradiction_resistance.py # Scenario 4
│   ├── test_training_convergence.py    # Scenario 5
│   ├── test_end_to_end.py             # Scenario 6
│   └── test_regression.py             # Scenario 7
├── generators/
│   ├── synthetic_usage.py             # Realistic vote/access sequences
│   ├── contradiction_pairs.py         # Conflicting entry generators
│   ├── training_data.py               # Synthetic training samples
│   └── coherence_scenarios.py         # KB states with varying health
├── assertions/
│   ├── confidence.py                  # Convergence, Wilson, monotonicity
│   ├── ranking.py                     # MRR, position change, boost verification
│   ├── coherence.py                   # Lambda formula, dimension scores
│   └── training.py                    # Catastrophic forgetting, EWC active
└── reports/
    └── validation_report.md
```

### Scenario Descriptions

#### Scenario 1: Confidence Convergence (Medium)
- Create 10 identical entries, apply different usage patterns
- Phase A: 100 accesses on entry #1 → usage_score increases
- Phase B: 50 helpful + 5 unhelpful votes on entry #2 → Wilson kicks in at 5 votes
- Phase C: Advance time 1 week → freshness halves
- **Assert**: entry1.confidence > entry2.confidence > entry3.confidence; all in [0,1]; monotonic with access

#### Scenario 2: Co-Access Ranking Improvement (Medium)
- Create 20 entries in 4 topics, establish baseline search ranking
- Simulate 50 searches generating co-access pairs
- Re-search same queries, verify boosted entries rank higher
- **Assert**: MRR improves; boost capped at 0.03; position change +1..+3

#### Scenario 3: Coherence & Maintenance (Large)
- Create 100 entries, introduce degradation (30% stale confidence, 15% stale graph, 3% quarantined)
- Measure lambda (expect ~0.825)
- Trigger maintenance (maintain=true)
- Re-measure lambda (expect >= 0.95)
- **Assert**: Lambda increases monotonically with maintenance; dimension scores match formulas

#### Scenario 4: Contradiction Detection & Quarantine (Large)
- Create 50 clean entries + inject 10 contradiction pairs
- Scan, detect, quarantine
- **Assert**: Detection rate >= 80%; false positive rate <= 5%; quarantined entries excluded from search

#### Scenario 5: Training Convergence & Catastrophic Forgetting (Very Large)
- Create 200-entry KB, record baseline search quality
- Generate 500 searches with 70% helpful / 30% unhelpful feedback
- Feed to training pipeline, run N iterations
- **Assert**: No catastrophic forgetting (MRR degradation < 5%); EWC penalty active; new learning detectable
- **BLOCKER**: Training pipeline integration not fully visible — investigation needed

#### Scenario 6: End-to-End Intelligence (Very Large)
- Minimal 50-entry KB, 200 searches with realistic feedback
- Monitor all metrics as they evolve (confidence, co-access, training, lambda)
- Trigger maintenance if lambda drops
- **Assert**: Search quality improves or stabilizes; KB health maintained; no deadlocks

#### Scenario 7: Regression Suite (Small)
- Re-run existing test_confidence.py, test_contradiction.py, test_adaptation.py
- Baseline for detecting regressions in existing behavior

### Synthetic Data Generators

**Usage patterns**: Zipfian access distribution (some entries hit often, most rare), session-scoped co-access, vote sequences with correction support.

**Contradiction pairs**: Negation opposition ("always X" vs "never X"), incompatible directives ("use X" vs "use Y"), opposing sentiment ("recommended" vs "anti-pattern").

**Coherence scenarios**: KB states with controlled degradation levels per dimension.

---

## Complexity & Phasing

| Scenario | Complexity | Est. Effort | Dependencies | Risk |
|----------|-----------|-------------|-------------|------|
| 7. Regression | Small | 1-2 days | None | Low |
| 1. Confidence | Medium | 3-5 days | Generators (vote seq) | Low |
| 2. Co-Access | Medium | 4-6 days | Generators (sessions) | Medium |
| 4. Contradiction | Large | 5-7 days | Generators (pairs) | Medium |
| 3. Coherence | Large | 6-8 days | Time manipulation | Medium |
| 5. Training | Very Large | 12-15 days | EWC module investigation | **High** |
| 6. End-to-End | Very Large | 8-10 days | All of above | High |
| **Total** | | **40-55 days** | | |

### Recommended Phasing

**Phase 1 (Weeks 1-2):** Scenarios 7 + 1 + 2
- Regression baseline + confidence convergence + co-access ranking
- Validates core intelligence loop; fast feedback

**Phase 2 (Weeks 3-4):** Scenarios 4 + 3
- Contradiction resistance + coherence maintenance
- Validates defense and health monitoring

**Phase 3 (Weeks 5-7):** Scenarios 5 + 6
- Training convergence + end-to-end
- Requires investigation of training pipeline first
- Highest risk, highest value

---

## Critical Unknowns (Must Investigate Before Phase 3)

1. **Is TrainingService active?** Where is it initialized? Does it run?
2. **Training trigger mechanism**: Periodic? Per-feedback? Manual?
3. **Adaptation vs Training**: Are MicroLoRA (AdaptationService) and EWC (TrainingService) separate systems or integrated?
4. **Signal persistence**: Are training signals persisted to DB or lost on restart?
5. **Maintenance implementation**: Is `maintain: true` fully wired in context_status?

---

## Key Metrics the Harness Will Track

| Metric | Scenario | What It Proves |
|--------|---------|---------------|
| Confidence trajectory over votes | 1 | Formula produces expected convergence curve |
| Wilson score accuracy at vote boundaries | 1 | 5-vote guard and CI calculation correct |
| MRR before/after co-access | 2 | Co-access actually improves search |
| Boost distribution | 2 | Boost capped, log-transform behaves |
| Lambda trajectory (degrade → maintain → recover) | 3 | Health metric is actionable |
| Dimension contribution breakdown | 3 | Weights and re-normalization correct |
| Contradiction precision/recall/F1 | 4 | Detection catches real conflicts |
| False positive rate | 4 | Not quarantining clean entries |
| MRR before/after training | 5 | Model learns without forgetting |
| EWC penalty magnitude | 5 | Catastrophic forgetting prevention active |
| End-to-end search quality trend | 6 | System improves with use |

---

## References

- Confidence: `crates/unimatrix-engine/src/confidence.rs`
- Co-access: `crates/unimatrix-server/src/infra/usage_dedup.rs`
- Coherence: `crates/unimatrix-server/src/infra/coherence.rs`
- Contradiction: `crates/unimatrix-server/src/infra/contradiction.rs`
- Training: `crates/unimatrix-learn/`
- Existing integration tests: `product/test/infra-001/suites/test_confidence.py`, `test_contradiction.py`, `test_adaptation.py`
