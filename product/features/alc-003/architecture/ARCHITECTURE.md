# alc-003: Session Identity via Env Var — Architecture

## System Overview

alc-003 transforms the Unimatrix capability system from vestigial to enforced. Today
`PERMISSIVE_AUTO_ENROLL = true` grants every caller `[Read, Write, Search]` regardless
of identity. Any process connecting to the MCP server can read and write the knowledge
base. The only "identity" is an `agent_id` string that the LLM fills in — spoofable,
unreliable, and used for both attribution and capability resolution in a conflated way.

After alc-003:

- **Authentication** moves to operator configuration (`UNIMATRIX_SESSION_AGENT` in
  `settings.json`) — outside LLM control.
- **Capabilities** are resolved once at startup, cached in `UnimatrixServer`, and
  applied uniformly to every tool call in that session. No per-call registry lookups.
- **Attribution** (per-call `agent_id`) remains LLM-controlled and is used exclusively
  for the audit log — it has no effect on what the caller can do.
- **Auto-enrollment of unknown callers** is eliminated entirely. `PERMISSIVE_AUTO_ENROLL`
  is deleted with no escape hatch.

This feature is W0-2 in the product roadmap. Its primary architectural contribution is
the `SessionIdentitySource` abstraction: a named seam between "how we learn who is
connecting" and "what we do with that identity." The W2-2 OAuth implementation slots
into this seam by providing a different `SessionIdentitySource` implementation without
touching capability resolution or audit attribution logic.

## Component Breakdown

### 1. `SessionIdentitySource` (new — `mcp/session_identity.rs`)

The abstraction boundary called out in SR-04. Encapsulates the mechanism by which the
server learns its session-level identity at startup time.

**Shape:** An enum, not a trait. Rationale: the number of source variants is small and
finite (env var now, JWT claims in W2-2, config file in W0-3). A trait object would add
`dyn` dispatch and lifetime complexity for no benefit given the fixed variant set. The
enum is resolved once at startup; it is never stored after the `SessionAgent` is produced.

```rust
/// The mechanism by which the server resolves its session-level identity.
///
/// Resolved once at startup. After `resolve()` succeeds, the resulting
/// `SessionAgent` is stored in `UnimatrixServer` and `SessionIdentitySource`
/// is dropped.
pub enum SessionIdentitySource {
    /// W0-2: identity from `UNIMATRIX_SESSION_AGENT` environment variable.
    EnvVar,
    /// W2-2: identity from OAuth JWT claims (token extracted from HTTP transport
    /// headers at connection time). Reserved; not implemented in alc-003.
    #[allow(dead_code)]
    JwtClaims { token: String },
}

impl SessionIdentitySource {
    /// Read and validate the identity from this source.
    ///
    /// Returns `Err` if the source is present but invalid (startup must fail).
    /// Returns `Err` with a descriptive message if the source is absent
    /// (the caller in `main.rs` maps this to a startup failure per AC-04).
    pub fn resolve(&self) -> Result<ValidatedAgentId, SessionIdentityError> { ... }
}

/// A validated agent identifier — guaranteed to match `[a-zA-Z0-9_-]{1,64}`
/// and not be a protected name.
pub struct ValidatedAgentId(String);

/// Error from identity source resolution.
pub enum SessionIdentityError {
    Missing { source: &'static str },
    Invalid { source: &'static str, reason: String },
    ProtectedName { name: String },
}
```

The `ValidatedAgentId` newtype enforces that validation happened — the type cannot be
constructed without calling `resolve()`. Startup code holds a `ValidatedAgentId`, never
a raw `String`.

**W2-2 replacement path:** `SessionIdentitySource::JwtClaims { token }` is added in
W2-2. Its `resolve()` parses and validates the JWT, extracts the `sub` claim, and
returns a `ValidatedAgentId`. The startup call site in `main.rs` is unchanged; only the
variant passed to it changes.

### 2. `SessionAgent` (new — `mcp/session_identity.rs`)

The resolved, enrolled session identity. Cached in `UnimatrixServer` and used on every
tool call.

```rust
/// The enrolled session agent for this server instance.
///
/// Constructed by `enroll_session_agent()` at startup. Immutable after construction.
/// `Clone` — `UnimatrixServer` is cloned into each MCP session task (daemon mode).
#[derive(Debug, Clone)]
pub struct SessionAgent {
    /// The agent's identifier (validated at construction).
    pub agent_id: String,
    /// The agent's trust level (always `TrustLevel::Internal` for W0-2).
    pub trust_level: TrustLevel,
    /// The agent's capability set (cached at startup; no per-call DB lookup).
    pub capabilities: Vec<Capability>,
}
```

### 3. `SESSION_AGENT_DEFAULT_CAPS` constant (new — `mcp/session_identity.rs`)

```rust
/// Default capability set for the session agent (W0-2 hardcoded; W0-3 makes this
/// configurable via `[agents] session_capabilities` in config).
///
/// W0-3 integration point: replace this constant with a config-file read in
/// `main.rs` startup before calling `enroll_session_agent()`. The enrollment
/// function accepts `Vec<Capability>` — it does not read this constant directly.
pub const SESSION_AGENT_DEFAULT_CAPS: &[Capability] = &[
    Capability::Read,
    Capability::Write,
    Capability::Search,
];
```

Isolated here so W0-3 can replace it with a config read at the single call site in
`main.rs` without touching any identity resolution logic.

### 4. `enroll_session_agent()` function (new — `mcp/session_identity.rs`)

```rust
/// Enroll the session agent at startup and return the cached `SessionAgent`.
///
/// Always upserts — the env var is authoritative over any existing registry record.
/// Runs after `registry.bootstrap_defaults()`. The resulting `SessionAgent` is stored
/// in `UnimatrixServer` and never re-derived during the server's lifetime.
///
/// `capabilities`: W0-2 passes `SESSION_AGENT_DEFAULT_CAPS`. W0-3 passes a
/// config-file-derived value.
pub fn enroll_session_agent(
    registry: &AgentRegistry,
    id: ValidatedAgentId,
    capabilities: Vec<Capability>,
) -> Result<SessionAgent, ServerError> { ... }
```

This function bypasses the `enroll_agent()` caller/target/protected-agent checks that
`context_enroll` applies — the session agent enrollment is an operator-level startup act,
not a peer enrollment. It calls `store.agent_enroll()` directly (the same underlying
upsert), with `TrustLevel::Internal` hardcoded.

### 5. `AgentRegistry` changes (`infra/registry.rs`)

**Deleted:**
- `const PERMISSIVE_AUTO_ENROLL: bool = true`
- The `permissive` parameter from `resolve_or_enroll()` / `agent_resolve_or_enroll()`
- All call sites that pass the permissive flag (there is one: `resolve_or_enroll` in
  `registry.rs`)

**`resolve_or_enroll()` post-alc-003 behavior:** Always enrolls unknown agents with
`[Read, Search]` (the non-permissive path that already exists in the store). The method
signature is unchanged externally; the `PERMISSIVE_AUTO_ENROLL` flag is simply removed
and the non-permissive path becomes the only path.

**No new methods added.** `enroll_session_agent()` calls the existing `store.agent_enroll()`
through a direct store call — it does not go through `AgentRegistry` so as to bypass the
protected-agent guard and self-lockout check that `enroll_agent()` applies.

### 6. `UnimatrixServer` changes (`server.rs`)

**New fields:**

```rust
pub struct UnimatrixServer {
    // ... existing fields ...

    /// Session agent identity and cached capabilities (alc-003).
    ///
    /// Populated at startup by `enroll_session_agent()`.
    /// Used by `build_context()` for every tool call — no per-call DB lookup.
    /// `Clone`-safe: String + TrustLevel + Vec<Capability> all implement Clone.
    pub(crate) session_agent: SessionAgent,
}
```

**`UnimatrixServer::new()` signature change:**

```rust
pub fn new(
    // ... existing params ...
    session_agent: SessionAgent,   // new required param
) -> Self
```

Session identity is required at construction. The type system enforces that a server
cannot be built without a resolved and enrolled session agent.

### 7. `build_context()` changes (`server.rs`)

The key behavioral change. `build_context()` now produces two distinct outputs from two
independent sources:

| Output | Source | Registry lookup? |
|--------|--------|-----------------|
| Capabilities (for `require_cap`) | `self.session_agent.capabilities` | Never |
| Audit attribution (`agent_id` in audit log) | `params.agent_id` if non-empty, else `self.session_agent.agent_id` | Never |

**Signature unchanged.** `build_context(&self, agent_id, format, session_id)` still
accepts the same parameters. The `agent_id` parameter changes meaning: it is now
exclusively an audit label, not a key for registry lookup.

**`resolve_agent()` is deleted.** It existed solely to look up the per-call `agent_id`
in the registry. With capabilities coming from the session, this function has no purpose.

**`require_cap()` changes:** Currently takes `agent_id: &str` and does a DB lookup via
`registry.require_capability()`. Post-alc-003, it checks `self.session_agent.capabilities`
directly — no registry, no DB, no `spawn_blocking`.

```rust
pub(crate) async fn require_cap(
    &self,
    cap: Capability,
) -> Result<(), rmcp::ErrorData> {
    if self.session_agent.capabilities.contains(&cap) {
        Ok(())
    } else {
        Err(rmcp::ErrorData::from(ServerError::CapabilityDenied {
            agent_id: self.session_agent.agent_id.clone(),
            capability: cap,
        }))
    }
}
```

Note: `agent_id` parameter is removed from `require_cap()` because capabilities no
longer depend on the per-call agent. All 12 tool handler call sites must be updated.

**`ToolContext` changes:** `trust_level` field is removed or retained from
`self.session_agent.trust_level` (session-level, not per-call). The `agent_id` in
`ToolContext` and `AuditContext` becomes the audit attribution label, sourced from:

```rust
let audit_agent_id = agent_id
    .as_deref()
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .unwrap_or(&self.session_agent.agent_id)
    .to_string();
```

### 8. Startup wiring (`main.rs`)

Both `tokio_main_daemon()` and `tokio_main_stdio()` gain identical session identity
setup after `registry.bootstrap_defaults()`:

```rust
// alc-003: resolve session identity — fails startup if absent or invalid (AC-04, AC-07)
let session_agent_id = SessionIdentitySource::EnvVar
    .resolve()
    .map_err(|e| ServerError::SessionIdentity(e.to_string()))?;

// Enroll with default capabilities (W0-3 replaces SESSION_AGENT_DEFAULT_CAPS with config)
let session_agent = enroll_session_agent(
    &registry,
    session_agent_id,
    SESSION_AGENT_DEFAULT_CAPS.to_vec(),
)?;

tracing::info!(
    agent_id = %session_agent.agent_id,
    trust_level = ?session_agent.trust_level,
    "session agent enrolled"
);
```

The `session_agent` is then threaded into `UnimatrixServer::new()`.

**`ServerError` variant added:**

```rust
SessionIdentity(String),  // startup failure from SessionIdentitySource::resolve()
```

## Component Interactions

```
startup
  │
  ├─ SessionIdentitySource::EnvVar.resolve()
  │    reads UNIMATRIX_SESSION_AGENT
  │    validates regex + protected name
  │    → ValidatedAgentId | SessionIdentityError (startup fail)
  │
  ├─ enroll_session_agent(registry, id, caps)
  │    calls store.agent_enroll() — upsert, TrustLevel::Internal
  │    → SessionAgent { agent_id, trust_level, capabilities }
  │
  └─ UnimatrixServer::new(..., session_agent)
       stores SessionAgent in self.session_agent

per tool call
  │
  ├─ build_context(params.agent_id, format, session_id)
  │    audit_agent_id = params.agent_id.trim() || self.session_agent.agent_id
  │    trust_level    = self.session_agent.trust_level   (no registry lookup)
  │    capabilities   = (not in ToolContext — checked separately)
  │    → ToolContext { agent_id: audit_agent_id, trust_level, format, audit_ctx, caller_id }
  │
  └─ require_cap(cap)
       checks self.session_agent.capabilities.contains(cap)
       → Ok(()) | Err(CapabilityDenied { agent_id: session_agent.agent_id })
       (no registry, no DB, no spawn_blocking)
```

## Technology Decisions

See ADR files for full rationale. Summary:

| Decision | Choice | ADR |
|----------|--------|-----|
| `SessionIdentitySource` shape | Enum with `resolve()` method | ADR-001 |
| Capability source post-alc-003 | `session_agent.capabilities` (cached at startup) | ADR-002 |
| `PERMISSIVE_AUTO_ENROLL` fate | Deleted entirely, no escape hatch | ADR-003 |
| ADR #1839 disposition | Deferred; alc-003 is the named-identity layer, #1839 adds token hardening later | ADR-004 |
| Test blast radius measurement | Pre-flight compile gate: force `PERMISSIVE_AUTO_ENROLL=false` stub before coding | ADR-005 |

## Integration Points

### Changed call sites (all within `unimatrix-server`)

1. `main.rs` / `tokio_main_daemon()` — adds session identity setup before `UnimatrixServer::new()`
2. `main.rs` / `tokio_main_stdio()` — identical session identity setup
3. `server.rs` / `UnimatrixServer::new()` — new `session_agent: SessionAgent` parameter
4. `server.rs` / `build_context()` — removes `resolve_agent()` call; uses `session_agent` directly
5. `server.rs` / `require_cap()` — removes `agent_id` parameter; checks `session_agent.capabilities`
6. All 12 tool handlers that call `require_cap(ctx.agent_id, Capability::X)` — update to `require_cap(Capability::X)`
7. `infra/registry.rs` — `resolve_or_enroll()` loses permissive flag; `PERMISSIVE_AUTO_ENROLL` deleted

### No changes required in

- `unimatrix-store` — the store-level `agent_resolve_or_enroll()` keeps its `permissive`
  parameter for now (only the server-side call site changes). The store's non-permissive
  path (`[Read, Search]`) is already tested and correct.
- Tool parameter structs — `agent_id: Option<String>` stays on all 12 tool call structs
- Hook/background identity paths — they use `"hook"` and `"background"` identities
  which are explicitly enrolled; their paths are unaffected
- `unimatrix-vector`, `unimatrix-embed`, `unimatrix-core`, `unimatrix-adapt` — no changes

### Startup audit record

At enrollment time, log a structured audit event so forensics can detect identity changes
across daemon restarts (SR-02):

```rust
AuditEvent {
    agent_id: "system".to_string(),
    event_type: "session_agent_enrolled".to_string(),
    detail: format!(
        "session_agent={} trust={:?} caps={:?} source=env_var",
        session_agent.agent_id, session_agent.trust_level, session_agent.capabilities
    ),
}
```

### MCP initialize logging (SR-01, SR-07)

In the `ServerHandler::initialize()` implementation, log the resolved session agent
identity so operators can confirm identity on every new client connection:

```rust
tracing::info!(
    session_agent = %self.session_agent.agent_id,
    trust_level = ?self.session_agent.trust_level,
    "MCP client connected; session identity applied"
);
```

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `SessionIdentitySource` | `enum` with `resolve() -> Result<ValidatedAgentId, SessionIdentityError>` | `mcp/session_identity.rs` (new) |
| `ValidatedAgentId` | `struct(String)` newtype; not constructible without `resolve()` | `mcp/session_identity.rs` (new) |
| `SessionAgent` | `struct { agent_id: String, trust_level: TrustLevel, capabilities: Vec<Capability> }` | `mcp/session_identity.rs` (new) |
| `SESSION_AGENT_DEFAULT_CAPS` | `&[Capability]` = `[Read, Write, Search]` | `mcp/session_identity.rs` (new) |
| `enroll_session_agent()` | `fn(registry: &AgentRegistry, id: ValidatedAgentId, capabilities: Vec<Capability>) -> Result<SessionAgent, ServerError>` | `mcp/session_identity.rs` (new) |
| `UnimatrixServer::new()` | adds `session_agent: SessionAgent` as final required param | `server.rs` (modified) |
| `UnimatrixServer::build_context()` | signature unchanged; behavior changed — no registry lookup | `server.rs` (modified) |
| `UnimatrixServer::require_cap()` | removes `agent_id: &str` param; becomes `fn(&self, cap: Capability) -> Result<(), rmcp::ErrorData>` | `server.rs` (modified) |
| `AgentRegistry::resolve_or_enroll()` | signature unchanged; `PERMISSIVE_AUTO_ENROLL` const deleted | `infra/registry.rs` (modified) |
| `ServerError::SessionIdentity(String)` | new error variant for startup identity failure | `error.rs` (modified) |
| `UNIMATRIX_SESSION_AGENT` | `std::env::var` key; validated against `[a-zA-Z0-9_-]{1,64}`; not `"system"` or `"human"` | `mcp/session_identity.rs` (new) |

## Pre-flight: Measuring Test Blast Radius (SR-06)

The risk assessment identifies that the "27 tests" figure is misleading. Before writing
any alc-003 implementation code, measure the true blast radius using this approach:

**Step 1 — Compile-gate stub.** In `infra/registry.rs`, change `PERMISSIVE_AUTO_ENROLL`
from `true` to `false` without any other changes. Run the full test suite:

```
cargo test --workspace 2>&1 | grep -E "FAILED|error\[" | sort -u
```

This produces an exact list of tests that currently depend on permissive enrollment.
The count is the true blast radius — not 27, not 185, but the actual number.

**Step 2 — Categorize failures.** Each failing test falls into one of:
- **Registry-permissive tests**: assert `[Read, Write, Search]` for unknown agents
  (directly in `infra/registry.rs` and `mcp/identity.rs`) — must be rewritten to
  explicitly enroll agents before calling Write tools
- **Implicit permissive tests**: integration tests that call Write tools without
  enrolling — must add explicit session agent setup in test fixtures
- **Unrelated failures**: tests broken by compile errors from signature changes in step 1

**Step 3 — Fix test infrastructure before alc-003 code.** The spec writer's AC-10
requires all 185 infra tests continue to pass. The implementation plan must sequence:
pre-flight blast radius measurement → test fixture updates → then alc-003 code changes.

**Test fixture pattern for post-alc-003 tests.** Add a `make_server_with_session()`
helper in the test infrastructure that constructs a `UnimatrixServer` with a
pre-configured `SessionAgent`. Every integration test that calls Write tools uses this
helper instead of the current `make_registry()`. This is the single fixture change that
fixes the implicit permissive test category.

## W2-2 OAuth Replacement Seam

The `SessionIdentitySource` enum is the concrete replacement target for W2-2. The
complete replacement is:

1. Add `SessionIdentitySource::JwtClaims { token: String }` variant
2. Implement `resolve()` for that variant: parse JWT, validate signature, extract `sub`
   claim, run the same `[a-zA-Z0-9_-]{1,64}` validation, return `ValidatedAgentId`
3. In the HTTP transport handler's `initialize()` callback, extract the `Authorization`
   header, construct `SessionIdentitySource::JwtClaims { token }`, call `.resolve()`,
   call `enroll_session_agent()`, and store the result as a per-connection (not
   per-server) `SessionAgent` — the startup-time assumption no longer holds for HTTP
   where each connection may have a different identity
4. `UnimatrixServer` in HTTP mode holds `session_agent: Option<SessionAgent>` set at
   connection time rather than server-construction time

**W2-2 does not touch:**
- Capability resolution logic in `require_cap()`
- Audit attribution logic in `build_context()`
- The `SessionAgent` struct itself
- The `enroll_session_agent()` function
- Any tool handler

This is the spec writer's acceptance criterion target: W2-2 can implement OAuth by
adding one enum variant and one `resolve()` implementation, period.

## Open Questions for the Spec Writer

1. **`resolve_or_enroll()` store-level signature**: The scope says to remove the
   `permissive` parameter from the store-level `agent_resolve_or_enroll()` as well.
   Should alc-003 scope include the store crate cleanup, or is removing the server-side
   call site sufficient for this feature? (Recommendation: clean the store-level
   signature too — leaving a dead parameter is confusing and W0-3 will touch the store
   anyway.)

2. **`resolve_agent()` / `identity::resolve_identity()` fate**: These functions become
   dead code after alc-003. Should they be deleted in this feature, or left with a
   `#[deprecated]` marker? Deleting is cleaner; leaving risks them being called again
   in a future feature that misunderstands the model.

3. **Test helper naming**: The `make_server_with_session()` test fixture helper —
   should this live in a `test_support` module shared across all integration test files,
   or inline in each test file? Given 185 existing infra tests, a shared module is
   strongly recommended. Is there an existing shared test helper module to extend?

4. **`require_cap()` signature change ripple**: The `agent_id` parameter removal from
   `require_cap()` requires updating all 12 tool handler call sites. This is mechanical
   but must be counted explicitly. The spec writer should enumerate the 12 tools and
   confirm the count before writing acceptance criteria — any new tools added after
   this architecture is written would not be counted.

5. **`ToolContext::trust_level` field**: Currently sourced from the per-call resolved
   identity. Post-alc-003 it comes from `self.session_agent.trust_level`. Is `trust_level`
   used anywhere in tool handlers beyond passing through to `AuditContext`? If it is
   used for conditional logic in any handler, the spec writer must enumerate those
   handlers explicitly so they are tested.

6. **Startup failure exit code**: AC-04 and AC-07 require non-zero exit. Should the
   exit code be a specific value (e.g., `78` for EX_CONFIG in sysexits.h convention),
   or is any non-zero sufficient? The daemon launcher polls for socket readiness and
   treats any non-zero exit as failure — the specific code only matters for operators
   scripting the startup.
