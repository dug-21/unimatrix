# Pseudocode: agent-registry

**Files**:
- `crates/unimatrix-server/src/infra/registry.rs` (modified)
- `crates/unimatrix-store/src/registry.rs` (modified)

## Purpose

Removes the hardcoded `const PERMISSIVE_AUTO_ENROLL: bool = true` from
`infra/registry.rs` and instead receives the permissive flag as a constructor
parameter. Extends `SqlxStore::agent_resolve_or_enroll` with a third parameter
`session_caps: Option<&[Capability]>` that, when `Some`, uses the provided
capability set instead of the permissive/strict default. All existing call sites
pass `None` to preserve current behavior.

---

## `infra/registry.rs` Changes

### Remove Constant

```
// REMOVE this line:
const PERMISSIVE_AUTO_ENROLL: bool = true;
```

### Struct Change

```
// BEFORE:
pub struct AgentRegistry {
    store: Arc<SqlxStore>,
}

// AFTER: add permissive field
pub struct AgentRegistry {
    store: Arc<SqlxStore>,
    // Received from config at construction time.
    // true = auto-enroll with [Read, Write, Search]; false = [Read, Search] only.
    // Replaces the removed PERMISSIVE_AUTO_ENROLL const.
    permissive: bool,
    // Capability set for auto-enrolled agents from config.
    // When non-empty, overrides the permissive/strict default on enrollment.
    session_caps: Vec<Capability>,
}
```

### Constructor Change

```
// BEFORE:
pub fn new(store: Arc<SqlxStore>) -> Result<Self, ServerError>

// AFTER: add permissive parameter
pub fn new(store: Arc<SqlxStore>, permissive: bool) -> Result<Self, ServerError>

BODY:
    Ok(AgentRegistry {
        store,
        permissive,
        session_caps: Vec::new(),  // populated by set_session_caps if needed
    })
```

**Note on session_caps**: The architecture says `session_caps: Vec<Capability>` from config
is passed as a plain parameter to `agent_resolve_or_enroll`. Two approaches are possible:

Option A: Store `session_caps: Vec<Capability>` on `AgentRegistry` (passed at construction
from config-extracted capabilities). `resolve_or_enroll` passes `Some(&self.session_caps)`
when the vec is non-empty, `None` when empty.

Option B: Pass `Option<&[Capability]>` directly as a third arg to `resolve_or_enroll`
at each call site in the server.

Architecture brief specifies: "session_caps: Vec<Capability> ... passed as a plain
Vec<Capability> parameter to agent_resolve_or_enroll". The integration surface shows
`Option<&[Capability]>` as the third param. Given `AgentRegistry::resolve_or_enroll`
is called with the context available at startup, Option A (store on struct) is simpler.

**Chosen approach (Option A)**:

```
pub fn new(
    store: Arc<SqlxStore>,
    permissive: bool,
    session_caps: Vec<Capability>,
) -> Result<Self, ServerError>

BODY:
    Ok(AgentRegistry {
        store,
        permissive,
        session_caps,
    })
```

The `session_caps` vector is non-empty when `config.agents.session_capabilities` is
non-default. `main.rs` converts the `Vec<String>` to `Vec<Capability>` before calling
`AgentRegistry::new`.

### `resolve_or_enroll` Change

```
// BEFORE:
pub fn resolve_or_enroll(&self, agent_id: &str) -> Result<AgentRecord, ServerError>
    // used PERMISSIVE_AUTO_ENROLL const

// AFTER: uses self.permissive and passes self.session_caps to store
pub fn resolve_or_enroll(&self, agent_id: &str) -> Result<AgentRecord, ServerError>

BODY:
    // Determine the session_caps to pass to the store.
    // Non-empty session_caps from config → pass Some; empty → pass None (store uses permissive default).
    let caps_arg: Option<&[Capability]> = if self.session_caps.is_empty() {
        None
    } else {
        Some(&self.session_caps)
    };

    block_sync(
        self.store
            .agent_resolve_or_enroll(agent_id, self.permissive, caps_arg),
    )
    .map_err(|e| ServerError::Registry(e.to_string()))
```

---

## `unimatrix-store/src/registry.rs` Changes

### `agent_resolve_or_enroll` Signature Change

```
// BEFORE:
pub async fn agent_resolve_or_enroll(
    &self,
    agent_id: &str,
    permissive: bool,
) -> Result<AgentRecord>

// AFTER: adds third parameter
pub async fn agent_resolve_or_enroll(
    &self,
    agent_id: &str,
    permissive: bool,
    session_caps: Option<&[Capability]>,  // Some → use provided caps; None → permissive/strict branch
) -> Result<AgentRecord>

BODY (only the changed section — capability determination):
    // Read-first: avoid write lock for existing agents (unchanged).
    if let Some(record) = self.agent_get(agent_id).await? {
        return Ok(record);
    }

    let now = current_unix_seconds();

    // Determine capabilities for the new agent.
    let default_caps: Vec<u8> = match session_caps {
        Some(caps) => {
            // Config-supplied capability set. Used when [agents] session_capabilities is set.
            // Converts Capability slice to Vec<u8> for JSON serialization.
            caps.iter().map(|c| *c as u8).collect()
        }
        None => {
            // Existing permissive/strict branch — unchanged behavior.
            if permissive {
                vec![
                    Capability::Read as u8,
                    Capability::Write as u8,
                    Capability::Search as u8,
                ]
            } else {
                vec![Capability::Read as u8, Capability::Search as u8]
            }
        }
    };

    // ... rest of INSERT OR IGNORE + re-read (unchanged) ...
```

### All Existing Call Sites

All existing call sites of `agent_resolve_or_enroll` in `unimatrix-store` tests and
any other callers pass `None` as the third argument:

```
// BEFORE (all call sites):
self.store.agent_resolve_or_enroll(agent_id, PERMISSIVE_AUTO_ENROLL)

// AFTER (infra/registry.rs — now uses self.permissive and passes caps_arg):
self.store.agent_resolve_or_enroll(agent_id, self.permissive, caps_arg)

// Any direct test calls to SqlxStore::agent_resolve_or_enroll:
store.agent_resolve_or_enroll("test-agent", true, None).await
```

---

## `permissive: bool` Derivation in `main.rs`

The `permissive` bool is derived from `config.agents.default_trust`:
```
let permissive = config.agents.default_trust == "permissive";
// "strict" → false (agents get [Read, Search] only)
// "permissive" → true (agents get [Read, Write, Search] by default)
// This derivation happens in startup-wiring.md (main.rs section).
```

## `session_caps: Vec<Capability>` Derivation in `main.rs`

```
use unimatrix_store::Capability;

// Convert Vec<String> from config to Vec<Capability>.
// validate_config already verified each string is in ["Read", "Write", "Search"].
let session_caps: Vec<Capability> = config.agents.session_capabilities.iter()
    .filter_map(|s| match s.as_str() {
        "Read"   => Some(Capability::Read),
        "Write"  => Some(Capability::Write),
        "Search" => Some(Capability::Search),
        _        => None,  // should not occur — validated at load time
    })
    .collect();
```

---

## Key Test Scenarios

1. **permissive=false enrolls with [Read, Search]** (AC-06):
   - Create `AgentRegistry::new(store, false, vec![])`.
   - `resolve_or_enroll("unknown-agent")` → assert capabilities == `[Read, Search]`.
   - Assert `Write` is NOT in the capabilities.

2. **session_caps override applied** (AC-06, R-14):
   - Create `AgentRegistry::new(store, true, vec![Capability::Read])`.
   - `resolve_or_enroll("unknown-agent")` → assert capabilities == `[Read]` only.
   - Assert `Write` and `Search` are NOT present (caps_arg = `Some([Read])` wins over permissive).

3. **session_caps=empty uses permissive default** (IR-02):
   - `AgentRegistry::new(store, true, vec![])`.
   - `resolve_or_enroll("unknown-agent")` → capabilities == `[Read, Write, Search]` (permissive default).

4. **Existing bootstrap tests still pass** (SR-07):
   - `AgentRegistry::new(store, true)` must now accept the session_caps parameter.
   - All existing tests should be updated to pass `vec![]` as the third arg.
   - Bootstrap defaults ("system", "human", "cortical-implant") are unaffected — they use
     `agent_enroll` not `agent_resolve_or_enroll`.

5. **All infra/registry.rs tests pass** (IR-01, IR-02):
   - The `test_enroll_unknown_agent` test should still pass (permissive=true, session_caps=empty).
   - The `test_enrolled_agent_has_write_when_permissive` test should still pass.

---

## Error Handling

`AgentRegistry::new` is infallible (returns `Ok(Self)` as before). The permissive flag and
session_caps are plain values — no validation needed at construction time.

`agent_resolve_or_enroll` error handling is unchanged: store errors propagate via
`StoreError::Database` and are wrapped in `ServerError::Registry` at the infra layer.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-store` — no patterns found for capability injection. The parameter-addition pattern follows ADR-002 (no Arc across crate boundaries).
- Deviations from established patterns: none.
