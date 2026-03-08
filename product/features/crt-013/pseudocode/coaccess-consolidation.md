# Component: Co-Access Signal Consolidation

## Purpose

Remove two dead code paths consuming CO_ACCESS data, leaving two well-defined surviving mechanisms (MicroLoRA + scalar boost).

## Changes

### 1. Remove W_COAC and co_access_affinity() from confidence.rs

**File**: `crates/unimatrix-engine/src/confidence.rs`

```
REMOVE constant:
  pub const W_COAC: f64 = 0.08;  (line 28)

REMOVE function (lines 239-250):
  pub fn co_access_affinity(partner_count, avg_partner_confidence) -> f64

REMOVE tests:
  - weight_sum_effective_invariant  (asserts stored + W_COAC = 1.0)
  - co_access_affinity_zero_partners
  - co_access_affinity_max_partners_max_confidence
  - co_access_affinity_large_partner_count_saturated
  - co_access_affinity_zero_confidence
  - co_access_affinity_negative_confidence
  - co_access_affinity_effective_sum_clamped
  - co_access_affinity_partial_partners
  - co_access_affinity_returns_f64

ALSO REMOVE from crt-005 f64 precision tests:
  - weight_sum_invariant_f64: remove the two lines asserting W_COAC == 0.08 and stored_sum + W_COAC == 1.0
    KEEP the assertion that stored_sum == 0.92

KEEP:
  - weight_sum_stored_invariant (asserts sum = 0.92)
  - All other confidence tests unchanged
  - W_BASE through W_TRUST unchanged
  - compute_confidence() unchanged
  - rerank_score() unchanged
  - DEPRECATED_PENALTY, SUPERSEDED_PENALTY unchanged
```

UPDATE comment block (lines 10-13):
```
// Six stored weights sum to exactly 0.92.
// The remaining 0.08 was previously reserved for co-access affinity (W_COAC)
// but was never integrated into stored confidence computation.
// Removed in crt-013 (dead code cleanup). See ADR-001.
```

### 2. Delete episodic.rs

**File**: `crates/unimatrix-adapt/src/episodic.rs`

DELETE entire file.

### 3. Remove episodic from lib.rs

**File**: `crates/unimatrix-adapt/src/lib.rs`

```
REMOVE line:
  pub mod episodic;

UPDATE module docstring: remove "episodic" from the Components list.
```

### 4. Remove episodic from service.rs

**File**: `crates/unimatrix-adapt/src/service.rs`

```
REMOVE import:
  use crate::episodic::EpisodicAugmenter;

REMOVE field from AdaptationService struct:
  episodic: EpisodicAugmenter,

REMOVE from constructor (Self { ... }):
  episodic: EpisodicAugmenter::new(0.02, 3),

REMOVE method (lines 239-246):
  pub fn episodic_adjustments(&self, result_ids, result_scores) -> Vec<f64>

REMOVE comment at line 37 referencing EpisodicAugmenter:
  // EpisodicAugmenter has no interior mutability (just config values).
```

## Error Handling

None needed. All changes are deletions. The Rust compiler is the verification mechanism: if anything references the removed items, it will fail to compile.

## Key Test Scenarios

1. `cargo build --workspace` succeeds after all removals
2. `grep -r "episodic" --include="*.rs" crates/` returns zero hits
3. `grep -r "co_access_affinity\|W_COAC" --include="*.rs" crates/` returns zero hits
4. `weight_sum_stored_invariant` still passes (sum = 0.92)
5. All remaining confidence tests pass unchanged
