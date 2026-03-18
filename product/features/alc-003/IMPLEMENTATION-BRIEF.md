# alc-003: Session Identity via Env Var — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/alc-003/SCOPE.md |
| Architecture | product/features/alc-003/architecture/ARCHITECTURE.md |
| Specification | product/features/alc-003/specification/SPECIFICATION.md |
| Risk Strategy | product/features/alc-003/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/alc-003/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| session_identity.rs (new) | pseudocode/session_identity.md | test-plan/session_identity.md |
| server.rs (modified) | pseudocode/server.md | test-plan/server.md |
| main.rs (modified) | pseudocode/main.md | test-plan/main.md |
| infra/registry.rs (modified) | pseudocode/infra_registry.md | test-plan/infra_registry.md |
| store/registry.rs (modified) | pseudocode/store_registry.md | test-plan/store_registry.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Replace the compile-time `PERMISSIVE_AUTO_ENROLL = true` constant — which silently grants
`[Read, Write, Search]` to every connecting process — with operator-configured session
identity. The server reads `UNIMATRIX_SESSION_AGENT` from the environment at startup,
enrolls the named agent with `[Read, Write, Search]`, and caches those capabilities for
every tool call in the session. If the env var is absent or invalid the server refuses to
start — no fallback, no degraded access. This is W0-2: the capability system becomes
non-vestigial and the `SessionIdentitySource` abstraction seam is established for the
W2-2/W2-3 OAuth replacement.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| `SessionIdentitySource` shape | Enum (not trait object) with `resolve() -> Result<ValidatedAgentId, SessionIdentityError>`. Fixed variant set; additive extension for W2-2 via new variant only. | Architecture §1 | architecture/ADR-001-session-identity-source-shape.md |
| Capability source post-alc-003 | Session capabilities cached in `UnimatrixServer` at startup. `require_cap()` checks `self.session_agent.capabilities` — no registry lookup, no DB, no `spawn_blocking` per call. | Architecture §7 | architecture/ADR-002-capability-source-session-not-per-call.md |
| `PERMISSIVE_AUTO_ENROLL` fate | Deleted entirely — constant, all call sites, and `permissive` parameter on store-level `agent_resolve_or_enroll`. No env var replacement, no escape hatch. | Architecture §5 | architecture/ADR-003-permissive-auto-enroll-deleted.md |
| ADR #1839 (`UNIMATRIX_CLIENT_TOKEN`) disposition | Deferred. alc-003 ships as named-identifier identity (no hashing); #1839 is the future token-hardening layer. Not implemented in this feature. | Architecture §Technology Decisions | architecture/ADR-004-adr-1839-deferred.md |
| Test blast radius measurement | Pre-flight first: change `PERMISSIVE_AUTO_ENROLL` to `false`, run full suite, record failures, fix test fixtures — all before any behavioral code. | Architecture §Pre-flight | architecture/ADR-005-preflight-blast-radius-measurement.md |

---

## Implementation Sequence

### Phase 0 — Pre-flight Measurement (not committed)

1. Edit `infra/registry.rs`: change `PERMISSIVE_AUTO_ENROLL` from `true` to `false`.
2. Run `cargo test --workspace 2>&1 | grep -c FAILED` — record the count.
3. Enumerate each failing test; categorize as registry-permissive vs. implicit integration failure.
4. Revert the single-line change.

### Phase 1 — Test Infrastructure (committed separately)

Fix all test fixtures identified in Phase 0:
- Add `make_server_with_session()` helper in a shared `test_support` module.
- Update registry-permissive tests to explicitly enroll agents or use the session agent path.
- Commit as: `test: update fixtures for alc-003 capability enforcement (pre-flight)`

Phase 2 may not begin until Phase 1 passes clean against the `PERMISSIVE_AUTO_ENROLL=false` stub.

### Phase 2 — alc-003 Behavioral Implementation

Implement in this order to keep the workspace compiling at each step:

1. **New file: `mcp/session_identity.rs`** — `SessionIdentitySource` enum, `ValidatedAgentId` newtype, `SessionAgent` struct, `SESSION_AGENT_DEFAULT_CAPS` constant, `enroll_session_agent()` function, `SessionIdentityError` enum.
2. **`error.rs`** — add `ServerError::SessionIdentity(String)` variant.
3. **`infra/registry.rs`** — delete `PERMISSIVE_AUTO_ENROLL` const and `permissive` parameter from `resolve_or_enroll()`.
4. **`store/registry.rs`** — remove `permissive` parameter from `agent_resolve_or_enroll()`.
5. **`server.rs`** — add `session_agent: SessionAgent` field; update `UnimatrixServer::new()` signature; rewrite `build_context()` and `require_cap()`.
6. **12 tool handlers** — update `require_cap(ctx.agent_id, Capability::X)` → `require_cap(Capability::X)` atomically.
7. **`main.rs`** — wire `SessionIdentitySource::EnvVar.resolve()` + `enroll_session_agent()` into both `tokio_main_daemon()` and `tokio_main_stdio()`.
8. **Integration tests** — AC-01 through AC-10; R-01 through R-14 scenarios as enumerated in RISK-TEST-STRATEGY.md.

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/mcp/session_identity.rs` | Create | New module: `SessionIdentitySource` enum, `ValidatedAgentId` newtype, `SessionAgent` struct, `SESSION_AGENT_DEFAULT_CAPS` constant, `enroll_session_agent()`, `SessionIdentityError` enum |
| `crates/unimatrix-server/src/error.rs` | Modify | Add `ServerError::SessionIdentity(String)` variant for startup identity failure |
| `crates/unimatrix-server/src/infra/registry.rs` | Modify | Delete `PERMISSIVE_AUTO_ENROLL` const; remove `permissive` param from `resolve_or_enroll()` |
| `crates/unimatrix-server/src/mcp/identity.rs` | Modify | Delete `resolve_identity()` and `resolve_agent()` (dead code post-ADR-002); retain `extract_agent_id()` (now returns an audit label, not a registry key) |
| `crates/unimatrix-server/src/server.rs` | Modify | Add `session_agent: SessionAgent` field; update `UnimatrixServer::new()` signature; rewrite `build_context()` (audit attribution from params or session); rewrite `require_cap()` (remove `agent_id` param, check session caps directly) |
| `crates/unimatrix-server/src/main.rs` | Modify | Both `tokio_main_daemon()` and `tokio_main_stdio()` add session identity setup: resolve → enroll → thread into `UnimatrixServer::new()` |
| `crates/unimatrix-server/src/mcp/tools/*.rs` (12 files) | Modify | Update all `require_cap(ctx.agent_id, Capability::X)` call sites to `require_cap(Capability::X)` |
| `crates/unimatrix-store/src/registry.rs` | Modify | Remove `permissive` parameter from `agent_resolve_or_enroll()`; remove all callers in this crate |
| Test files (infra integration tests) | Modify | Add `make_server_with_session()` shared helper; update permissive-dependent fixtures per Phase 1 plan |

---

## Data Structures

```rust
// mcp/session_identity.rs

pub enum SessionIdentitySource {
    EnvVar,
    #[allow(dead_code)]
    JwtClaims { token: String },  // reserved for W2-2; unimplemented in alc-003
}

pub struct ValidatedAgentId(String);  // private inner; constructible only via resolve()

#[derive(Debug, Clone)]
pub struct SessionAgent {
    pub agent_id: String,
    pub trust_level: TrustLevel,
    pub capabilities: Vec<Capability>,
}

pub enum SessionIdentityError {
    Missing { source: &'static str },
    Invalid { source: &'static str, reason: String },
    ProtectedName { name: String },
}

pub const SESSION_AGENT_DEFAULT_CAPS: &[Capability] = &[
    Capability::Read,
    Capability::Write,
    Capability::Search,
];
```

```rust
// server.rs — UnimatrixServer additions
pub struct UnimatrixServer {
    // ... existing fields unchanged ...
    pub(crate) session_agent: SessionAgent,  // required; populated at construction
}
```

---

## Function Signatures

```rust
// mcp/session_identity.rs

impl SessionIdentitySource {
    pub fn resolve(&self) -> Result<ValidatedAgentId, SessionIdentityError>;
}

pub fn enroll_session_agent(
    registry: &AgentRegistry,
    id: ValidatedAgentId,
    capabilities: Vec<Capability>,
) -> Result<SessionAgent, ServerError>;
```

```rust
// server.rs

impl UnimatrixServer {
    pub fn new(
        // ... existing params ...
        session_agent: SessionAgent,   // new required param (last)
    ) -> Self;

    pub(crate) async fn require_cap(
        &self,
        cap: Capability,               // agent_id param removed
    ) -> Result<(), rmcp::ErrorData>;
}
```

```rust
// build_context() audit attribution logic (server.rs)
let audit_agent_id = agent_id
    .as_deref()
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .unwrap_or(&self.session_agent.agent_id)
    .to_string();
```

```rust
// main.rs — startup wiring (both tokio_main_daemon and tokio_main_stdio)
let session_agent_id = SessionIdentitySource::EnvVar
    .resolve()
    .map_err(|e| ServerError::SessionIdentity(e.to_string()))?;

let session_agent = enroll_session_agent(
    &registry,
    session_agent_id,
    SESSION_AGENT_DEFAULT_CAPS.to_vec(),   // W0-3 replaces this argument with config read
)?;
```

---

## Constraints

### Security Posture (Accepted)
The capability set granted to the session agent (`[Read, Write, Search]`) is identical to
what `PERMISSIVE_AUTO_ENROLL=true` previously gave every caller. The improvement in alc-003
is **authentication structure**, not reduced permissions: Write is now granted by the
operator (via `settings.json`) rather than automatically. LLM cannot self-elevate by
sending a different `agent_id`.

### Forward Compatibility Requirements
- **W0-3**: `SESSION_AGENT_DEFAULT_CAPS` must be defined as a named constant, not an inline
  literal, at the boundary where `main.rs` calls `enroll_session_agent()`. W0-3 replaces
  the argument value with a config-file read — no other file changes needed.
- **W2-2**: `main.rs` must resolve identity (via `SessionIdentitySource::EnvVar.resolve()`)
  and pass the result to `UnimatrixServer::new()` — not inside the constructor. This
  call-site discipline allows W2-2 to assign identity per-connection rather than at
  server construction, by changing only the caller.

### Technical Constraints
- `AgentRegistry` is constructed synchronously (`block_sync`). Env var reading does not
  need async.
- `UnimatrixServer` is cloned into each MCP session task (daemon mode). `SessionAgent`
  must be `Clone` — `String`, `TrustLevel`, and `Vec<Capability>` all satisfy this.
  No `Arc`, `Mutex`, or `RwLock` on session identity fields.
- `build_context()` is async. Session identity must be threaded through `UnimatrixServer`
  fields, not free functions.
- Hook identities (`"hook"`, `"background"`) use dedicated startup paths and are
  unaffected.
- `enroll_session_agent()` calls `store.agent_enroll()` directly, bypassing the
  `AgentRegistry` protected-agent guard. The protected-agent check must occur in
  `SessionIdentitySource::resolve()` (case-insensitive) before this function is called.
  `store.agent_enroll()` itself has no protected-agent guard — document this invariant.

### Breaking Change
alc-003 is a breaking change for all existing Unimatrix deployments. Any deployment that
upgrades without setting `UNIMATRIX_SESSION_AGENT` in `settings.json` will find the server
refuses to start. Stderr must name the env var explicitly and state the required action —
no Rust panic trace as primary output.

---

## Dependencies

### Internal Crates
| Crate | Role |
|-------|------|
| `unimatrix-server` | Primary change target — identity resolution, startup wiring, server construction |
| `unimatrix-store` | Remove `permissive` param from `agent_resolve_or_enroll()`; session agent enrolled via existing upsert path |

### External / Stdlib
| Dependency | Version | Role |
|------------|---------|------|
| `std::env` | stdlib | Read `UNIMATRIX_SESSION_AGENT`. No new crate dependency. |
| `sqlx` | existing (nxs-011) | Session agent enrollment uses async write pool |
| `regex` or `char::is_alphanumeric` | existing or stdlib | Validate `[a-zA-Z0-9_-]{1,64}` pattern |
| `rmcp` | 0.16.0 | `require_cap()` returns `rmcp::ErrorData` |

### Prerequisite Features
| Feature | Status |
|---------|--------|
| vnc-005 Daemon Mode | COMPLETE — `tokio_main_daemon()` path must be updated alongside `tokio_main_stdio()` |
| nxs-011 sqlx Migration | COMPLETE — session agent enrollment uses async sqlx write pool |

---

## NOT in Scope

1. **W0-3 config externalization** — `SESSION_AGENT_DEFAULT_CAPS` is hardcoded as
   `[Read, Write, Search]`. Making it configurable via `[agents] session_capabilities`
   in `config.toml` is W0-3.
2. **ADR #1839 `UNIMATRIX_CLIENT_TOKEN`** — token hashing, bcrypt/argon2 storage, and
   the `unimatrix enroll --token` CLI are out of scope.
3. **HTTP transport or OAuth** — W2-2/W2-3 concern. `UNIMATRIX_SESSION_AGENT` is not a
   credential.
4. **Multi-session identity differentiation** — one env var, one session-level identity.
5. **AGENT_REGISTRY schema changes** — no new columns, no migration version bump.
6. **Renaming `agent_id` tool parameters** — stays `Optional<String>` on all 12 structs;
   only interpretation changes.
7. **Hook identity differentiation** — `"hook"` / `"background"` paths unaffected.
8. **Converting `PERMISSIVE_AUTO_ENROLL` to a runtime env var** — deleted entirely.
9. **Per-connection identity in daemon mode** — W2-2 concern.

---

## Alignment Status

**Source**: ALIGNMENT-REPORT.md reviewed 2026-03-18.

| Check | Status |
|-------|--------|
| Vision Alignment | RESOLVED (see below) |
| Milestone Fit | PASS |
| Scope Gaps | PASS |
| Scope Additions | RESOLVED (see below) |
| Architecture Consistency | PASS |
| Risk Completeness | PASS |

### VARIANCE-01 — Resolved

The product vision W0-2 [Critical] item originally stated that an absent
`UNIMATRIX_SESSION_AGENT` must produce `[Read, Search]` only (degraded mode). All three
source documents implement the opposite: server refuses to start entirely (non-zero exit,
no tool calls). The spawn prompt confirms this variance is **resolved in favor of the
source documents (Option A — fail-fast)**. The product vision has been updated to match:
`UNIMATRIX_SESSION_AGENT` absent → server refuses to start. No source document changes
required.

### WARN — Store-level permissive param cleanup

Architecture §Open Questions #1 raises whether the `permissive` parameter on
`unimatrix-store/src/registry.rs::agent_resolve_or_enroll()` is removed in this feature.
The spawn prompt confirms this is **in scope** per ADR-003. The implementation must remove
the parameter from the store crate as well, not only the server-side call site. R-09 covers
the workspace-compilation blast radius check.

### Remaining Open Questions for Human Review

The architecture identifies six open questions; two are confirmed resolved by context (store
cleanup in scope, VARIANCE-01 resolved). The remaining four require delivery-time decisions:

1. **`resolve_agent()` / `identity::resolve_identity()` fate** — delete or deprecate?
   Recommendation: delete (dead code post-ADR-002 creates future confusion risk).
2. **Test helper placement** — `make_server_with_session()` in a shared `test_support`
   module vs. inline per test file. Recommendation: shared module given 185 existing tests.
3. **`require_cap()` call site count** — architecture says 12 tool handlers; confirm the
   actual count before starting Phase 2 in case new tools were added after this doc.
4. **Startup failure exit code** — any non-zero, or specific value (e.g., `78` for
   EX_CONFIG)? The daemon launcher treats any non-zero as failure. A specific value only
   matters for operator scripting.
