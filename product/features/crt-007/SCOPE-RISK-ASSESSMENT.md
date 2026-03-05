# crt-007: Scope Risk Assessment

## Summary

Assessment of scope-level risks for the Neural Extraction Pipeline feature. These risks are identified from SCOPE.md and PRODUCT-VISION.md before architecture begins, to inform architectural decisions.

---

## SR-01: ruv-fann Maturity and RPROP Sufficiency

**Category**: Dependency
**Severity**: HIGH
**Likelihood**: MEDIUM

**Description**: ruv-fann is v0.2.0 with ~4K downloads. Limited production validation means undiscovered bugs in RPROP implementation, model serialization, or numerical stability could surface during integration. RPROP may be insufficient for the planned model architectures (softmax output, sigmoid activations).

**Impact**: If ruv-fann fails, the team must vendor-and-fix or fall back to ndarray hand-rolled training. Either path is bounded (~1-2 days for two small models) but delays delivery and changes the architecture.

**Mitigation**:
- ruv-fann integration is Phase 2; shared infrastructure (Phase 1) is framework-agnostic
- Clear exit ramps defined: (a) vendor and fix, (b) ndarray fallback
- Validate ruv-fann RPROP + serialization on a toy MLP before wiring into pipeline
- `unimatrix-learn` abstracts over the ML backend via traits so fallback doesn't cascade

**Architect attention**: Design `unimatrix-learn` model traits to be ML-framework-agnostic. The `ModelRegistry` and `ShadowEvaluator` must not depend on ruv-fann types.

---

## SR-02: unimatrix-adapt Refactoring Regression

**Category**: Integration
**Severity**: HIGH
**Likelihood**: LOW

**Description**: Extracting `TrainingReservoir`, `EwcState`, and persistence helpers from a working crate risks breaking existing MicroLoRA adaptation. The refactoring moves ~200 lines while preserving the public API, but subtle coupling (import paths, type bounds, serde compatibility) could cause regressions.

**Impact**: Broken MicroLoRA adaptation degrades embedding quality for all search/briefing operations. This is a regression in a shipped, working feature (crt-006).

**Mitigation**:
- All 174+ existing unimatrix-adapt tests must pass after refactoring (AC-03)
- Extract-then-redirect pattern: create shared types in unimatrix-learn, then update unimatrix-adapt imports. No logic changes.
- Persistence format compatibility: new `unimatrix-learn` persistence helpers must load existing `adaptation.state` files
- Run full workspace test suite (`cargo test --workspace`) before declaring Phase 1 complete

**Architect attention**: Ensure serde-compatible persistence format. The `AdaptationState` struct in persistence.rs uses bincode v2; extracted helpers must preserve this exact encoding.

---

## SR-03: SignalDigest Feature Vector Stability

**Category**: Design
**Severity**: MEDIUM
**Likelihood**: LOW (mitigated by resolved question 5)

**Description**: The fixed-width `[f32; 32]` SignalDigest format creates a contract between signal producers (extraction rules) and consumers (neural models). If feature semantics shift (e.g., slot 3 changes from `consistency_score` to something else), models trained on old semantics produce garbage predictions.

**Impact**: Silently incorrect classifications that bypass quality gates. Worse than no classification because the system trusts neural predictions.

**Mitigation**:
- Schema version field in SignalDigest metadata
- ModelRegistry detects schema version mismatch and demotes old model, cold-starts new one
- Feature slots are append-only; existing slots never reordered or repurposed
- Reserved slots initialized to zero (neutral to learned weights)

**Architect attention**: Define a versioned SignalDigest schema. Document slot assignments in a canonical registry (e.g., a const array or enum in unimatrix-learn).

---

## SR-04: Shadow Mode Accuracy Measurement Validity

**Category**: Validation
**Severity**: MEDIUM
**Likelihood**: MEDIUM

**Description**: Shadow mode compares neural predictions against rule-only extraction as ground truth. But rule-only extraction is itself imperfect -- rules have false negatives (missed patterns) and false positives (noise classified as convention). Using an imperfect baseline to validate neural models means accuracy metrics are relative, not absolute.

**Impact**: A model that learns to mimic rule failures gets promoted. The neural layer adds complexity without improving extraction quality.

**Mitigation**:
- Track divergence rate (where neural disagrees with rules) alongside accuracy
- Log both rule and neural predictions for human review during early features
- Promotion requires minimum 20 evaluations AND no per-category regression -- not just aggregate accuracy
- Post-promotion rolling accuracy monitor catches systematic errors over time
- crt-008 (Continuous Self-Retraining) introduces utilization feedback as an independent quality signal

**Architect attention**: Shadow evaluation schema should capture both rule prediction and neural prediction side-by-side, not just agreement/disagreement.

---

## SR-05: col-013 Integration Surface

**Category**: Integration
**Severity**: MEDIUM
**Likelihood**: MEDIUM

**Description**: crt-007 depends on col-013's extraction pipeline being complete and exposing stable integration points (signal digests from rules, quality gate hooks). col-013 is currently in implementation on the `feature/col-013` branch. If the col-013 API surface changes during implementation, crt-007 architecture may need revision.

**Impact**: Rework of crt-007 architecture and specification if col-013 integration points shift.

**Mitigation**:
- Design crt-007 around the col-013 SCOPE.md and ARCHITECTURE.md interfaces (which are approved)
- Neural models consume `SignalDigest` (a crt-007-owned type), not col-013 internal types directly
- Integration is a thin adapter layer: col-013 produces ProposedEntry, crt-007 converts relevant fields to SignalDigest
- If col-013 changes, only the adapter layer needs updating

**Architect attention**: Define a clean boundary type (`SignalDigest`) owned by unimatrix-learn. The extraction pipeline fills it; models consume it. No direct dependency on col-013 internal types.

---

## SR-06: Model File Management and Disk Footprint

**Category**: Operational
**Severity**: LOW
**Likelihood**: LOW

**Description**: Model versioning (production/shadow/previous) for 2 models means up to 6 model files (~42MB total at max). With crt-008/009 expanding to 5 models with versioning, this could reach ~500MB. The `~/.unimatrix/{project_hash}/models/` directory grows monotonically unless pruned.

**Impact**: Disk usage growth, slower server startup if loading multiple model versions.

**Mitigation**:
- ModelRegistry prunes versions beyond `previous` on promotion (keep only production + previous + active shadow)
- Lazy loading: only production model loaded at startup; shadow loaded on demand
- Model files are small (~7MB total for crt-007's two models)
- Explicit cleanup during maintenance tick

**Architect attention**: ModelRegistry should implement a retention policy. Default: keep production, previous, active shadow. Archived versions deleted.

---

## SR-07: Conservative Bias Effectiveness

**Category**: Validation
**Severity**: LOW
**Likelihood**: MEDIUM

**Description**: Hand-tuned baseline weights biased toward `noise` (classifier) and low scores (scorer) may be so conservative that models never produce useful predictions until retrained (crt-008). If the bias is too strong, shadow mode never accumulates enough positive predictions to validate promotion. If too weak, conservative intent is defeated.

**Impact**: Models sit in shadow mode indefinitely (too conservative) or produce noisy predictions from cold start (too permissive).

**Mitigation**:
- Calibrate bias experimentally: set classifier noise-class bias to ~0.6 (not 0.99), allowing non-trivial probability mass on other classes for strong signals
- Convention scorer bias toward 0.3 (not 0.0), so genuine conventions can reach the 0.6 promotion threshold
- Document baseline weight rationale in model initialization code
- Shadow mode logs prediction distributions for tuning during early features

**Architect attention**: Baseline weights should be configurable, not hardcoded. Include bias calibration as part of the integration test suite.

---

## Risk Summary

| ID | Risk | Severity | Likelihood | Mitigation Status |
|----|------|----------|------------|-------------------|
| SR-01 | ruv-fann maturity | HIGH | MEDIUM | Exit ramps defined, framework-agnostic traits |
| SR-02 | unimatrix-adapt refactoring regression | HIGH | LOW | Full test coverage, extract-then-redirect pattern |
| SR-03 | SignalDigest feature vector stability | MEDIUM | LOW | Fixed-width schema, version detection, append-only |
| SR-04 | Shadow mode accuracy measurement validity | MEDIUM | MEDIUM | Divergence tracking, human review logs, post-promotion monitoring |
| SR-05 | col-013 integration surface | MEDIUM | MEDIUM | Clean boundary type, thin adapter layer |
| SR-06 | Model file management | LOW | LOW | Retention policy, lazy loading |
| SR-07 | Conservative bias effectiveness | LOW | MEDIUM | Configurable bias, calibration tests |

**Top 3 risks for architect attention:**
1. **SR-01** (ruv-fann maturity) -- drives ML-framework-agnostic trait design
2. **SR-02** (adapt refactoring) -- drives serde-compatible persistence, extract-then-redirect pattern
3. **SR-05** (col-013 integration surface) -- drives clean boundary type ownership
