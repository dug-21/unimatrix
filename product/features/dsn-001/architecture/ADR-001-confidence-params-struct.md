## ADR-001: ConfidenceParams Struct for Engine API (Revised — Preset System)

### Context

This ADR was originally written for the first design pass of dsn-001. It decided to
introduce a `ConfidenceParams` struct carrying `freshness_half_life_hours`, `alpha0`,
and `beta0` rather than bare positional parameters (Unimatrix #2284).

The preset system (added after that pass) changes the scope of `ConfidenceParams`
materially. SR-02 from SCOPE-RISK-ASSESSMENT.md identifies the critical gap: the
current struct definition carries only `freshness_half_life_hours`, `alpha0`, and
`beta0`. It does NOT carry the six weight constants (W_BASE, W_USAGE, W_FRESH,
W_HELP, W_CORR, W_TRUST). The preset system sets all six weights. If those values
never enter `ConfidenceParams`, a preset is loaded but silently never applied to the
confidence formula — the config changes but scoring does not.

The compiled weight constants in `confidence.rs` are:

```
W_BASE  = 0.16   W_USAGE = 0.16   W_FRESH = 0.18
W_HELP  = 0.12   W_CORR  = 0.14   W_TRUST = 0.16
Sum     = 0.92   (stored-factor invariant — must hold for every preset)
```

`compute_confidence` uses these six constants directly in its weighted sum. To make
preset-selected weights effective, the function must use values from `ConfidenceParams`
instead.

The `collaborative` preset must reproduce these compiled defaults exactly (SR-10).
`ConfidenceParams::default()` is already the backward-compat contract. Therefore the
six weight fields' defaults must be exactly the six compiled constants above.

W3-1 (GNN Confidence Learning) will eventually replace these weights with learned
values. The struct must absorb that extension without further API churn. The
PRODUCT-VISION W3-1 section states: "Cold-start from config-defined weights (not
hardcoded dev-domain defaults) — W3-1 initializes from the weights in `[confidence]
weights` config." This confirms `ConfidenceParams` is the W3-1 cold-start carrier.

### Decision

Extend `ConfidenceParams` in `unimatrix-engine/src/confidence.rs` with six weight
fields alongside the existing three:

```rust
/// Parameters controlling all aspects of confidence computation.
///
/// `Default` reproduces the compiled constants exactly — the `collaborative`
/// preset and all code paths that do not configure a preset produce identical
/// results to the pre-dsn-001 binary.
///
/// The six `w_*` fields carry the per-domain weight vector set by the active
/// preset (or `custom` weights from `[confidence]`). W3-1 will add
/// `Option<LearnedWeights>` here without touching any call site that uses
/// `Default`.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfidenceParams {
    // Six weight fields — must sum to exactly 0.92 (stored-factor invariant).
    pub w_base:  f64,   // Weight for base quality (status + trust_source)
    pub w_usage: f64,   // Weight for usage frequency
    pub w_fresh: f64,   // Weight for freshness (recency of access)
    pub w_help:  f64,   // Weight for helpfulness (Bayesian posterior)
    pub w_corr:  f64,   // Weight for correction chain quality
    pub w_trust: f64,   // Weight for creator trust level

    // Freshness decay parameter.
    pub freshness_half_life_hours: f64,

    // Bayesian prior parameters for helpfulness scoring.
    pub alpha0: f64,
    pub beta0:  f64,
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

`compute_confidence` uses `params.w_*` instead of the compiled constants:

```rust
pub fn compute_confidence(entry: &EntryRecord, now: u64, params: &ConfidenceParams) -> f64 {
    let b = base_score(entry.status, &entry.trust_source);
    let u = usage_score(entry.access_count);
    let f = freshness_score(entry.last_accessed_at, entry.created_at, now, params);
    let h = helpfulness_score(entry.helpful_count, entry.unhelpful_count,
                              params.alpha0, params.beta0);
    let c = correction_score(entry.correction_count);
    let t = trust_score(&entry.trust_source);

    let composite = params.w_base  * b
                  + params.w_usage * u
                  + params.w_fresh * f
                  + params.w_help  * h
                  + params.w_corr  * c
                  + params.w_trust * t;
    composite.clamp(0.0, 1.0)
}
```

`freshness_score` uses `params.freshness_half_life_hours`:

```rust
pub fn freshness_score(last_accessed_at: u64, created_at: u64, now: u64,
                       params: &ConfidenceParams) -> f64 {
    // ... existing logic, replacing FRESHNESS_HALF_LIFE_HOURS with
    // params.freshness_half_life_hours
}
```

The compiled constants `W_BASE`, `W_USAGE`, `W_FRESH`, `W_HELP`, `W_CORR`, `W_TRUST`,
and `FRESHNESS_HALF_LIFE_HOURS` remain in the file as public constants that document
the defaults backing `ConfidenceParams::default()`. They are no longer used directly
in computation — only in `Default::default()`.

The mechanical guard for SR-10 is a unit test that must be added to the engine crate:

```rust
#[test]
fn collaborative_preset_equals_default() {
    // SR-10: collaborative preset must reproduce compiled defaults exactly.
    // Any difference here means existing behavior has silently changed.
    assert_eq!(
        ConfidenceParams::from_preset(Preset::Collaborative),
        ConfidenceParams::default()
    );
}
```

`ConfidenceParams::from_preset()` is defined in `unimatrix-server/src/infra/config.rs`
(see ADR-005 and ADR-006). Because `ConfidenceParams` lives in `unimatrix-engine`, the
`from_preset` constructor either lives there (importing `Preset`) or the server constructs
it. To avoid a dependency from `unimatrix-engine` onto server types, the test above lives
in `unimatrix-server` integration tests, not in `unimatrix-engine`. The `ConfidenceParams`
equality assertion is sufficient at either location.

### Consequences

**Easier:**
- Every call site migrates mechanically: `compute_confidence(entry, now, &ConfidenceParams::default())`.
  Tests that need non-default values use struct update syntax.
- W3-1 adds `Option<LearnedWeights>` to `ConfidenceParams` — no existing call site changes.
- The SR-02 risk is eliminated: preset weight selection flows directly into the formula.
- `ConfidenceParams::default()` is self-documenting: all tunable knobs in one place.
- The weight-sum invariant test `W_BASE + ... + W_TRUST == 0.92` continues to cover the
  compiled defaults; a separate invariant test covers `ConfidenceParams::default()`.

**Harder:**
- `freshness_score()` and `compute_confidence()` both change signature. All call sites in
  engine tests (approximately 15) and all server-side callers must be updated.
- `ConfidenceParams` now has 9 fields. Struct construction in tests that override specific
  fields must use `..Default::default()` for the rest.
- The weight-sum constraint (sum to 0.92) is a runtime invariant, not a type-system
  invariant. The delivery team must add a `validate_config` check for the `custom` preset
  (see ADR-005) and a test for `ConfidenceParams::default()` sum.
