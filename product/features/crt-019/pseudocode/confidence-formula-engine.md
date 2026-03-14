# Component: confidence-formula-engine

**File**: `crates/unimatrix-engine/src/confidence.rs`

## Purpose

Pure formula module containing all confidence scoring functions. All functions
remain stateless — no I/O, no shared state, deterministic given inputs. This
module is the single source of truth for confidence computation. The server layer
passes runtime state (alpha0, beta0, confidence_weight) as arguments.

## Constants — Remove

These constants are removed entirely. Any test referencing them must be updated.

```
REMOVE: MINIMUM_SAMPLE_SIZE: u32 = 5
REMOVE: WILSON_Z: f64 = 1.96
REMOVE: SEARCH_SIMILARITY_WEIGHT: f64 = 0.85
```

## Constants — Update

```
W_BASE:  f64 = 0.16   // was 0.18
W_USAGE: f64 = 0.16   // was 0.14
W_FRESH: f64 = 0.18   // unchanged
W_HELP:  f64 = 0.12   // was 0.14
W_CORR:  f64 = 0.14   // unchanged
W_TRUST: f64 = 0.16   // was 0.14
// Sum = 0.92 (invariant)
```

Sum invariant check: `0.16 + 0.16 + 0.18 + 0.12 + 0.14 + 0.16 == 0.92_f64`
This holds exactly in IEEE 754 binary64. The `assert_eq!` test (not tolerance)
in `weight_sum_invariant_f64` will verify this.

## Constants — Add

```
// Cold-start defaults for Bayesian prior (documentation constants; values
// passed as arguments to compute_confidence and helpfulness_score, not
// read from these constants in the formula itself)
pub const COLD_START_ALPHA: f64 = 3.0
pub const COLD_START_BETA: f64  = 3.0
```

## Function: base_score (MODIFIED)

**Old signature**: `pub fn base_score(status: Status) -> f64`
**New signature**: `pub fn base_score(status: Status, trust_source: &str) -> f64`

```
fn base_score(status: Status, trust_source: &str) -> f64:
    match status:
        Status::Active =>
            if trust_source == "auto":
                return 0.35
            else:
                return 0.50
        Status::Proposed =>
            return 0.50   // ALWAYS 0.50 regardless of trust_source (C-03, R-10)
        Status::Deprecated =>
            return 0.20
        Status::Quarantined =>
            return 0.10
```

CRITICAL: The `trust_source == "auto"` branch MUST be inside the `Active` arm
only. Placing it outside the `Active` match would apply the 0.35 to
`Status::Proposed` entries with `trust_source = "auto"`, breaking T-REG-01
ordering `auto > stale > quarantined` (R-10).

`auto_extracted_new()` in test_scenarios uses `Status::Proposed`, `trust_source
= "auto"`. After this change it still returns `base_score = 0.5`. This is the
T-REG-01 verification anchor.

## Function: helpfulness_score (REWRITTEN)

**Old signature**: `pub fn helpfulness_score(helpful_count: u32, unhelpful_count: u32) -> f64`
**New signature**: `pub fn helpfulness_score(helpful: u32, unhelpful: u32, alpha0: f64, beta0: f64) -> f64`

Replaces Wilson score lower bound with Bayesian Beta-Binomial posterior mean.
Remove `wilson_lower_bound` (private function — safe to delete).

```
fn helpfulness_score(helpful: u32, unhelpful: u32, alpha0: f64, beta0: f64) -> f64:
    // Cast u32 to f64 BEFORE arithmetic to prevent overflow at u32::MAX (EC-03)
    let h = helpful as f64
    let u = unhelpful as f64
    let total = h + u

    // Bayesian posterior mean: (helpful + alpha0) / (total_votes + alpha0 + beta0)
    let score = (h + alpha0) / (total + alpha0 + beta0)

    // Clamp to [0.0, 1.0] as defense against degenerate prior inputs (R-12)
    score.clamp(0.0, 1.0)
```

Exact assertions (must hold — AC-02):
- `helpfulness_score(0, 0, 3.0, 3.0) == 3/6 == 0.5`    (cold-start neutral)
- `helpfulness_score(0, 2, 3.0, 3.0) == 3/8 == 0.375`   (unhelpful lowers score)
- `helpfulness_score(2, 2, 3.0, 3.0) == 5/10 == 0.5`    (balanced = neutral)
- `helpfulness_score(2, 0, 3.0, 3.0) == 5/8 == 0.625`   (helpful raises score)

No minimum sample size guard. The prior (alpha0/beta0) provides regularization.
The formula responds immediately to any vote without a 5-vote floor.

## Function: compute_confidence (MODIFIED)

**Old signature**: `pub fn compute_confidence(entry: &EntryRecord, now: u64) -> f64`
**New signature**: `pub fn compute_confidence(entry: &EntryRecord, now: u64, alpha0: f64, beta0: f64) -> f64`

```
fn compute_confidence(entry: &EntryRecord, now: u64, alpha0: f64, beta0: f64) -> f64:
    let b = base_score(entry.status, &entry.trust_source)   // CHANGED: add trust_source arg
    let u = usage_score(entry.access_count)
    let f = freshness_score(entry.last_accessed_at, entry.created_at, now)
    let h = helpfulness_score(entry.helpful_count, entry.unhelpful_count, alpha0, beta0)  // CHANGED
    let c = correction_score(entry.correction_count)
    let t = trust_score(&entry.trust_source)

    let composite = W_BASE * b
                  + W_USAGE * u
                  + W_FRESH * f
                  + W_HELP  * h
                  + W_CORR  * c
                  + W_TRUST * t

    composite.clamp(0.0, 1.0)
```

## Function: rerank_score (MODIFIED)

**Old signature**: `pub fn rerank_score(similarity: f64, confidence: f64) -> f64`
**New signature**: `pub fn rerank_score(similarity: f64, confidence: f64, confidence_weight: f64) -> f64`

`SEARCH_SIMILARITY_WEIGHT` constant is removed. `confidence_weight` is passed
by callers from `ConfidenceState` at query time.

```
fn rerank_score(similarity: f64, confidence: f64, confidence_weight: f64) -> f64:
    let similarity_weight = 1.0 - confidence_weight
    (similarity_weight * similarity) + (confidence_weight * confidence)
```

No clamping needed: inputs are both in [0.0, 1.0] and `confidence_weight` is
clamped to [0.15, 0.25] by `adaptive_confidence_weight`, so result is in
[0.0, 1.0] by construction.

## Function: adaptive_confidence_weight (NEW)

**Signature**: `pub fn adaptive_confidence_weight(observed_spread: f64) -> f64`

```
fn adaptive_confidence_weight(observed_spread: f64) -> f64:
    (observed_spread * 1.25).clamp(0.15, 0.25)
```

Exact assertions (must hold — AC-06):
- `adaptive_confidence_weight(0.20) == 0.25`     (at target spread, full weight)
- `adaptive_confidence_weight(0.10) == 0.15`     (below spread, floor applies)
- `adaptive_confidence_weight(0.30) == 0.25`     (above spread, cap applies)
- `adaptive_confidence_weight(0.1471) ~= 0.184`  (initial server state)

## Unchanged Functions

These functions are NOT modified:
- `cosine_similarity` — no change
- `usage_score` — no change
- `freshness_score` — no change
- `correction_score` — no change
- `trust_score` — no change

The penalty constants (`DEPRECATED_PENALTY`, `SUPERSEDED_PENALTY`,
`PROVENANCE_BOOST`, `MAX_MEANINGFUL_ACCESS`, `FRESHNESS_HALF_LIFE_HOURS`) are
also NOT modified.

## Error Handling

All functions return `f64` (no `Result`). They are pure and cannot fail. The
only error condition is NaN input from a degenerate prior — `helpfulness_score`
clamps via `.clamp(0.0, 1.0)` which propagates NaN as 0.0 in Rust
(`f64::NAN.clamp(0.0, 1.0)` returns NaN in stable Rust — use explicit NaN guard
if needed per R-12 defense-in-depth). The empirical prior computation clamps
alpha0/beta0 to [0.5, 50.0] upstream, preventing NaN from entering here.

## Key Test Scenarios

### Tests to REMOVE (Wilson-era)

```
// Remove all tests that reference MINIMUM_SAMPLE_SIZE, WILSON_Z, or wilson_lower_bound:
helpfulness_no_votes                      // replace with Bayesian version
helpfulness_below_minimum_three_helpful   // replace
helpfulness_below_minimum_two_each        // replace
helpfulness_below_minimum_four_total      // replace
helpfulness_at_minimum_wilson_kicks_in    // remove entirely
helpfulness_all_helpful                   // rewrite with explicit alpha0/beta0
helpfulness_all_unhelpful                 // rewrite
helpfulness_mixed_mostly_helpful          // rewrite
wilson_reference_n100_p80                 // remove (function deleted)
wilson_reference_n10_p80                  // remove
wilson_reference_large_n_p50             // remove
search_similarity_weight_is_f64          // remove (constant deleted)
rerank_score_f64_precision               // update (no SEARCH_SIMILARITY_WEIGHT ref)
```

### Tests to UPDATE

```
// base_score tests: add second argument
base_score_active:     base_score(Status::Active, "agent") == 0.5
base_score_proposed:   base_score(Status::Proposed, "auto") == 0.5  // C-03 proof
base_score_deprecated: base_score(Status::Deprecated, "human") == 0.2
base_score_quarantined: base_score(Status::Quarantined, "agent") == 0.1

// rerank_score tests: add confidence_weight argument
rerank_score_both_max:         rerank_score(1.0, 1.0, 0.15) == 1.0
rerank_score_both_zero:        rerank_score(0.0, 0.0, 0.15) == 0.0
rerank_score_similarity_only:  rerank_score(1.0, 0.0, 0.15) ~= 0.85
rerank_score_confidence_only:  rerank_score(0.0, 1.0, 0.15) ~= 0.15
rerank_score_confidence_tiebreaker: rerank_score(0.90, 0.80, 0.15) > rerank_score(0.90, 0.20, 0.15)
rerank_score_similarity_dominant:   rerank_score(0.95, 0.0, 0.15) > rerank_score(0.70, 1.0, 0.15)

// weight_sum_invariant_f64: no change needed except constants now have new values
weight_sum_invariant_f64: W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST == 0.92_f64

// compute_confidence tests: add alpha0/beta0 args (3.0, 3.0 for cold-start)
compute_confidence_all_defaults: pass alpha0=3.0, beta0=3.0
   // expected = 0.16 * 0.5 + 0.16 * 0.0 + 0.18 * 0.0 + 0.12 * 0.5 + 0.14 * 0.5 + 0.16 * 0.3
   //           (base:Active/"" = 0.5, help:0/0 = 0.5, corr:0 = 0.5, trust:"" = 0.3)
   //         = 0.080 + 0.0 + 0.0 + 0.060 + 0.070 + 0.048 = 0.258
compute_confidence_all_max: pass alpha0=3.0, beta0=3.0
   // with helpful_count=100, unhelpful=0: h = (100+3)/(100+6) ~= 0.972 > 0.5
```

### Tests to ADD

```
// Bayesian posterior exact assertions (AC-02):
bayesian_cold_start_neutral:
    helpfulness_score(0, 0, 3.0, 3.0) == 0.5

bayesian_two_unhelpful_lowers_score:
    helpfulness_score(0, 2, 3.0, 3.0) == 0.375  // 3/8 exact

bayesian_balanced_returns_neutral:
    helpfulness_score(2, 2, 3.0, 3.0) == 0.5    // 5/10 exact (NOT > 0.5 — R-14)

bayesian_two_helpful_raises_score:
    helpfulness_score(2, 0, 3.0, 3.0) > 0.5     // 5/8 = 0.625

// base_score trust-source differentiation (AC-05):
base_score_active_auto:
    base_score(Status::Active, "auto") == 0.35

base_score_active_agent:
    base_score(Status::Active, "agent") == 0.5

base_score_active_human:
    base_score(Status::Active, "human") == 0.5

base_score_active_system:
    base_score(Status::Active, "system") == 0.5

auto_proposed_base_score_unchanged:            // R-10 guard
    base_score(Status::Proposed, "auto") == 0.5

// adaptive_confidence_weight (AC-06):
adaptive_weight_at_target_spread:
    adaptive_confidence_weight(0.20) == 0.25

adaptive_weight_at_floor:
    adaptive_confidence_weight(0.10) == 0.15

adaptive_weight_at_cap:
    adaptive_confidence_weight(0.30) == 0.25

adaptive_weight_initial_spread:
    (adaptive_confidence_weight(0.1471) - 0.184).abs() < 0.001

// NaN defense (R-12 defense-in-depth):
helpfulness_score_nan_prior_clamped:
    // if NaN escapes the prior computation, result should still be finite
    // Implementation note: f64::NAN.clamp() behavior — add explicit NaN check
    // if needed: score.is_nan().then(|| 0.5).unwrap_or(score.clamp(0.0, 1.0))
```

## Call-Site Inventory for compute_confidence

All callers must be updated to pass `alpha0, beta0`:

1. `services/confidence.rs` `ConfidenceService::recompute()` line ~45:
   `compute_confidence(&entry, now)` -> `compute_confidence(&entry, now, alpha0, beta0)`
   where `alpha0`/`beta0` are read from `ConfidenceStateHandle` before `spawn_blocking`.

2. `services/status.rs` `run_maintenance()` Step 2:
   `compute_confidence(e, now_ts)` -> `compute_confidence(e, now_ts, snapshot_alpha0, snapshot_beta0)`
   where the snapshot is taken OUTSIDE the loop (IR-02).

3. `services/usage.rs` `record_mcp_usage()` spawn_blocking closure:
   `Some(&crate::confidence::compute_confidence)` -> capturing closure
   `Some(Box::new(move |entry, now| compute_confidence(entry, now, alpha0, beta0)))`
   This is the R-01 critical change — see deliberate-retrieval-signal.md.

## Call-Site Inventory for base_score

1. `confidence.rs` `compute_confidence` body (already updated above)
2. `confidence.rs` unit tests (4 base_score_* tests — update to 2-arg)
3. `tests/pipeline_calibration.rs` `confidence_with_adjusted_weight()` line 94:
   `base_score(entry.status)` -> `base_score(entry.status, &entry.trust_source)`
4. `test_scenarios.rs`: no direct call to base_score (confirmed by grep)

## Call-Site Inventory for rerank_score

All in `services/search.rs` — add `confidence_weight` parameter sourced from
`ConfidenceState` (see confidence-state.md for read pattern):

1. Line 275: `rerank_score(*sim_a, entry_a.confidence)` -> `rerank_score(*sim_a, entry_a.confidence, confidence_weight)`
2. Line 327: `rerank_score(*sim_a, entry_a.confidence)` -> `rerank_score(*sim_a, entry_a.confidence, confidence_weight)`
3. Line 328: `rerank_score(*sim_b, entry_b.confidence)` -> `rerank_score(*sim_b, entry_b.confidence, confidence_weight)`
4. Line 370: `rerank_score(*sim, entry.confidence)` -> `rerank_score(*sim, entry.confidence, confidence_weight)`

Also update in `pipeline_retrieval.rs` (any direct `rerank_score` calls there)
and any inline tests in `confidence.rs`.
