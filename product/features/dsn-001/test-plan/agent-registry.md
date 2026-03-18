# dsn-001 Test Plan — agent-registry

Components:
- `crates/unimatrix-server/src/infra/registry.rs` (AgentRegistry)
- `crates/unimatrix-store/src/registry.rs` (SqlxStore::agent_resolve_or_enroll)

Risks covered: R-14, IR-02, AC-06, EC-08.

---

## Scope of Changes

### `unimatrix-store/src/registry.rs`

`agent_resolve_or_enroll` gains a third parameter:

```rust
pub async fn agent_resolve_or_enroll(
    &self,
    agent_id: &str,
    permissive: bool,
    session_caps: Option<&[Capability]>,
) -> Result<AgentRecord>
```

When `session_caps` is `Some`, the provided capability set is used for new agents.
When `session_caps` is `None`, the existing permissive/strict branch runs unchanged.
All existing call sites pass `None`.

### `unimatrix-server/src/infra/registry.rs`

`AgentRegistry::new(store, permissive: bool)` replaces `const PERMISSIVE_AUTO_ENROLL`.
`AgentRegistry::resolve_or_enroll()` passes the config-derived `session_caps`
(not `None`) when `session_caps` is configured.

---

## Store-Layer: `session_caps` None Preserves Existing Behavior (IR-02)

### test_agent_resolve_or_enroll_none_caps_uses_permissive_default

```rust
#[tokio::test]
async fn test_agent_resolve_or_enroll_none_caps_uses_permissive_default() {
    let store = open_test_store().await;
    // permissive=true, session_caps=None → existing behavior: full capability set.
    let record = store.agent_resolve_or_enroll("test-agent-none", true, None).await.unwrap();
    // Pre-dsn-001 permissive behavior: Read + Write + Search.
    assert!(record.capabilities.contains(&Capability::Read));
    assert!(record.capabilities.contains(&Capability::Write));
    assert!(record.capabilities.contains(&Capability::Search));
}
```

### test_agent_resolve_or_enroll_none_caps_strict_default

```rust
#[tokio::test]
async fn test_agent_resolve_or_enroll_none_caps_strict_default() {
    let store = open_test_store().await;
    // permissive=false, session_caps=None → existing strict behavior.
    let record = store.agent_resolve_or_enroll("test-agent-strict", false, None).await.unwrap();
    // Pre-dsn-001 strict behavior: Read only (or Read + Search).
    assert!(record.capabilities.contains(&Capability::Read));
    assert!(!record.capabilities.contains(&Capability::Write),
        "strict mode must not grant Write by default");
}
```

---

## Store-Layer: `session_caps` Some Uses Provided Caps (R-14, AC-06)

### test_agent_resolve_or_enroll_some_caps_overrides_permissive

```rust
#[tokio::test]
async fn test_agent_resolve_or_enroll_some_caps_overrides_permissive() {
    let store = open_test_store().await;
    let caps = [Capability::Read, Capability::Search];
    // Even with permissive=true, provided session_caps override the default.
    let record = store
        .agent_resolve_or_enroll("test-agent-caps", true, Some(&caps))
        .await
        .unwrap();
    // Exactly the provided caps — Write must not be added by permissive logic.
    assert!(record.capabilities.contains(&Capability::Read));
    assert!(record.capabilities.contains(&Capability::Search));
    assert!(!record.capabilities.contains(&Capability::Write),
        "Some(session_caps) must override permissive default; Write must not be added");
    // Exactly 2 capabilities.
    assert_eq!(record.capabilities.len(), 2,
        "capabilities must be exactly the provided set, no extras");
}
```

### test_agent_resolve_or_enroll_some_caps_read_only

```rust
#[tokio::test]
async fn test_agent_resolve_or_enroll_some_caps_read_only() {
    let store = open_test_store().await;
    let caps = [Capability::Read];
    let record = store
        .agent_resolve_or_enroll("test-agent-readonly", true, Some(&caps))
        .await
        .unwrap();
    assert_eq!(record.capabilities, vec![Capability::Read]);
}
```

---

## Server-Infra Layer: `permissive` Flag From Config (R-14)

### test_agent_registry_new_receives_permissive_flag

```rust
#[tokio::test]
async fn test_agent_registry_new_receives_permissive_flag() {
    let store = Arc::new(open_test_store().await);
    // permissive=false passed from config.
    let registry = AgentRegistry::new(store.clone(), false).await.unwrap();
    // Enrolling an unknown agent must use strict behavior.
    let record = registry.resolve_or_enroll("unknown-agent-strict").await.unwrap();
    assert!(!record.capabilities.contains(&Capability::Write),
        "permissive=false from config must produce strict capability set");
}
```

### test_agent_registry_session_caps_not_silently_none (R-14 critical)

This test verifies that `AgentRegistry::resolve_or_enroll()` passes the configured
`session_caps` as `Some(...)`, not `None`, when `session_capabilities` is configured.

```rust
#[tokio::test]
async fn test_agent_registry_session_caps_propagated_to_store() {
    let store = Arc::new(open_test_store().await);
    let session_caps = vec![Capability::Read, Capability::Search];
    let registry = AgentRegistry::new_with_session_caps(
        store.clone(),
        false, // permissive
        session_caps.clone()
    ).await.unwrap();
    // Enrolling unknown agent must use the configured session_caps.
    let record = registry.resolve_or_enroll("test-agent-configured").await.unwrap();
    assert!(record.capabilities.contains(&Capability::Read));
    assert!(record.capabilities.contains(&Capability::Search));
    assert!(!record.capabilities.contains(&Capability::Write),
        "session_caps = [Read, Search] must not result in Write capability");
}
```

Note: The exact constructor signature for `AgentRegistry` depends on the
implementation. If the constructor is `new(store, permissive, session_caps)`, adjust
accordingly. The test must exercise the full path through `AgentRegistry` → store.

---

## Integration Test (AC-06, R-14)

The following integration-level behavior requires a server started with a specific
config (see OVERVIEW.md §Harness Fixture Gap):

**Scenario**: Server configured with `default_trust = "strict"`,
`session_capabilities = ["Read", "Search"]`. An unknown agent makes any tool call.
The enrolled `AgentRecord.capabilities` must be `[Read, Search]` — `Write` absent.

If the harness config-injection fixture is available:
```python
def test_agent_enrollment_strict_session_caps(config_server):
    """AC-06: strict trust + session_capabilities enforced for new agents."""
    # config_server: server started with strict config
    resp = config_server.context_search(query="test query", agent_id="new-unknown-agent")
    # Agent should be enrolled; check capabilities via context_status or enrollment path.
    status = config_server.context_status()
    # Look for the enrolled agent's capabilities in the registry output.
    assert "Read" in status["agent_capabilities"]
    assert "Write" not in status["agent_capabilities"]
```

If harness fixture not available in Stage 3c: document as gap, covered by unit test above.

---

## Duplicate `session_capabilities` (EC-08)

### test_session_capabilities_duplicate_values_behavior

The SPECIFICATION.md does not specify deduplication behavior. This test documents
the chosen behavior:

```rust
fn test_session_capabilities_duplicate_read_documented_behavior() {
    let config = UnimatrixConfig {
        agents: AgentsConfig {
            session_capabilities: vec!["Read".into(), "Read".into(), "Write".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    // validate_config must either:
    // (a) accept duplicates (Vec-based, duplicates stored) — must be documented, or
    // (b) deduplicate silently (HashSet-based), or
    // (c) reject duplicates (stricter validation).
    // Test the actual behavior and document it.
    let result = validate_config(&config, Path::new("/fake"));
    // Record actual behavior here. If (a) or (b): assert Ok.
    // The behavior must be deterministic — test either Ok or Err, not both.
    // If Ok: duplicate "Read" must not cause capability-check failures downstream.
    let _ = result; // adjust assertion to match chosen behavior
}
```
