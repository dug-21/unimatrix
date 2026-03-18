# alc-003: Session Identity via Env Var — Specification

**Feature ID**: alc-003
**GH Issue**: #293
**Roadmap position**: W0-2
**Status**: Specification

---

## Objective

Replace the compile-time `PERMISSIVE_AUTO_ENROLL = true` constant — which silently grants Write
access to every connecting process — with an operator-configured session identity. The server reads
`UNIMATRIX_SESSION_AGENT` from the environment at startup, enrolls the named agent with
`[Read, Write, Search]`, and uses those capabilities for every tool call in the session. If the
env var is absent or invalid, the server refuses to start. No fallback, no degraded access mode.

This feature makes the capability system non-vestigial. It also establishes the identity
resolution abstraction boundary that the OAuth replacement (W2-2/W2-3) will slot into without
touching capability resolution or audit attribution logic.

---

## Domain Model

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| **SessionIdentity** | The authenticated identity of the MCP client process. Set once by the operator in `settings.json` via `UNIMATRIX_SESSION_AGENT`. Resolved at server startup. Valid for the lifetime of the server process. Cannot be changed without restarting the server. |
| **CallIdentity** | The attribution label for a single tool call. Contains two fields: `attribution` (the per-call `agent_id` parameter value, or the session agent name if absent) and `capabilities` (always copied from SessionIdentity — never from the per-call label). |
| **Session Agent** | The single enrolled agent whose capabilities govern all tool calls in the session. Enrolled with `TrustLevel::Internal` at startup. Identity comes from operator configuration, not from the LLM. |
| **Attribution Label** | The value recorded in the audit log to identify which role or specialist made a call. Set by `agent_id` in each tool call. Does not affect capability resolution. Examples: `alc-003-researcher`, `alc-003-architect`. |
| **Authentication** | The act of establishing which MCP client process is connecting. In alc-003 this is the operator setting `UNIMATRIX_SESSION_AGENT` in `settings.json` — outside LLM control. |
| **Permissive Auto-Enroll** | The removed mechanism. Previously: `const PERMISSIVE_AUTO_ENROLL: bool = true` caused every unknown `agent_id` to be enrolled with `[Read, Write, Search]`. Deleted entirely in alc-003 — no env var replacement, no escape hatch. |
| **SessionIdentitySource** | An abstraction boundary (trait or enum) wrapping "how the session agent identity is obtained at startup." The env-var implementation is one variant. JWT claim extraction (W2-2/W2-3) is the next variant. Capability resolution and audit attribution logic do not depend on which variant is active. |
| **Protected Agent** | Agent names reserved by the system: `"system"` and `"human"`. `UNIMATRIX_SESSION_AGENT` must not equal either of these names. Startup fails if it does. |
| **SESSION_AGENT_DEFAULT_CAPS** | Named constant (not inline literal) for the capability set `[Read, Write, Search]` granted to the session agent. Isolated at the module boundary W0-3 will replace with a config-file read. |

### Identity Concept Separation

Two concepts that must not be conflated:

| Concept | Source | Scope | Purpose | Capability effect |
|---------|--------|-------|---------|-------------------|
| `UNIMATRIX_SESSION_AGENT` | Operator (`settings.json`) | One per server process | Authentication — is this MCP client authorized? | Sole capability authority |
| `agent_id` (tool parameter) | LLM, per call | One per tool call | Attribution — which specialist made this call? | None — audit log only |

### Entity Relationships

```
Operator
  └─ sets UNIMATRIX_SESSION_AGENT in settings.json
       │
       ▼
Server Startup
  ├─ SessionIdentitySource::EnvVar reads + validates env var
  ├─ Enrolls SessionAgent with TrustLevel::Internal + SESSION_AGENT_DEFAULT_CAPS
  └─ UnimatrixServer holds { session_agent_id: String, session_capabilities: AgentCapabilities }
       │
       ▼
Tool Call (per call)
  ├─ CallIdentity.attribution = params.agent_id if non-empty, else session_agent_id
  └─ CallIdentity.capabilities = session_capabilities  (no registry lookup)
       │
       ▼
Audit Log
  └─ Records attribution label + capability check result
```

---

## Functional Requirements

**FR-01**: The server reads `UNIMATRIX_SESSION_AGENT` from the process environment at startup,
before any MCP tool call is processed. Both startup paths — `tokio_main_stdio` and
`tokio_main_daemon` — perform this read.

**FR-02**: If `UNIMATRIX_SESSION_AGENT` is absent (env var not set), the server exits with a
non-zero exit code and emits a message naming the missing variable. No tool calls are processed.
No fallback identity is used. No degraded-access mode is entered.

**FR-03**: If `UNIMATRIX_SESSION_AGENT` is set but fails validation (see FR-04), the server exits
with a non-zero exit code and emits a message identifying the specific validation failure. No tool
calls are processed.

**FR-04**: Validation rules for `UNIMATRIX_SESSION_AGENT`:
- Value must be non-empty.
- Value must match the pattern `[a-zA-Z0-9_-]{1,64}` (alphanumeric, underscore, hyphen; 1–64
  characters).
- Value must not equal `"system"` or `"human"` (protected agent names; comparison is
  case-insensitive to prevent spoofing by casing variation).
- Any violation triggers FR-03 behavior.

**FR-05**: On successful validation, the server enrolls the session agent via an idempotent upsert
into `AGENT_REGISTRY` with `TrustLevel::Internal` and capability set `SESSION_AGENT_DEFAULT_CAPS`
(`[Read, Write, Search]`). The env var value is authoritative: any existing registry record for
this agent name is overwritten to match. This enrollment runs after `bootstrap_defaults()`.

**FR-06**: `UnimatrixServer` holds `session_agent_id: String` and
`session_capabilities: AgentCapabilities` as required fields (not `Option`). Both are populated at
construction time from the validated session agent. These fields are `Clone`-safe so they survive
per-session task cloning in daemon mode.

**FR-07**: For every tool call, capability resolution uses `session_capabilities` from
`UnimatrixServer` directly. No registry lookup occurs at call time. The per-call `agent_id`
parameter does not affect capability resolution under any circumstance — not for known agents, not
for unknown agents, not for swarm specialists.

**FR-08**: For every tool call, audit attribution is determined as follows:
- If `params.agent_id` is present and non-empty after trimming: use that value.
- Otherwise: use `session_agent_id`.
The attribution value is recorded in the audit log. It does not change the capability check.

**FR-09**: `PERMISSIVE_AUTO_ENROLL` is removed entirely: the compile-time constant, all call sites
passing the permissive flag, and the `permissive` parameter on `agent_resolve_or_enroll`. No env
var replacement is added. Auto-enrollment of unknown agents does not occur in any path.

**FR-10**: `SessionIdentitySource` is defined as an explicit abstraction boundary (trait or enum)
between "how the session agent name is obtained" and "what is done with that name." The env-var
implementation is one variant. The abstraction must be defined such that W2-2/W2-3 can add a JWT
claim extraction variant without modifying capability resolution, audit attribution, or
`UnimatrixServer` construction logic.

**FR-11**: The session agent identity is logged at server startup (including after each MCP
`initialize` event in daemon mode). The log record includes: the session agent name, the trust
level, the capability set, and whether this was a fresh enrollment or an upsert over an existing
record.

**FR-12**: `SESSION_AGENT_DEFAULT_CAPS` is defined as a named constant at a module boundary that
W0-3 can replace with a config-file read. It must not appear as an inline literal inside identity
resolution logic, startup logic, or `main.rs`.

---

## Non-Functional Requirements

**NFR-01 — Startup latency**: The env var read, validation, and session agent enrollment must
complete within 50ms on a cold start. The enrollment is a single upsert against an already-open
SQLite pool; no additional I/O is introduced beyond what `bootstrap_defaults()` already performs.

**NFR-02 — Failure visibility**: Startup failures due to missing or invalid `UNIMATRIX_SESSION_AGENT`
must produce a message on stderr that:
- Names the env var explicitly.
- States the specific failure reason (absent / invalid format / protected name).
- Does not produce a Rust panic trace as the primary error output.

**NFR-03 — No per-call DB overhead**: Capability resolution at tool-call time must not perform any
database read. The session capabilities must be available from in-memory fields on `UnimatrixServer`.
This is verifiable by inspection: `build_context()` must not call any `Store` or `AgentRegistry`
method.

**NFR-04 — Clone safety**: `UnimatrixServer` is cloned into each MCP session task in daemon mode.
`session_agent_id` and `session_capabilities` must be `Clone`. No `Arc`, `Mutex`, or `RwLock` is
required for these fields — they are read-only after construction.

**NFR-05 — Idempotent startup**: Restarting the server with the same `UNIMATRIX_SESSION_AGENT`
value must not increase the row count in `AGENT_REGISTRY`. The enrollment is an upsert. This
applies equally to daemon mode between connection cycles.

**NFR-06 — No schema migration**: `AGENT_REGISTRY` schema is unchanged. No migration version bump
is required for alc-003.

**NFR-07 — Test isolation**: Tests that previously relied on `PERMISSIVE_AUTO_ENROLL=true` must be
updated to use one of two explicit patterns: (a) enroll the test agent before exercising the Write
path, or (b) set `UNIMATRIX_SESSION_AGENT` in the test process environment before constructing the
server under test. No test may assume permissive behavior implicitly.

---

## Acceptance Criteria

**AC-01 — Session agent enrolled at startup**
- Precondition: `UNIMATRIX_SESSION_AGENT=my-agent` is set in the environment before server start.
- Action: Server starts.
- Verification: Query `AGENT_REGISTRY` for `agent_id = "my-agent"`. Row exists with
  `trust_level = "internal"` and capabilities `[Read, Write, Search]`. Row is present before any
  tool call is served.
- Method: Integration test asserting registry state immediately after server construction.

**AC-02 — Absent agent_id falls back to session agent for attribution**
- Precondition: `UNIMATRIX_SESSION_AGENT=my-agent` is set. Server is running.
- Action: Issue a tool call with no `agent_id` parameter.
- Verification: Audit log records `agent_id = "my-agent"` for the call. The string `"anonymous"` does not appear.
- Method: Integration test inspecting `AUDIT_LOG` after the call.

**AC-03 — Per-call agent_id is attribution only; capabilities come from session**
- Precondition: `UNIMATRIX_SESSION_AGENT=my-agent` is set. `alc-003-researcher` is not enrolled in `AGENT_REGISTRY`.
- Action: Issue a `context_store` (Write) tool call with `agent_id: "alc-003-researcher"`.
- Verification: (1) Call succeeds (not rejected with CapabilityDenied). (2) Audit log records `agent_id = "alc-003-researcher"`. (3) No new row for `alc-003-researcher` exists in `AGENT_REGISTRY`. (4) No registry lookup occurs during the call (verifiable by mocking or instrumentation).
- Method: Integration test covering all four assertions.

**AC-04 — Absent env var causes startup refusal**
- Precondition: `UNIMATRIX_SESSION_AGENT` is not set in the environment.
- Action: Attempt server start.
- Verification: Process exits with non-zero exit code. Stderr contains the string `"UNIMATRIX_SESSION_AGENT"`. No MCP tool call is served.
- Method: Integration test spawning server as subprocess, capturing exit code and stderr.

**AC-05 — Swarm specialist passes attribution through without enrollment**
- Precondition: `UNIMATRIX_SESSION_AGENT=my-agent` is set. `alc-003-spec-writer` is not in `AGENT_REGISTRY`.
- Action: Call `context_store` with `agent_id: "alc-003-spec-writer"`.
- Verification: (1) Call succeeds. (2) Audit log entry records `"alc-003-spec-writer"`. (3) `AGENT_REGISTRY` row count is unchanged (no new enrollment).
- Method: Integration test asserting registry row count before and after the call.

**AC-06 — PERMISSIVE_AUTO_ENROLL removed entirely**
- Verification: (1) `grep -r "PERMISSIVE_AUTO_ENROLL" crates/` returns no results. (2) No env var named `PERMISSIVE_AUTO_ENROLL` is read anywhere in the codebase. (3) No test references the constant or passes `permissive=true` as a boolean flag.
- Method: CI-enforced grep check added to the test suite or build script.

**AC-07 — Invalid env var causes startup refusal with named reason**
- Precondition: `UNIMATRIX_SESSION_AGENT` is set to each of the following values in separate test cases: `""` (empty), a 65-character string (too long), `"my agent"` (space in name), `"system"` (protected), `"human"` (protected), `"HUMAN"` (protected, uppercase variant).
- Action: Attempt server start for each value.
- Verification: Each attempt exits with non-zero exit code. Stderr names the validation failure reason. No tool calls are served.
- Method: Parameterized integration test covering all six invalid cases.

**AC-08 — Session agent enrollment is idempotent**
- Precondition: `UNIMATRIX_SESSION_AGENT=my-agent` is set.
- Action: Start server, stop server, start server again with same env var.
- Verification: `AGENT_REGISTRY` contains exactly one row for `"my-agent"` after the second start. Trust level and capabilities are unchanged.
- Method: Integration test asserting row count = 1 after two startup cycles.

**AC-09 — Bootstrap agents unaffected**
- Precondition: `UNIMATRIX_SESSION_AGENT` is absent.
- Action: Attempt server start (will fail per AC-04), but inspect `AGENT_REGISTRY` state at the point of failure. Also: start server with a valid env var and inspect bootstrap agent records.
- Verification: `"system"` and `"human"` registry records have their original capabilities and trust levels unchanged by the alc-003 session agent enrollment.
- Method: Integration test asserting `system` and `human` records are present and unchanged after session agent upsert.

**AC-10 — Existing test suite passes**
- Verification: All 185 infra integration tests pass. Tests previously relying on permissive behavior are updated to use explicit enrollment or the session agent path. No test is deleted — only updated.
- Method: `cargo test --workspace` passes in CI with `UNIMATRIX_SESSION_AGENT` set to a valid test value and `PERMISSIVE_AUTO_ENROLL` absent from the environment.

---

## User Workflows

### Workflow 1: Operator Configuring settings.json (New Deployment)

1. Operator adds an `"env"` key to `.claude/settings.json`:
   ```json
   {
     "env": {
       "UNIMATRIX_SESSION_AGENT": "my-project-agent"
     },
     "hooks": { ... }
   }
   ```
2. Operator starts the Unimatrix server (or Claude Code starts it automatically).
3. Server reads `UNIMATRIX_SESSION_AGENT`, validates `"my-project-agent"` (matches pattern, not protected).
4. Server enrolls `"my-project-agent"` with `TrustLevel::Internal` and `[Read, Write, Search]`.
5. Server logs: `"Session agent enrolled: my-project-agent (internal) [Read, Write, Search] — fresh enrollment"`.
6. Server begins accepting MCP tool calls.
7. All tool calls use session capabilities. Swarm specialist labels (e.g., `alc-003-researcher`) appear in audit log but are not enrolled.

### Workflow 2: Operator Upgrading an Existing Deployment (Migration)

1. Operator has an existing Unimatrix deployment with no `UNIMATRIX_SESSION_AGENT` set.
2. Before upgrading to the alc-003 build, operator adds `UNIMATRIX_SESSION_AGENT` to `settings.json` (see Workflow 1, step 1).
3. Operator installs the new binary.
4. Server starts. Old permissive behavior is gone. Session identity is in effect.
5. **If operator skips step 2**: Server refuses to start (AC-04). Error message on stderr names the missing configuration. No silent degraded access.

**Operator action required before upgrading**: Set `UNIMATRIX_SESSION_AGENT` in `settings.json`. This is the only migration action. No data migration. No schema change. No registry cleanup needed.

### Workflow 3: Developer Running Tests

1. Developer runs `cargo test --workspace`.
2. Test harness sets `UNIMATRIX_SESSION_AGENT=test-session-agent` in the test process environment before constructing any `UnimatrixServer` under test.
3. Tests that previously relied on auto-enrollment of arbitrary `agent_id` values (permissive path) now either:
   - Call `registry.enroll_agent("test-agent", TrustLevel::Internal, SESSION_AGENT_DEFAULT_CAPS)` before issuing Write tool calls, or
   - Use the session agent identity (no `agent_id` parameter) and assert on `"test-session-agent"` in the audit log.
4. Tests assert `PERMISSIVE_AUTO_ENROLL` is absent. CI grep check enforces this.

### Workflow 4: Daemon Restart Scenario

1. Daemon starts with `UNIMATRIX_SESSION_AGENT=project-bot` in its environment.
2. Daemon enrolls `"project-bot"` and caches capabilities in memory.
3. Operator changes `UNIMATRIX_SESSION_AGENT` in `settings.json` to `"project-bot-v2"`.
4. Existing daemon connections continue to use `"project-bot"` identity — the env var is read once at daemon startup, not per connection.
5. New connections to the same running daemon also use `"project-bot"` — the daemon's startup identity.
6. To apply the identity change: operator restarts the daemon. On restart, `"project-bot-v2"` is enrolled and becomes the session identity.
7. **Operational constraint**: `UNIMATRIX_SESSION_AGENT` changes require a daemon restart to take effect. This is documented behavior, not a bug. The startup log records the session agent name so operators can verify which identity is active.
8. At each MCP `initialize` event, the daemon logs the active session agent name, making identity mismatches visible without restarting.

---

## Migration Path

### What Existing Deployments Must Do

| Step | Action | Required? |
|------|--------|-----------|
| 1 | Add `"env": { "UNIMATRIX_SESSION_AGENT": "<name>" }` to `.claude/settings.json` | Yes — server will not start without it |
| 2 | Choose a name matching `[a-zA-Z0-9_-]{1,64}`, not `"system"` or `"human"` | Yes |
| 3 | Restart the server (or daemon) after installing the alc-003 build | Yes |
| 4 | No data migration, no schema change, no registry cleanup | N/A |

### What Does NOT Require Operator Action

- Existing `AGENT_REGISTRY` rows for `"system"` and `"human"` are preserved unchanged.
- Existing knowledge entries, audit log, and all other data are unaffected.
- Swarm specialist `agent_id` values in existing audit logs are unaffected — attribution history is preserved.

### Breaking Change Declaration

**alc-003 is a breaking change for all existing Unimatrix deployments.**

The previous behavior — accepting any connection without configuration — no longer exists. Any
deployment that upgrades without setting `UNIMATRIX_SESSION_AGENT` will find the server refuses to
start. This is intentional. Silent degraded access (the previous behavior) is the problem being
solved. The error message must make the required action clear.

---

## Constraints

### Security Posture

The capability set granted to the session agent (`[Read, Write, Search]`) is identical to what
`PERMISSIVE_AUTO_ENROLL=true` previously granted to any caller. The security improvement in
alc-003 is authentication structure, not reduced permissions:

| Property | Before (permissive) | After (alc-003) |
|----------|--------------------|--------------------|
| Who grants Write | Nobody — automatic | Operator, via `settings.json` |
| LLM can grant itself Write | Yes (any `agent_id`) | No |
| Unconfigured server | Starts with full Write | Refuses to start |
| Swarm subagents | Auto-enrolled in registry | Audit-only, inherit session caps |
| Audit attribution | `"anonymous"` or LLM-supplied | Named session agent or subagent role |

The STDIO local deployment model has no transport-level authentication available until HTTP + OAuth
(W2-2). `UNIMATRIX_SESSION_AGENT` provides attribution identity, not a cryptographic credential.
It must not be treated as a secret. This is an accepted risk documented at W0-2 in the product
vision. The forward path to real credentials is W2-2/W2-3.

### ADR #1839 Reconciliation

ADR #1839 designs `UNIMATRIX_CLIENT_TOKEN` as token-hashed, pre-enrolled credential validation.
alc-003 implements named-identifier identity (no hashing, no pre-enrollment). These are sequential
stages of the staged identity model, not competing mechanisms. ADR #1839 remains open; it is the
future hardening layer that extends alc-003 by adding token validation. The architect must update
ADR #1839 status in Unimatrix to "deferred — superseded in scope by alc-003 for W0-2; remains open
for future hardening wave."

### W0-3 Forward Compatibility

`SESSION_AGENT_DEFAULT_CAPS` (the `[Read, Write, Search]` constant) is hardcoded in alc-003. W0-3
will replace it with `[agents] session_capabilities` from the config file. The implementation must
expose this constant at a named location (module boundary) so W0-3 can wire the config reader to it
without modifying `identity.rs`, `main.rs`, or startup logic.

### W2-2/W2-3 Forward Compatibility

The `SessionIdentitySource` abstraction must be defined in alc-003 such that JWT claim extraction
(W2-2/W2-3) is a new variant, not a rewrite. The capability resolution path and `UnimatrixServer`
construction must not encode any assumption about where the session agent name comes from. The env
var implementation is one `SessionIdentitySource` variant. HTTP OAuth tokens are another. Both
produce the same output: a validated session agent name that is then enrolled and cached.

### Technical Constraints

- `AgentRegistry` is constructed synchronously (via `block_sync`). Env var reading at construction
  time does not require async. The permissive flag must be read once at construction, not per call.
- `UnimatrixServer` is cloned into each MCP session task in daemon mode. `session_agent_id` and
  `session_capabilities` must be `Clone`. `String` and `AgentCapabilities` satisfy this if
  `AgentCapabilities` is not wrapped in a non-Clone container.
- `build_context()` is async. Threading `session_agent_id` through it requires adding the field to
  `UnimatrixServer`, not to free functions.
- Hook identities (`"hook"`, `"background"`) use dedicated startup paths that are unaffected by
  this change.

---

## Dependencies

### Internal

| Component | Location | Role in alc-003 |
|-----------|----------|----------------|
| `identity.rs` | `crates/unimatrix-server/src/mcp/identity.rs` | Replace `extract_agent_id()` with `resolve_call_identity()` returning `CallIdentity`. Add `SessionIdentitySource` trait/enum. |
| `registry.rs` (infra) | `crates/unimatrix-server/src/infra/registry.rs` | Remove `PERMISSIVE_AUTO_ENROLL`. Add `session_capabilities: AgentCapabilities`. |
| `registry.rs` (store) | `crates/unimatrix-store/src/registry.rs` | Remove `permissive` parameter from `agent_resolve_or_enroll`. |
| `server.rs` | `crates/unimatrix-server/src/server.rs` | Add `session_agent_id: String` and `session_capabilities: AgentCapabilities`. Update `build_context()`. |
| `main.rs` | `crates/unimatrix-server/src/main.rs` | Both `tokio_main_daemon` and `tokio_main_stdio` read and validate env var, enroll session agent. |

### External / Crate-Level

| Dependency | Version | Role |
|------------|---------|------|
| `std::env` | stdlib | Read `UNIMATRIX_SESSION_AGENT`. No new crate dependency. |
| `sqlx` | existing | Session agent upsert uses existing async write path (W0-1 foundation). |
| `regex` / pattern match | existing or stdlib | Validate `[a-zA-Z0-9_-]{1,64}` pattern. |

### Prerequisite Features

| Feature | Status | Dependency |
|---------|--------|-----------|
| W0-0 Daemon Mode (`vnc-005`) | COMPLETE | `tokio_main_daemon` path must be updated alongside `tokio_main_stdio`. |
| W0-1 sqlx Migration (`nxs-011`) | COMPLETE | Session agent enrollment uses the async sqlx write pool. No `rusqlite` or `spawn_blocking` paths remain. |

---

## NOT in Scope

The following are explicitly excluded from alc-003 to prevent scope creep:

1. **W0-3 config externalization** — `SESSION_AGENT_DEFAULT_CAPS` is hardcoded as `[Read, Write, Search]` in this feature. Making it configurable via `[agents] session_capabilities` in `config.toml` is W0-3.
2. **ADR #1839 UNIMATRIX_CLIENT_TOKEN** — Token hashing, bcrypt/argon2 storage, and the `unimatrix enroll --token` CLI are not implemented. ADR #1839 remains open for a future hardening wave.
3. **HTTP transport or OAuth** — `UNIMATRIX_SESSION_AGENT` is not a credential and is not validated as one. OAuth token claim replacement is W2-2/W2-3.
4. **Multi-session identity differentiation** — A single env var sets one session-level identity for the entire server process. Per-subagent capability differentiation is out of scope.
5. **Changing the AGENT_REGISTRY schema** — No columns added, no migration version bump.
6. **Renaming `agent_id` tool parameters** — The parameter remains `Optional<String>` on all 12 tool call structs. Only interpretation changes.
7. **Hook identity differentiation** — `"hook"` and `"background"` identities use dedicated paths and are unaffected.
8. **Converting PERMISSIVE_AUTO_ENROLL to a runtime env var** — It is deleted entirely. No env var replacement ships.
9. **Per-connection identity in daemon mode** — All connections to a running daemon share the startup identity. Per-connection identity is a W2-2 concern.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for agent authentication, capability resolution, session identity — found ADR-003 (agent identity via tool parameters, entries #31 and #79), ADR #1839 (UNIMATRIX_CLIENT_TOKEN STDIO security model, entry #1839), ToolContext pattern (pre-validated handler context, entry #317), specification role duties (entries #110 and #223), AC coverage convention (entry #138), testable requirements convention (entry #133), and pre-flight enrollment lesson (entry #265). No prior specification used `SessionIdentitySource` as a named abstraction — this is a new pattern introduced in alc-003.
