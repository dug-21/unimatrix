# Test Plan: config_extension

## Component

**File modified:** `crates/unimatrix-server/src/infra/config.rs`

**Change:** Add `max_co_access_promotion_per_tick: usize` to `InferenceConfig` with:
- `#[serde(default = "default_max_co_access_promotion_per_tick")]`
- `fn default_max_co_access_promotion_per_tick() -> usize { 200 }`
- `validate()` range check [1, 10000]
- `Default` impl stanza
- `merge_configs()` stanza (project overrides global)

**Risks covered:** R-07 (merge_configs omission silently ignores project-level override)

---

## Unit Test Expectations

### Test: `test_max_co_access_promotion_per_tick_default`

**Covers:** AC-06(a)

**Arrange:**
- Deserialize an `InferenceConfig` from empty TOML string `""`

**Act:**
- Call `serde_toml::from_str::<InferenceConfig>("")`

**Assert:**
- `config.max_co_access_promotion_per_tick == 200`

**Location:** `crates/unimatrix-server/src/infra/config.rs` `#[cfg(test)]` block

---

### Test: `test_max_co_access_promotion_per_tick_validation_zero`

**Covers:** AC-06(b), AC-10, R-07

**Arrange:**
- Construct an `InferenceConfig` with `max_co_access_promotion_per_tick = 0`

**Act:**
- Call `InferenceConfig::validate()`

**Assert:**
- Returns `Err(ConfigError::NliFieldOutOfRange { .. })` (or whichever error variant `max_graph_inference_per_tick` uses)
- Error message string contains `"max_co_access_promotion_per_tick"`

**Location:** `crates/unimatrix-server/src/infra/config.rs` `#[cfg(test)]` block

---

### Test: `test_max_co_access_promotion_per_tick_validation_over_limit`

**Covers:** AC-06(c)

**Arrange:**
- Construct `InferenceConfig` with `max_co_access_promotion_per_tick = 10001`

**Act:**
- Call `InferenceConfig::validate()`

**Assert:**
- Returns `Err(_)`
- Error message contains `"max_co_access_promotion_per_tick"`

**Location:** `crates/unimatrix-server/src/infra/config.rs` `#[cfg(test)]` block

---

### Test: `test_max_co_access_promotion_per_tick_validation_boundary_values`

**Covers:** ADR-004 boundary compliance

**Arrange:**
- Two configs: one with value `1`, one with value `10000`

**Act:**
- Call `validate()` on each

**Assert:**
- `validate()` returns `Ok(())` for both boundary values
- Confirms the range is `[1, 10000]` inclusive (not `(1, 10000)` exclusive)

---

### Test: `test_merge_configs_project_overrides_global_co_access_cap`

**Covers:** AC-06(d), R-07

**Arrange:**
- `global_config.max_co_access_promotion_per_tick = 200`
- `project_config.max_co_access_promotion_per_tick = 50`

**Act:**
- Call `merge_configs(global_config, project_config)`

**Assert:**
- `merged.max_co_access_promotion_per_tick == 50`
- Project-level value wins

**Location:** `crates/unimatrix-server/src/infra/config.rs` `#[cfg(test)]` block

**Critical note:** This test directly targets R-07. If `merge_configs()` does not include the
new field in its stanza, the merge will return the global value (200) and this test will fail,
catching the omission before Gate 3c.

---

### Test: `test_merge_configs_global_only_co_access_cap`

**Covers:** R-07 (secondary scenario: project does not override)

**Arrange:**
- `global_config.max_co_access_promotion_per_tick = 300`
- `project_config` uses default (200) or is absent

**Act:**
- Call `merge_configs(global_config, project_config_with_default)`

**Assert:**
- `merged.max_co_access_promotion_per_tick == 300`
- Global value is preserved when project does not override

---

## Integration Test Expectations

No new infra-001 integration tests required. Config validation is unit-testable in-process.
The config field affects the LIMIT parameter of the batch SQL query — tested by
`co_access_promotion_tick.md` tests (AC-04: cap=3 selects only 3 highest-count pairs).

---

## Acceptance Criteria Mapped

| AC-ID | Test Function | Expected Result |
|-------|--------------|-----------------|
| AC-06(a) | `test_max_co_access_promotion_per_tick_default` | Deserializes to 200 |
| AC-06(b) | `test_max_co_access_promotion_per_tick_validation_zero` | Returns validation error |
| AC-06(c) | `test_max_co_access_promotion_per_tick_validation_over_limit` | Returns validation error |
| AC-06(d) | `test_merge_configs_project_overrides_global_co_access_cap` | Project value (50) wins over global (200) |
| AC-10 | `test_max_co_access_promotion_per_tick_validation_zero` | Error message contains field name |
