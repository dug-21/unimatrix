# Test Plan: test-infrastructure
## Component: Pipeline test files and unit test updates

### Component Scope

This component covers changes to existing test infrastructure files:
- `crates/unimatrix-engine/tests/pipeline_regression.rs` — T-REG-01, T-REG-02 updates
- `crates/unimatrix-engine/tests/pipeline_calibration.rs` — helper update, new scenario AC-12, T-CAL-SPREAD-01
- `crates/unimatrix-engine/tests/pipeline_retrieval.rs` — T-RET-01 signature update
- `crates/unimatrix-engine/src/test_scenarios.rs` — base_score call site update if present

### Risk Coverage

| Risk | Severity | Test(s) |
|------|----------|---------|
| R-04 | High | T-REG-02 must be updated FIRST — implementation ordering guard |
| R-02 | High | T-RET-01 updated to remove SEARCH_SIMILARITY_WEIGHT import |
| AC-10 | Acceptance | pipeline_regression + pipeline_calibration both green |
| AC-12 | Acceptance | `auto_vs_agent_spread` scenario passes |
| NFR-01 | Non-functional | T-CAL-SPREAD-01 confirms spread >= 0.20 with new formula |

---

## `pipeline_regression.rs` Changes

### T-REG-02: Weight Constants (R-04 — Must Be First)

**Implementation ordering mandate**: This update must appear as the first commit or first hunk
of the implementation diff. If weight constants in `confidence.rs` change before T-REG-02 is
updated, CI fails in a non-obvious way during multi-change cycles.

Replace current assertions (old values) with new values:

```rust
#[test]
fn test_weight_constants() {
    assert_eq!(W_BASE,  0.16, "W_BASE changed");
    assert_eq!(W_USAGE, 0.16, "W_USAGE changed");
    assert_eq!(W_FRESH, 0.18, "W_FRESH unchanged");
    assert_eq!(W_HELP,  0.12, "W_HELP changed");
    assert_eq!(W_CORR,  0.14, "W_CORR unchanged");
    assert_eq!(W_TRUST, 0.16, "W_TRUST changed");

    // Update to exact equality (assert_eq!) consistent with weight_sum_invariant_f64 (AC-11)
    let sum = W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST;
    assert_eq!(sum, 0.92_f64, "weight sum must equal exactly 0.92");
}
```

Note: The existing test uses tolerance `(sum - 0.92).abs() < 0.001`. The new vector
`{0.16, 0.16, 0.18, 0.12, 0.14, 0.16}` satisfies IEEE 754 exact equality, so change to
`assert_eq!` consistent with the unit test in `confidence.rs`.

### T-REG-01: Golden Values with New Formula

The `test_golden_confidence_values` test verifies `expert > good > auto` ordering. With the
new formula, the golden values (confidence scores for the named profiles) will change. The
ordering constraints must still hold.

**Update procedure**:
1. Run with `--nocapture` to print new golden values.
2. Verify `expert > good > auto` ordering holds.
3. Verify `conf_expert > 0.5`, `conf_good > 0.3`, `conf_auto > 0.1` (range assertions).
4. Critically: `conf_auto` uses `auto_extracted_new()` which has `Status::Proposed` and
   `trust_source: "auto"`. With the new formula, `base_score(Proposed, "auto") = 0.5`
   (unchanged — C-03 constraint). So the T-REG-01 ordering is preserved.

**The `compute_confidence` call sites in regression.rs must be updated** to pass `alpha0`
and `beta0` parameters. Use cold-start defaults (`3.0, 3.0`) for these golden-value tests
(they do not exercise empirical priors):

```rust
let conf_expert = compute_confidence(&expert, now, 3.0, 3.0);
let conf_good   = compute_confidence(&good,   now, 3.0, 3.0);
let conf_auto   = compute_confidence(&auto,   now, 3.0, 3.0);
```

### T-REG-03: Ranking Stability

`test_ranking_stability` calls `compute_confidence` in a map closure. Update all call sites
to pass `alpha0/beta0`:

```rust
let mut scored: Vec<(u64, f64)> = records
    .iter()
    .map(|e| (e.id, compute_confidence(e, scenario.now, 3.0, 3.0)))
    .collect();
```

---

## `pipeline_calibration.rs` Changes

### confidence_with_adjusted_weight Helper (line 94)

The helper at line 94 calls `base_score(entry.status)` with the old single-parameter
signature. Update to two-parameter form:

```rust
fn confidence_with_adjusted_weight(
    entry: &unimatrix_core::EntryRecord,
    now: u64,
    weight_index: usize,
    delta: f64,
    alpha0: f64,
    beta0: f64,
) -> f64 {
    let weights = [W_BASE, W_USAGE, W_FRESH, W_HELP, W_CORR, W_TRUST];
    let scores = [
        base_score(entry.status, &entry.trust_source),    // ← updated
        usage_score(entry.access_count),
        freshness_score(entry.last_accessed_at, entry.created_at, now),
        helpfulness_score(entry.helpful_count, entry.unhelpful_count, alpha0, beta0), // ← updated
        correction_score(entry.correction_count),
        trust_score(&entry.trust_source),
    ];
    // rest unchanged
}
```

The `test_weight_sensitivity` test that calls this helper must also pass `alpha0/beta0`:
```rust
confidence_with_adjusted_weight(e, scenario.now, weight_idx, delta, 3.0, 3.0)
```

### ablation_pair "helpfulness" Case Update

The existing ablation pair for `"helpfulness"` sets `helpful_count=100/0` with many votes.
The Bayesian formula produces the correct direction (high > low) with these counts even with
cold-start prior. But the `compute_confidence` call must be updated to pass `alpha0/beta0`:

```rust
ablation_test! macro calls compute_confidence(&high, now) and compute_confidence(&low, now)
// Must become:
ablation_test! macro calls compute_confidence(&high, now, 3.0, 3.0) and compute_confidence(&low, now, 3.0, 3.0)
```

The ablation_test! macro expansion must be updated to accept `alpha0/beta0` or the macro must
use hard-coded cold-start defaults internally.

### AC-12: New Scenario — auto_vs_agent_spread

**New test** in `pipeline_calibration.rs`:

```rust
// ---------------------------------------------------------------------------
// AC-12: auto vs. agent spread (trust-source differentiation for Active status)
// ---------------------------------------------------------------------------

#[test]
fn auto_vs_agent_spread() {
    let now = CANONICAL_NOW;

    // Three signal levels: zero, mid, high
    let signals: &[(&str, u32, u32, u32)] = &[
        // (label, access_count, helpful_count, correction_count)
        ("zero",  0,   0, 0),
        ("mid",   20,  5, 1),
        ("high",  50, 20, 1),
    ];

    for (label, access_count, helpful_count, correction_count) in signals {
        let base = EntryProfile {
            label,
            status: Status::Active, // Active only — C-03 constraint
            access_count: *access_count,
            last_accessed_at: now - 7 * 24 * 3600,
            created_at: now - 30 * 24 * 3600,
            helpful_count: *helpful_count,
            unhelpful_count: 0,
            correction_count: *correction_count,
            trust_source: "agent", // will be overridden per entry
            category: "decision",
        };

        let mut auto_profile  = base.clone();
        let mut agent_profile = base.clone();
        auto_profile.trust_source  = "auto";
        agent_profile.trust_source = "agent";

        let auto_entry  = profile_to_entry_record(&auto_profile,  1, now);
        let agent_entry = profile_to_entry_record(&agent_profile, 2, now);

        let conf_auto  = compute_confidence(&auto_entry,  now, 3.0, 3.0);
        let conf_agent = compute_confidence(&agent_entry, now, 3.0, 3.0);

        assert!(
            conf_agent > conf_auto,
            "AC-12 signal={}: agent ({conf_agent:.4}) must exceed auto ({conf_auto:.4}); \
             base_score(Active,'auto')=0.35 < base_score(Active,'agent')=0.50",
            label
        );
    }
}
```

**Note**: The scenario must use `Status::Active` entries. Using `Status::Proposed` would not
test the differentiation since Proposed/auto = 0.5 (C-03).

### T-CAL-SPREAD-01: Confidence Spread Target (AC-01, NFR-01)

**New test** verifying spread >= 0.20 with a synthetic population:

```rust
#[test]
fn test_cal_spread_synthetic_population() {
    // Construct 50 entries spanning the signal space
    // Expected: p95 - p5 confidence spread >= 0.20
    let now = CANONICAL_NOW;
    let mut confidences: Vec<f64> = Vec::new();

    // 10 zero-signal auto entries (low confidence end)
    for i in 0..10 {
        let p = EntryProfile {
            label: "low", status: Status::Active,
            access_count: 0, last_accessed_at: now - 365 * 24 * 3600,
            created_at: now - 400 * 24 * 3600,
            helpful_count: 0, unhelpful_count: 5,
            correction_count: 0, trust_source: "auto", category: "decision",
        };
        let e = profile_to_entry_record(&p, i as u64 + 1, now);
        confidences.push(compute_confidence(&e, now, 3.0, 3.0));
    }

    // 30 moderate-signal agent entries
    for i in 0..30 {
        let p = EntryProfile {
            label: "mid", status: Status::Active,
            access_count: 15, last_accessed_at: now - 14 * 24 * 3600,
            created_at: now - 60 * 24 * 3600,
            helpful_count: 3, unhelpful_count: 1,
            correction_count: 1, trust_source: "agent", category: "decision",
        };
        let e = profile_to_entry_record(&p, (i + 11) as u64, now);
        confidences.push(compute_confidence(&e, now, 3.0, 3.0));
    }

    // 10 high-signal human entries
    for i in 0..10 {
        let p = EntryProfile {
            label: "high", status: Status::Active,
            access_count: 50, last_accessed_at: now - 1 * 24 * 3600,
            created_at: now - 30 * 24 * 3600,
            helpful_count: 20, unhelpful_count: 0,
            correction_count: 1, trust_source: "human", category: "decision",
        };
        let e = profile_to_entry_record(&p, (i + 41) as u64, now);
        confidences.push(compute_confidence(&e, now, 3.0, 3.0));
    }

    confidences.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p5_idx  = (confidences.len() as f64 * 0.05) as usize;
    let p95_idx = (confidences.len() as f64 * 0.95) as usize;
    let spread = confidences[p95_idx] - confidences[p5_idx];

    assert!(
        spread >= 0.20,
        "T-CAL-SPREAD-01: synthetic spread={spread:.4}, must be >= 0.20 (AC-01/NFR-01)"
    );
}
```

---

## `pipeline_retrieval.rs` Changes

### T-RET-01: Remove SEARCH_SIMILARITY_WEIGHT Import

The current test imports `SEARCH_SIMILARITY_WEIGHT` and calls `rerank_score(0.95, 0.50)` with
two parameters. After crt-019:
- `SEARCH_SIMILARITY_WEIGHT` constant is removed — the import will fail to compile.
- `rerank_score` requires a third parameter `confidence_weight: f64`.

**Updated T-RET-01**:

```rust
// Remove: use unimatrix_engine::confidence::SEARCH_SIMILARITY_WEIGHT;

#[test]
fn test_rerank_blend_ordering() {
    // Same semantic test but with explicit confidence_weight
    // Use initial server-start weight (0.184) as the test value.
    // The test verifies ordering, not a specific weight value.
    let cw = 0.184_f64; // adaptive weight at initial spread 0.1471
    let score_high_sim  = rerank_score(0.95, 0.50, cw);
    let score_high_conf = rerank_score(0.70, 1.0,  cw);

    // At cw=0.184: similarity_weight = 0.816
    // high_sim:  0.816 * 0.95 + 0.184 * 0.50 = 0.8692
    // high_conf: 0.816 * 0.70 + 0.184 * 1.00 = 0.7552
    assert!(
        score_high_sim > score_high_conf,
        "similarity-dominant entry ({score_high_sim:.4}) should beat \
         confidence-dominant entry ({score_high_conf:.4}) at confidence_weight={cw}"
    );
}
```

All other `rerank_score` calls in `pipeline_retrieval.rs` must also be updated to pass
`confidence_weight`. The `test_combined_interaction_ordering` test uses `rerank_score(sim, conf)`
— update to pass a representative `confidence_weight` (e.g., `0.15` for floor or `0.184` for
initial default).

---

## `test_scenarios.rs` Changes

Check for `base_score` direct calls:
```bash
grep -n "base_score" crates/unimatrix-engine/src/test_scenarios.rs
```

If present, update from `base_score(status)` to `base_score(status, trust_source)`. The
`auto_extracted_new()` profile has `trust_source: "auto"` and `Status::Proposed`, so
`base_score(Proposed, "auto") = 0.5` — no ordering change (C-03 preserved).
