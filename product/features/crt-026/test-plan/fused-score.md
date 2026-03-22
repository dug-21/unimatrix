# crt-026: Test Plan — Component 5: FusedScoreInputs / FusionWeights / compute_fused_score

**File under test**: `crates/unimatrix-server/src/services/search.rs`
**Test module**: `#[cfg(test)] mod tests` at bottom of `search.rs`

---

## AC Coverage

| AC-ID | Test |
|-------|------|
| AC-06 | Transitively via AC-12 |
| AC-08 | `test_cold_start_search_produces_identical_scores` (gate blocker) |
| AC-09 | `test_phase_explicit_norm_placeholder_fields_present` |
| AC-10 | `test_status_penalty_applied_after_histogram_boost` |
| AC-12 | `test_histogram_boost_score_delta_at_p1_equals_weight` (gate blocker) |
| AC-13 | `test_absent_category_phase_histogram_norm_is_zero` (gate blocker) |

Risk coverage: R-01, R-02, R-06, R-07, R-08, R-09.

---

## Test Helper

The existing `make_test_entry` helper in `search.rs` tests is reused. A new helper for
constructing `FusedScoreInputs` with all six existing fields at known values is useful:

```rust
fn make_baseline_inputs(sim: f64, category_matches: bool) -> FusedScoreInputs {
    FusedScoreInputs {
        similarity: sim,
        nli_entailment: 0.0,
        confidence: 0.5,
        coac_norm: 0.0,
        util_norm: 0.5,   // neutral
        prov_norm: 0.0,
        phase_histogram_norm: if category_matches { 1.0 } else { 0.0 },
        phase_explicit_norm: 0.0,  // ADR-003: always 0.0 in crt-026
    }
}

fn make_baseline_weights() -> FusionWeights {
    FusionWeights {
        w_sim: 0.25,
        w_nli: 0.35,
        w_conf: 0.15,
        w_coac: 0.10,
        w_util: 0.05,
        w_prov: 0.05,
        w_phase_histogram: 0.02,
        w_phase_explicit: 0.0,  // ADR-003
    }
}
```

---

## Tests

### T-FS-01: `test_histogram_boost_score_delta_at_p1_equals_weight` **(GATE BLOCKER)**
**AC-12 | R-01 scenario 1**

**Arrange**:
```rust
// histogram = {"decision": 5}, total = 5, p("decision") = 1.0
// Entry A: category="decision" → phase_histogram_norm = 5/5 = 1.0
// Entry B: category="lesson-learned" → phase_histogram_norm = 0/5 = 0.0
// All other inputs identical.
let weights = make_baseline_weights(); // w_phase_histogram = 0.02
let inputs_a = make_baseline_inputs(0.70, true);   // phase_histogram_norm = 1.0
let inputs_b = make_baseline_inputs(0.70, false);  // phase_histogram_norm = 0.0
```

**Act**:
```rust
let score_a = compute_fused_score(&inputs_a, &weights);
let score_b = compute_fused_score(&inputs_b, &weights);
let delta = score_a - score_b;
```

**Assert**:
```rust
assert!(
    delta >= 0.02,
    "score delta at p=1.0 must be >= 0.02 (w_phase_histogram * 1.0); \
     got delta={delta:.6}"
);
assert!(
    (delta - 0.02).abs() < 1e-10,
    "score delta at p=1.0 must be exactly 0.02 with default weights; \
     got delta={delta:.6}"
);
```

**Notes**: Both assertions are required. The `>= 0.02` assertion is the gate-blocking
floor (AC-12). The `== 0.02` assertion documents that the formula adds exactly
`w_phase_histogram * 1.0` — not a range but a precise computation.

**Module**: `services/search.rs` `#[cfg(test)] mod tests`

---

### T-FS-02: `test_60_percent_concentration_score_delta`
**AC-12 partial | R-01 scenario 2**

**Arrange**:
```rust
// histogram = {"decision": 3, "pattern": 2}, total = 5
// p("decision") = 0.6
let weights = make_baseline_weights();
let mut inputs_decision = make_baseline_inputs(0.70, false);
inputs_decision.phase_histogram_norm = 0.6;

let inputs_other = make_baseline_inputs(0.70, false); // phase_histogram_norm = 0.0
```

**Act**:
```rust
let score_decision = compute_fused_score(&inputs_decision, &weights);
let score_other = compute_fused_score(&inputs_other, &weights);
let delta = score_decision - score_other;
```

**Assert**:
```rust
assert!(
    (delta - 0.012).abs() < 1e-10,
    "60% concentration must produce delta = 0.02 * 0.6 = 0.012; \
     got delta={delta:.6}"
);
```

**Module**: `services/search.rs` `#[cfg(test)] mod tests`

---

### T-FS-03: `test_absent_category_phase_histogram_norm_is_zero` **(GATE BLOCKER)**
**AC-13 | R-01 scenario 3, R-13**

**Arrange**:
```rust
// histogram = {"decision": 5}, total = 5
// Entry has category = "lesson-learned" (not in histogram)
use std::collections::HashMap;
let mut histogram: HashMap<String, u32> = HashMap::new();
histogram.insert("decision".to_string(), 5);
let total: u32 = histogram.values().sum(); // 5

let entry_category = "lesson-learned";
```

**Act** (simulating the scoring loop's `phase_histogram_norm` computation):
```rust
let phase_histogram_norm = if total > 0 {
    histogram.get(entry_category).copied().unwrap_or(0) as f64 / total as f64
} else {
    0.0
};
```

**Assert**:
```rust
assert_eq!(
    phase_histogram_norm, 0.0,
    "absent category must produce phase_histogram_norm = 0.0; \
     got {phase_histogram_norm}"
);
```

**Module**: `services/search.rs` `#[cfg(test)] mod tests`

**Notes**: The scoring loop formula must produce exactly `0.0` for an absent category,
not a near-zero float. The `unwrap_or(0) as f64 / total as f64` with `total=5` and
`count=0` produces `0.0 / 5.0 = 0.0` exactly.

---

### T-FS-04: `test_cold_start_search_produces_identical_scores` **(GATE BLOCKER)**
**AC-08 | R-02**

**Arrange**:
```rust
// Pre-crt-026 baseline: six-term fused score only
// crt-026 cold start: same six terms + phase_histogram_norm = 0.0 + phase_explicit_norm = 0.0
let weights = make_baseline_weights(); // includes w_phase_histogram = 0.02

let pre_crt026_inputs = FusedScoreInputs {
    similarity: 0.75,
    nli_entailment: 0.40,
    confidence: 0.60,
    coac_norm: 0.20,
    util_norm: 0.50,
    prov_norm: 0.0,
    phase_histogram_norm: 0.0,  // cold start
    phase_explicit_norm: 0.0,   // always 0.0 (ADR-003)
};

// Expected: identical to six-term-only formula
// 0.25*0.75 + 0.35*0.40 + 0.15*0.60 + 0.10*0.20 + 0.05*0.50 + 0.05*0.0
// = 0.1875 + 0.14 + 0.09 + 0.02 + 0.025 + 0.0 = 0.4625
let expected = 0.25 * 0.75 + 0.35 * 0.40 + 0.15 * 0.60 + 0.10 * 0.20
             + 0.05 * 0.50 + 0.05 * 0.0;
```

**Act**:
```rust
let actual = compute_fused_score(&pre_crt026_inputs, &weights);
```

**Assert**:
```rust
assert!(
    (actual - expected).abs() < f64::EPSILON,
    "cold-start score must be bit-for-bit identical to pre-crt-026 six-term formula; \
     expected={expected:.10}, actual={actual:.10}"
);
```

**Notes**: The zero terms (`w_phase_histogram * 0.0` and `w_phase_explicit * 0.0`) must
contribute exactly zero to the sum. No floating-point drift from the additions. This test
verifies the NFR-02 cold-start safety guarantee at the pure function level.

**Module**: `services/search.rs` `#[cfg(test)] mod tests`

---

### T-FS-05: `test_status_penalty_applied_after_histogram_boost`
**AC-10 | R-08**

**Arrange**:
```rust
// Entry: category="decision" matches histogram (p=1.0), status=Deprecated (penalty=0.5)
let weights = make_baseline_weights();
let inputs = FusedScoreInputs {
    similarity: 0.70,
    nli_entailment: 0.0,
    confidence: 0.5,
    coac_norm: 0.0,
    util_norm: 0.5,
    prov_norm: 0.0,
    phase_histogram_norm: 1.0,  // p=1.0: histogram matches
    phase_explicit_norm: 0.0,
};
let status_penalty = 0.5_f64;
```

**Act**:
```rust
// Correct order: (fused + boost) * penalty  — boost is INSIDE compute_fused_score
let fused = compute_fused_score(&inputs, &weights);
let final_score = fused * status_penalty;

// Wrong order (must be different): base_without_boost * penalty + boost
let base_inputs_no_boost = FusedScoreInputs {
    phase_histogram_norm: 0.0,
    ..inputs
};
let fused_no_boost = compute_fused_score(&base_inputs_no_boost, &weights);
let wrong_score = fused_no_boost * status_penalty + weights.w_phase_histogram * 1.0;
```

**Assert**:
```rust
// Correct formula: (base + 0.02) * 0.5
// Wrong formula:   base * 0.5 + 0.02
// These differ by: 0.02 * 0.5 = 0.01 (boost reduction under penalty = 0.02 - 0.01 = 0.01)
assert!(
    (final_score - wrong_score).abs() > 1e-6,
    "correct and wrong penalty-ordering formulas must produce different results; \
     correct={final_score:.6}, wrong={wrong_score:.6}"
);
assert!(
    final_score < wrong_score,
    "correct ordering ((base+boost)*penalty) must be less than \
     wrong ordering (base*penalty+boost) for penalty < 1.0; \
     correct={final_score:.6}, wrong={wrong_score:.6}"
);
// Verify exact formula
let expected = fused_no_boost * status_penalty + weights.w_phase_histogram * 1.0 * status_penalty;
// Rephrase: expected = (fused_no_boost + 0.02) * 0.5
let expected2 = (fused_no_boost + 0.02) * status_penalty;
assert!(
    (final_score - expected2).abs() < f64::EPSILON,
    "final_score must equal (fused_without_boost + w_phase_histogram) * status_penalty; \
     got final_score={final_score:.10}, expected={expected2:.10}"
);
```

**Notes**: This test verifies the C-06 application order invariant. The numerical difference
between correct and wrong ordering is `0.02 * (1.0 - 0.5) = 0.01` — easily detectable.

**Module**: `services/search.rs` `#[cfg(test)] mod tests`

---

### T-FS-06: `test_phase_histogram_norm_zero_when_total_is_zero`
**R-09 (division by zero guard)**

**Arrange/Act** (simulating the scoring loop inline):
```rust
use std::collections::HashMap;
let histogram: Option<HashMap<String, u32>> = Some(HashMap::new());
// The handler maps empty → None; this tests the inner guard if it exists
let total: u32 = histogram.as_ref()
    .map(|h| h.values().sum())
    .unwrap_or(0);

let phase_histogram_norm = if total > 0 {
    histogram.as_ref()
        .and_then(|h| h.get("decision"))
        .copied()
        .unwrap_or(0) as f64 / total as f64
} else {
    0.0
};
```

**Assert**:
```rust
assert_eq!(total, 0);
assert_eq!(phase_histogram_norm, 0.0,
    "total=0 must produce phase_histogram_norm=0.0, not NaN or panic");
assert!(!phase_histogram_norm.is_nan(),
    "phase_histogram_norm must not be NaN");
```

**Notes**: The primary R-09 guard is in the handler (`is_empty() → None`). This test
documents the secondary in-function guard (`if total > 0`), ensuring NaN cannot propagate
even if `Some(empty_map)` reaches the scoring loop.

**Module**: `services/search.rs` `#[cfg(test)] mod tests`

---

### T-FS-07: `test_phase_explicit_norm_placeholder_fields_present`
**AC-09 | R-07 (ADR-003 placeholder)**

**Arrange/Act**:
```rust
// Verify FusedScoreInputs has both new fields (ADR-003 placeholder)
let inputs = FusedScoreInputs {
    similarity: 0.5,
    nli_entailment: 0.0,
    confidence: 0.5,
    coac_norm: 0.0,
    util_norm: 0.5,
    prov_norm: 0.0,
    phase_histogram_norm: 0.3,
    phase_explicit_norm: 0.0,  // ADR-003: always 0.0 in crt-026
};

// Verify FusionWeights has both new fields
let weights = FusionWeights {
    w_sim: 0.25,
    w_nli: 0.35,
    w_conf: 0.15,
    w_coac: 0.10,
    w_util: 0.05,
    w_prov: 0.05,
    w_phase_histogram: 0.02,
    w_phase_explicit: 0.0,  // ADR-003: always 0.0 in crt-026
};
```

**Assert**:
```rust
// Fields exist (compile-time) and have correct values
assert_eq!(inputs.phase_explicit_norm, 0.0,
    "phase_explicit_norm must be 0.0 in crt-026 (ADR-003 placeholder)");
assert_eq!(weights.w_phase_explicit, 0.0,
    "w_phase_explicit must be 0.0 in crt-026 (ADR-003 placeholder)");
assert_eq!(weights.w_phase_histogram, 0.02,
    "w_phase_histogram default must be 0.02");

// A non-zero w_phase_explicit with phase_explicit_norm=0.0 contributes nothing
let score_with_explicit = compute_fused_score(&inputs,
    &FusionWeights { w_phase_explicit: 0.99, ..weights });
let score_without_explicit = compute_fused_score(&inputs, &weights);
assert!(
    (score_with_explicit - score_without_explicit).abs() < f64::EPSILON,
    "phase_explicit_norm=0.0 must contribute 0.0 regardless of w_phase_explicit; \
     score_with={score_with_explicit:.10}, score_without={score_without_explicit:.10}"
);
```

**Notes**: Includes a comment in the test body citing ADR-003. This discourages future
removal of the placeholder fields as "dead code." The W3-1 field population path will
make this test fail (expected) — remove the assertion at that point.

**Module**: `services/search.rs` `#[cfg(test)] mod tests`

---

### T-FS-08: `test_fusion_weights_effective_nli_absent_excludes_phase_from_denominator` **(GATE BLOCKER)**
**R-06 | AC-08**

**Arrange**:
```rust
let weights = FusionWeights {
    w_sim: 0.25,
    w_nli: 0.35,
    w_conf: 0.15,
    w_coac: 0.10,
    w_util: 0.05,
    w_prov: 0.05,
    w_phase_histogram: 0.02,
    w_phase_explicit: 0.0,
};
```

**Act**:
```rust
let effective_nli_absent = weights.effective(false);
```

**Assert**:
```rust
// w_nli must be zeroed out
assert_eq!(effective_nli_absent.w_nli, 0.0,
    "w_nli must be 0.0 in NLI-absent mode");

// denominator = w_sim + w_conf + w_coac + w_util + w_prov = 0.60 (five terms)
// NOT including w_phase_histogram or w_phase_explicit
let expected_denom = 0.25 + 0.15 + 0.10 + 0.05 + 0.05;  // 0.60

assert!(
    (effective_nli_absent.w_sim - 0.25 / expected_denom).abs() < f64::EPSILON,
    "w_sim must be re-normalized by five-term denominator; \
     expected={}, got={}",
    0.25 / expected_denom, effective_nli_absent.w_sim
);

// w_phase_histogram must be passed through UNCHANGED (not re-normalized)
assert_eq!(
    effective_nli_absent.w_phase_histogram, 0.02,
    "w_phase_histogram must be 0.02 unchanged in NLI-absent mode (not in denominator); \
     got={}", effective_nli_absent.w_phase_histogram
);

// w_phase_explicit must be passed through unchanged
assert_eq!(
    effective_nli_absent.w_phase_explicit, 0.0,
    "w_phase_explicit must be 0.0 unchanged in NLI-absent mode"
);
```

**Notes**: This is the R-06 invariant test. Including `w_phase_histogram` in the
denominator would silently dilute existing weights. The five-term denominator is
`0.25 + 0.15 + 0.10 + 0.05 + 0.05 = 0.60`. If `w_phase_histogram` were incorrectly
included, the denominator would be `0.62` and all re-normalized weights would be wrong.

**Module**: `services/search.rs` `#[cfg(test)] mod tests`

---

### T-FS-09: `test_fusion_weights_effective_nli_active_phase_fields_pass_through`
**R-06 (NLI-active path)**

**Arrange/Act**:
```rust
let weights = FusionWeights {
    w_sim: 0.25, w_nli: 0.35, w_conf: 0.15, w_coac: 0.10,
    w_util: 0.05, w_prov: 0.05, w_phase_histogram: 0.02, w_phase_explicit: 0.0,
};
let effective_nli_active = weights.effective(true);
```

**Assert**:
```rust
// NLI-active: all fields returned unchanged
assert_eq!(effective_nli_active.w_phase_histogram, 0.02);
assert_eq!(effective_nli_active.w_phase_explicit, 0.0);
assert_eq!(effective_nli_active.w_nli, 0.35);
```

**Notes**: Companion to T-FS-08. Confirms both paths of `effective()` pass the phase
fields through correctly.

**Module**: `services/search.rs` `#[cfg(test)] mod tests`

---

## Code-Review Assertions (not automated)

**AC-14 / R-14**: `grep "WA-2 extension" crates/unimatrix-server/src/services/search.rs`
must return zero matches after implementation. The stubs at lines 55, 89, 179 must be
replaced with field declarations and doc-comments citing crt-026 and ADR-003.
