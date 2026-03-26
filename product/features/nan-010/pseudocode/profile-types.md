# Component 1: Profile Types

**File**: `eval/profile/types.rs`
**Action**: Modify

---

## Purpose

Extend `EvalProfile` with two new fields and introduce `DistributionTargets`. These types
are the in-memory representation; they carry no serde derives. The JSON sidecar uses
separate `ProfileMetaEntry` / `DistributionTargetsJson` types in `runner/profile_meta.rs`.

Consistent with the existing pattern where `UnimatrixConfig` is parsed from TOML but never
written back — in-memory types stay free of serde to avoid accidental serialization.

---

## New Type: `DistributionTargets`

```
// Human-specified floor values for the distribution gate.
// All three fields are required together when distribution_change = true.
// No serde derives — in-memory only. JSON representation lives in
// DistributionTargetsJson in runner/profile_meta.rs.
pub struct DistributionTargets {
    pub cc_at_k_min: f64,    // minimum mean CC@k required
    pub icd_min: f64,        // minimum mean ICD required
    pub mrr_floor: f64,      // absolute minimum mean MRR (veto, not co-equal target)
}
```

Derive: `Debug`, `Clone`. No `Copy` (struct with f64 fields, Clone is sufficient).
No `Default` — partial construction is not valid; all three fields are always required.

---

## Modified Type: `EvalProfile`

Append two fields to the existing struct. No changes to existing fields.

```
pub struct EvalProfile {
    // --- existing fields (unchanged) ---
    pub name: String,
    pub description: Option<String>,
    pub config_overrides: UnimatrixConfig,

    // --- new fields (nan-010) ---
    /// Whether this profile declares an intentional distribution shift.
    /// Default: false. When true, distribution_targets is populated.
    pub distribution_change: bool,

    /// Distribution gate targets. Some(_) when distribution_change = true.
    /// None when distribution_change = false.
    pub distribution_targets: Option<DistributionTargets>,
}
```

No `Default` derive on `EvalProfile` — construction is done by `parse_profile_toml` only.
Doc comment must note: "populated by parse_profile_toml in validation.rs; never construct
directly".

---

## Data Flow

Inputs: none (type definitions only)
Outputs: `DistributionTargets` and extended `EvalProfile` used by:
- `eval/profile/validation.rs` (construction)
- `eval/runner/profile_meta.rs` (reads fields to build sidecar types)
- `eval/report/aggregate/distribution.rs` (reads `DistributionTargets` via profile_meta)

---

## Error Handling

No functions in this file — type definitions only. No errors produced here.

---

## Key Test Scenarios

Tests live in `eval/profile/tests.rs` (not this file). This component is fully tested
through parse round-trips in validation tests.

```
test_parse_distribution_change_profile_valid:
    Given: TOML with distribution_change = true + valid [profile.distribution_targets]
    Assert: EvalProfile.distribution_change == true
            EvalProfile.distribution_targets == Some(DistributionTargets {
                cc_at_k_min: <value from TOML>,
                icd_min: <value from TOML>,
                mrr_floor: <value from TOML>,
            })

test_parse_no_distribution_change_flag:
    Given: TOML with no distribution_change key
    Assert: EvalProfile.distribution_change == false
            EvalProfile.distribution_targets == None
```

---

## Notes

- `DistributionTargets` is introduced in this file, not in validation.rs. Validation.rs
  reads from `raw TOML` and constructs `DistributionTargets` — the type must exist here first.
- The fields appended to `EvalProfile` follow the existing pattern: doc comment, then field.
- No re-exports needed beyond the existing `pub use types::{AnalyticsMode, EvalProfile}`
  in `profile/mod.rs` — `DistributionTargets` must also be added to that re-export list so
  that `runner/profile_meta.rs` can import it from `crate::eval::profile::DistributionTargets`.
