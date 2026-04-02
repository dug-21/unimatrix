# Test Plan: infra/config.rs — InferenceConfig

Component: `crates/unimatrix-server/src/infra/config.rs`
Pseudocode: `product/features/crt-039/pseudocode/config.md`

---

## What Changes

1. `default_nli_informs_cosine_floor()` return value: `0.45_f32` → `0.5_f32`
2. `InferenceConfig::default()` field `nli_informs_cosine_floor`: `0.45` → `0.5`
3. No validation changes — 0.5 is within existing `(0.0, 1.0)` exclusive range (C-09)
4. No other `InferenceConfig` fields change

The `nli_enabled` flag, `informs_category_pairs`, `supports_candidate_threshold`,
`max_graph_inference_per_tick`, and all other fields are unchanged.

---

## TC-06 — `test_cosine_floor_default` (Unit)

**Risk**: R-05 (candidate pool), R-12 (boundary semantics)
**AC**: AC-04, FR-07, ADR-003

**Existing test to update**: `test_inference_config_default_nli_informs_cosine_floor` at line ~6853.

```rust
#[test]
fn test_inference_config_default_nli_informs_cosine_floor() {
    // After crt-039: default must be 0.5, not 0.45
    assert_eq!(
        default_nli_informs_cosine_floor(),
        0.5_f32,
        "TC-06a: default_nli_informs_cosine_floor() must return 0.5"
    );
    assert_eq!(
        InferenceConfig::default().nli_informs_cosine_floor,
        0.5_f32,
        "TC-06b: InferenceConfig::default() nli_informs_cosine_floor must be 0.5"
    );
}
```

**Grep verification (AC-08)**:
```bash
grep -n '0.45' crates/unimatrix-server/src/infra/config.rs
```
Must return no assertions in the test section asserting `nli_informs_cosine_floor == 0.45`.
(The function body `0.45` reference must be replaced with `0.5`.)

---

## Existing Tests to Update

### `test_validate_nli_informs_cosine_floor_valid_value_is_ok` (line ~6930)

This test asserts a nominal valid value passes validation. After crt-039, use `0.5` as the
nominal value (was `0.45` or similar):

```rust
#[test]
fn test_validate_nli_informs_cosine_floor_valid_value_is_ok() {
    let config = InferenceConfig {
        nli_informs_cosine_floor: 0.5_f32, // was 0.45
        ..InferenceConfig::default()
    };
    assert!(config.validate().is_ok(), "0.5 is a valid nli_informs_cosine_floor");
}
```

### `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold` (in nli_detection_tick.rs)

Update the band from `[0.45, 0.50)` to `[0.50, supports_threshold)`. Use cosine = 0.50
(inclusive floor) to prove the test is testing the correct boundary:

```rust
// After crt-039: floor = 0.50, threshold = 0.65 (default)
// Band that differentiates "floor vs threshold" is [0.50, 0.65)
let cosine_at_floor = 0.50_f32; // exactly at floor — included by Phase 4b (>=), excluded by Phase 4 (>)
let config = InferenceConfig::default();
assert_eq!(config.nli_informs_cosine_floor, 0.5_f32, "sanity: floor is 0.5 after crt-039");

let (src_cat, tgt_cat) = (config.informs_category_pairs[0][0].as_str(), ...);
let phase4b_accepts = phase4b_candidate_passes_guards(
    cosine_at_floor, src_cat, tgt_cat, 1_000, 2_000, "crt-020", "crt-030", &config
);
assert!(
    phase4b_accepts,
    "AC-18 (updated): cosine 0.50 >= nli_informs_cosine_floor 0.50 must be accepted by Phase 4b"
);

// Verify cosine is below supports threshold (strict), so Phase 4 would not select it
assert!(
    cosine_at_floor <= config.supports_candidate_threshold,
    "sanity: cosine at floor is not above supports threshold"
);
```

---

## Existing Tests to Retain Unchanged

All validation boundary tests for `nli_informs_cosine_floor` remain valid — 0.5 is still
within `(0.0, 1.0)` exclusive:

| Test | Status |
|------|--------|
| `test_validate_nli_informs_cosine_floor_zero_is_error` | Retain unchanged |
| `test_validate_nli_informs_cosine_floor_one_is_error` | Retain unchanged |
| `test_validate_nli_informs_cosine_floor_near_boundaries` | Retain unchanged |
| `test_inference_config_default_passes_validate` | Retain — 0.5 still passes validate() |
| `test_inference_config_defaults` | Update if it asserts `nli_informs_cosine_floor == 0.45` |

---

## AC-09 — Floor Default Assertion Grep

After updating all tests, verify no test still asserts the old 0.45 default:

```bash
grep -n 'nli_informs_cosine_floor.*0\.45\|0\.45.*nli_informs_cosine_floor' \
  crates/unimatrix-server/src/infra/config.rs
```
Must return empty in test section.

---

## Risk Coverage for This Component

| Risk | Test | Verification |
|------|------|-------------|
| R-05: Floor raise eliminates candidate pool | AC-11 eval gate (external) | Eval harness MRR >= 0.2913 |
| R-12: `>=` semantics preserved | TC-05 in nli_detection_tick.rs uses 0.500/0.499 boundary values | Unit test |
| ADR-003: Default value correct | TC-06 (`test_inference_config_default_nli_informs_cosine_floor`) | Unit test |
| C-09: 0.5 passes validation | `test_inference_config_default_passes_validate` (existing) | Unit test |

---

## No New Validation Logic

`InferenceConfig::validate()` is not changed. The existing `(0.0, 1.0)` exclusive range
check covers 0.5 correctly. No new validation test is required.

The `nli_enabled` flag continues to gate NLI cross-encoder, rayon pool floor of 6, and
contradiction scan scheduling. Its default and behavior are unchanged. No test changes
for `nli_enabled` handling.
