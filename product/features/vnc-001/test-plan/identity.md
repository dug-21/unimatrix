# Test Plan: identity.rs

## Risks Covered
- R-06: Auto-enrollment incorrect capabilities (Critical)
- R-12: Agent identity not threaded through audit (Critical)

## Unit Tests

### extract_agent_id

```
test_extract_some_value
  Act: extract_agent_id(&Some("test-agent".into()))
  Assert: "test-agent"

test_extract_none
  Act: extract_agent_id(&None)
  Assert: "anonymous"

test_extract_empty_string
  Act: extract_agent_id(&Some("".into()))
  Assert: "anonymous"

test_extract_whitespace_only
  Act: extract_agent_id(&Some("   ".into()))
  Assert: "anonymous"

test_extract_trims
  Act: extract_agent_id(&Some("  test  ".into()))
  Assert: "test"

test_extract_special_characters
  Act: extract_agent_id(&Some("uni-architect-v2".into()))
  Assert: "uni-architect-v2"
```

### resolve_identity

```
test_resolve_known_agent
  Arrange: Store with bootstrapped registry (has "human")
  Act: resolve_identity(registry, "human")
  Assert: trust_level == Privileged, capabilities == [Read, Write, Search, Admin]

test_resolve_unknown_agent
  Arrange: fresh Store with bootstrapped registry
  Act: resolve_identity(registry, "new-agent")
  Assert: trust_level == Restricted, capabilities == [Read, Search]

test_resolve_updates_last_seen
  Arrange: fresh Store, bootstrap, resolve "human" once
  Act: sleep briefly, resolve "human" again
  Assert: last_seen_at updated

test_resolve_anonymous
  Arrange: fresh Store with bootstrapped registry
  Act: resolve_identity(registry, "anonymous")
  Assert: auto-enrolled as Restricted
```
