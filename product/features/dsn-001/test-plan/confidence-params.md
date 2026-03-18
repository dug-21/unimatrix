# dsn-001 Test Plan — confidence-params

Component: `crates/unimatrix-engine/src/confidence.rs`

Risks covered: R-01, R-02, R-03, IR-01, AC-01, AC-04, AC-21, AC-22, AC-23, AC-27.

---

## Scope of Changes

`ConfidenceParams` is extended from 3 fields to 9 fields. `Default` reproduces
compiled constants exactly. `compute_confidence` uses `params.w_*` instead of
compiled weight constants. `freshness_score` uses `params.freshness_half_life_hours`.
All callers (~15 test sites) migrate to pass `&ConfidenceParams`.

The migration gate (IR-01): `cargo test --workspace 2>&1 | tail -30` must show zero
failures after all call sites are updated.

---

## Struct Shape Test (AC-27)

### test_confidence_params_has_nine_fields

Assert that `ConfidenceParams` has exactly nine public fields with the expected names
and `f64` type. This can be a compile-time test:

```rust
fn test_confidence_params_has_nine_fields() {
    // If any field is missing or renamed, this test fails to compile.
    let _p = ConfidenceParams {
        w_base:  0.0,
        w_usage: 0.0,
        w_fresh: 0.0,
        w_help:  0.0,
        w_corr:  0.0,
        w_trust: 0.0,
        freshness_half_life_hours: 0.0,
        alpha0: 0.0,
        beta0:  0.0,
    };
}
```

---

## Default Reproduces Compiled Constants (AC-22)

### test_confidence_params_default_values

```rust
fn test_confidence_params_default_values() {
    let p = ConfidenceParams::default();
    // Each field must equal the compiled constant.
    assert!((p.w_base  - 0.16).abs() < 1e-9, "w_base  must be 0.16");
    assert!((p.w_usage - 0.16).abs() < 1e-9, "w_usage must be 0.16");
    assert!((p.w_fresh - 0.18).abs() < 1e-9, "w_fresh must be 0.18");
    assert!((p.w_help  - 0.12).abs() < 1e-9, "w_help  must be 0.12");
    assert!((p.w_corr  - 0.14).abs() < 1e-9, "w_corr  must be 0.14");
    assert!((p.w_trust - 0.16).abs() < 1e-9, "w_trust must be 0.16");
    assert!((p.freshness_half_life_hours - 168.0).abs() < 1e-9,
        "freshness_half_life_hours must be 168.0");
    assert!((p.alpha0 - 3.0).abs() < 1e-9, "alpha0 must be 3.0");
    assert!((p.beta0  - 3.0).abs() < 1e-9, "beta0  must be 3.0");
}
```

---

## SR-10 Mandatory Test (R-02, AC-21)

This test is non-negotiable. The exact comment text is required verbatim.

```rust
fn collaborative_preset_equals_default_confidence_params() {
    // SR-10: If this test fails, fix the weight table, not the test.
    assert_eq!(
        confidence_params_from_preset(Preset::Collaborative),
        ConfidenceParams::default()
    );
}
```

Note on function name: `confidence_params_from_preset` is defined in
`unimatrix-server/src/infra/config.rs` as a free function, not as a method on
`ConfidenceParams` (per ADR-006 placement decision). The SR-10 test lives in
`unimatrix-server`.

---

## Named Preset Field Values (R-03, AC-23)

All four named presets must match the ADR-005 weight table exactly. Use individual
field assertions, not just equality, so failures identify the wrong field.

### test_authoritative_preset_exact_weights

```rust
fn test_authoritative_preset_exact_weights() {
    let p = confidence_params_from_preset(Preset::Authoritative);
    assert!((p.w_base  - 0.14).abs() < 1e-9, "authoritative w_base  must be 0.14");
    assert!((p.w_usage - 0.14).abs() < 1e-9, "authoritative w_usage must be 0.14");
    assert!((p.w_fresh - 0.10).abs() < 1e-9, "authoritative w_fresh must be 0.10");
    assert!((p.w_help  - 0.14).abs() < 1e-9, "authoritative w_help  must be 0.14");
    assert!((p.w_corr  - 0.18).abs() < 1e-9, "authoritative w_corr  must be 0.18");
    assert!((p.w_trust - 0.22).abs() < 1e-9, "authoritative w_trust must be 0.22");
    assert!((p.freshness_half_life_hours - 8760.0).abs() < 1e-9,
        "authoritative half_life must be 8760.0h");
}
```

### test_operational_preset_exact_weights

```rust
fn test_operational_preset_exact_weights() {
    let p = confidence_params_from_preset(Preset::Operational);
    assert!((p.w_base  - 0.14).abs() < 1e-9);
    assert!((p.w_usage - 0.18).abs() < 1e-9);
    assert!((p.w_fresh - 0.24).abs() < 1e-9);
    assert!((p.w_help  - 0.08).abs() < 1e-9);
    assert!((p.w_corr  - 0.18).abs() < 1e-9);
    assert!((p.w_trust - 0.10).abs() < 1e-9);
    assert!((p.freshness_half_life_hours - 720.0).abs() < 1e-9);
}
```

### test_empirical_preset_exact_weights

```rust
fn test_empirical_preset_exact_weights() {
    let p = confidence_params_from_preset(Preset::Empirical);
    assert!((p.w_base  - 0.12).abs() < 1e-9);
    assert!((p.w_usage - 0.16).abs() < 1e-9);
    assert!((p.w_fresh - 0.34).abs() < 1e-9);
    assert!((p.w_help  - 0.04).abs() < 1e-9);
    assert!((p.w_corr  - 0.06).abs() < 1e-9);
    assert!((p.w_trust - 0.20).abs() < 1e-9);
    assert!((p.freshness_half_life_hours - 24.0).abs() < 1e-9);
}
```

### test_collaborative_preset_exact_weights

```rust
fn test_collaborative_preset_exact_weights() {
    let p = confidence_params_from_preset(Preset::Collaborative);
    assert!((p.w_base  - 0.16).abs() < 1e-9);
    assert!((p.w_usage - 0.16).abs() < 1e-9);
    assert!((p.w_fresh - 0.18).abs() < 1e-9);
    assert!((p.w_help  - 0.12).abs() < 1e-9);
    assert!((p.w_corr  - 0.14).abs() < 1e-9);
    assert!((p.w_trust - 0.16).abs() < 1e-9);
    assert!((p.freshness_half_life_hours - 168.0).abs() < 1e-9);
}
```

---

## Weight Sum Invariant for All Named Presets (R-03)

```rust
fn test_all_named_presets_sum_to_0_92() {
    for preset in [Preset::Collaborative, Preset::Authoritative,
                   Preset::Operational, Preset::Empirical] {
        let p = confidence_params_from_preset(preset);
        let sum = p.w_base + p.w_usage + p.w_fresh + p.w_help + p.w_corr + p.w_trust;
        assert!((sum - 0.92).abs() < 1e-9,
            "preset {:?} weights sum to {:.10}, expected 0.92", preset, sum);
    }
}
```

---

## Weight Fields Are Load-Bearing (R-01)

These tests must fail if `compute_confidence` still uses compiled constants instead
of `params.w_*`.

### test_compute_confidence_uses_params_w_fresh

```rust
fn test_compute_confidence_uses_params_w_fresh() {
    // Use an entry with a known age such that freshness differs significantly.
    let now = 1_000_000u64;
    let entry = entry_accessed_hours_ago(now, 48); // entry from 48h ago

    let params_default = ConfidenceParams::default(); // w_fresh = 0.18
    let params_empirical = ConfidenceParams {
        w_fresh: 0.34,
        ..Default::default()
    };

    let score_default  = compute_confidence(&entry, now, &params_default);
    let score_empirical = compute_confidence(&entry, now, &params_empirical);

    // A compiled-constant implementation returns the same value for both.
    // A correct implementation returns different values.
    assert!(
        (score_default - score_empirical).abs() > 0.01,
        "compute_confidence must use params.w_fresh; got identical scores: \
         default={:.6}, empirical={:.6}",
        score_default, score_empirical
    );
}
```

### test_freshness_score_uses_params_half_life

```rust
fn test_freshness_score_uses_params_half_life() {
    let now = 1_000_000u64;
    let one_hour_secs = 3600u64;
    let age_hours = 24.0_f64;
    let last = now - (age_hours as u64) * one_hour_secs;

    let params_default = ConfidenceParams::default(); // half_life = 168.0h
    let params_short   = ConfidenceParams {
        freshness_half_life_hours: 24.0,
        ..Default::default()
    };

    let score_168 = freshness_score(last, last, now, &params_default);
    let score_24  = freshness_score(last, last, now, &params_short);

    // At 24h age: score_168 = exp(-24*ln2/168) ≈ 0.906
    //             score_24  = exp(-24*ln2/24)  = 0.5 exactly
    // Ratio of scores = exp(24*ln2*(1/24 - 1/168)) ≠ 1.0
    assert!(
        (score_168 - score_24).abs() > 0.1,
        "freshness_score must use params.freshness_half_life_hours; \
         score_168h={:.6}, score_24h={:.6}", score_168, score_24
    );

    // Verify the expected exponential decay ratio.
    let expected_ratio = (-24.0 * std::f64::consts::LN_2 / 168.0_f64).exp()
                       / (-24.0 * std::f64::consts::LN_2 / 24.0_f64).exp();
    let actual_ratio = score_168 / score_24;
    assert!((actual_ratio - expected_ratio).abs() < 0.001,
        "ratio mismatch: expected {:.6}, got {:.6}", expected_ratio, actual_ratio);
}
```

---

## Call Site Migration Tests (IR-01)

### test_existing_call_sites_use_default_params_no_behavior_change

After migration, all existing call sites that previously used compiled constants
now use `&ConfidenceParams::default()`. The behavior must be identical.

This is validated by the full test suite: `cargo test --workspace 2>&1 | tail -30`
must show zero failures. Any test that asserts a specific confidence score and
passes after migration confirms `Default` = prior behavior.

Static audit — these patterns must not appear in `compute_confidence` or
`freshness_score` function bodies (only allowed in `Default` impls):

```bash
# Run in Stage 3c:
grep -n 'W_BASE\|W_USAGE\|W_FRESH\|W_HELP\|W_CORR\|W_TRUST\|FRESHNESS_HALF_LIFE_HOURS' \
    crates/unimatrix-engine/src/confidence.rs
# Only matches in Default::default() impl and const definitions are acceptable.
# Zero matches in compute_confidence / freshness_score function bodies required.
```

---

## `from_preset(Custom)` Panic-by-Design (R-18)

```rust
#[should_panic(expected = "from_preset(Custom)")]
fn test_confidence_params_from_preset_custom_panics() {
    confidence_params_from_preset(Preset::Custom);
}
```

This test is intentional — the panic is by design. It serves as an audit gate to
ensure the panic path has not been accidentally removed.

---

## `resolve_confidence_params` No-Config Returns Default (AC-22)

```rust
fn test_resolve_confidence_params_default_config_equals_default() {
    let config = UnimatrixConfig::default();
    let params = resolve_confidence_params(&config).unwrap();
    assert_eq!(params, ConfidenceParams::default(),
        "no-config path must produce ConfidenceParams::default() exactly");
}
```

---

## `resolve_confidence_params` Populates All Nine Fields (AC-27)

For each preset variant including `Custom`:

```rust
fn test_resolve_confidence_params_all_fields_nonzero_for_all_presets() {
    let named_presets = [
        Preset::Collaborative,
        Preset::Authoritative,
        Preset::Operational,
        Preset::Empirical,
    ];
    for preset in named_presets {
        let config = UnimatrixConfig {
            profile: ProfileConfig { preset },
            ..Default::default()
        };
        let params = resolve_confidence_params(&config).unwrap();
        assert!(params.w_base  > 0.0, "{:?}: w_base must be > 0", preset);
        assert!(params.w_usage > 0.0, "{:?}: w_usage must be > 0", preset);
        assert!(params.w_fresh > 0.0, "{:?}: w_fresh must be > 0", preset);
        assert!(params.w_help  > 0.0, "{:?}: w_help must be > 0", preset);
        assert!(params.w_corr  > 0.0, "{:?}: w_corr must be > 0", preset);
        assert!(params.w_trust > 0.0, "{:?}: w_trust must be > 0", preset);
        assert!(params.freshness_half_life_hours > 0.0);
        assert!(params.alpha0 > 0.0);
        assert!(params.beta0  > 0.0);
    }

    // Custom preset with valid weights:
    let custom_config = UnimatrixConfig {
        profile: ProfileConfig { preset: Preset::Custom },
        confidence: ConfidenceConfig {
            weights: Some(ConfidenceWeights {
                base: 0.12, usage: 0.16, fresh: 0.34,
                help: 0.04, corr: 0.06, trust: 0.20,
            }),
        },
        knowledge: KnowledgeConfig {
            freshness_half_life_hours: Some(24.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let custom_params = resolve_confidence_params(&custom_config).unwrap();
    assert!(custom_params.w_base > 0.0);
    assert!(custom_params.alpha0 > 0.0); // inherited from Default
    assert!(custom_params.beta0  > 0.0);
}
```

Note: `alpha0` and `beta0` for `Custom` preset must use `ConfidenceParams::default()`
values (3.0 each) since `[confidence] weights` does not include these fields.

---

## `freshness_score` with Configurable Half Life (AC-04)

```rust
fn test_freshness_score_configurable_half_life() {
    let one_hour = 3600u64;
    let now = 10_000_000u64;
    let age_hours = 168u64; // 1 week old
    let last = now - age_hours * one_hour;

    let p_168 = ConfidenceParams::default(); // half_life = 168h
    let p_24  = ConfidenceParams { freshness_half_life_hours: 24.0, ..Default::default() };

    let s_168 = freshness_score(last, last, now, &p_168);
    let s_24  = freshness_score(last, last, now, &p_24);

    // At 168h age with half_life=168h: score = 0.5 (one half-life elapsed).
    assert!((s_168 - 0.5).abs() < 0.01,
        "168h old with 168h half_life must be ~0.5; got {:.6}", s_168);
    // At 168h age with half_life=24h: score ≈ exp(-7*ln2) ≈ 0.0078 (7 half-lives).
    assert!(s_24 < 0.02,
        "168h old with 24h half_life must be near zero; got {:.6}", s_24);
    // The values must differ — compiled-constant impl would return same.
    assert!((s_168 - s_24).abs() > 0.4);
}
```
