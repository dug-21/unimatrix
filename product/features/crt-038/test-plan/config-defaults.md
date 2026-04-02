# Test Plan: InferenceConfig Default Weight Constants

**Component**: `crates/unimatrix-server/src/infra/config.rs`
**AC Coverage**: AC-01
**Risk Coverage**: R-09, R-10
**Wave**: Wave 1 (parallel with effective-short-circuit)

---

## Unit Test Expectations

### Existing Tests (must be updated — not deleted)

#### `test_inference_config_weight_defaults_when_absent`

**Current state**: Asserts old default values (`w_sim=0.25`, `w_nli=0.35`, `w_conf=0.15`, `w_util=0.05`, `w_prov=0.05`, `nli_enabled=true`).

**Required change**: Update all assertions to reflect the new conf-boost-c defaults.

**Arrange**: Construct `InferenceConfig::default()` (or deserialize from empty TOML — the `#[serde(default = "...")]` decorators route to the `default_w_*()` functions when fields are absent from config).

**Act**: Access fields directly or deserialize from `""` (empty TOML input).

**Assert** (all must use exact equality `==`):
```rust
assert_eq!(config.w_sim,       0.50, "w_sim default changed to conf-boost-c");
assert_eq!(config.w_nli,       0.00, "w_nli default zeroed");
assert_eq!(config.w_conf,      0.35, "w_conf default raised from 0.15 to 0.35");
assert_eq!(config.w_util,      0.00, "w_util default zeroed");
assert_eq!(config.w_prov,      0.00, "w_prov default zeroed");
assert_eq!(config.w_coac,      0.00, "w_coac unchanged");
assert_eq!(config.nli_enabled, false, "nli_enabled default changed to false");
```

**Verify these are NOT in the assertions** (old values must not appear):
- `w_sim = 0.25` (old)
- `w_nli = 0.35` (old)
- `w_conf = 0.15` (old)
- `w_util = 0.05` (old)
- `w_prov = 0.05` (old)
- `nli_enabled = true` (old)

---

#### `test_inference_config_default_weights_sum_within_headroom`

**Current state**: Asserts that the sum of the six core weights is `≤ 0.95`.

**Required change**: The new sum is `0.50 + 0.00 + 0.35 + 0.00 + 0.00 + 0.00 = 0.85`. The assertion `sum ≤ 0.95` still holds (0.85 ≤ 0.95) — no change to the assertion logic is needed.

**Update only if**: The test asserts an exact old sum value (e.g., `assert_eq!(sum, 0.85f64)` where `0.85` was the old sum). Verify the test uses `≤` inequality, not exact equality. If it uses exact equality against the old sum, update the expected value to `0.85`.

**Assert**: `w_sim + w_nli + w_conf + w_coac + w_util + w_prov ≤ 0.95`. The full weight total including phase terms: `0.85 + 0.02 + 0.05 = 0.92 ≤ 1.0` (validate() constraint holds).

---

### New Tests (optional but recommended — R-10)

#### `test_inference_config_explicit_override_wins_over_zero_default`

**Rationale**: R-10 — operators may have non-default `w_util` or `w_prov` values in a config file. Verify that explicit TOML values override the new zero defaults.

**Arrange**: Deserialize `InferenceConfig` from a TOML string with explicit values:
```toml
w_util = 0.05
w_prov = 0.05
```

**Act**: Deserialize the TOML into `InferenceConfig`.

**Assert**:
```rust
assert_eq!(config.w_util, 0.05f64, "explicit TOML value overrides zero default");
assert_eq!(config.w_prov, 0.05f64, "explicit TOML value overrides zero default");
```

**Coverage**: Confirms `#[serde(default = "default_w_util")]` mechanism works correctly — TOML-specified values win over the function default.

**Priority**: Low — the existing test framework already exercises deserialization; this is belt-and-suspenders for R-10. Mark as optional if the existing test suite already covers explicit TOML deserialization.

---

## Integration Test Expectations

Config defaults flow into `FusionWeights::from_config` at server startup. The infra-001 `tools` and `lifecycle` suites implicitly test that the server starts with a valid config (any config deserialization failure would prevent the server from starting, causing all integration tests to fail at the handshake stage).

No new integration tests are needed for this component. Existing suites detect config-level regressions by catching server startup failures.

---

## Edge Cases

### `default_nli_enabled()` returns false

`nli_enabled` gates `run_graph_inference_tick` in `background.rs`. With the default now `false`, the tick does not run in production unless explicitly overridden. Verify the test for `nli_enabled` default is `false` and NOT the old `true`. This is a behavioral change — `SearchService.nli_enabled` and `background.rs` maintenance_tick both read this field.

**Specific risk**: A test asserting `nli_enabled=true` that was not updated will continue to compile and pass with the old default behavior if the assertion is accidentally inverted. Double-check the boolean direction.

### Additive phase weight terms

`w_phase_histogram` (`default=0.02`) and `w_phase_explicit` (`default=0.05`) are NOT changed by this feature. Verify these fields are not modified in the `default_w_*()` function changes. Their sum contribution (`0.07`) plus the core weight sum (`0.85`) = `0.92` is the expected total.

### validate() constraint

`InferenceConfig::validate()` checks `sum ≤ 1.0`. The new core weight sum is `0.85`; with phase terms `0.92`. Both are ≤ 1.0. The test `test_inference_config_default_weights_sum_within_headroom` covers this via the `≤ 0.95` bound. No new validate() test needed.

---

## Failure Modes

**Old default value left in one function**: `test_inference_config_weight_defaults_when_absent` fails with a clear equality mismatch. The function name tells you which weight is wrong.

**`nli_enabled` default not changed**: Test catches this (`assert_eq!(config.nli_enabled, false)`). Behavioral impact: server starts with NLI enabled by default — `run_graph_inference_tick` fires in production, `try_nli_rerank` can activate if the NLI model is available.

**Grep residual for old value in test body**: After updating the test, grep the test body for `0.35` to confirm `w_nli=0.35` (old) is gone and `w_conf=0.35` (new, correct) remains. The same value appears in different contexts — verify the field names match.

---

## Assertions Summary

| Test | Update Type | Key Assertion Change |
|------|-------------|---------------------|
| `test_inference_config_weight_defaults_when_absent` | Values changed | w_sim:0.25→0.50, w_nli:0.35→0.00, w_conf:0.15→0.35, w_util:0.05→0.00, w_prov:0.05→0.00, nli_enabled:true→false |
| `test_inference_config_default_weights_sum_within_headroom` | Verify no change needed | sum=0.85 ≤ 0.95 still holds; update only if test asserted an exact old value |
| `test_inference_config_explicit_override_wins_over_zero_default` | New (optional) | TOML explicit values override zero defaults |
