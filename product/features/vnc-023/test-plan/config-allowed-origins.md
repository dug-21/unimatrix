# Test Plan: config-allowed-origins (C5)

## Component

`crates/unimatrix-server/src/infra/config.rs` -- add `allowed_origins: Vec<String>` field to `HttpConfig`.

## Risks Covered

- **R-04 (High)**: allowed_origins config wiring disconnected (first hop)
- **R-09 (Medium)**: Backward-incompatible config deserialization

## Unit Test Expectations

### T-01: Default HttpConfig has empty allowed_origins (R-09, AC-09)
```
arrange: (none)
act:     let config = HttpConfig::default();
assert:  config.allowed_origins == Vec::<String>::new()
```

### T-02: TOML without allowed_origins deserializes successfully (R-09, AC-09)
```
arrange: toml_str = r#"
         enabled = true
         content_port = 8443
         "#
act:     let config: HttpConfig = toml::from_str(toml_str).unwrap();
assert:  config.allowed_origins.is_empty()
assert:  config.enabled == true
assert:  config.content_port == 8443
```

### T-03: TOML with allowed_origins deserializes correctly (R-04, AC-09)
```
arrange: toml_str = r#"
         enabled = true
         allowed_origins = ["https://claude.ai", "vscode-webview://abc"]
         "#
act:     let config: HttpConfig = toml::from_str(toml_str).unwrap();
assert:  config.allowed_origins == vec!["https://claude.ai", "vscode-webview://abc"]
```

### T-04: TOML with empty allowed_origins array (edge case)
```
arrange: toml_str = r#"
         enabled = true
         allowed_origins = []
         "#
act:     let config: HttpConfig = toml::from_str(toml_str).unwrap();
assert:  config.allowed_origins.is_empty()
```

### T-05: Full config.toml without allowed_origins (R-09 regression guard)
```
arrange: full_toml_str with all existing HttpConfig fields but NO allowed_origins
act:     let config: HttpConfig = toml::from_str(full_toml_str).unwrap();
assert:  config.allowed_origins.is_empty()
assert:  all other fields retain their specified values
```

## Compile Gate

### C-01: HttpConfig has allowed_origins field with #[serde(default)]
- **Assert**: `grep 'allowed_origins' crates/unimatrix-server/src/infra/config.rs` shows field definition
- **Assert**: `HttpConfig` struct has `#[serde(default)]` on the struct (already present) ensuring absent fields get defaults

## Integration Test Expectations

None for this component alone -- config deserialization is a unit-testable concern. Integration coverage comes through router-origin-wiring and main-call-site.

## Edge Cases from Risk Strategy

- **Existing config.toml files without allowed_origins (R-09)**: T-02 and T-05 cover this. The `#[serde(default)]` attribute is the defense.
- **allowed_origins with unusual strings**: Origins with ports (`https://example.com:443`), paths (`https://example.com/path`), or protocol-only (`vscode-webview://`) should deserialize correctly. T-03 tests one such case. Validation is rmcp's responsibility, not ours.
