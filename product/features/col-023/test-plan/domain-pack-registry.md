# Test Plan: domain-pack-registry

**Component**: `crates/unimatrix-observe/src/domain/mod.rs` (new module)
**AC Coverage**: AC-03, AC-04, AC-05, AC-07, AC-08
**Risk Coverage**: R-09 (startup failure), R-10 (CategoryAllowlist poisoning), IR-01, IR-02

---

## Unit Test Expectations

### Location: `crates/unimatrix-observe/tests/domain_pack_tests.rs` (new file)

### T-DPR-01: Built-in claude-code pack always present

```rust
// test_with_builtin_claude_code_pack_always_loads
// Arrange: DomainPackRegistry::with_builtin_claude_code()
// Assert: registry.lookup("claude-code").is_some()
// Assert: pack.event_types contains "PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop"
```

### T-DPR-02: Default config (empty domain_packs) loads built-in pack

```rust
// test_default_config_loads_claude_code_pack
// Arrange: DomainPackRegistry::new(vec![]) — empty config
// Assert: registry.lookup("claude-code").is_some()
// This verifies AC-03: absent [observation] section still yields claude-code pack
```

### T-DPR-03: Custom pack registration alongside built-in

```rust
// test_custom_pack_registered_alongside_builtin
// Arrange: DomainPackRegistry::new(vec![DomainPack { source_domain: "sre", ... }])
// Assert: registry.lookup("sre").is_some()
// Assert: registry.lookup("claude-code").is_some() — built-in not displaced
```

### T-DPR-04: lookup returns None for unregistered domain

```rust
// test_lookup_unregistered_domain_returns_none
// Arrange: default registry (claude-code only)
// Act: registry.lookup("sre")
// Assert: result is None
```

### T-DPR-05: resolve_source_domain returns correct domain for known event type

```rust
// test_resolve_source_domain_known_event_type
// Arrange: registry with claude-code pack
// Act: registry.resolve_source_domain("PostToolUse")
// Assert: returns "claude-code"
```

### T-DPR-06: resolve_source_domain returns "unknown" for unregistered event type

```rust
// test_resolve_source_domain_unknown_event_type_returns_unknown
// Arrange: default registry
// Act: registry.resolve_source_domain("incident_opened")
// Assert: returns "unknown" (IR-01 safety valve)
```

### T-DPR-07: source_domain = "unknown" registration is rejected

```rust
// test_registry_rejects_unknown_as_source_domain
// Arrange: attempt DomainPackRegistry::new(vec![DomainPack { source_domain: "unknown", ... }])
// Assert: returns Err(ObserveError::InvalidSourceDomain { domain: "unknown" })
// EC-04: "unknown" is reserved
```

### T-DPR-08: source_domain regex validation at registration

```rust
// test_registry_rejects_invalid_source_domain_formats
// Cases to test — each must return InvalidSourceDomain:
// - "" (empty)
// - "Claude-Code" (uppercase)
// - "my domain" (space)
// - "a".repeat(65) (too long — 65 chars)
// - "sre!" (special character)
// Valid boundary case:
// - "a".repeat(64) (exactly 64 chars) — must succeed
// - "sre-monitoring_v2" (all valid chars) — must succeed
// AC-07 coverage
```

### T-DPR-09: rules_for_domain returns RuleEvaluator instances

```rust
// test_rules_for_domain_returns_evaluators_for_registered_pack
// Arrange: pack with two RuleDescriptors (one Threshold, one TemporalWindow)
// Act: registry.rules_for_domain("sre")
// Assert: returns Vec of length 2
// Assert: each implements DetectionRule trait
```

### T-DPR-10: rules_for_domain returns empty for unregistered domain

```rust
// test_rules_for_domain_unregistered_returns_empty
// Act: registry.rules_for_domain("unknown-domain")
// Assert: returns empty Vec (not an error)
```

### T-DPR-11: Structural assertion — no MCP write path (AC-08)

```rust
// test_domain_pack_registry_no_runtime_write_path
// This test verifies AC-08 by inspecting the public API surface:
// Assert: the only method that modifies registry state is load_from_config()
//         or the constructor new()/with_builtin_claude_code()
// Implementation: attempt to call any non-constructor write method — if one
// exists beyond the startup path, this test should be added as a compile-time
// assertion or documented explicitly in the test as a surface-check.
// Note: this is a code review gate item; the unit test documents the invariant.
```

### T-DPR-12: CategoryAllowlist integration — duplicate category idempotent (R-10)

```rust
// test_registry_duplicate_category_idempotent
// Arrange: DomainPack with categories that include a value already in INITIAL_CATEGORIES
//          (e.g., "convention" or "pattern")
// Act: register the pack
// Assert: CategoryAllowlist does not have duplicate entries
// Assert: existing context_store behavior is unchanged (no error on valid category)
```

### T-DPR-13: CategoryAllowlist integration — invalid category format rejected (R-10)

```rust
// test_registry_invalid_category_format_rejected_at_startup
// Arrange: DomainPack with categories = ["My Category", "UPPER_CASE"]
// Act: attempt to register
// Assert: Err or startup panic with clear message naming the invalid category
// Not a silent pass-through.
```

### T-DPR-14: Empty event_types list matches all events for that domain (EC-05)

```rust
// test_registry_empty_event_types_matches_all
// Arrange: DomainPack { source_domain: "sre", event_types: vec![], ... }
// Act: registry.resolve_source_domain("any_event_string")
// Assert: returns "sre" (empty list = "all events match this domain")
// Note: this applies only when the ingress explicitly assigns source_domain = "sre"
//       via a non-hook path. For the hook path, source_domain is always "claude-code".
```

---

## Integration Test Expectations

### IR-01: DomainPackRegistry injected into parse_observation_rows

This is validated by the `ingest-security` and `detection-rules` tests which rely on
the registry being properly threaded at startup. The specific IR-01 test is:

```rust
// test_event_type_pretooluse_resolves_to_claude_code_domain
// Arrange: DomainPackRegistry::with_builtin_claude_code()
// Act: registry.resolve_source_domain("PreToolUse")
// Assert: returns "claude-code" (not "unknown")
// This test ensures IR-01 can never silently fail — if the registry is empty,
// this assertion catches it.
```

### IR-02: CategoryAllowlist initialized before requests arrive

Covered by T-DPR-12/T-DPR-13 combined with the server startup sequencing test in
`config-extension.md`.

---

## Edge Cases

- `window_secs = 0` in a TemporalWindowRule passed during pack registration → must
  return `InvalidRuleDescriptor` at registration time, not at first `detect()` call
  (EC-08). This is tested in `rule-dsl-evaluator.md` T-DSL-09.
- Two packs with overlapping event_type strings (EC-07): since source_domain is
  assigned from ingress path (not from event_type lookup in W1-5), this is not a
  conflict at the hook ingress. Test: register two packs sharing `event_type = "start"`;
  assert `resolve_source_domain("start")` returns a deterministic result (first registered
  wins, or last wins — document which; do not panic).
