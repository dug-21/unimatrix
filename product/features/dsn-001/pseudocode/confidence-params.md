# Pseudocode: confidence-params

**File**: `crates/unimatrix-engine/src/confidence.rs` (modified)

## Purpose

Extends `ConfidenceParams` from 3 fields to 9 fields to carry the six confidence
weight constants as instance values rather than compiled constants. Updates
`compute_confidence` and `freshness_score` to use `params.*` fields instead of
the module-level constants. All existing call sites migrate to pass
`&ConfidenceParams` — when using the default preset, behavior is identical to
pre-dsn-001.

---

## Existing State (Pre-dsn-001)

`confidence.rs` currently has:
- Weight constants: `W_BASE=0.16, W_USAGE=0.16, W_FRESH=0.18, W_HELP=0.12, W_CORR=0.14, W_TRUST=0.16`
- `FRESHNESS_HALF_LIFE_HOURS=168.0`, `COLD_START_ALPHA=3.0`, `COLD_START_BETA=3.0`
- `compute_confidence(entry, now, alpha0, beta0) -> f64` using compiled weight constants
- `freshness_score(last_accessed_at, created_at, now) -> f64` using `FRESHNESS_HALF_LIFE_HOURS`
- No `ConfidenceParams` struct (this is new)

---

## New / Modified Structs

### `ConfidenceParams` (new struct)

```
// Parameters controlling all aspects of confidence computation.
//
// `Default` reproduces the compiled constants exactly — the `collaborative`
// preset and all code paths that do not configure a preset produce identical
// results to the pre-dsn-001 binary.
//
// The six `w_*` fields carry the per-domain weight vector set by the active
// preset (or `custom` weights from `[confidence]`). W3-1 will add
// `Option<LearnedWeights>` here without touching any call site that uses
// `Default`.
//
// Weight sum invariant: w_base + w_usage + w_fresh + w_help + w_corr + w_trust == 0.92
// (tolerance: (sum - 0.92).abs() < 1e-9). Enforced by validate_config for custom
// presets; asserted by the SR-10 test for named presets.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfidenceParams {
    // Six weight fields — must sum to exactly 0.92.
    pub w_base:  f64,   // Weight for base quality (status + trust_source). Default: W_BASE (0.16)
    pub w_usage: f64,   // Weight for usage frequency. Default: W_USAGE (0.16)
    pub w_fresh: f64,   // Weight for freshness (recency of access). Default: W_FRESH (0.18)
    pub w_help:  f64,   // Weight for helpfulness (Bayesian posterior). Default: W_HELP (0.12)
    pub w_corr:  f64,   // Weight for correction chain quality. Default: W_CORR (0.14)
    pub w_trust: f64,   // Weight for creator trust level. Default: W_TRUST (0.16)

    // Freshness decay parameter.
    pub freshness_half_life_hours: f64,  // Default: FRESHNESS_HALF_LIFE_HOURS (168.0)

    // Bayesian prior parameters for helpfulness scoring.
    pub alpha0: f64,  // Default: COLD_START_ALPHA (3.0)
    pub beta0:  f64,  // Default: COLD_START_BETA  (3.0)
}

impl Default for ConfidenceParams {
    fn default() -> Self {
        ConfidenceParams {
            w_base:  W_BASE,
            w_usage: W_USAGE,
            w_fresh: W_FRESH,
            w_help:  W_HELP,
            w_corr:  W_CORR,
            w_trust: W_TRUST,
            freshness_half_life_hours: FRESHNESS_HALF_LIFE_HOURS,
            alpha0: COLD_START_ALPHA,
            beta0:  COLD_START_BETA,
        }
    }
}
```

---

## Modified Functions

### `compute_confidence` (signature change)

```
// BEFORE (pre-dsn-001):
//   pub fn compute_confidence(entry: &EntryRecord, now: u64, alpha0: f64, beta0: f64) -> f64
//
// AFTER (dsn-001):
//   - alpha0 and beta0 are removed as bare params; they come from params
//   - all six compiled weight constants are replaced with params.w_*

pub fn compute_confidence(entry: &EntryRecord, now: u64, params: &ConfidenceParams) -> f64

BODY:
    let b = base_score(entry.status, &entry.trust_source);
    let u = usage_score(entry.access_count);
    let f = freshness_score(entry.last_accessed_at, entry.created_at, now, params);
    let h = helpfulness_score(entry.helpful_count, entry.unhelpful_count,
                              params.alpha0, params.beta0);
    let c = correction_score(entry.correction_count);
    let t = trust_score(&entry.trust_source);

    // params.w_* replace the compiled weight constants W_BASE, W_USAGE, etc.
    // When params == ConfidenceParams::default(), the results are identical to
    // the pre-dsn-001 formula — behavioral backward compatibility is guaranteed.
    let composite = params.w_base  * b
                  + params.w_usage * u
                  + params.w_fresh * f
                  + params.w_help  * h
                  + params.w_corr  * c
                  + params.w_trust * t;

    composite.clamp(0.0, 1.0)
```

### `freshness_score` (signature change)

```
// BEFORE (pre-dsn-001):
//   pub fn freshness_score(last_accessed_at: u64, created_at: u64, now: u64) -> f64
//
// AFTER (dsn-001):
//   - adds params: &ConfidenceParams
//   - replaces FRESHNESS_HALF_LIFE_HOURS with params.freshness_half_life_hours

pub fn freshness_score(last_accessed_at: u64, created_at: u64, now: u64,
                       params: &ConfidenceParams) -> f64

BODY:
    let reference = if last_accessed_at > 0 {
        last_accessed_at
    } else {
        created_at
    };

    if reference == 0 { return 0.0; }
    if now <= reference { return 1.0; }

    let age_seconds = now - reference;
    let age_hours = age_seconds as f64 / 3600.0;

    // params.freshness_half_life_hours replaces the compiled FRESHNESS_HALF_LIFE_HOURS const.
    // When params == ConfidenceParams::default(), behavior is identical to pre-dsn-001.
    (-age_hours / params.freshness_half_life_hours).exp()
```

---

## Constants (retained as public, now used only in Default)

The six weight constants and `FRESHNESS_HALF_LIFE_HOURS` remain exported as public constants.
They are no longer used directly in `compute_confidence` or `freshness_score` computation.
Their only roles after dsn-001:
1. Back the `ConfidenceParams::default()` implementation
2. Serve as documentation of the collaborative preset values
3. Continue to pass existing tests that assert their exact values

```
// These constants remain public. They are no longer used in compute_confidence
// or freshness_score — only in ConfidenceParams::default(). Do not remove them;
// they document the collaborative preset values and are used by Default.
pub const W_BASE:  f64 = 0.16;
pub const W_USAGE: f64 = 0.16;
pub const W_FRESH: f64 = 0.18;
pub const W_HELP:  f64 = 0.12;
pub const W_CORR:  f64 = 0.14;
pub const W_TRUST: f64 = 0.16;
pub const FRESHNESS_HALF_LIFE_HOURS: f64 = 168.0;
pub const COLD_START_ALPHA: f64 = 3.0;
pub const COLD_START_BETA:  f64 = 3.0;
pub const PROVENANCE_BOOST: f64 = 0.02;  // unchanged
```

---

## Call Site Migration

All existing call sites of `compute_confidence` and `freshness_score` in the engine
test suite (~15 functions) must be updated. The migration is mechanical.

### Pattern for migrating `compute_confidence`

```
// BEFORE:
compute_confidence(&entry, now, 3.0, 3.0)

// AFTER (using default params = no behavioral change):
compute_confidence(&entry, now, &ConfidenceParams::default())

// AFTER (for tests overriding one specific field):
compute_confidence(&entry, now, &ConfidenceParams {
    w_fresh: 0.34,
    ..Default::default()
})
```

### Pattern for migrating `freshness_score`

```
// BEFORE:
freshness_score(last, created, now)

// AFTER:
freshness_score(last, created, now, &ConfidenceParams::default())

// AFTER (with override):
freshness_score(last, created, now, &ConfidenceParams {
    freshness_half_life_hours: 24.0,
    ..Default::default()
})
```

### Server-side callers

These callers live outside `unimatrix-engine` and must also be updated:

1. `crates/unimatrix-server/src/services/confidence.rs` line ~138:
   ```
   // BEFORE: compute_confidence(&entry, now, alpha0, beta0)
   // AFTER:  compute_confidence(&entry, now, &ConfidenceParams::default())
   //         (or: &*confidence_params if Arc<ConfidenceParams> is threaded through)
   ```

2. `crates/unimatrix-server/src/services/status.rs` line ~885:
   ```
   // BEFORE: crate::confidence::compute_confidence(e, now_ts, alpha0, beta0)
   // AFTER:  unimatrix_engine::confidence::compute_confidence(e, now_ts, &ConfidenceParams::default())
   ```

3. `crates/unimatrix-server/src/services/usage.rs` lines ~209, ~325:
   ```
   // BEFORE: crate::confidence::compute_confidence(entry, now, alpha0, beta0)
   // AFTER:  unimatrix_engine::confidence::compute_confidence(entry, now, &ConfidenceParams::default())
   ```

Implementation note for server callers: The background tick receives
`Arc<ConfidenceParams>` from startup wiring (see startup-wiring.md). For the
background tick's confidence refresh calls, it should use `&*confidence_params`
(the Arc-dereferenced value). Other callers that are not on the hot path and
don't need preset weights can use `&ConfidenceParams::default()` for Wave 1
migration; they can be threaded with the real params in a follow-up.

The `background.rs` `background_tick_loop` function is the primary caller that
must receive and use `Arc<ConfidenceParams>` (see background section in startup-wiring.md).

---

## Key Test Scenarios

1. **SR-10 mandatory test** (lives in `unimatrix-server`, not engine, because it requires `confidence_params_from_preset`):
   ```
   // SR-10: If this test fails, fix the weight table, not the test.
   assert_eq!(confidence_params_from_preset(Preset::Collaborative), ConfidenceParams::default())
   ```

2. **Weight fields are load-bearing** (R-01):
   - Call `compute_confidence(entry, now, &params_with_w_fresh_0_34)` and
     `compute_confidence(entry, now, &ConfidenceParams::default())` on same entry with known age.
   - Assert the results differ measurably. A compiled-constant implementation would return the same.

3. **freshness_score uses params** (R-01):
   - Call `freshness_score(last, created, now, &params_24h)` vs `freshness_score(last, created, now, &params_168h)`.
   - Assert ratio matches expected exponential decay ratio for the two half-lives.

4. **Default is backward-compatible** (AC-01, IR-01):
   - All existing engine tests pass with `ConfidenceParams::default()` at call sites.
   - `compute_confidence(entry, now, &ConfidenceParams::default())` produces identical values
     to the pre-dsn-001 `compute_confidence(entry, now, 3.0, 3.0)` formula.

5. **ConfidenceParams::default() sum invariant**:
   ```
   let p = ConfidenceParams::default();
   let sum = p.w_base + p.w_usage + p.w_fresh + p.w_help + p.w_corr + p.w_trust;
   assert!((sum - 0.92).abs() < 1e-9);
   ```

6. **Struct update syntax** for tests overriding single field:
   ```
   let params = ConfidenceParams { w_trust: 0.22, ..Default::default() };
   // asserts results differ from default
   ```

---

## Error Handling

`compute_confidence` and `freshness_score` are pure functions with no error paths.
`ConfidenceParams` construction is infallible. The weight sum invariant for custom
presets is enforced by `validate_config` in `config.rs`, not in the engine crate.
The engine crate trusts that the `ConfidenceParams` it receives is valid.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-engine` — no config/struct-extension patterns found for this crate. The extension follows the existing pattern in the codebase (ADR-001 from Unimatrix #2284).
- Deviations from established patterns: none. `#[derive(Debug, Clone, PartialEq)]` matches existing struct conventions. `Default` impl is explicit rather than derived, consistent with other structs in this crate that have non-trivial defaults.
