# Test Plan: config-extension

**Component**: `crates/unimatrix-server/src/infra/config.rs`
**AC Coverage**: AC-03
**Risk Coverage**: R-09 (startup failure behavior), R-10 (partial — CategoryAllowlist ordering)

---

## Unit Test Expectations

### Location: inline `#[cfg(test)]` in `unimatrix-server/src/infra/config.rs`

### T-CFG-01: ObservationConfig deserializes from absent section as default (AC-03)

```rust
// test_observation_config_absent_section_is_default
// Arrange: TOML string with no [observation] section
// Act: toml::from_str::<UnimatrixConfig>(&toml_str)
// Assert: config.observation.domain_packs.is_empty()
// This verifies #[serde(default)] is correctly applied.
// AC-03: the registry then loads the built-in pack regardless.
```

### T-CFG-02: ObservationConfig deserializes from TOML domain_packs array

```rust
// test_observation_config_toml_domain_pack_deserialization
// Arrange: TOML string:
//   [[observation.domain_packs]]
//   source_domain = "sre"
//   event_types = ["incident_opened", "incident_resolved"]
//   categories = ["runbook", "post-mortem"]
// Act: toml::from_str::<UnimatrixConfig>(&toml_str)
// Assert: config.observation.domain_packs.len() == 1
// Assert: pack.source_domain == "sre"
// Assert: pack.event_types == ["incident_opened", "incident_resolved"]
// Assert: pack.categories == ["runbook", "post-mortem"]
// Assert: pack.rule_file.is_none() (absent = None)
```

### T-CFG-03: DomainPackConfig with rule_file path deserializes correctly

```rust
// test_domain_pack_config_rule_file_deserialization
// Arrange: TOML with rule_file = "/etc/unimatrix/sre-rules.toml"
// Assert: config.observation.domain_packs[0].rule_file == Some(PathBuf::from("..."))
```

### T-CFG-04: Multiple domain packs deserialized

```rust
// test_observation_config_multiple_packs
// Arrange: TOML with two [[observation.domain_packs]] stanzas
// Assert: config.observation.domain_packs.len() == 2
```

### T-CFG-05: UnimatrixConfig follows the same two-level hierarchy pattern

```rust
// test_observation_config_follows_existing_config_hierarchy_pattern
// Verify that ObservationConfig is nested within UnimatrixConfig following
// the same pattern as KnowledgeConfig (existing model). This is a
// compile-time structural check: UnimatrixConfig.observation: ObservationConfig.
// No runtime assertion needed — it compiles or it doesn't.
```

---

## Integration Test Expectations (Startup Wiring — R-09)

### T-CFG-06: Server starts with no [observation] section — claude-code pack active

This is a higher-level test that spans config-extension and domain-pack-registry.
Specifically:

```rust
// test_server_starts_default_config_claude_code_active
// Arrange: Server startup with UnimatrixConfig.observation = ObservationConfig::default()
// Act: DomainPackRegistry::from_config(&config.observation)
// Assert: registry.lookup("claude-code").is_some()
// Assert: registry.resolve_source_domain("PreToolUse") == "claude-code"
// Verifies IR-01 at the config-to-registry plumbing level.
```

### T-CFG-07: Server starts with invalid source_domain in config — startup failure (R-09)

```rust
// test_server_startup_fails_invalid_source_domain_in_config
// Arrange: DomainPackConfig { source_domain: "My Domain", ... } (invalid regex)
// Act: DomainPackRegistry::from_config(&config.observation)
// Assert: Err(ObserveError::InvalidSourceDomain { domain: "My Domain" })
// Server should not start. Error names the offending pack.
```

### T-CFG-08: Server starts with missing rule_file — startup failure (R-09)

```rust
// test_server_startup_fails_missing_rule_file
// Arrange: DomainPackConfig { rule_file: Some("/nonexistent/path.toml"), ... }
// Act: DomainPackRegistry::from_config(&config.observation)
// Assert: Err with message identifying the missing file path
// Not a generic IO error — the error must name the file.
```

### T-CFG-09: Server starts with malformed rule descriptor in rule_file — startup failure (R-09)

```rust
// test_server_startup_fails_malformed_rule_descriptor
// Arrange: rule_file contains a rule with window_secs = 0 (EC-08)
// Act: DomainPackRegistry::from_config(...)
// Assert: Err(ObserveError::InvalidRuleDescriptor { rule_name: ..., reason: "window_secs must be > 0" })
```

---

## Edge Cases

- TOML with `[[observation.domain_packs]]` containing `source_domain = "unknown"` →
  rejected at startup with `InvalidSourceDomain` (EC-04).
- `categories = []` (empty list) → valid; pack registers no additional categories.
- `event_types = []` (empty list) → valid per EC-05; pack matches all event types for
  its domain (relevant only for `resolve_source_domain` calls, not the hook ingress).
