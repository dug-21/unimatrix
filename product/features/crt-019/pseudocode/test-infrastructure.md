# Component: test-infrastructure

**Files**:
- `crates/unimatrix-engine/tests/pipeline_regression.rs`  (T-REG-02 first)
- `crates/unimatrix-engine/tests/pipeline_calibration.rs`
- `crates/unimatrix-engine/tests/pipeline_retrieval.rs`
- `crates/unimatrix-engine/src/confidence.rs` (inline unit tests)
- `crates/unimatrix-server/src/services/usage.rs` (new integration tests)
- `crates/unimatrix-server/src/services/status.rs` or confidence.rs (prior tests)
- `crates/unimatrix-server/src/infra/coherence.rs` (batch size constant test)

## Purpose

The test infrastructure component covers all test additions, removals, and
updates. The MANDATORY ordering constraint (C-02, R-04) governs the first
change: T-REG-02 update must be the absolute first diff hunk.

## ORDERING: T-REG-02 is the first commit

Before touching any constant in `confidence.rs`, update
`pipeline_regression.rs` T-REG-02 to assert the NEW values:

```
// pipeline_regression.rs — T-REG-02: Update these assertions FIRST
#[test]
fn test_weight_constants():
    assert_eq!(W_BASE,  0.16, "W_BASE changed")   // was 0.18
    assert_eq!(W_USAGE, 0.16, "W_USAGE changed")  // was 0.14
    assert_eq!(W_FRESH, 0.18, "W_FRESH changed")  // unchanged
    assert_eq!(W_HELP,  0.12, "W_HELP changed")   // was 0.14
    assert_eq!(W_CORR,  0.14, "W_CORR changed")   // unchanged
    assert_eq!(W_TRUST, 0.16, "W_TRUST changed")  // was 0.14

    let sum = W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST
    assert!(
        (sum - 0.92).abs() < 0.001,
        "weight sum changed: {sum}, expected 0.92"
    )
```

This test will FAIL immediately after the update (constants still have old values).
That is expected — it will go green when the constants in `confidence.rs` are
updated in the next step.

## T-REG-01: Verify ordering holds after formula changes

T-REG-01 (`test_golden_confidence_values`) calls `compute_confidence` — which
will need the new 4-argument signature. Update the call sites in this test to
pass `alpha0=3.0, beta0=3.0` (cold-start).

Also verify the `auto_extracted_new()` profile still satisfies `good > auto`
after the `base_score` change. `auto_extracted_new()` uses `Status::Proposed`
and `trust_source = "auto"` — `base_score(Proposed, "auto") = 0.5` is unchanged.
The ordering must continue to hold.

```
// Updated T-REG-01 calls:
let conf_expert = compute_confidence(&expert, now, 3.0, 3.0)  // cold-start prior
let conf_good   = compute_confidence(&good,   now, 3.0, 3.0)
let conf_auto   = compute_confidence(&auto,   now, 3.0, 3.0)
```

## pipeline_calibration.rs Changes

### confidence_with_adjusted_weight helper (line 94)

The helper manually recomputes confidence with a perturbed weight. It calls
`base_score(entry.status)` (old signature). Update to:

```
fn confidence_with_adjusted_weight(
    entry: &unimatrix_core::EntryRecord,
    now: u64,
    weight_index: usize,
    delta: f64,
) -> f64:
    let weights = [W_BASE, W_USAGE, W_FRESH, W_HELP, W_CORR, W_TRUST]
    let scores = [
        base_score(entry.status, &entry.trust_source),   // CHANGED: add trust_source
        usage_score(entry.access_count),
        freshness_score(entry.last_accessed_at, entry.created_at, now),
        helpfulness_score(entry.helpful_count, entry.unhelpful_count, 3.0, 3.0),  // CHANGED
        correction_score(entry.correction_count),
        trust_score(&entry.trust_source),
    ]

    // rest of helper unchanged
    let mut adjusted = weights.to_vec()
    adjusted[weight_index] *= 1.0 + delta

    adjusted
        .iter()
        .zip(scores.iter())
        .map(|(w, s)| w * s)
        .sum::<f64>()
        .clamp(0.0, 1.0)
```

Also update the import line at the top of `pipeline_calibration.rs` to include
`helpfulness_score` (already imported) — verify no `SEARCH_SIMILARITY_WEIGHT`
import remains.

### New scenario: auto_vs_agent_spread (AC-12)

```
// In pipeline_calibration.rs:
#[test]
fn auto_vs_agent_spread():
    // Verifies base_score differentiation for Active/"auto" vs Active/"agent"
    // Uses three signal levels: zero-signal, mid-signal, high-signal
    let now = CANONICAL_NOW

    // Zero-signal pair: no access, no votes, no corrections
    let auto_zero = make_active_entry(
        id=10, trust_source="auto", access_count=0,
        helpful_count=0, unhelpful_count=0, correction_count=0,
        last_accessed_at=0, created_at=now
    )
    let agent_zero = make_active_entry(
        id=11, trust_source="agent", access_count=0,
        helpful_count=0, unhelpful_count=0, correction_count=0,
        last_accessed_at=0, created_at=now
    )
    assert!(
        compute_confidence(&agent_zero, now, 3.0, 3.0)
        > compute_confidence(&auto_zero, now, 3.0, 3.0),
        "agent zero-signal should beat auto zero-signal"
    )

    // Mid-signal pair
    let auto_mid = make_active_entry(
        id=12, trust_source="auto", access_count=5,
        helpful_count=3, unhelpful_count=0, correction_count=1,
        last_accessed_at=now - 3600, created_at=now - 7200
    )
    let agent_mid = make_active_entry(
        id=13, trust_source="agent", access_count=5,
        helpful_count=3, unhelpful_count=0, correction_count=1,
        last_accessed_at=now - 3600, created_at=now - 7200
    )
    assert!(
        compute_confidence(&agent_mid, now, 3.0, 3.0)
        > compute_confidence(&auto_mid, now, 3.0, 3.0),
        "agent mid-signal should beat auto mid-signal"
    )

    // High-signal pair
    let auto_high = make_active_entry(
        id=14, trust_source="auto", access_count=50,
        helpful_count=20, unhelpful_count=0, correction_count=1,
        last_accessed_at=now, created_at=now - 3600
    )
    let agent_high = make_active_entry(
        id=15, trust_source="agent", access_count=50,
        helpful_count=20, unhelpful_count=0, correction_count=1,
        last_accessed_at=now, created_at=now - 3600
    )
    assert!(
        compute_confidence(&agent_high, now, 3.0, 3.0)
        > compute_confidence(&auto_high, now, 3.0, 3.0),
        "agent high-signal should beat auto high-signal"
    )
```

Note: this scenario uses `Status::Active` entries explicitly (R-10 guard — do
not accidentally test `Status::Proposed` here).

### ablation_pair helpfulness case

The ablation pair for helpfulness uses `helpful_count=100, unhelpful_count=0`.
With new signature `helpfulness_score(100, 0, alpha0, beta0)` the direction is
still correct (high helpful_count -> high score). Update the call to pass
`alpha0=3.0, beta0=3.0`:

```
// In ablation_pair or the T-ABL-05 equivalent test:
helpfulness_score(100, 0, 3.0, 3.0)  // was helpfulness_score(100, 0)
```

### rerank_score calls in pipeline_calibration.rs

Any direct `rerank_score(sim, conf)` calls must add `confidence_weight`:
```
rerank_score(sim, conf, 0.15)   // use initial confidence_weight for calibration tests
```

## pipeline_retrieval.rs Changes

Grep for `rerank_score` calls and add the `confidence_weight` argument:
```
rerank_score(sim, conf)  ->  rerank_score(sim, conf, 0.15)
```
Use `0.15` (the floor value) for retrieval tests — this represents the
conservative baseline and makes tests independent of runtime state.

## confidence.rs Unit Tests

### Remove (Wilson-era tests)

These tests reference deleted functions or constants and must be removed:
- `helpfulness_no_votes` -> replace
- `helpfulness_below_minimum_three_helpful` -> replace
- `helpfulness_below_minimum_two_each` -> replace
- `helpfulness_below_minimum_four_total` -> replace
- `helpfulness_at_minimum_wilson_kicks_in` -> remove entirely
- `helpfulness_all_helpful` -> rewrite with alpha0/beta0
- `helpfulness_all_unhelpful` -> rewrite
- `helpfulness_mixed_mostly_helpful` -> rewrite
- `wilson_reference_n100_p80` -> remove
- `wilson_reference_n10_p80` -> remove
- `wilson_reference_large_n_p50` -> remove
- `search_similarity_weight_is_f64` -> remove
- `rerank_score_f64_precision` -> update (remove SEARCH_SIMILARITY_WEIGHT ref)

### Add (Bayesian, base_score differentiation, adaptive weight)

Add the following new tests to the inline `#[cfg(test)] mod tests` section of
`confidence.rs`. (Full pseudocode in confidence-formula-engine.md — this file
lists them for reference.)

Bayesian posterior (AC-02):
- `bayesian_cold_start_neutral`: `helpfulness_score(0, 0, 3.0, 3.0) == 0.5`
- `bayesian_two_unhelpful_lowers_score`: `helpfulness_score(0, 2, 3.0, 3.0) == 0.375`
- `bayesian_balanced_returns_neutral`: `helpfulness_score(2, 2, 3.0, 3.0) == 0.5`
- `bayesian_two_helpful_raises_score`: `helpfulness_score(2, 0, 3.0, 3.0) > 0.5`

base_score differentiation (AC-05):
- `base_score_active_auto`: `base_score(Status::Active, "auto") == 0.35`
- `base_score_active_agent`: `base_score(Status::Active, "agent") == 0.5`
- `base_score_active_human`: `base_score(Status::Active, "human") == 0.5`
- `base_score_active_system`: `base_score(Status::Active, "system") == 0.5`
- `auto_proposed_base_score_unchanged`: `base_score(Status::Proposed, "auto") == 0.5`

Adaptive weight (AC-06):
- `adaptive_weight_at_target_spread`: `adaptive_confidence_weight(0.20) == 0.25`
- `adaptive_weight_at_floor`: `adaptive_confidence_weight(0.10) == 0.15`
- `adaptive_weight_at_cap`: `adaptive_confidence_weight(0.30) == 0.25`
- `adaptive_weight_initial_spread`: within 0.001 of 0.184 for input 0.1471

NaN defense (R-12 defense-in-depth):
- `helpfulness_score_large_counts_no_overflow`: `helpfulness_score(u32::MAX, 0, 3.0, 3.0)` is in [0.0, 1.0] and not NaN

### Update Existing Tests

All existing tests referencing `compute_confidence`, `base_score`,
`helpfulness_score`, or `rerank_score` must add the new parameters:

```
// compute_confidence: add alpha0=3.0, beta0=3.0
compute_confidence_all_defaults:
    compute_confidence(&entry, 1_000_000, 3.0, 3.0)
    // expected = 0.16*0.5 + 0.16*0.0 + 0.18*0.0 + 0.12*0.5 + 0.14*0.5 + 0.16*0.3
    //          = 0.080 + 0 + 0 + 0.060 + 0.070 + 0.048 = 0.258

compute_confidence_all_max:
    // trust_source="human", helpful=100/0: h=(100+3)/(106) ~= 0.972
    compute_confidence(&entry, now, 3.0, 3.0)
    // still > 0.7

// base_score: add trust_source "agent" to existing Active test
base_score_active: base_score(Status::Active, "agent") == 0.5

// rerank_score: add 0.15 as third arg
rerank_score_both_max:      rerank_score(1.0, 1.0, 0.15) == 1.0
rerank_score_both_zero:     rerank_score(0.0, 0.0, 0.15) == 0.0
rerank_score_similarity_only: rerank_score(1.0, 0.0, 0.15) ~= 0.85
rerank_score_confidence_only: rerank_score(0.0, 1.0, 0.15) ~= 0.15
```

## Integration Tests (usage.rs or status.rs)

These are the highest-value integration tests per the Risk-Test Strategy.

### T-INT-03: context_get implicit helpful vote (AC-08a)

```
// In services/usage.rs tests or a new integration test file:
async fn test_context_get_implicit_helpful_vote():
    let (service, store, _dir) = make_usage_service()
    let id = insert_test_entry(&store)

    // Simulate context_get: helpful = None -> injected as Some(true)
    service.record_access(
        &[id],
        AccessSource::McpTool,
        UsageContext {
            session_id:    None,
            agent_id:      Some("agent-1".to_string()),
            helpful:       Some(true),   // as injected by context_get handler
            feature_cycle: None,
            trust_level:   Some(TrustLevel::Internal),
            access_weight: 1,
        },
    )

    tokio::time::sleep(Duration::from_millis(50)).await

    let entry = store.get(id).unwrap()
    assert_eq!(entry.helpful_count, 1)
    assert_eq!(entry.access_count, 1)
```

### T-INT-04: context_lookup doubled access count (AC-08b)

```
async fn test_context_lookup_doubled_access_new_entry():
    let (service, store, _dir) = make_usage_service()
    let id = insert_test_entry(&store)

    // First call: access_weight=2 for new agent-entry pair
    service.record_access(
        &[id],
        AccessSource::McpTool,
        UsageContext {
            session_id:    None,
            agent_id:      Some("agent-1".to_string()),
            helpful:       None,
            feature_cycle: None,
            trust_level:   Some(TrustLevel::Internal),
            access_weight: 2,
        },
    )
    tokio::time::sleep(Duration::from_millis(50)).await
    assert_eq!(store.get(id).unwrap().access_count, 2)
    assert_eq!(store.get(id).unwrap().helpful_count, 0)

async fn test_context_lookup_dedup_prevents_second_increment():
    // Same agent, same entry, two calls
    // First call: access_count becomes 2
    // Second call: dedup suppresses access_ids -> 0 increment
    // Assert access_count remains 2 after second call

async fn test_context_lookup_two_agents():
    // agent-1 and agent-2 each call lookup once
    // Assert access_count == 4
```

### T-INT-05: R-11 store-layer dedup verification (mandatory gate)

```
// In tests/pipeline_retrieval.rs or a dedicated store integration test:
fn test_store_duplicate_id_doubles_access_count():
    // This is the R-11 gate that must pass before flat_map approach is committed
    let store = Store::open(tempdir.path().join("test.db")).unwrap()
    let id = insert_test_entry(&store)
    let before = store.get(id).unwrap().access_count  // should be 0

    store.record_usage_with_confidence(
        &[id, id],   // all_ids with duplicate
        &[id, id],   // access_ids with duplicate
        &[], &[], &[], &[],
        None,
    ).unwrap()

    let after = store.get(id).unwrap().access_count
    assert_eq!(
        after, before + 2,
        "store must not deduplicate IDs; expected +2, got +{}",
        after - before
    )
```

### T-INT-06: R-01 empirical prior flows to stored confidence

```
// Integration test proving the capture closure carries alpha0/beta0 to the store:
async fn test_empirical_prior_flows_to_stored_confidence():
    let dir = tempfile::tempdir().unwrap()
    let store = Arc::new(Store::open(dir.path().join("test.db")).unwrap())

    // Insert and vote on entries to push ConfidenceState alpha0 up
    // (insert 10 entries, vote all helpful to simulate skewed population)
    // Then insert a new unvoted entry
    // Record access with a ConfidenceState that has alpha0=8.0 (not cold-start 3.0)
    //   -> stored confidence for the unvoted entry should reflect alpha0=8.0
    //   -> expected h = (0+8)/(0+10) = 0.8 (vs cold-start 3/6 = 0.5)
    // Assert stored confidence > confidence with cold-start prior

    let unvoted_entry_id = insert_test_entry(&store)
    let confidence_state = Arc::new(RwLock::new(ConfidenceState {
        alpha0: 8.0, beta0: 2.0,
        observed_spread: 0.2, confidence_weight: 0.25,
    }))

    let usage_dedup = Arc::new(UsageDedup::new())
    let service = UsageService::new(
        Arc::clone(&store), usage_dedup, Arc::clone(&confidence_state)
    )

    service.record_access(
        &[unvoted_entry_id],
        AccessSource::McpTool,
        UsageContext {
            session_id: None,
            agent_id: Some("agent-1".to_string()),
            helpful: None,
            feature_cycle: None,
            trust_level: Some(TrustLevel::Internal),
            access_weight: 1,
        },
    )
    tokio::time::sleep(Duration::from_millis(50)).await

    let entry = store.get(unvoted_entry_id).unwrap()
    // With alpha0=8.0, beta0=2.0, 0 votes: h = 8/(10) = 0.8
    // With cold-start alpha0=3.0, beta0=3.0, 0 votes: h = 3/6 = 0.5
    // The empirical prior entry should have higher confidence than cold-start
    let cold_start_conf = compute_confidence(&entry, now, 3.0, 3.0)
    let empirical_conf  = compute_confidence(&entry, now, 8.0, 2.0)
    assert!(
        (entry.confidence - empirical_conf).abs() < 0.001,
        "stored confidence should use empirical prior (alpha0=8.0), \
         expected ~{empirical_conf:.4}, got {:.4}",
        entry.confidence
    )
    assert!(
        empirical_conf > cold_start_conf,
        "empirical prior (alpha0=8.0) should produce higher confidence than cold-start"
    )
```

## Acceptance Criteria Verification Map

| AC | Test(s) |
|----|---------|
| AC-01 (spread >= 0.20) | T-CAL-SPREAD-01 (synthetic 50-entry, see calibration) |
| AC-02 (Bayesian posterior) | `bayesian_cold_start_neutral`, `bayesian_two_unhelpful_lowers_score`, `bayesian_balanced_returns_neutral`, `bayesian_two_helpful_raises_score` |
| AC-03 (weight sum) | `weight_sum_invariant_f64` (updated constants) |
| AC-04 (ablation T-ABL-01..06) | Updated ablation tests in pipeline_calibration.rs |
| AC-05 (trust-source base_score) | `base_score_active_auto`, `base_score_active_agent`, `auto_proposed_base_score_unchanged` |
| AC-06 (adaptive blend) | `adaptive_weight_*` unit tests; integration T-INT-02 |
| AC-07 (batch 500, guard) | `max_confidence_refresh_batch_is_500`; `confidence_refresh_duration_guard_fires_pre_iteration` |
| AC-08a (context_get implicit vote) | T-INT-03 |
| AC-08b (context_lookup doubled) | T-INT-04a, T-INT-04b (dedup) |
| AC-09 (skill files updated) | Manual review |
| AC-10 (pipeline tests pass) | `cargo test --test pipeline_calibration --test pipeline_regression` |
| AC-11 (weight sum exact f64) | `weight_sum_invariant_f64` with `assert_eq!` |
| AC-12 (auto vs agent spread) | `auto_vs_agent_spread` in pipeline_calibration.rs |
| R-11 gate | T-INT-05 (store duplicate ID) |
| R-01 gate | T-INT-06 (empirical prior flows to stored confidence) |
