# alc-003: Session Identity via Env Var

## Problem Statement

Today the only "authentication" Unimatrix has is an optional `agent_id` parameter
filled in by the LLM on each tool call. This is not authentication — it is an
attribution label that the LLM controls. `PERMISSIVE_AUTO_ENROLL = true` (compile-time
constant) enrolls every unknown caller with `[Read, Write, Search]` automatically.

The capability system exists on paper. It is not enforced. Any process connecting gets
full Write access.

### Two distinct identity concepts

`agent_id` in tool calls and `UNIMATRIX_SESSION_AGENT` serve **different purposes** and
must not be conflated:

| Concept | Purpose | Who sets it | Scope |
|---------|---------|-------------|-------|
| `UNIMATRIX_SESSION_AGENT` | **Authentication** — is this MCP client authorized? | Operator, in `settings.json` (outside LLM) | One per server instance |
| `agent_id` in tool params | **Attribution** — which role/specialist made this call? | LLM, per call | Many per session (researcher, architect, etc.) |

`agent_id` is already used correctly as a role/specialty label in the swarm pattern —
`alc-003-researcher`, `alc-003-architect`, etc. That does not change. What changes is
that `agent_id` is no longer the authentication mechanism. It is attribution only.

### The change

Move authentication out of LLM control (per-call `agent_id`) into operator configuration
(`UNIMATRIX_SESSION_AGENT` in `settings.json`). If the env var is not set, the server
refuses to start — zero access, not degraded access. An unauthenticated deployment has
no operational use case for local STDIO.

Capability resolution shifts: the session authentication is the capability authority.
Per-call `agent_id` is audit attribution. Unknown subagents (swarm specialists) that
are not explicitly enrolled inherit session capabilities without being auto-enrolled —
their identities appear in the audit log but do not pollute the registry.

`PERMISSIVE_AUTO_ENROLL` is removed entirely — no env var, no escape hatch.

Forward path: `UNIMATRIX_SESSION_AGENT` is a bridge mechanism. When Unimatrix moves to
HTTP + OAuth (W2-2/W2-3), JWT claims replace this env var. The identity resolution layer
must be structured so the session identity source is swappable without touching capability
resolution or audit attribution logic.

This is W0-2 from the product roadmap, tracked by GH #293.

## Goals

1. `UNIMATRIX_SESSION_AGENT` is required at server startup. If absent, the server refuses
   to start with a clear error. No fallback access level. No degraded mode.
2. Validate `UNIMATRIX_SESSION_AGENT` at startup: `[a-zA-Z0-9_-]{1,64}`, non-empty, not
   a protected agent name (`system`, `human`). Startup fails fast if invalid.
3. Auto-enroll the session agent at startup with `[Read, Write, Search]` (capability set
   hardcoded in alc-003; made configurable in W0-3 via `[agents] session_capabilities`).
4. Remove `PERMISSIVE_AUTO_ENROLL` entirely. No env var, no compile-time const, no escape hatch.
5. Capability resolution uses the authenticated session exclusively. Per-call `agent_id`
   does not affect capabilities — not for unknown agents, not for explicitly enrolled ones.
   Session capabilities are the only capability source.
6. Per-call `agent_id` is audit attribution only. Swarm specialist agents (researcher,
   architect, etc.) continue to pass meaningful `agent_id` values; they appear in the
   audit log and are never enrolled for capability purposes.
7. No registry lookup occurs for per-call `agent_id` at tool call time. The capability
   check uses the session agent's capabilities, resolved once at startup.
8. A tool call with no `agent_id` parameter uses the session agent identity for audit
   attribution.

## Non-Goals

- **W0-3 config externalization** — `UNIMATRIX_SESSION_AGENT` capabilities (`[Read, Write, Search]`)
  are hardcoded in this feature. Making them configurable via `[agents] bootstrap` in a
  config file is W0-3 scope.
- **ADR #1839 UNIMATRIX_CLIENT_TOKEN full implementation** — token hashing, bcrypt/argon2
  storage, and the `unimatrix enroll --token` CLI are out of scope. See reconciliation
  note in Constraints.
- **HTTP transport or OAuth** — `UNIMATRIX_SESSION_AGENT` is explicitly not a credential;
  the OAuth token claim replacement is a future W2-3 concern.
- **Multi-agent session differentiation** — a single env var sets one session-level
  identity. Per-subagent identity differentiation is not addressed here.
- **Changing the AGENT_REGISTRY schema** — no schema migration is required; the existing
  `agent_resolve_or_enroll` path with `permissive=false` already stores `[Read, Search]`.
- **Renaming `agent_id` tool parameters** — the parameter stays `Optional<String>` on
  all 12 tool call structs. Only interpretation changes.
- **Hook identity differentiation** — hooks already use dedicated identities
  (`"hook"`, `"background"`); their path is unaffected.

## Background Research

### Current Identity Flow (end-to-end)

1. Tool call arrives; `build_context()` in `server.rs` calls `identity::extract_agent_id()`.
2. `extract_agent_id()` trims the `agent_id: Option<String>` parameter; returns `"anonymous"` if absent/empty.
3. `resolve_identity()` calls `AgentRegistry::resolve_or_enroll()`.
4. `resolve_or_enroll()` delegates to `store.agent_resolve_or_enroll(agent_id, PERMISSIVE_AUTO_ENROLL)`.
5. In the store, `permissive=true` causes new agents to be enrolled with `[Read, Write, Search]`.
6. The resolved identity is returned; capability check runs against that identity.

### Post-alc-003 Identity Flow

**Startup:**
1. Read `UNIMATRIX_SESSION_AGENT` from env. Fail fast if absent or invalid.
2. Enroll session agent with `[Read, Write, Search]` (idempotent, authoritative over existing record).
3. `UnimatrixServer` holds `session_agent_id: String` (required, not optional).

**Per tool call:**
1. `build_context()` resolves capability from the session agent (already in memory — no DB lookup).
2. Audit attribution = `params.agent_id` if non-empty, else session agent identity.
3. Capability check runs against session capabilities.
4. No registry lookup. No auto-enrollment. `agent_id` is a label, not a key.

All code is in `crates/unimatrix-server/src/mcp/identity.rs` (extraction),
`crates/unimatrix-server/src/infra/registry.rs` (registry facade), and
`crates/unimatrix-store/src/registry.rs` (SQL implementation). The `permissive` flag
is already plumbed all the way through — only the call site value needs to change.

### What the Proposed Change Touches Architecturally

- **`crates/unimatrix-server/src/infra/registry.rs`**: `PERMISSIVE_AUTO_ENROLL` const
  is deleted. `AgentRegistry` gains a `session_capabilities: AgentCapabilities` field
  populated at construction from the enrolled session agent.
- **`crates/unimatrix-server/src/mcp/identity.rs`**: `extract_agent_id()` is replaced
  by `resolve_call_identity(params_agent_id, session_agent_id) -> CallIdentity` where
  `CallIdentity` holds audit attribution (the per-call label) and capabilities (always
  from the session — no registry lookup). The per-call agent_id becomes a label, not a key.
- **`crates/unimatrix-server/src/main.rs`**: Both `tokio_main_daemon()` and
  `tokio_main_stdio()` read and validate `UNIMATRIX_SESSION_AGENT`, fail fast if absent,
  enroll the session agent, and thread the resolved capabilities into server construction.
- **`crates/unimatrix-server/src/server.rs`**: `UnimatrixServer` holds
  `session_agent_id: String` and `session_capabilities: AgentCapabilities` (resolved
  once at startup). `build_context()` uses these directly — no per-call DB lookup.

### ADR #1839 Reconciliation (Critical Pre-Implementation Question)

ADR #1839 (Unimatrix entry #1839) designs `UNIMATRIX_CLIENT_TOKEN` as a token that is
hashed, stored in AGENT_REGISTRY, and validated at MCP initialize time. Token validation
would run before any tool calls. Unknown tokens would reject connection entirely.

`UNIMATRIX_SESSION_AGENT` (W0-2 / alc-003) names the session agent directly (no hashing,
no bcrypt). It is enrolled on-the-fly at startup, not pre-registered. The value is treated
as an attribution label, not a credential.

These two mechanisms address the same problem (session-level identity) with different
trust assumptions:

| Attribute | `UNIMATRIX_SESSION_AGENT` (W0-2) | `UNIMATRIX_CLIENT_TOKEN` (ADR #1839) |
|-----------|----------------------------------|--------------------------------------|
| Value type | Plain identifier | Opaque token (hashed) |
| Enrollment | Auto at startup | Pre-enrolled by admin |
| Rejection | Caps fall to `[Read, Search]` | Connection rejected at initialize |
| Trust claim | Attribution only | Weak credential |
| Schema change | None | Token hash field in AGENT_REGISTRY |
| Effort | Low (env var + fallback) | High (schema, enrollment CLI, hashing) |

**Resolution (proposed)**: W0-2 ships as designed (`UNIMATRIX_SESSION_AGENT`, no hashing).
ADR #1839 is a future extension that strengthens the trust claim by adding token validation.
The two do not conflict at the implementation level: `UNIMATRIX_SESSION_AGENT` is named
identity; `UNIMATRIX_CLIENT_TOKEN` would be credential-based identity. They represent
different points on the staged identity model (Gen 2 → Gen 3 in ADR #1839's own history).

This reconciliation must be confirmed with the human before implementation.

### Failure Mode Analysis

- **Current**: Unnamed caller → `"anonymous"` → `[Read, Write, Search]`. Silent. No
  indication anything is wrong. Write gates are nominal not real.
- **Post-alc-003 with `UNIMATRIX_SESSION_AGENT` set**: Unknown caller → session agent
  identity used as default → `[Read, Write, Search]` under that identity. Correct.
- **Post-alc-003 with no env var, `PERMISSIVE_AUTO_ENROLL=false`**: Unknown caller →
  `[Read, Search]`. Write attempts return `CapabilityDenied`. Loud failure.
- **Post-alc-003 with invalid `UNIMATRIX_SESSION_AGENT`**: Server refuses to start.
  Error message names the validation failure.

### Existing Infrastructure (no new schema required)

- `agent_resolve_or_enroll(id, permissive=false)` already produces `[Read, Search]` — confirmed in `crates/unimatrix-store/src/registry.rs:113-121`.
- `bootstrap_defaults()` already runs idempotently at startup — the session agent can
  be enrolled via the same path.
- `CapabilityDenied` error variant already exists and returns a structured MCP error.
- 27 tests in `registry.rs` cover the `permissive=true` path; the `permissive=false`
  path has one test (the store-level check). Test coverage for the new default will need
  expanding.

### Settings.json / Env Var Delivery

`UNIMATRIX_SESSION_AGENT` would be set in `.claude/settings.json` under an `"env"` key.
Currently `settings.json` contains only `"hooks"`. No `"env"` key is present. Adding
an env key is a Claude Code configuration concern, not a Unimatrix server concern —
the server simply reads from `std::env::var()`.

## Proposed Approach

### 1. Read and validate `UNIMATRIX_SESSION_AGENT` at startup

Both startup paths (`tokio_main_daemon`, `tokio_main_stdio`) add a call to a new
`read_session_agent_env()` function. The function:
- Returns `None` if `UNIMATRIX_SESSION_AGENT` is unset (no failure — permissive=false takes effect).
- Validates the value matches `[a-zA-Z0-9_-]{1,64}`.
- Rejects `"system"` and `"human"` (protected agent names).
- Returns `Err` and fails startup if the value is present but invalid.

### 2. Enroll the session agent at startup

Call `registry.enroll_agent()` with `TrustLevel::Internal` and `[Read, Write, Search]`.
Runs after `bootstrap_defaults()`. Always upserts — env var is authoritative over any
existing registry record for this agent name. Cache the resolved `AgentCapabilities`
in `UnimatrixServer` for use on every tool call thereafter.

### 3. Delete `PERMISSIVE_AUTO_ENROLL`

Remove `const PERMISSIVE_AUTO_ENROLL: bool = true` from `infra/registry.rs` entirely.
Remove all call sites that pass the permissive flag. Remove `agent_resolve_or_enroll`'s
`permissive` parameter — auto-enrollment of unknown agents no longer occurs. Update all
tests that relied on permissive behavior to either enroll agents explicitly or use the
session agent path.

### 4. Thread session identity into `UnimatrixServer`

Add `session_agent_id: String` and `session_capabilities: AgentCapabilities` to
`UnimatrixServer` (both required, resolved at startup). `build_context()` uses
`session_capabilities` directly for every capability check — no per-call registry lookup.
Audit attribution uses `params.agent_id` if provided, else `session_agent_id`.

### 5. Define `SessionIdentitySource` abstraction boundary

Wrap the startup identity resolution behind a named abstraction (trait or enum) so
W2-2/W2-3 can replace env-var reading with JWT claim extraction without touching
capability resolution logic. The architect defines the boundary shape.

### Key design rationale

- **No schema migration** — AGENT_REGISTRY schema is unchanged; session agent is enrolled
  via existing upsert path.
- **Startup fail-fast** — invalid env var causes startup failure, not a silent fallback,
  because a misconfigured identity is a configuration error not an operational condition.
- **Per-call `agent_id` preserved for audit** — the tool parameter remains on all 12
  tool structs. When present and non-empty, it is used for audit attribution. It does not
  override capability resolution for unknown agents (those still fall through to session
  default or `[Read, Search]`).
- **Swappable identity source** — the session identity resolution is abstracted so W2-2
  (HTTP + OAuth) can replace `std::env::var("UNIMATRIX_SESSION_AGENT")` with JWT claim
  extraction without touching capability resolution or audit attribution logic.

## Acceptance Criteria

- AC-01: When `UNIMATRIX_SESSION_AGENT=my-agent` is set in the environment, the server
  enrolls `my-agent` with `[Read, Write, Search]` during startup before any tool calls
  are processed.
- AC-02: When `UNIMATRIX_SESSION_AGENT` is set and a tool call omits `agent_id`, the
  resolved identity is the session agent, not `"anonymous"`.
- AC-03: When `UNIMATRIX_SESSION_AGENT` is set and a tool call provides a non-empty
  `agent_id`, that value is used for audit attribution only. Capabilities are resolved
  from the session — no registry lookup occurs for the per-call agent_id.
- AC-04: When `UNIMATRIX_SESSION_AGENT` is unset, the server refuses to start with a
  non-zero exit code and a message naming the missing configuration. No tool calls are served.
- AC-05: A swarm specialist agent passing `agent_id: "alc-003-researcher"` can call any
  Write tool. The audit log records `"alc-003-researcher"` as the calling agent. No registry
  lookup or enrollment occurs. Capability comes from the session, not the agent_id.
- AC-06: `PERMISSIVE_AUTO_ENROLL` no longer exists as a compile-time constant or env var.
  Any reference to it in code or tests is removed.
- AC-07: When `UNIMATRIX_SESSION_AGENT` is set to an invalid value (empty, too long,
  illegal characters, or a protected agent name), the server exits at startup with a
  non-zero exit code and a message naming the validation failure. No tool calls are served.
- AC-08: `UNIMATRIX_SESSION_AGENT` enrollment is idempotent — restarting the server with
  the same value does not create a duplicate registry entry or change the agent's trust level
  if already enrolled.
- AC-09: When `UNIMATRIX_SESSION_AGENT` is unset and `PERMISSIVE_AUTO_ENROLL` is unset,
  existing bootstrap agents (`system`, `human`) retain their capabilities unchanged.
- AC-10: The existing 185 infra integration tests continue to pass. Tests that currently
  rely on `PERMISSIVE_AUTO_ENROLL=true` behavior are updated to explicitly set the env var
  or to enroll agents before calling Write tools.

## Constraints

### Security Posture Acknowledgement

The security reviewer will flag that an operator-configured session agent receives
`[Read, Write, Search]` — the same as the current permissive default. This is correct
and intentional. The security improvement in alc-003 is not reduced permissions; it is
**authentication structure**:

| Property | Before (permissive) | After (alc-003) |
|----------|--------------------|--------------------|
| Who grants Write | Nobody — it's automatic | Operator, via settings.json |
| LLM can grant itself Write | Yes (any agent_id value) | No |
| Unconfigured server | Starts, full Write access | Refuses to start |
| Swarm subagents (researcher, etc.) | Auto-enrolled in registry | Audit-only, inherit session caps |
| Audit attribution | "anonymous" or whatever LLM sends | Named session agent or subagent role |

The STDIO local deployment model has no better option at this stage: the LLM must have
Write access to be useful, and STDIO provides no transport-level authentication. The env
var set in `settings.json` (outside LLM control) is the only available authentication
boundary until HTTP + OAuth (W2-2) arrives. This is documented as an accepted risk.

The HTTP transport wave (W2-2) + OAuth (W2-3) are where the security posture materially
improves. alc-003 is the necessary first step that makes the capability system non-vestigial
and builds the identity resolution layer that OAuth will later slot into.

### Technical
- `PERMISSIVE_AUTO_ENROLL` was `const bool = true`. Tests in `registry.rs` assert
  `[Read, Write, Search]` for unknown agents (e.g., `test_enrolled_agent_has_write_when_permissive`,
  `test_enroll_unknown_agent`). These tests must be updated when the default changes.
  The test fixture has no isolation from the process environment — tests that need
  permissive behavior must either set the env var or enroll the agent explicitly.
- `AgentRegistry` is constructed synchronously via `block_sync`. Env var reading at
  construction time is correct (no async needed); the permissive flag must be read once,
  not per-call.
- `UnimatrixServer` is cloned into each MCP session task (daemon mode). The session
  agent field must be `Clone` — `Option<String>` satisfies this.
- The `identity.rs` `extract_agent_id()` function is called from `build_context()` which
  is async. Threading `session_agent_id` through requires adding it to `UnimatrixServer`
  not to a free function.

### Reconciliation with ADR #1839
ADR #1839 designs token hashing and pre-enrollment (`UNIMATRIX_CLIENT_TOKEN`) as a
stronger identity mechanism. Resolution: alc-003 ships first as named-identifier identity
(no hashing, no pre-enrollment). ADR #1839 is the future hardening layer — it extends
this by adding token validation. The two do not conflict: `UNIMATRIX_SESSION_AGENT` is
operator-configured attribution; `UNIMATRIX_CLIENT_TOKEN` would be a weak credential.
Sequential stages, not competing defaults. ADR #1839 remains open for a future feature.

### Scope boundary with W0-3
The session agent's capability set (`[Read, Write, Search]`) is hardcoded in alc-003.
W0-3 will replace this hardcoded default with `[agents] session_capabilities` from config.
The alc-003 implementation must expose the capability list as a constant or parameter that
W0-3 can later wire to the config reader without touching identity logic.

## Decisions

1. **Session agent trust level**: `TrustLevel::Internal` — signals operator-configured
   process identity, distinct from `Privileged` (human) or `Restricted` (unknown).

2. **Re-enrollment behavior**: Startup env var is authoritative — always upsert the
   session agent record with `[Read, Write, Search]` regardless of what exists in the
   registry. Since capability resolution no longer runs through the registry at call time,
   this is purely a startup consistency step. No conflict possible.

3. **Bridge mode identity**: Daemon reads `UNIMATRIX_SESSION_AGENT` directly from its
   own environment at startup. The bridge is a transparent pipe and does not forward or
   interpret the env var. Changing `settings.json` requires restarting the daemon session
   to take effect — acceptable.

## Tracking
GH Issue: #293
