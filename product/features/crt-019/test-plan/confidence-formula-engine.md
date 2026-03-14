# Test Plan: confidence-formula-engine
## Component: `crates/unimatrix-engine/src/confidence.rs`

### Risk Coverage

| Risk | Severity | Test(s) |
|------|----------|---------|
| R-01 | Critical | Unit test for `helpfulness_score` Bayesian signature; integration test for closure flow |
| R-02 | High | `rerank_score` 3-param unit tests; SEARCH_SIMILARITY_WEIGHT removal check |
| R-03 | High | `weight_sum_invariant_f64` (exact equality) |
| R-10 | High | `auto_proposed_base_score_unchanged` |
| R-12 | High | NaN guard for `helpfulness_score` with degenerate inputs |
| R-14 | Medium | `bayesian_helpfulness_balanced_votes_exact_half` |
| R-17 | Low | Compile verification: removed constants cause compile errors at old call sites |

---

## Unit Tests

### T-01: Weight Sum Invariant (updated — AC-03, AC-11)

**Function**: `weight_sum_invariant_f64` (existing test, updated assertion)

The existing test uses `assert_eq!` which is correct. After updating weight constants, this test
must continue to use `assert_eq!` (not tolerance). Verify the new vector satisfies IEEE 754
binary64 exact equality.

```rust
#[test]
fn weight_sum_invariant_f64() {
    let stored_sum = W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST;
    assert_eq!(stored_sum, 0.92_f64, "stored weight sum should be 0.92");
}
```

Expected result: `0.16 + 0.16 + 0.18 + 0.12 + 0.14 + 0.16 == 0.92_f64` (exact in IEEE 754 binary64).

Also add a standalone assertion verifying individual constants:

```rust
#[test]
fn weight_constants_values() {
    assert_eq!(W_BASE,  0.16_f64, "W_BASE");
    assert_eq!(W_USAGE, 0.16_f64, "W_USAGE");
    assert_eq!(W_FRESH, 0.18_f64, "W_FRESH");
    assert_eq!(W_HELP,  0.12_f64, "W_HELP");
    assert_eq!(W_CORR,  0.14_f64, "W_CORR");
    assert_eq!(W_TRUST, 0.16_f64, "W_TRUST");
}
```

---

### T-02: base_score Two-Parameter Signature (AC-05)

**Function**: `base_score(status: Status, trust_source: &str) -> f64`

Replace the four single-parameter `base_score_*` tests with expanded coverage.

```rust
// Active + non-auto trust sources all return 0.5
#[test]
fn base_score_active_agent() {
    assert_eq!(base_score(Status::Active, "agent"), 0.5);
}
#[test]
fn base_score_active_human() {
    assert_eq!(base_score(Status::Active, "human"), 0.5);
}
#[test]
fn base_score_active_system() {
    assert_eq!(base_score(Status::Active, "system"), 0.5);
}

// Active + auto returns 0.35 (differentiation applies only to Active)
#[test]
fn base_score_active_auto() {
    assert_eq!(base_score(Status::Active, "auto"), 0.35);
}

// R-10: Proposed + auto must still return 0.5 — ADR-003 constraint
#[test]
fn auto_proposed_base_score_unchanged() {
    assert_eq!(base_score(Status::Proposed, "auto"), 0.5,
        "Proposed/auto must retain 0.5 to preserve T-REG-01 ordering");
}

// All non-Active statuses retain existing values regardless of trust_source
#[test]
fn base_score_deprecated_any_trust() {
    assert_eq!(base_score(Status::Deprecated, "auto"), 0.2);
    assert_eq!(base_score(Status::Deprecated, "human"), 0.2);
}
#[test]
fn base_score_quarantined_any_trust() {
    assert_eq!(base_score(Status::Quarantined, "auto"), 0.1);
    assert_eq!(base_score(Status::Quarantined, "human"), 0.1);
}

// Active/auto strictly less than Active/agent (drives AC-12)
#[test]
fn base_score_auto_less_than_agent_for_active() {
    assert!(base_score(Status::Active, "auto") < base_score(Status::Active, "agent"));
}
```

---

### T-05: Bayesian Helpfulness Score (AC-02 — replaces Wilson tests)

**Function**: `helpfulness_score(helpful: u32, unhelpful: u32, alpha0: f64, beta0: f64) -> f64`

Remove all existing Wilson score tests (T-05 group) and the `wilson_reference_*` tests (T-06).
Replace with Bayesian assertions. All assertions from AC-02 must be exact (`assert_eq!`):

```rust
// AC-02 exact assertions — cold-start prior alpha0=3, beta0=3
#[test]
fn bayesian_helpfulness_cold_start_neutral() {
    // (0 + 3) / (0 + 3 + 3) = 3/6 = 0.5 exactly
    assert_eq!(helpfulness_score(0, 0, 3.0, 3.0), 0.5);
}

#[test]
fn bayesian_helpfulness_two_unhelpful_votes() {
    // (0 + 3) / (2 + 3 + 3) = 3/8 = 0.375 exactly
    assert_eq!(helpfulness_score(0, 2, 3.0, 3.0), 0.375);
}

#[test]
fn bayesian_helpfulness_balanced_votes_exact_half() {
    // (2 + 3) / (4 + 3 + 3) = 5/10 = 0.5 exactly
    // R-14: corrected from SCOPE which said > 0.5; SPEC AC-02 says == 0.5
    assert_eq!(helpfulness_score(2, 2, 3.0, 3.0), 0.5);
}

#[test]
fn bayesian_helpfulness_two_helpful_votes_above_neutral() {
    // (2 + 3) / (2 + 3 + 3) = 5/8 = 0.625 > 0.5
    assert!(helpfulness_score(2, 0, 3.0, 3.0) > 0.5);
}

// Immediate responsiveness: 2 unhelpful votes lower the score below neutral
// even without a 5-vote floor — confirms Wilson floor is gone
#[test]
fn bayesian_helpfulness_immediate_response_no_floor() {
    let score = helpfulness_score(0, 2, 3.0, 3.0);
    assert!(score < 0.5, "two unhelpful votes should lower score below 0.5, got {score}");
}

// All helpful with large n — should be high but < 1.0
#[test]
fn bayesian_helpfulness_all_helpful_large_n() {
    let result = helpfulness_score(100, 0, 3.0, 3.0);
    assert!(result > 0.9, "100 helpful votes should give score > 0.9, got {result}");
    assert!(result < 1.0);
}

// All unhelpful with large n — should approach 0 but clamped >= 0
#[test]
fn bayesian_helpfulness_all_unhelpful_large_n() {
    let result = helpfulness_score(0, 100, 3.0, 3.0);
    assert!(result >= 0.0);
    assert!(result < 0.1, "100 unhelpful votes should give score < 0.1, got {result}");
}

// R-12 defense-in-depth: NaN inputs must not produce NaN output
#[test]
fn bayesian_helpfulness_nan_inputs_clamped() {
    let result = helpfulness_score(0, 0, f64::NAN, f64::NAN);
    assert!(!result.is_nan(), "NaN inputs must not produce NaN output");
    assert!(result >= 0.0 && result <= 1.0);
}

// EC-03: u32 counts must be cast to f64 before arithmetic
#[test]
fn bayesian_helpfulness_u32_max_does_not_overflow() {
    // u32::MAX as f64 is representable; addition in f64 space
    let result = helpfulness_score(u32::MAX, 0, 3.0, 3.0);
    assert!(result >= 0.0 && result <= 1.0, "result out of range: {result}");
}

// Asymmetric prior test — non-default alpha0/beta0
#[test]
fn bayesian_helpfulness_asymmetric_prior() {
    // alpha0=2.0, beta0=8.0 → cold-start = 2/10 = 0.2; with 0,0 votes stays 0.2
    let score = helpfulness_score(0, 0, 2.0, 8.0);
    assert!((score - 0.2).abs() < 1e-10, "expected 0.2 with alpha=2 beta=8, got {score}");
}
```

---

### T-11: rerank_score Three-Parameter Signature (AC-06)

**Function**: `rerank_score(similarity: f64, confidence: f64, confidence_weight: f64) -> f64`

Replace the two-parameter unit tests with three-parameter versions:

```rust
#[test]
fn rerank_score_both_max() {
    assert_eq!(rerank_score(1.0, 1.0, 0.15), 1.0);
}

#[test]
fn rerank_score_both_zero() {
    assert_eq!(rerank_score(0.0, 0.0, 0.15), 0.0);
}

// Similarity-only case with various confidence_weight values
#[test]
fn rerank_score_similarity_only_floor_weight() {
    // confidence_weight=0.15, similarity_weight=0.85
    let result = rerank_score(1.0, 0.0, 0.15);
    assert!((result - 0.85).abs() < f64::EPSILON);
}

#[test]
fn rerank_score_similarity_only_full_weight() {
    // confidence_weight=0.25, similarity_weight=0.75
    let result = rerank_score(1.0, 0.0, 0.25);
    assert!((result - 0.75).abs() < f64::EPSILON);
}

// Adaptive weight produces different ordering than fixed 0.85/0.15
// R-02: This demonstrates the non-static weight produces different results
#[test]
fn rerank_score_adaptive_differs_from_fixed() {
    let fixed   = rerank_score(0.9, 0.8, 0.15); // 0.85*0.9 + 0.15*0.8 = 0.885
    let adaptive = rerank_score(0.9, 0.8, 0.25); // 0.75*0.9 + 0.25*0.8 = 0.875
    // Both valid; the point is they differ
    assert_ne!(fixed, adaptive, "adaptive weight must produce different result than fixed");
    assert!((fixed - 0.885).abs() < 1e-10, "fixed blend: {fixed}");
    assert!((adaptive - 0.875).abs() < 1e-10, "adaptive blend: {adaptive}");
}

#[test]
fn rerank_score_confidence_tiebreaker() {
    // Higher confidence wins when similarity is equal
    assert!(rerank_score(0.90, 0.80, 0.15) > rerank_score(0.90, 0.20, 0.15));
}

// f64 precision round-trip
#[test]
fn rerank_score_f64_precision() {
    let sim  = 0.123456789012345_f64;
    let conf = 0.987654321098765_f64;
    let cw   = 0.25_f64;
    let result = rerank_score(sim, conf, cw);
    let expected = (1.0 - cw) * sim + cw * conf;
    assert_eq!(result, expected);
}
```

---

### T-NEW: adaptive_confidence_weight (AC-06)

**New function**: `adaptive_confidence_weight(observed_spread: f64) -> f64`
Formula: `clamp(observed_spread * 1.25, 0.15, 0.25)`

```rust
#[test]
fn adaptive_confidence_weight_at_target_spread() {
    // 0.20 * 1.25 = 0.25 — at full activation
    assert_eq!(adaptive_confidence_weight(0.20), 0.25);
}

#[test]
fn adaptive_confidence_weight_floor() {
    // 0.10 * 1.25 = 0.125 < 0.15 — clamps to floor
    assert_eq!(adaptive_confidence_weight(0.10), 0.15);
}

#[test]
fn adaptive_confidence_weight_cap() {
    // 0.30 * 1.25 = 0.375 > 0.25 — clamps to cap
    assert_eq!(adaptive_confidence_weight(0.30), 0.25);
}

#[test]
fn adaptive_confidence_weight_initial_spread() {
    // Pre-crt-019 measured spread: 0.1471 * 1.25 = 0.18375
    // Between 0.15 and 0.25, so no clamping
    let result = adaptive_confidence_weight(0.1471);
    assert!((result - 0.18375).abs() < 1e-10, "initial spread weight: {result}");
    assert!(result > 0.15 && result < 0.25);
}

#[test]
fn adaptive_confidence_weight_zero_spread() {
    // 0.0 * 1.25 = 0.0 — clamps to floor
    assert_eq!(adaptive_confidence_weight(0.0), 0.15);
}

#[test]
fn adaptive_confidence_weight_one_spread() {
    // 1.0 * 1.25 = 1.25 — clamps to cap
    assert_eq!(adaptive_confidence_weight(1.0), 0.25);
}
```

---

### T-09: compute_confidence Updated Golden Values

**Function**: `compute_confidence(entry: &EntryRecord, now: u64, alpha0: f64, beta0: f64) -> f64`

The `compute_confidence_all_defaults` golden value assertion must be updated to match the new
formula. Key changes:
- `W_BASE` → 0.16 (was 0.18)
- `W_HELP` → 0.12 (was 0.14)
- `W_USAGE` → 0.16 (was 0.14)
- `W_TRUST` → 0.16 (was 0.14)
- `base_score` now takes `trust_source` argument
- `helpfulness_score` now takes `alpha0`, `beta0` arguments

```rust
#[test]
fn compute_confidence_all_defaults_new_formula() {
    // Status::Active, trust_source="", all counts 0, timestamps 0
    // base_score(Active, "") = 0.5 (non-auto)
    // usage_score(0) = 0.0
    // freshness_score(0, 0, now) = 0.0
    // helpfulness_score(0, 0, 3.0, 3.0) = 0.5
    // correction_score(0) = 0.5
    // trust_score("") = 0.3
    // = 0.16*0.5 + 0.16*0.0 + 0.18*0.0 + 0.12*0.5 + 0.14*0.5 + 0.16*0.3
    // = 0.08 + 0.0 + 0.0 + 0.06 + 0.07 + 0.048 = 0.258
    let entry = make_test_entry(Status::Active, 0, 0, 0, 0, 0, 0, "");
    let result = compute_confidence(&entry, 1_000_000, 3.0, 3.0);
    let expected = 0.16*0.5 + 0.16*0.0 + 0.18*0.0 + 0.12*0.5 + 0.14*0.5 + 0.16*0.3;
    assert!((result - expected).abs() < 0.001, "expected ~{expected:.4}, got {result:.4}");
}

// Verify auto-active entry scores lower than agent-active entry
// (R-10 complement — this uses Active, not Proposed)
#[test]
fn compute_confidence_auto_active_lower_than_agent_active() {
    let now = 1_000_000u64;
    let auto_entry   = make_test_entry(Status::Active, 20, now - 1000, now - 2000, 5, 1, 1, "auto");
    let agent_entry  = make_test_entry(Status::Active, 20, now - 1000, now - 2000, 5, 1, 1, "agent");
    let conf_auto  = compute_confidence(&auto_entry,  now, 3.0, 3.0);
    let conf_agent = compute_confidence(&agent_entry, now, 3.0, 3.0);
    assert!(conf_auto < conf_agent,
        "auto active ({conf_auto:.4}) should be < agent active ({conf_agent:.4})");
}
```

---

### Removed Tests (R-17 verification)

The following tests must be removed because they reference removed constants or removed logic:

| Test Name | Reason |
|-----------|--------|
| `helpfulness_no_votes` (old Wilson form) | Replaced by Bayesian cold-start test |
| `helpfulness_below_minimum_three_helpful` | No minimum sample size in Bayesian |
| `helpfulness_below_minimum_two_each` | No minimum sample size in Bayesian |
| `helpfulness_below_minimum_four_total` | No minimum sample size in Bayesian |
| `helpfulness_at_minimum_wilson_kicks_in` | No MINIMUM_SAMPLE_SIZE threshold |
| `wilson_reference_n100_p80` | `wilson_lower_bound` fn removed |
| `wilson_reference_n10_p80` | `wilson_lower_bound` fn removed |
| `wilson_reference_large_n_p50` | `wilson_lower_bound` fn removed |
| `search_similarity_weight_is_f64` | `SEARCH_SIMILARITY_WEIGHT` constant removed |
| `rerank_score_f64_precision` (old form) | `SEARCH_SIMILARITY_WEIGHT` reference removed |
| `rerank_score_similarity_only` (old form) | References old 2-param signature |
| `rerank_score_confidence_only` (old form) | References old 2-param signature |

**Verify with grep**: after implementation, `grep -r "MINIMUM_SAMPLE_SIZE\|WILSON_Z\|SEARCH_SIMILARITY_WEIGHT"` in `crates/` should return zero results.

---

### Integration Expectations

The confidence-formula-engine component has no direct MCP surface — it is pure functions.
Integration-level verification happens indirectly through:
- `test_confidence.py` suite: confidence formula produces valid range values per entry
- `test_lifecycle.py` R-01 scenario: compute_confidence is called correctly with closure-captured alpha0/beta0
- `test_tools.py` AC-08b scenario: doubled access_count reflects multiplier working with the formula pipeline
