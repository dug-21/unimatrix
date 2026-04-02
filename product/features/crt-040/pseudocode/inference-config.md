# Wave 1b: InferenceConfig — supports_cosine_threshold + nli_post_store_k Removal

## Purpose

Add `supports_cosine_threshold: f32` (the Path C detection gate) to `InferenceConfig`
using the mandatory dual-site default pattern (ADR-002). Remove the dead `nli_post_store_k`
field from all 6 sites. Update the config merge function to propagate project-level
`supports_cosine_threshold` overrides.

---

## File Modified

`crates/unimatrix-server/src/infra/config.rs`

---

## Part A: Add `supports_cosine_threshold`

### Site 1 — Field declaration in the struct

Insert after `nli_informs_ppr_weight` (the last field before the closing `}` of
`InferenceConfig`), or at a logical grouping point after the crt-037 Informs fields.

```
/// Cosine similarity threshold for cosine Supports edge detection (Path C).
///
/// Path C in `run_graph_inference_tick` writes a `Supports` edge when
/// `cosine >= supports_cosine_threshold` AND the category pair is in
/// `informs_category_pairs`. Threshold validated as exclusive (0.0, 1.0).
///
/// Note: `supports_candidate_threshold` (Phase 4 pre-filter, default 0.5) must
/// remain <= this value or Path C receives zero candidates (IR-02). This invariant
/// is not enforced by validate() but must be respected in operator config.
///
/// Default: 0.65 (empirically validated on production corpus, ASS-035).
/// Range: (0.0, 1.0) exclusive.
#[serde(default = "default_supports_cosine_threshold")]
pub supports_cosine_threshold: f32,
```

### Site 2 — Serde backing function

Add adjacent to the other backing functions in the "Background graph inference tick
default value functions (crt-029)" section, after `default_nli_informs_ppr_weight`:

```
fn default_supports_cosine_threshold() -> f32 {
    0.65
}
```

### Site 3 — `impl Default for InferenceConfig` struct literal

Inside the `impl Default` block, add the new field using a CALL to the backing function
(NOT a repeated literal — ADR-002 mandates this to prevent future divergence):

```
// crt-040: cosine Supports detection threshold
supports_cosine_threshold: default_supports_cosine_threshold(),
```

Placement: after `nli_informs_ppr_weight: default_nli_informs_ppr_weight()`, before the
closing `}` of the struct literal.

### Site 4 — `validate()` range check

Add after the `nli_informs_cosine_floor` range check block (around line 1124), following
the exact same pattern:

```
// -- crt-040: supports_cosine_threshold range check (0.0, 1.0) exclusive --
if self.supports_cosine_threshold <= 0.0 || self.supports_cosine_threshold >= 1.0 {
    return Err(ConfigError::NliFieldOutOfRange {
        path: path.to_path_buf(),
        field: "supports_cosine_threshold",
        value: self.supports_cosine_threshold.to_string(),
        reason: "must be in range (0.0, 1.0) exclusive",
    });
}
```

### Site 5 — Config merge function

**Locate site**: grep for `nli_informs_cosine_floor` in the merge function body (around
line 2414). The merge pattern for f32 fields uses epsilon comparison:

```
nli_informs_cosine_floor: if (project.inference.nli_informs_cosine_floor
    - default.inference.nli_informs_cosine_floor)
    .abs()
    > f32::EPSILON
{
    project.inference.nli_informs_cosine_floor
} else {
    global.inference.nli_informs_cosine_floor
},
```

Add `supports_cosine_threshold` immediately after `nli_informs_ppr_weight` in the merge
function, using the identical epsilon-comparison pattern:

```
supports_cosine_threshold: if (project.inference.supports_cosine_threshold
    - default.inference.supports_cosine_threshold)
    .abs()
    > f32::EPSILON
{
    project.inference.supports_cosine_threshold
} else {
    global.inference.supports_cosine_threshold
},
```

---

## Part B: Remove `nli_post_store_k` (all 6 sites)

### Site 1 — Field declaration in the struct

Remove the entire field declaration block including its doc comment:

```
// REMOVE these lines:
/// Neighbor count for post-store NLI detection.
///
/// After `context_store`, the NLI task queries `nli_post_store_k` HNSW neighbors.
/// Distinct from `nli_top_k` (D-04, AC-19). Default: 10. Valid range: `[1, 100]`.
#[serde(default = "default_nli_post_store_k")]
pub nli_post_store_k: usize,
```

Note: The doc comment on `nli_top_k` mentions `nli_post_store_k` in its "Distinct from"
clause. Update that reference to remove the stale cross-reference after deletion.

### Site 2 — Serde backing function

Remove completely:

```
// REMOVE:
fn default_nli_post_store_k() -> usize {
    10
}
```

### Site 3 — `impl Default` struct literal entry

Remove the line:

```
// REMOVE:
nli_post_store_k: 10,
```

### Site 4 — `validate()` range check block

Remove the entire block:

```
// REMOVE:
if self.nli_post_store_k < 1 || self.nli_post_store_k > 100 {
    return Err(ConfigError::NliFieldOutOfRange {
        path: path.to_path_buf(),
        field: "nli_post_store_k",
        value: self.nli_post_store_k.to_string(),
        reason: "must be in range [1, 100]",
    });
}
```

Also update the `validate()` doc comment that lists "nli_top_k and nli_post_store_k in
[1, 100]" — remove `nli_post_store_k` from that list.

### Site 5 — Test assertions referencing `nli_post_store_k`

Search the test module for any assertions on `nli_post_store_k`. Remove all such
assertions. If a test's only purpose was to verify `nli_post_store_k`, remove the
entire test. If it was checking multiple fields, remove only the `nli_post_store_k`
assertion line.

Verification command (must return zero results after removal):
```
grep -n "nli_post_store_k" crates/unimatrix-server/src/infra/config.rs
```

### Site 6 — Config merge function

Remove the merge block for `nli_post_store_k` (around lines 2222-2228):

```
// REMOVE:
nli_post_store_k: if project.inference.nli_post_store_k
    != default.inference.nli_post_store_k
{
    project.inference.nli_post_store_k
} else {
    global.inference.nli_post_store_k
},
```

Note: The `nli_post_store_k` merge uses integer equality (`!=`) rather than f32 epsilon
comparison. That is correct for `usize`. Remove the entire block.

---

## Error Handling

- `validate()` uses the existing `ConfigError::NliFieldOutOfRange` variant — no new error
  type needed. The variant already accepts a `field: &'static str` and `value: String`.
- Serde silently ignores `nli_post_store_k` in existing config files (no
  `#[serde(deny_unknown_fields)]` on `InferenceConfig` — verified by AC-18).

---

## Key Test Scenarios

### AC-10 / AC-16 (R-03): Dual-site default — three independent assertions

```
fn test_supports_cosine_threshold_backing_fn() {
    // Site 2 backing function
    assert_eq!(default_supports_cosine_threshold(), 0.65_f32,
        "backing function must return 0.65");
}

fn test_supports_cosine_threshold_impl_default() {
    // Site 3 impl Default path (DIFFERENT from serde deserialization)
    assert_eq!(InferenceConfig::default().supports_cosine_threshold, 0.65_f32,
        "impl Default must return 0.65");
}

fn test_supports_cosine_threshold_serde_default() {
    // Site 1 serde path — empty TOML triggers #[serde(default = ...)]
    let config: InferenceConfig = toml::from_str("").expect("empty TOML must parse");
    assert_eq!(config.supports_cosine_threshold, 0.65_f32,
        "serde default must return 0.65");
}
```

### AC-09: validate() accepts in-range values, rejects boundary and out-of-range values

```
fn test_validate_supports_cosine_threshold_boundaries() {
    // reject 0.0 (exclusive lower)
    let c = InferenceConfig { supports_cosine_threshold: 0.0, ..InferenceConfig::default() };
    assert!(c.validate(Path::new("/fake")).is_err());

    // reject 1.0 (exclusive upper)
    let c = InferenceConfig { supports_cosine_threshold: 1.0, ..InferenceConfig::default() };
    assert!(c.validate(Path::new("/fake")).is_err());

    // accept 0.65 (nominal default)
    let c = InferenceConfig { supports_cosine_threshold: 0.65, ..InferenceConfig::default() };
    assert!(c.validate(Path::new("/fake")).is_ok());

    // accept 0.001 (just above exclusive lower)
    let c = InferenceConfig { supports_cosine_threshold: 0.001, ..InferenceConfig::default() };
    assert!(c.validate(Path::new("/fake")).is_ok());

    // accept 0.999 (just below exclusive upper)
    let c = InferenceConfig { supports_cosine_threshold: 0.999, ..InferenceConfig::default() };
    assert!(c.validate(Path::new("/fake")).is_ok());
}
```

### R-13: Config merge function propagates project-level override

```
fn test_merge_supports_cosine_threshold_project_override() {
    // project sets 0.70 (differs from default 0.65 by > f32::EPSILON)
    // assert merged config.inference.supports_cosine_threshold == 0.70
    // assert does NOT use global value (0.65 default)
}
```

### AC-17 (R-04): nli_post_store_k fully absent from config.rs

Static verification — not a runtime test:
```
// grep -n "nli_post_store_k" crates/unimatrix-server/src/infra/config.rs
// must return zero lines
```

### AC-18 (R-04): Serde forward-compatibility — config file with nli_post_store_k deserializes without error

```
fn test_serde_ignores_nli_post_store_k() {
    let toml_with_removed_field = r#"nli_post_store_k = 5"#;
    let result = toml::from_str::<InferenceConfig>(toml_with_removed_field);
    assert!(result.is_ok(),
        "deserializing config with removed field must not error (serde unknown field is silent)");
}
```

---

## Checklist

- [ ] Three sites updated for `supports_cosine_threshold`: field decl, backing fn, impl Default
- [ ] `impl Default` literal calls backing fn (`default_supports_cosine_threshold()`) not literal `0.65`
- [ ] validate() range check added for `supports_cosine_threshold`
- [ ] Merge function updated at the `nli_informs_cosine_floor` pattern site (f32 epsilon)
- [ ] All 6 `nli_post_store_k` sites removed (struct field, backing fn, impl Default, validate block, tests, merge)
- [ ] `grep "nli_post_store_k" config.rs` returns zero results
- [ ] Three independent default tests pass (backing fn, impl Default, serde)
- [ ] serde forward-compatibility test passes (AC-18)
- [ ] validate() boundary tests pass (AC-09)
- [ ] config merge test passes (R-13)
