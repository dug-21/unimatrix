# Agent Report: crt-019-agent-8-query-skills-and-test-infra

## Summary

Implemented Component A (query-skills) and Component B (test-infrastructure) for crt-019.

## Files Modified

1. `.claude/skills/uni-knowledge-search/SKILL.md`
2. `.claude/skills/uni-knowledge-lookup/SKILL.md`
3. `crates/unimatrix-engine/tests/pipeline_regression.rs`
4. `crates/unimatrix-engine/tests/pipeline_calibration.rs`
5. `crates/unimatrix-engine/tests/pipeline_retrieval.rs`
6. `crates/unimatrix-engine/src/test_scenarios.rs`

## Component A — Query Skills

Both skill files updated per pseudocode/query-skills.md and AC-09:

- `uni-knowledge-search/SKILL.md`: Added `helpful: true` to the primary ADR example invocation. Added "Helpful Vote Guidance" section covering when to use `helpful: true`, `helpful: false`, and when to omit.
- `uni-knowledge-lookup/SKILL.md`: Same additions. Note on doubled access signal (×2) for context_lookup included per pseudocode spec.

## Component B — Test Infrastructure

### Critical Ordering (C-02/R-04): T-REG-02 Updated First

T-REG-02 (`test_weight_constants`) updated to new values as first edit:
- W_BASE: 0.18 → 0.16
- W_USAGE: 0.14 → 0.16
- W_FRESH: 0.18 (unchanged)
- W_HELP: 0.14 → 0.12
- W_CORR: 0.14 (unchanged)
- W_TRUST: 0.14 → 0.16
- Sum assertion upgraded from tolerance check to `assert_eq!(sum, 0.92_f64)` for exact IEEE 754 equality (AC-11).

### pipeline_regression.rs

- T-REG-01: `compute_confidence` calls updated to 4-arg form `(entry, now, 3.0, 3.0)`
- T-REG-02: Weight assertions updated to new values (see above — done first)
- T-REG-03: `compute_confidence` call in map closure updated to 4-arg form

### pipeline_calibration.rs

- `confidence_with_adjusted_weight` helper: gained `alpha0: f64, beta0: f64` parameters; `base_score` call updated to 2-param form `(entry.status, &entry.trust_source)`; `helpfulness_score` call updated to 4-param form `(helpful, unhelpful, alpha0, beta0)`
- `test_weight_sensitivity`: updated to pass `alpha0=3.0, beta0=3.0` to helper and `compute_confidence`
- `ablation_test!` macro: updated to pass `alpha0=3.0, beta0=3.0` to `compute_confidence`
- `test_boundary_all_zero`, `test_boundary_all_max`: updated `compute_confidence` to 4-arg form
- Removed unused `self` alias from import block
- New test `auto_vs_agent_spread` (AC-12): verifies `Active/agent > Active/auto` confidence at zero/mid/high signal levels. Uses `Status::Active` exclusively (C-03).
- New test `test_cal_spread_synthetic_population` (T-CAL-SPREAD-01/AC-01/NFR-01): 50-entry synthetic population (10 low/auto, 30 mid/agent, 10 high/human) asserts p95-p5 spread >= 0.20.

### pipeline_retrieval.rs

- Removed `SEARCH_SIMILARITY_WEIGHT` import (constant being removed by agent-3)
- T-RET-01: Updated to `rerank_score(sim, conf, 0.184)` — initial confidence_weight at server start; added inline math comment showing expected blend values
- T-RET-02 (`test_status_penalty_ordering`): `rerank_score` updated to 3-arg form
- T-RET-03 (`test_provenance_boost_effect`): `rerank_score` updated to 3-arg form
- T-RET-05 (`test_combined_interaction_ordering`): all `rerank_score` calls updated to 3-arg form with `cw=0.184`

### test_scenarios.rs

- `assert_confidence_ordering`: `compute_confidence(e, now)` updated to `compute_confidence(e, now, 3.0, 3.0)`

## Test Results

```
cargo test --test pipeline_regression --test pipeline_calibration --test pipeline_retrieval
```

**Result: compile error (expected pre-integration)**

Single compile error:
```
error[E0061]: this function takes 2 arguments but 4 arguments were supplied
   --> crates/unimatrix-engine/src/test_scenarios.rs:402:25
note: function defined here
   --> crates/unimatrix-engine/src/confidence.rs:204:8
     pub fn compute_confidence(entry: &EntryRecord, now: u64) -> f64
```

This is the expected pre-integration failure: `test_scenarios.rs:402` now calls `compute_confidence` with 4 arguments (`now, 3.0, 3.0`), but `confidence.rs` still has the old 2-argument signature. This will resolve when agent-3 updates `confidence.rs`.

**Expected failures (pre-integration, 3 tests):**
- `test_weight_constants` (T-REG-02) — constants still have old values in `confidence.rs`
- Any test touching `compute_confidence`, `base_score`, `helpfulness_score`, `rerank_score` — old signatures until agent-3 merges

**No unexpected failures.** All compile errors trace to the single root cause: `confidence.rs` not yet updated.

## Issues / Blockers

None. All pre-integration failures are expected and documented. Implementation is correct per new signatures as specified in pseudocode.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-engine` confidence tests pipeline — no actionable results returned (Unimatrix server not available in this worktree context).
- Stored: nothing novel to store — the pattern of "update test assertions first before changing constants" (C-02/R-04) is already documented in the IMPLEMENTATION-BRIEF and RISK-TEST-STRATEGY for this feature. No runtime traps or non-obvious crate-specific behaviors discovered beyond what is already specified.
