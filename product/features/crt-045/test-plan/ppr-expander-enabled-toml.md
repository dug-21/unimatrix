# Test Plan: ppr-expander-enabled.toml
# File: product/research/ass-037/harness/profiles/ppr-expander-enabled.toml

## Component Summary

The `ppr-expander-enabled.toml` profile TOML currently causes `eval run` to fail at parse
time with `EvalError::ConfigInvariant` because `distribution_change=true` is declared
without the required `[profile.distribution_targets]` sub-table. crt-045 fixes the TOML
by setting `distribution_change=false` with human-approved metric gates and a TOML comment
explaining the intentional choice.

Primary risk for this component is **R-05**: parse failure before the graph code is even
reached, and **SR-04**: a future editor re-introduces `distribution_change=true` without
the required targets.

---

## Unit Test Expectations

### Test: `test_ppr_expander_enabled_profile_parses_cleanly` (NEW — AC-03, R-05)

**Location:** `crates/unimatrix-server/src/eval/profile/tests.rs`
(existing file for profile parsing tests — extend, do not create a new file)

**Purpose:** Assert that the fixed TOML parses without error and all required field
values are correct per ADR-005 and SPECIFICATION.md C-06.

**Arrange:**
```rust
// Read the actual profile file from the repository
let toml_content = std::fs::read_to_string(
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../product/research/ass-037/harness/profiles/ppr-expander-enabled.toml"
    )
).expect("ppr-expander-enabled.toml must exist");
```

**Act:**
```rust
let result = parse_profile_toml(&toml_content);
```

**Assert:**
```rust
let profile = result.expect("ppr-expander-enabled.toml must parse without EvalError::ConfigInvariant");

// ADR-005: distribution_change must be false (C-06, OQ-01 resolution)
assert!(
    !profile.distribution_change,
    "distribution_change must be false in ppr-expander-enabled.toml (ADR-005)"
);

// ADR-005: metric gates must match human-approved thresholds (C-06)
// mrr_floor = 0.2651
// p_at_5_min = 0.1083
// (exact field access depends on EvalProfile structure — verify field names)
if let Some(gates) = &profile.gates {
    assert!(
        (gates.mrr_floor - 0.2651).abs() < 1e-6,
        "mrr_floor must be 0.2651 (no regression from crt-042 baseline)"
    );
    assert!(
        (gates.p_at_5_min - 0.1083).abs() < 1e-6,
        "p_at_5_min must be 0.1083 (first-run improvement gate)"
    );
}

// Inference section: ppr_expander_enabled must be true
assert!(
    profile.config_overrides.inference.ppr_expander_enabled,
    "ppr_expander_enabled must be true in ppr-expander-enabled.toml"
);
```

**Fallback assertion if gate fields are accessed differently:** Assert that
`parse_profile_toml()` returns `Ok(_)` and does not return
`Err(EvalError::ConfigInvariant(_))` — this is the non-negotiable check for AC-03.

---

### Test: `test_ppr_expander_enabled_no_distribution_targets_block` (NEW — SR-04 regression guard)

**Purpose:** Assert that `distribution_targets` is `None` when `distribution_change=false`.
This prevents a future editor from accidentally re-introducing a partial
`distribution_targets` block.

**Act + Assert:**
```rust
let profile = parse_profile_toml(&toml_content).expect("must parse");
assert!(
    profile.distribution_targets.is_none(),
    "distribution_targets must be absent when distribution_change=false (prevents EC-05 ambiguity)"
);
```

**Note on EC-05:** If the TOML parser accepts a `[profile.distribution_targets]` block
when `distribution_change=false` (i.e., targets are structurally optional when the flag
is false), this test should instead assert that parsing does not error — not that targets
are absent. Verify the actual `parse_profile_toml()` behavior before writing this
assertion.

---

### Test: `test_distribution_change_true_without_targets_returns_config_invariant` (EXISTING — regression guard)

This test should already exist in `eval/profile/tests.rs` or should be added alongside
the new test. It verifies that the original bug remains detectable:

**Assert:**
```rust
let broken_toml = r#"
[profile]
distribution_change = true
# No [profile.distribution_targets] block

[inference]
ppr_expander_enabled = true
"#;
let result = parse_profile_toml(broken_toml);
assert!(
    matches!(result, Err(EvalError::ConfigInvariant(_))),
    "distribution_change=true without targets must return ConfigInvariant"
);
```

This is a regression guard: if `parse_profile_toml()` is ever relaxed to not require
targets, this test catches the regression.

---

## Integration Test Expectations (Manual — AC-02, AC-03)

These cannot be automated without a live snapshot containing real graph edges and
embedding vectors.

### Manual AC-03 Verification

```bash
# Must exit 0 and produce metric output (not EvalError::ConfigInvariant)
unimatrix eval run --profile ppr-expander-enabled.toml
```

Expected: no `ConfigInvariant` error. `mrr_floor` and `p_at_5_min` gates are evaluated
against actual metric output.

### Manual AC-02 Verification (pre-merge)

```bash
# Run both profiles against the same populated snapshot
unimatrix eval run --profile baseline.toml > /tmp/baseline-results.txt
unimatrix eval run --profile ppr-expander-enabled.toml > /tmp/ppr-results.txt
diff /tmp/baseline-results.txt /tmp/ppr-results.txt
```

Expected: MRR and P@5 values differ (not bit-identical). Any difference confirms that
`ppr_expander_enabled=true` with a rebuilt graph is activating the PPR+graph_expand path.

### Manual R-09 Verification (pre-merge)

```bash
# Delivery agent must confirm current baseline MRR >= 0.2651
unimatrix eval run --profile baseline.toml | grep MRR
```

If MRR < 0.2651, the `mrr_floor` threshold must be revised via scope variance flag
before merge. Do not silently lower the floor.

---

## TOML Content Assertions (code review)

The following elements must be present in the fixed TOML file (PR review checklist):

| Element | Required Value | Rationale |
|---------|---------------|-----------|
| `distribution_change` | `false` | ADR-005, C-06, OQ-01 |
| `mrr_floor` | `0.2651` | C-06, ADR-005 |
| `p_at_5_min` | `0.1083` | C-06, ADR-005 |
| `ppr_expander_enabled` | `true` | Feature purpose |
| TOML comment on `distribution_change` | Explanation of intentional `false` | SR-04 guard |

TOML comment that must be present (per ADR-005):
```toml
# distribution_change = false intentionally.
# CC@k and ICD floors cannot be set without a first-run measurement.
# Gate on mrr_floor and p_at_5_min only until baseline data is collected.
# See crt-045 ADR-005 and SCOPE.md OQ-01.
```

---

## Edge Cases

| Edge Case | Expected Behavior |
|-----------|------------------|
| EC-05: TOML has `distribution_change=false` AND `[profile.distribution_targets]` block | Parser must accept — targets are structurally optional when flag is false |
| NaN or infinity in `mrr_floor` | Gate comparison may fail silently (SR-SEC-02). Acceptable risk — developer-facing file, not user input. |
| Missing `[inference]` section in ppr-expander-enabled.toml | `ppr_expander_enabled` defaults to `false`; fix must ensure section is present |

---

## Specific Assertions Summary

| Assertion | Method | Guards |
|-----------|--------|--------|
| `parse_profile_toml()` returns `Ok(profile)` | `result.expect(...)` | AC-03, R-05 |
| `profile.distribution_change == false` | `assert!(!profile.distribution_change)` | ADR-005, C-06 |
| `profile.config_overrides.inference.ppr_expander_enabled == true` | `assert!(...)` | Feature purpose |
| `mrr_floor == 0.2651` | float comparison with 1e-6 tolerance | C-06, ADR-005 |
| `p_at_5_min == 0.1083` | float comparison with 1e-6 tolerance | C-06, ADR-005 |
| `distribution_targets.is_none()` | `assert!(profile.distribution_targets.is_none())` | SR-04 |
| TOML comment present | Code review gate | SR-04 |
