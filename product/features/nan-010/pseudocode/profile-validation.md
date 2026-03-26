# Component 2: Profile Validation

**File**: `eval/profile/validation.rs`
**Action**: Modify

---

## Purpose

Extend `parse_profile_toml` to extract `distribution_change` and `distribution_targets` from
the raw TOML value before the `[profile]` section is stripped for `UnimatrixConfig`
deserialization. Validate completeness at parse time and reject invalid configurations with
`EvalError::ConfigInvariant`.

Extraction follows the established pattern at lines 66-80 (name, description are already
extracted before the strip). The same `raw.get("profile")` access point is used.

---

## Modified Function: `parse_profile_toml`

**Signature** (unchanged):
```
pub(crate) fn parse_profile_toml(path: &Path) -> Result<EvalProfile, EvalError>
```

**Pseudocode** (only the new insertion block; existing steps 1-5 shown for context):

```
fn parse_profile_toml(path: &Path) -> Result<EvalProfile, EvalError>:

    // (existing) Read file
    content = fs::read_to_string(path)?   // EvalError::Io on failure

    // (existing) Parse TOML
    raw: toml::Value = toml::from_str(content)?  // EvalError::ConfigInvariant on parse error

    // (existing) Extract name (required)
    name = raw.get("profile").get("name").as_str()
              .ok_or(ConfigInvariant("[profile].name is required"))?

    // (existing) Extract description (optional)
    description = raw.get("profile").get("description").as_str()

    // ---- NEW: Extract distribution_change and distribution_targets ----
    // Must happen BEFORE the [profile] section is stripped (FR-04, SR-07 constraint).
    // Stripping happens below; extraction here is the invariant.

    profile_section = raw.get("profile")  // Option<&toml::Value>

    // Step A: read distribution_change flag (optional boolean, default false)
    distribution_change: bool =
        profile_section
            .and_then(|p| p.get("distribution_change"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)

    // Step B: if flag is set, extract and validate targets
    distribution_targets: Option<DistributionTargets> =
        if distribution_change:
            // Step B1: check baseline rejection (ADR-001, constraint 8)
            // The baseline is identified as the first profile in the configs slice.
            // parse_profile_toml does not know position — baseline check is by name.
            // IMPLEMENTATION NOTE: The baseline check is: if name == "baseline"
            // (case-insensitive) AND distribution_change == true → ConfigInvariant.
            // This is determined here before any further validation.
            if name.eq_ignore_ascii_case("baseline"):
                return Err(ConfigInvariant(
                    "baseline profile must not declare `distribution_change = true`"
                ))

            // Step B2: locate [profile.distribution_targets] sub-table
            targets_table = profile_section
                .and_then(|p| p.get("distribution_targets"))
                .and_then(|v| v.as_table())

            if targets_table is None:
                return Err(ConfigInvariant(
                    "[profile.distribution_targets] is required when distribution_change = true"
                ))

            // Step B3: extract all three required fields
            cc_at_k_min = targets_table.get("cc_at_k_min").and_then(|v| v.as_float())
            icd_min     = targets_table.get("icd_min").and_then(|v| v.as_float())
            mrr_floor   = targets_table.get("mrr_floor").and_then(|v| v.as_float())

            // Step B4: each field is required; name the missing one explicitly (NFR-06)
            if cc_at_k_min is None:
                return Err(ConfigInvariant(
                    "[profile.distribution_targets].cc_at_k_min is required"
                ))
            if icd_min is None:
                return Err(ConfigInvariant(
                    "[profile.distribution_targets].icd_min is required"
                ))
            if mrr_floor is None:
                return Err(ConfigInvariant(
                    "[profile.distribution_targets].mrr_floor is required"
                ))

            Some(DistributionTargets {
                cc_at_k_min: cc_at_k_min.unwrap(),
                icd_min: icd_min.unwrap(),
                mrr_floor: mrr_floor.unwrap(),
            })

        else:
            // distribution_change = false or absent → targets not needed
            None

    // ---- END NEW ----

    // (existing) Strip [profile] section from raw value
    config_value = raw.clone()
    config_value.as_table_mut().remove("profile")

    // (existing) Serialize + deserialize as UnimatrixConfig
    config_str = toml::to_string(config_value)?
    config_overrides: UnimatrixConfig = toml::from_str(config_str)?

    // (existing + extended) Construct EvalProfile with new fields
    Ok(EvalProfile {
        name,
        description,
        config_overrides,
        distribution_change,         // NEW
        distribution_targets,        // NEW
    })
```

---

## Baseline Profile Detection

The baseline check relies on the profile `name` field being `"baseline"` (case-insensitive).
This is consistent with how `compute_aggregate_stats` in `aggregate.rs` identifies the
baseline profile (it uses `to_lowercase() == "baseline"`).

Callers of `parse_profile_toml` do not pass a "is this baseline?" flag — the name is
sufficient. If a user names a non-baseline profile "baseline", they get this error. This is
correct behavior.

---

## Existing Function: `validate_confidence_weights`

No changes to this function. It is called by `EvalServiceLayer::from_profile` (in
`profile/layer.rs`), not by `parse_profile_toml` directly. The call chain is unchanged.

---

## Data Flow

Inputs: `path: &Path` (TOML file on disk)
Outputs: `Result<EvalProfile, EvalError>`
- `Ok(EvalProfile { ..., distribution_change, distribution_targets })` on success
- `Err(EvalError::ConfigInvariant(...))` for any validation failure
- `Err(EvalError::Io(...))` for file read failure

---

## Error Handling

| Condition | Error |
|-----------|-------|
| File unreadable | `EvalError::Io(e)` |
| Invalid TOML syntax | `EvalError::ConfigInvariant("failed to parse profile TOML at {path}: {e}")` |
| Missing `[profile].name` | `EvalError::ConfigInvariant("[profile].name is required in profile TOML")` |
| Baseline + `distribution_change=true` | `EvalError::ConfigInvariant("baseline profile must not declare \`distribution_change = true\`")` |
| `distribution_change=true`, no targets table | `EvalError::ConfigInvariant("[profile.distribution_targets] is required when distribution_change = true")` |
| `distribution_change=true`, missing `cc_at_k_min` | `EvalError::ConfigInvariant("[profile.distribution_targets].cc_at_k_min is required")` |
| `distribution_change=true`, missing `icd_min` | `EvalError::ConfigInvariant("[profile.distribution_targets].icd_min is required")` |
| `distribution_change=true`, missing `mrr_floor` | `EvalError::ConfigInvariant("[profile.distribution_targets].mrr_floor is required")` |

All errors are returned before `EvalServiceLayer` construction — purely at parse time (FR-03,
constraint 4).

---

## Key Test Scenarios

Tests in `eval/profile/tests.rs` (existing file, extended):

```
test_parse_distribution_change_profile_valid:
    TOML: [profile] name="candidate", distribution_change=true
          [profile.distribution_targets] cc_at_k_min=0.60, icd_min=1.20, mrr_floor=0.35
    Assert: Ok(EvalProfile) with distribution_change=true,
            distribution_targets=Some(DistributionTargets{0.60,1.20,0.35})

test_parse_distribution_change_missing_targets:
    TOML: [profile] name="candidate", distribution_change=true
          (no [profile.distribution_targets])
    Assert: Err(ConfigInvariant) with message containing "distribution_targets"

test_parse_distribution_change_missing_cc_at_k:
    TOML: distribution_change=true, targets table present, cc_at_k_min absent
    Assert: Err(ConfigInvariant) with message containing "cc_at_k_min"

test_parse_distribution_change_missing_icd:
    TOML: distribution_change=true, targets table present, icd_min absent
    Assert: Err(ConfigInvariant) with message containing "icd_min"

test_parse_distribution_change_missing_mrr_floor:
    TOML: distribution_change=true, targets table present, mrr_floor absent
    Assert: Err(ConfigInvariant) with message containing "mrr_floor"

test_parse_no_distribution_change_flag:
    TOML: [profile] name="candidate" (no distribution_change key)
    Assert: Ok(EvalProfile) with distribution_change=false, distribution_targets=None

test_distribution_gate_baseline_rejected:
    (also in tests_distribution_gate.rs for report side — but parse test goes here)
    TOML: [profile] name="baseline", distribution_change=true
          [profile.distribution_targets] cc_at_k_min=0.6, icd_min=1.2, mrr_floor=0.35
    Assert: Err(ConfigInvariant) with message
            "baseline profile must not declare `distribution_change = true`"
```

---

## Notes

- Extraction must occur before the `table.remove("profile")` call on line 86 of the
  current file. The code ordering is: read name → read description → read
  distribution_change → read distribution_targets → strip [profile] → deserialize config.
- `toml::Value::as_float()` returns `Option<f64>` — use this for the three f64 fields.
- `toml::Value::as_bool()` returns `Option<bool>` — use this for the flag.
- No regex, no manual string scanning — use `toml::Value` tree navigation only.
