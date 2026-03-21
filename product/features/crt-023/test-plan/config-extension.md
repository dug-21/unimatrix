# Test Plan: Config Extension (`unimatrix-server/src/infra/config.rs`)

## Component Scope

File: `crates/unimatrix-server/src/infra/config.rs`

Changes: 10 new fields in `InferenceConfig`, extended `InferenceConfig::validate()`,
pool floor logic.

## Risks Covered

R-15 (High): Invalid `nli_model_name` reaches runtime if not caught at `validate()`.
R-22 (Med): `sha2` crate absent from server dependencies.
AC-07: All 10 fields present with correct defaults.
AC-17: All field ranges validated; cross-field invariant enforced.

---

## Unit Tests

### AC-07: Default Deserialization

```rust
#[test]
fn test_inference_config_nli_defaults_all_present() {
    // An empty [inference] section must deserialize with all 10 NLI fields at defaults.
    let config: InferenceConfig = toml::from_str("").unwrap();

    assert_eq!(config.nli_enabled, true);
    assert_eq!(config.nli_model_name, None);
    assert_eq!(config.nli_model_path, None);
    assert_eq!(config.nli_model_sha256, None);
    assert_eq!(config.nli_top_k, 20);
    assert_eq!(config.nli_post_store_k, 10);
    assert!((config.nli_entailment_threshold - 0.6f32).abs() < 1e-6);
    assert!((config.nli_contradiction_threshold - 0.6f32).abs() < 1e-6);
    assert_eq!(config.max_contradicts_per_tick, 10);
    assert!((config.nli_auto_quarantine_threshold - 0.85f32).abs() < 1e-6);
}

#[test]
fn test_inference_config_nli_fields_override_individually() {
    let toml_str = r#"
        nli_enabled = false
        nli_top_k = 5
        max_contradicts_per_tick = 1
    "#;
    let config: InferenceConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.nli_enabled, false);
    assert_eq!(config.nli_top_k, 5);
    assert_eq!(config.max_contradicts_per_tick, 1);
    // Other fields retain defaults
    assert_eq!(config.nli_post_store_k, 10);
}
```

### AC-17: Field-Level Range Validation

```rust
// Helper: build InferenceConfig with one field out of range, assert validate() errors.
fn assert_validate_fails_with_field(config: InferenceConfig, field_name: &str) {
    let err = config.validate().unwrap_err();
    assert!(
        err.to_string().contains(field_name),
        "Error message must name the offending field '{}'; got: '{}'", field_name, err
    );
}

#[test]
fn test_validate_nli_top_k_zero_fails() {
    let c = InferenceConfig { nli_top_k: 0, ..InferenceConfig::default() };
    assert_validate_fails_with_field(c, "nli_top_k");
}

#[test]
fn test_validate_nli_top_k_101_fails() {
    let c = InferenceConfig { nli_top_k: 101, ..InferenceConfig::default() };
    assert_validate_fails_with_field(c, "nli_top_k");
}

#[test]
fn test_validate_nli_post_store_k_zero_fails() {
    let c = InferenceConfig { nli_post_store_k: 0, ..InferenceConfig::default() };
    assert_validate_fails_with_field(c, "nli_post_store_k");
}

#[test]
fn test_validate_nli_entailment_threshold_zero_fails() {
    let c = InferenceConfig { nli_entailment_threshold: 0.0, ..InferenceConfig::default() };
    assert_validate_fails_with_field(c, "nli_entailment_threshold");
}

#[test]
fn test_validate_nli_entailment_threshold_one_fails() {
    let c = InferenceConfig { nli_entailment_threshold: 1.0, ..InferenceConfig::default() };
    assert_validate_fails_with_field(c, "nli_entailment_threshold");
}

#[test]
fn test_validate_nli_contradiction_threshold_out_of_range_fails() {
    let c = InferenceConfig { nli_contradiction_threshold: 1.1, ..InferenceConfig::default() };
    assert_validate_fails_with_field(c, "nli_contradiction_threshold");
}

#[test]
fn test_validate_max_contradicts_per_tick_zero_fails() {
    let c = InferenceConfig { max_contradicts_per_tick: 0, ..InferenceConfig::default() };
    assert_validate_fails_with_field(c, "max_contradicts_per_tick");
}

#[test]
fn test_validate_nli_auto_quarantine_threshold_out_of_range_fails() {
    let c = InferenceConfig { nli_auto_quarantine_threshold: 0.0, ..InferenceConfig::default() };
    assert_validate_fails_with_field(c, "nli_auto_quarantine_threshold");
}

#[test]
fn test_validate_nli_model_sha256_wrong_length_fails() {
    // nli_model_sha256 must be exactly 64 hex chars when set.
    let c = InferenceConfig {
        nli_model_sha256: Some("short_hash".to_string()),
        ..InferenceConfig::default()
    };
    assert_validate_fails_with_field(c, "nli_model_sha256");
}

#[test]
fn test_validate_nli_model_sha256_non_hex_fails() {
    let c = InferenceConfig {
        nli_model_sha256: Some("z".repeat(64)), // 64 chars but not hex
        ..InferenceConfig::default()
    };
    assert_validate_fails_with_field(c, "nli_model_sha256");
}
```

### AC-17 + R-15: nli_model_name Validation

```rust
#[test]
fn test_validate_unrecognized_model_name_fails() {
    // R-15: invalid nli_model_name must be caught at validate(), not at start_loading().
    let c = InferenceConfig {
        nli_model_name: Some("gpt4".to_string()),
        ..InferenceConfig::default()
    };
    assert_validate_fails_with_field(c, "nli_model_name");
}

#[test]
fn test_validate_recognized_model_names_pass() {
    for name in ["minilm2", "deberta"] {
        let c = InferenceConfig {
            nli_model_name: Some(name.to_string()),
            ..InferenceConfig::default()
        };
        assert!(c.validate().is_ok(), "Model name '{}' should pass validation", name);
    }
}
```

### AC-17 Cross-Field Invariant: nli_auto_quarantine_threshold > nli_contradiction_threshold

```rust
#[test]
fn test_validate_auto_quarantine_equal_to_contradiction_threshold_fails() {
    // nli_auto_quarantine_threshold must be STRICTLY greater than nli_contradiction_threshold.
    let c = InferenceConfig {
        nli_contradiction_threshold: 0.7,
        nli_auto_quarantine_threshold: 0.7, // equal, not greater
        ..InferenceConfig::default()
    };
    let err = c.validate().unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("nli_auto_quarantine_threshold"),
        "Error must name nli_auto_quarantine_threshold; got: '{msg}'");
    assert!(msg.contains("nli_contradiction_threshold"),
        "Error must name nli_contradiction_threshold; got: '{msg}'");
}

#[test]
fn test_validate_auto_quarantine_less_than_contradiction_fails() {
    let c = InferenceConfig {
        nli_contradiction_threshold: 0.7,
        nli_auto_quarantine_threshold: 0.65, // less than contradiction
        ..InferenceConfig::default()
    };
    let err = c.validate().unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("nli_auto_quarantine_threshold") && msg.contains("nli_contradiction_threshold"));
}

#[test]
fn test_validate_auto_quarantine_greater_than_contradiction_passes() {
    let c = InferenceConfig {
        nli_contradiction_threshold: 0.6,
        nli_auto_quarantine_threshold: 0.85,
        ..InferenceConfig::default()
    };
    assert!(c.validate().is_ok());
}
```

### AC-19: Separate nli_top_k and nli_post_store_k

```rust
#[test]
fn test_nli_top_k_and_post_store_k_are_independent() {
    // Setting one must not affect the other.
    let c = InferenceConfig {
        nli_top_k: 50,
        nli_post_store_k: 3,
        ..InferenceConfig::default()
    };
    assert_eq!(c.nli_top_k, 50);
    assert_eq!(c.nli_post_store_k, 3);
    assert!(c.validate().is_ok());
}
```

### R-22: sha2 Crate Build Gate

This is a compile-time / CI check, not a runtime test:

```rust
// Verification: cargo check -p unimatrix-server must compile without errors.
// Additional: cargo tree -p unimatrix-server | grep sha2 must return a result.
// This is documented in the delivery report, not as a test function.
```

---

## Integration Assertions (via infra-001)

The config validation path is exercised at server startup. The infra-001 tests do not
directly test config validation (they run against a successfully started server). Config
validation is a pure unit concern — if `validate()` fails, the server never starts,
which is caught by the startup integration test pattern in `tools` suite
(test_server_starts_and_responds_to_initialize).

The one config-related integration test worth adding is:
- Server with a full NLI config section (valid values) starts and reports NLI status
  (AC-07 round-trip: deserialized, validated, wired into AppState).
