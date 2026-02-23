# Test Plan: registry.rs

## Risks Covered
- R-04: Table creation backward compatibility (Critical)
- R-05: Default agent bootstrap idempotency (High)
- R-06: Auto-enrollment incorrect capabilities (Critical)
- R-16: Concurrent state corruption (High)

## Unit Tests

### bootstrap_defaults

```
test_bootstrap_creates_system_and_human
  Arrange: fresh Store
  Act: registry.bootstrap_defaults()
  Assert: "system" has TrustLevel::System, capabilities [Read, Write, Search, Admin]
         "human" has TrustLevel::Privileged, capabilities [Read, Write, Search, Admin]

test_bootstrap_idempotent
  Arrange: fresh Store, bootstrap once
  Act: bootstrap again
  Assert: no error, agents unchanged (same enrolled_at)

test_bootstrap_preserves_existing_agents
  Arrange: fresh Store, bootstrap, modify "human" capabilities
  Act: bootstrap again
  Assert: "human" retains modified capabilities

test_bootstrap_fresh_db_exactly_two_agents
  Arrange: fresh Store
  Act: bootstrap
  Assert: exactly 2 agents in registry
```

### resolve_or_enroll

```
test_resolve_existing_agent
  Arrange: bootstrap, so "human" exists
  Act: resolve_or_enroll("human")
  Assert: returns Privileged with [Read, Write, Search, Admin]

test_enroll_unknown_agent
  Act: resolve_or_enroll("unknown-agent-123")
  Assert: trust_level == Restricted, capabilities == [Read, Search]

test_enroll_anonymous
  Act: resolve_or_enroll("anonymous")
  Assert: trust_level == Restricted, capabilities == [Read, Search]

test_enrolled_agent_lacks_write
  Act: resolve_or_enroll("new-agent")
  Assert: capabilities does NOT contain Write

test_enrolled_agent_lacks_admin
  Act: resolve_or_enroll("new-agent")
  Assert: capabilities does NOT contain Admin
```

### has_capability / require_capability

```
test_has_capability_true
  Arrange: bootstrap "human"
  Act: has_capability("human", Read)
  Assert: true

test_has_capability_false
  Arrange: enroll "agent-x" (Restricted)
  Act: has_capability("agent-x", Write)
  Assert: false

test_require_capability_ok
  Arrange: bootstrap "human"
  Act: require_capability("human", Write)
  Assert: Ok(())

test_require_capability_denied
  Arrange: enroll "agent-x" (Restricted)
  Act: require_capability("agent-x", Write)
  Assert: Err(CapabilityDenied { agent_id: "agent-x", capability: Write })

test_has_capability_all_trust_levels
  For each TrustLevel, verify the expected default capabilities:
    System: [Read, Write, Search, Admin]
    Privileged: [Read, Write, Search, Admin]
    Internal: [Read, Write, Search]
    Restricted: [Read, Search]
```

### update_last_seen

```
test_update_last_seen_changes_timestamp
  Arrange: enroll agent, note original last_seen_at
  Act: sleep briefly, update_last_seen
  Assert: last_seen_at > original

test_update_last_seen_preserves_capabilities
  Arrange: enroll agent
  Act: update_last_seen
  Assert: capabilities unchanged
```

### Serialization

```
test_agent_record_roundtrip
  Arrange: create AgentRecord with all fields populated
  Act: serialize then deserialize
  Assert: equal to original

test_agent_record_roundtrip_optional_fields
  Arrange: create AgentRecord with allowed_topics=Some, allowed_categories=None
  Act: serialize then deserialize
  Assert: equal to original
```

## Integration (backward compatibility)

```
test_store_open_creates_10_tables (AC-17)
  Arrange: fresh Store::open
  Act: read transaction, open all 10 tables
  Assert: all succeed (8 original + AGENT_REGISTRY + AUDIT_LOG)

test_existing_store_tests_still_pass (IR-01)
  This is verified by running `cargo test -p unimatrix-store` after schema changes
```
