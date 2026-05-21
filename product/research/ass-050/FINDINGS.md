# FINDINGS: Security Model Review — OSS + Enterprise Foundation

**Spike**: ASS-050
**Date**: 2026-04-22
**Approach**: audit + design
**Confidence**: high

All recommendations are marked:
- `[CODE]` = confirmed by code read
- `[PRIOR]` = derived from prior spike finding (ASS-041, ASS-048, or ASS-049)
- `[INFERENCE]` = reasoned inference — confidence level stated inline

> **CORRECTION — 2026-04-22**: Seam 5 analysis (Section 5) contains a factual error
> discovered during vnc-014 design review. `cycle_events.cycle_id` is the feature
> identifier (`topic` / `feature_cycle`, e.g. `"crt-027"`) — confirmed by code:
> `let cycle_id = feature_cycle.clone()` in `uds/listener.rs`. It is NOT the MCP
> session ID. The direct join `audit_log.session_id = cycle_events.cycle_id` does
> not work. The correct two-hop provenance chain is:
> `audit_log.session_id → sessions.session_id → sessions.feature_cycle = cycle_events.cycle_id`.
> Additionally, the `context_cycle` MCP handler does NOT write to `cycle_events` or
> update `sessions.feature_cycle` — both are written exclusively by the Claude Code
> hook path, breaking behavioral provenance for Codex CLI and Gemini CLI. Issue #574
> tracks the fix. The rmcp UUID (OQ-03) is NOT the right value for `audit_log.session_id`
> — the two-hop chain requires the agent-declared session_id to match `sessions.session_id`.
> Affected sections corrected below: Seam 5, Invariant 2, Invariant 6, OQ-03, Section 9.

---

## Executive Summary

The current Unimatrix security implementation is a known compensating design: per-call declared `agent_id` strings substituting for the session-pinned identity that Anthropic revoked. The audit confirmed that the compensation is shallow — in permissive mode, any caller gets full capabilities regardless of what they declare, and the `"human"` agent_id hack is the only guard on Admin-gated tools. This is security-through-obscurity on the STDIO path and no security at all on an HTTPS path.

The path forward does not require reconstruction. The architecture is already close to right: `AgentRegistry`, `ResolvedIdentity`, `AuditLog`, and `build_context()` are all preserved. The required changes are: (1) add a `BearerValidator` tower middleware layer as a purely additive seam for OSS HTTPS auth, (2) one schema migration adding four fields to `audit_log` and installing append-only triggers, and (3) two non-breaking extensions to `extract_agent_id()` and `build_context()` to accept identity from connection context rather than only from tool params.

The `agent_id` tool parameter is reclassified from a security mechanism to an attribution/persona metadata hint. `AgentRegistry` is retained for persona persistence and analytics, not security.

This spike's output gates W2-2 (HTTPS transport), W2-3 (enterprise identity), and the developer cloud security model. The audit log schema migration must land before any Wave 2 feature that touches auth, identity, or audit — do not defer.

---

## 1. Implementation Audit

### 1.1 `infra/registry.rs` — `AgentRegistry`

**Built to do**: Identity-and-capability store. Enrolls agents, resolves trust level and capabilities, gates operations via `require_capability()`. Auto-enrolls unknown agents (`permissive = true`: full `[Read, Write, Search]`; `permissive = false`: `[Read, Search]`). Bootstraps three protected agents: `system` (System trust, full caps), `human` (Privileged trust, full caps), `cortical-implant` (Internal trust, Read+Search). [CODE]

**Identity assumption**: Per-call declared string identity only. `resolve_or_enroll(agent_id: &str)` is called with a string from tool params — the string is the identity boundary. No session or connection anchor exists. [CODE]

**What breaks under bearer-token model**:
- `require_capability()` calls in tool handlers become no-ops for access control — checks still run and pass but add latency and DB round-trips for no security value.
- Auto-enroll logic becomes vestigial: creates registry records for declared strings, but the strings are no longer a security boundary.
- `context_enroll` as a security gate disappears: in OSS tier, Admin is implicit for any bearer-authenticated client.
- `PROTECTED_AGENTS` (`["system", "human"]`) loses security significance; still useful if registry is retained for attribution.
- The `"human"` agent_id hack for Admin-gated tools (`context_quarantine`, `context_enroll`) is eliminated.

**What is load-bearing and preserved**:
- `agent_id` string as attribution field in `AuditEvent` (persona tracking). [CODE]
- `last_seen_at` update — analytics value.
- `permissive` / `session_caps` config for future stricter modes.
- `bootstrap_defaults()` idempotency for backward compat.
- `AgentRecord.allowed_topics` and `allowed_categories` columns — partially-implemented ABAC foundation; must not be dropped. [CODE: `schema.rs:303-305`]

---

### 1.2 `infra/audit.rs` — `AuditLog`

**Built to do**: Append-only record of tool calls. Wraps `AuditEvent` structs, writes to `audit_log` SQLite table via `SqlxStore`. Provides sync (`log_event` via `block_in_place`) and async (`log_event_async`) write paths to avoid write-pool starvation. [CODE]

**Identity assumption**: Records `agent_id` as freeform string from tool parameter, or `"anonymous"` if absent. No token fingerprint, credential type, or non-spoofable attribution field. `session_id` present in struct but populated as `String::new()` at multiple call sites — breaking the join to `cycle_events`. [CODE: `tools.rs:1383-1385`, `tools.rs:1416-1418`]

**Current `audit_log` DDL (8 fields)**:
```sql
CREATE TABLE IF NOT EXISTS audit_log (
    event_id   INTEGER PRIMARY KEY,
    timestamp  INTEGER NOT NULL,
    session_id TEXT    NOT NULL,
    agent_id   TEXT    NOT NULL,
    operation  TEXT    NOT NULL,
    target_ids TEXT    NOT NULL DEFAULT '[]',
    outcome    INTEGER NOT NULL,
    detail     TEXT    NOT NULL DEFAULT ''
);
-- Indexes: agent_id, timestamp
```

**Compliance gaps**:
- No `credential_type` — cannot distinguish STDIO, OSS bearer token, enterprise JWT.
- No `capability_used` — the capability gate evaluated is unrecorded.
- No `agent_attribution` distinct from `agent_id` — in OSS tier, attribution comes from `clientInfo.name`, not from the tool param.
- No `metadata` JSON field for ISO 42001 AI system attributes.
- `session_id` often `""` at call sites — join to `cycle_events` broken in practice.
- Append-only: application-level convention only; no DDL trigger enforcement. [CODE: verified DDL has no triggers]

---

### 1.3 `mcp/identity.rs` — `ResolvedIdentity`

**Built to do**: Extracts `agent_id` from tool call parameters, normalizes it (trim whitespace, default to `"anonymous"`), resolves against `AgentRegistry` to produce `ResolvedIdentity` with `trust_level` and `capabilities`. [CODE: `identity.rs:22-34`]

**Identity assumption**: Per-call declared identity only. No mechanism exists to accept identity from connection context, request extensions, or any source outside the tool payload.

**`ResolvedIdentity` struct assessment**:
```rust
pub struct ResolvedIdentity {
    pub agent_id: String,
    pub trust_level: TrustLevel,
    pub capabilities: Vec<Capability>,
}
```
Structurally sufficient for both OSS bearer-token and enterprise JWT cases. The population path is wrong (tool params, not connection context), but the struct itself needs no changes. [INFERENCE: high confidence]

---

### 1.4 `main.rs` — Startup Wiring

**Current pattern**: `AgentRegistry` and `AuditLog` constructed as `Arc<T>` and injected into `UnimatrixServer::new()`. No `BearerValidator` trait or auth plugin hook exists. Identity comes entirely from tool params at dispatch time. [CODE: `main.rs:540-548`]

**Can current pattern support `BearerValidator` injection?** Yes, without a `ServerBuilder` abstraction. The `BearerValidator` injects into the tower middleware layer wrapping `StreamableHttpService<UnimatrixServer>`, not into `UnimatrixServer` directly. `UnimatrixServer` is `Clone` (required by rmcp); any injected validator must be `Arc<dyn BearerValidator + Send + Sync>`. [CODE: `main.rs:700-708`, `server.rs:190`; PRIOR: ASS-041]

---

### 1.5 `server.rs` — `build_context()`

**What it does**: Extracts `agent_id` from `Option<String>` tool param, calls `resolve_agent()` → `identity::extract_agent_id()` → `identity::resolve_identity()`, produces `ToolContext` with `agent_id`, `trust_level`, `format`, `audit_ctx`, `caller_id`. All 12 tool handlers go through `build_context()`. [CODE: `server.rs:368-410`]

This is the highest-value injection point. A single additive overload makes identity injectable for all 12 tool handlers simultaneously.

**`agent_id` in tool parameter structs**: All 12 tool handler parameter structs include `pub agent_id: Option<String>`. [CODE: `tools.rs` lines 59, 97, 125, 143, 175, 190, 207, 220, 244, 264, 275, 314] After reclassification, this field drives attribution only.

---

### 1.6 Change Categorization Table

| Component | Required Change | Category |
|-----------|-----------------|----------|
| `BearerValidator` trait | Create new trait in `unimatrix-server::infra::auth` | **(a) Additive** |
| `StaticTokenAuth` impl | Create in `unimatrix-server` | **(a) Additive** |
| Token file generation at startup | Add to daemon/stdio startup paths | **(a) Additive** |
| Tower auth middleware layer | `BearerAuthLayer` wrapping `StreamableHttpService` | **(a) Additive** |
| `build_context()` in `server.rs` | Add optional external identity parameter path | **(b) Non-breaking** — existing param path preserved |
| `extract_agent_id()` in `identity.rs` | Add `extract_agent_id_with_context()` overload | **(a) Additive** — existing function untouched |
| `AuditEvent` struct | Add 4 fields: `credential_type`, `capability_used`, `agent_attribution`, `metadata` | **(c) Breaking** — struct change + all call sites |
| `audit_log` DDL | Add 4 columns + append-only triggers + 2 indexes | **(c) Breaking** — schema migration required |
| `AgentRegistry::require_capability()` call sites | No change — preserved for enterprise capability enforcement | No change |
| `context_enroll` / `context_quarantine` Admin gate | No change — bearer-auth clients get Admin via full-cap `ResolvedIdentity` | No change |
| `EnterpriseAuditWriter` trait | Optional field on `UnimatrixServer` | **(a) Additive** |

**Breaking change blast radius**: `AuditEvent` is in `unimatrix-store/src/schema.rs`. Used in `unimatrix-server/src/infra/audit.rs`, `unimatrix-store/src/audit.rs`, and at minimum 20 `AuditEvent` literal construction sites in `tools.rs`. Every literal must be updated. `audit_log` DDL migration: `ALTER TABLE ... ADD COLUMN` with defaults is supported in SQLite; existing rows receive valid defaults. `read_audit_event()` and `log_audit_event()` SQL must be updated. `write_count_since()` query does not reference new fields — no change.

**STDIO mode**: No bearer token in STDIO mode. `BearerValidator` is only invoked on the HTTPS path. STDIO remains unchanged. [INFERENCE: high confidence; confirmed by ASS-041 flow diagram]

---

## 2. OSS Personal Cloud Security Model

### Token Lifecycle

[PRIOR: ASS-041; CODE: main.rs startup pattern confirmed]

1. **Generation**: First run (token file absent at `{data_volume}/token`): generate 32 bytes via `rand::rngs::OsRng`, hex-encode as 64 lowercase chars, write with mode 0600.
2. **First-run print**: Print once to stdout: `[UNIMATRIX TOKEN] <hex>`. Only appearance in plaintext output.
3. **Subsequent runs**: Read file silently into `Arc<String>`. No print.
4. **In-memory**: `Arc<String>` in `StaticTokenAuth` middleware struct. Never stored elsewhere in process memory.
5. **Validation**: `subtle::ConstantTimeEq` comparison on every bearer token header. Timing-safe.
6. **Rotation**: Stop server → delete `{data_volume}/token` → restart. New token printed on next start.

### Token Validation Placement

`StaticTokenAuth` tower middleware wraps `StreamableHttpService<UnimatrixServer>`. Intercepts every HTTP request before the rmcp service layer. On success: writes full-capability `ResolvedIdentity` to request extensions. On failure: returns HTTP 401 immediately. Purely additive — no changes to tool dispatch or tool handler code. [PRIOR: ASS-041]

### `agent_id` as Optional Attribution Metadata

In the OSS personal cloud tier, `agent_id` from tool params is observation metadata only — not a security mechanism. The security decision is made by middleware based on bearer token alone.

Attribution source hierarchy (OSS tier):
1. **Primary**: `clientInfo.name` from MCP `initialize` handshake — non-spoofable per-session source. [PRIOR: ASS-049 confirms availability]
2. **Secondary**: `agent_id` tool parameter — agent persona (architect, researcher, etc.) — useful for analytics and correlation.
3. **Fallback**: `"anonymous"`.

The `agent_id` parameter in all 12 tool structs does not need to be removed — it remains an optional persona hint. In OSS tier with bearer auth, any bearer-authenticated caller auto-resolves to full capabilities; `require_cap()` passes unconditionally. [CODE + PRIOR]

### `AgentRegistry` and `context_enroll` Disposition

`AgentRegistry` is retained for persona persistence and `last_seen_at` analytics. Auto-enrollment with permissive defaults is correct in OSS tier: any `clientInfo.name`-derived agent_id gets full caps on first appearance, consistent with bearer-token full-access model.

`context_enroll` is no longer a required onboarding step. It remains callable but is not user-facing in the OSS developer experience.

**Zero-enrollment-friction confirmed**: Any client presenting a valid bearer token gets full access immediately. No `context_enroll` call, no agent_id pre-registration, no configuration required beyond the token. [INFERENCE: high confidence]

The `"human"` agent_id hack for Admin-gated tools is eliminated: any bearer-authenticated client receives a full-capability `ResolvedIdentity` from `StaticTokenAuth`, including `Capability::Admin`. The undocumented `"human"` workaround is no longer needed.

### Audit Log at OSS Tier

The current `AuditLog` struct cannot record token fingerprint, credential_type, or agent_attribution as distinct fields. Schema changes are required (see Section 4). At OSS tier, each audit record must include:

| Field | Source | Value |
|-------|--------|-------|
| `credential_type` | Middleware layer | `"static_token"` |
| `agent_attribution` | `clientInfo.name` from MCP handshake | Non-spoofable per-session |
| `agent_id` | Tool param (existing) | Persona hint, `"anonymous"` if absent |
| `capability_used` | Tool handler | Which capability gate was evaluated |
| `metadata.token_fingerprint` | `StaticTokenAuth` | SHA-256 of bearer token |
| `session_id` | `ToolContext.audit_ctx.session_id` | Must not default to `""` |

---

## 3. Enterprise Extension Surface

### `BearerValidator` Trait

**Location**: `unimatrix-server`, module `infra::auth`

```rust
pub trait BearerValidator: Send + Sync {
    /// Validate a bearer token and return resolved identity on success.
    /// Called on every HTTP request before tool dispatch.
    async fn validate(&self, token: &str) -> Result<ResolvedIdentity, AuthError>;
}

pub enum AuthError {
    MissingToken,       // no Authorization header
    InvalidToken,       // bad signature, wrong format
    TokenExpired,       // valid signature but exp claim failed
    InsufficientScope,  // valid token but required capability not in scopes
    Internal(String),   // JWKS fetch failure, etc.
}
```

**OSS default impl — `StaticTokenAuth`**:
```rust
pub struct StaticTokenAuth {
    token: Arc<String>,         // plaintext for constant-time compare
    token_fingerprint: String,  // SHA-256 hex for audit records
}

impl BearerValidator for StaticTokenAuth {
    async fn validate(&self, presented: &str) -> Result<ResolvedIdentity, AuthError> {
        use subtle::ConstantTimeEq;
        if presented.as_bytes().ct_eq(self.token.as_bytes()).into() {
            Ok(ResolvedIdentity {
                agent_id: "owner".to_string(), // overridden by clientInfo.name for attribution
                trust_level: TrustLevel::Privileged,
                capabilities: vec![
                    Capability::Read, Capability::Write,
                    Capability::Search, Capability::Admin,
                ],
            })
        } else {
            Err(AuthError::InvalidToken)
        }
    }
}
```

**Enterprise impl contract — `JwtBearerAuth`** (lives in private repo `unimatrix-collective`, crate `unimatrix-compliance`):
- Decode bearer token as JWT. Validate `exp`, `iss`, `aud` (configured Unimatrix audience), signature (RS256/ES256 via JWKS cache). [PRIOR: ASS-041]
- Extract `sub` claim as agent identifier.
- Perform `AgentRegistry` lookup: `sub` → role → `ResolvedIdentity` with role-scoped capabilities.
- Return distinct errors: `AuthError::TokenExpired` for expired tokens, `AuthError::InvalidToken` for signature failures.
- JWKS cache: background refresh tick plus per-validation-failure cache miss fallback.
- Library recommendation: `jsonwebtoken` for JWT decode/verify. [PRIOR: ASS-041]

**Is `ResolvedIdentity` sufficient for both cases?** Yes — confirmed structurally. The population path changes (middleware vs. tool params); the struct itself is unchanged. [CODE: struct fields confirmed adequate for both cases]

### Capability Gating

**OSS tier**: `require_cap()` calls in tool handlers remain unchanged. `ResolvedIdentity.capabilities` always contains all caps for OSS bearer-authenticated clients. Capability checks pass unconditionally but serve as documentation of which capability each tool requires and as enterprise enforcement hooks.

**Enterprise tier**: `JwtBearerAuth` returns role-scoped `ResolvedIdentity`:
- Admin role: `[Read, Write, Search, Admin]`
- Operator role: `[Read, Write, Search]`
- Auditor role: `[Read, Search]`

Tool handlers are unchanged — only the `ResolvedIdentity` population path changes. Enterprise capability enforcement is injected through the identity layer, not through RBAC logic in tool handlers. This is the critical design property for clean OSS/enterprise separation. [CODE + PRIOR]

### Startup Plugin Registration

No `ServerBuilder` abstraction needed for Wave 2. The `BearerValidator` is injected into the tower middleware layer at startup:

```rust
// OSS startup (main.rs):
let validator: Arc<dyn BearerValidator> = Arc::new(
    StaticTokenAuth::load_or_create(&paths.token_path)?
);
let auth_layer = BearerAuthLayer::new(Arc::clone(&validator));
// auth_layer wraps StreamableHttpService<UnimatrixServer> in the HTTPS bind
```

Enterprise binary replaces `StaticTokenAuth::load_or_create` with `JwtBearerAuth::new(config)`. No other code changes. [CODE: confirmed by main.rs post-construction field pattern; INFERENCE: high confidence]

### `EnterpriseAuditWriter` Trait

An enterprise compliance audit log (SIEM-exportable, retention-policy enforced) differs from the SQLite-backed `AuditLog`. Recommended approach:

Keep `Arc<AuditLog>` in `UnimatrixServer` (SQLite-backed, existing behavior). Add `enterprise_audit: Option<Arc<dyn EnterpriseAuditWriter>>` as an optional field, initially `None`. Enterprise startup populates it. Tool dispatch calls both writers when the enterprise writer is set.

**Trait signature** (location: `unimatrix-server::infra::audit`):
```rust
pub trait EnterpriseAuditWriter: Send + Sync {
    fn write_compliance_event(&self, event: &ComplianceAuditEvent);
}

pub struct ComplianceAuditEvent {
    pub event_id: u64,
    pub timestamp: u64,
    pub session_id: String,
    pub credential_type: String,    // "jwt"
    pub agent_attribution: String,  // JWT sub claim
    pub token_fingerprint: String,  // SHA-256 of JWT
    pub operation: String,
    pub capability_used: String,
    pub target_ids: Vec<u64>,
    pub outcome: String,
    pub detail: String,
    pub metadata: serde_json::Value, // ISO 42001 extensible field
}
```

Classification: **(a) Additive** — OSS path is unchanged; enterprise adds on top.

---

## 4. Audit Log Schema Recommendation

### Recommended Schema (12 fields — 4 additions to current 8)

```sql
CREATE TABLE IF NOT EXISTS audit_log (
    -- Existing fields (unchanged)
    event_id          INTEGER PRIMARY KEY,
    timestamp         INTEGER NOT NULL,
    session_id        TEXT    NOT NULL DEFAULT '',
    agent_id          TEXT    NOT NULL DEFAULT '',  -- persona/attribution hint from tool param
    operation         TEXT    NOT NULL,
    target_ids        TEXT    NOT NULL DEFAULT '[]',
    outcome           INTEGER NOT NULL,
    detail            TEXT    NOT NULL DEFAULT '',

    -- New fields (Wave 2 migration)
    credential_type   TEXT    NOT NULL DEFAULT 'none',
    -- Values: 'none' (STDIO), 'static_token' (OSS HTTPS), 'jwt' (enterprise)

    capability_used   TEXT    NOT NULL DEFAULT '',
    -- Capability gate evaluated: 'read', 'write', 'search', 'admin', 'session_write'
    -- Empty only when no capability check ran (acceptable for legacy rows)

    agent_attribution TEXT    NOT NULL DEFAULT '',
    -- clientInfo.name (OSS HTTPS), JWT sub claim (enterprise), or agent_id tool param
    -- Non-spoofable: populated from connection/auth layer, not from tool param

    metadata          TEXT    NOT NULL DEFAULT '{}'
    -- JSON object for AI system attributes
    -- Minimum shape when known: {"model": str, "agent_role": str, "context_version": int}
    -- ISO 42001 extensibility: new attributes added to JSON without schema migration
    -- token_fingerprint stored here: {"token_fingerprint": "<sha256 hex>"}
);

-- Additional indexes (same migration)
CREATE INDEX IF NOT EXISTS idx_audit_log_session ON audit_log(session_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_cred    ON audit_log(credential_type);

-- Append-only enforcement triggers (same migration)
CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: UPDATE not permitted'); END;

CREATE TRIGGER audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: DELETE not permitted'); END;
```

### Field-by-Field Rationale

| Field | Required by | Rationale |
|-------|-------------|-----------|
| `credential_type` | SOC 2 CC6.1, ISO 27001 5.15 | Distinguishes auth tier. Enables "show all JWT-authenticated actions" queries. Value is `'none'` for STDIO today, meaningful for HTTPS and enterprise. |
| `capability_used` | SOC 2 CC6.3 | Duty segregation audit: proves write operations were performed only by authorized roles. Current schema has no record of which capability gate was evaluated. |
| `agent_attribution` | ISO 42001, SOC 2 CC7.1 | Non-spoofable attribution anchor. `agent_id` (tool param) is self-declared and spoofable; `agent_attribution` comes from MCP handshake or JWT sub — not changeable per call. |
| `metadata` | ISO 42001 AI governance | AI system attributes for the "AI agent X with model Y called tool T" audit trail. JSON avoids migration cost as AI attribute tracking matures. Includes `token_fingerprint`. |

### Migration Plan

| Change | Category | Execution |
|--------|----------|-----------|
| Add `credential_type TEXT NOT NULL DEFAULT 'none'` | **(c) Breaking** | `ALTER TABLE audit_log ADD COLUMN credential_type TEXT NOT NULL DEFAULT 'none'` |
| Add `capability_used TEXT NOT NULL DEFAULT ''` | **(c) Breaking** | `ALTER TABLE audit_log ADD COLUMN capability_used TEXT NOT NULL DEFAULT ''` |
| Add `agent_attribution TEXT NOT NULL DEFAULT ''` | **(c) Breaking** | `ALTER TABLE audit_log ADD COLUMN agent_attribution TEXT NOT NULL DEFAULT ''` |
| Add `metadata TEXT NOT NULL DEFAULT '{}'` | **(c) Breaking** | `ALTER TABLE audit_log ADD COLUMN metadata TEXT NOT NULL DEFAULT '{}'` |
| Add `session_id` index | **(a) Additive** | `CREATE INDEX` only |
| Add `credential_type` index | **(a) Additive** | `CREATE INDEX` only |
| Add UPDATE/DELETE triggers | **(a) Additive** | `CREATE TRIGGER` — no data change |

**Recommended execution**: One schema version bump. All four `ALTER TABLE` statements in one migration. Existing rows receive valid defaults — no data loss. `AuditEvent` Rust struct gains 4 fields with `#[serde(default)]`. `log_audit_event()` SQL INSERT updated. All `AuditEvent` literal construction sites in `tools.rs` updated to provide new fields.

---

## 5. Seam Map

Five critical identity resolution seams that must remain injectable. Ordered by blast radius (hardest to retrofit later listed first).

### Seam 1 — `identity.rs::extract_agent_id()` — Primary Identity Source

**Current state**: Identity extracted from `Option<String>` tool parameter only. No connection context consulted. [CODE: `identity.rs:22-34`]

**Injectable now?** Yes — additive overload:
```rust
pub fn extract_agent_id_with_context(
    params_agent_id: &Option<String>,
    connection_identity: Option<&str>, // clientInfo.name or JWT sub
) -> String
```
If `connection_identity` is `Some`, it takes precedence for attribution.

**Confirmed source** (OQ-01 resolved): `clientInfo.name` lives at `ctx.peer.peer_info().map(|ci| ci.client_info.name.as_str())` in any `RequestContext<RoleServer>`. Not in `extensions`. [CODE: rmcp source read, empirical]

**Cost now**: Low — new function alongside existing, zero existing call sites changed.
**Cost of retrofitting later**: Medium — requires touching all `build_context()` call sites across 12 tool handlers.
**Pattern that would foreclose this seam**: Removing the `agent_id` field from tool param structs before an external identity source is wired in.

---

### Seam 2 — `server.rs::build_context()` — Central Identity Dispatch

**Current state**: Takes `agent_id: &Option<String>` from tool params, produces `ToolContext`. [CODE: `server.rs:368-410`]

**Injectable now?** Yes — additive overload:
```rust
pub(crate) async fn build_context_with_external_identity(
    &self,
    params_agent_id: &Option<String>,
    format: &Option<String>,
    session_id: &Option<String>,
    external_identity: Option<&ResolvedIdentity>, // from middleware
) -> Result<ToolContext, rmcp::ErrorData>
```
If `external_identity` is `Some`, bypass `resolve_agent()` entirely.

**Cost now**: Low — additive overload, existing `build_context()` unchanged.
**Cost of retrofitting later**: High — 12 tool handlers all call `build_context()` directly; adding the external identity parameter requires touching all simultaneously.
**Pattern that would foreclose this seam**: Making `build_context()` a sealed trait method, or making tool handlers private such that wrapping is impossible.

---

### Seam 3 — `AuditEvent::agent_id` Field — Attribution Record

**Current state**: `AuditEvent.agent_id` populated from `ctx.agent_id`, which comes from the tool parameter. No separate `agent_attribution` field. [CODE: `schema.rs:AuditEvent`]

**Resolution**: The schema migration (Section 4) adds `agent_attribution` as a distinct field. Downstream analytics then choose the non-spoofable attribution source independently of the tool-param persona hint.

**Cost of not doing it now**: Every audit record permanently loses the distinction between "who the connection said it was" and "what agent persona the call declared." Cannot be reconstructed from existing records.

---

### Seam 4 — `AgentRegistry::resolve_or_enroll()` — Identity Grounding

**Current state**: All capability resolution goes through `resolve_or_enroll(agent_id: &str)`. [CODE: `registry.rs:67-78`]

**Injectable now?** Yes — no changes to `AgentRegistry` needed. Seam 2's `build_context_with_external_identity()` bypasses `resolve_or_enroll` entirely when `external_identity` is `Some`. The bypass condition lives in `resolve_agent()` in `server.rs` — the correct abstraction boundary.

**Pattern that would foreclose this seam**: Forcing all identity resolution through `resolve_or_enroll` without a bypass path.

---

### Seam 5 — `cycle_events` / `audit_log` / `sessions` Provenance Chain

> **CORRECTED — 2026-04-22**: The original Seam 5 analysis was wrong. See correction
> notice at top of file.

**Field semantics (confirmed by code):**

| Field | Concept | Example |
|-------|---------|---------|
| `cycle_events.cycle_id` | Feature being worked on | `"crt-027"` (= `topic` = `feature_cycle`) |
| `audit_log.session_id` | Conversation / MCP session | `"mcp::some-uuid"` |

These are categorically different. Code evidence: `let cycle_id = feature_cycle.clone()`
in `uds/listener.rs` line 2556. `cycle_events.cycle_id` is always the `topic` parameter
from `context_cycle` — not the MCP session identifier.

**The correct two-hop provenance chain:**
```sql
SELECT ae.*, ce.goal, ce.goal_embedding
FROM audit_log ae
JOIN sessions s ON s.session_id = ae.session_id
JOIN cycle_events ce
    ON ce.cycle_id = s.feature_cycle
   AND ce.event_type = 'cycle_start'
WHERE ae.session_id = 'mcp::some-uuid'
```

**Current state of the chain for non-Claude-Code clients (BROKEN):**

The `context_cycle` MCP handler does NOT write to `cycle_events` or update
`sessions.feature_cycle`. Both are written exclusively by the Claude Code hook path
(`uds/listener.rs`). Codex CLI and Gemini CLI do not fire MCP tool call hooks, so:
- `cycle_events` is never written for those sessions
- `sessions.feature_cycle` is never set
- Both hops of the provenance chain are broken

Issue #574 tracks the fix: move these writes to the `context_cycle` MCP handler.

**Pattern that would foreclose this seam**: Renaming `sessions.feature_cycle`,
`cycle_events.cycle_id`, or `audit_log.session_id`. Do not rename any of these fields.
Compacting `cycle_events` in a way that drops `goal_embedding` rows is also prohibited.

**`String::new()` fix approach**: The fix for `audit_log.session_id = String::new()` at
call sites must use `ToolContext.audit_ctx.session_id` (agent-declared), NOT the rmcp
UUID. The two-hop chain requires `audit_log.session_id` to match `sessions.session_id`,
which is keyed on the agent-declared session_id. The rmcp UUID and the agent-declared
session_id are different values; using the rmcp UUID breaks the first hop.

---

## 6. Don't-Foreclose List

Seven behavioral provenance invariants. These must be documented as code review gates, not just conventions.

### Invariant 1 — `audit_log.detail` Must Never Be Truncated or Compressed

`audit_log.detail` contains the human-readable record of what was stored or corrected. For write operations this includes content summaries or actual content. Future goal-action alignment analysis reads actual action payloads — not summaries. If `detail` is compressed or truncated in any schema optimization, the behavioral provenance record becomes permanently incomplete for those rows.

**Invariant**: `audit_log.detail` is never truncated, compressed, or replaced with a summary. If storage pressure demands mitigation, add a separate `summary` column and keep `detail` full.

**Current state**: No truncation today. [CODE: `audit.rs` — `log_audit_event` writes `detail` as provided]

---

### Invariant 2 — `cycle_events.goal_embedding` Must Remain Reachable via the Provenance Chain

`cycle_events.goal_embedding` (BLOB, bincode-encoded `Vec<f32>`) is populated for `cycle_start` events with non-empty `goal`. `idx_cycle_events_cycle_id ON cycle_events (cycle_id)` enables O(log N) lookup. [CODE: `db.rs:637`, `db.rs:643`]

**Invariant**: `cycle_events.cycle_id` must never be renamed or repurposed — it is the
feature identifier (`topic`). `sessions.feature_cycle` must remain joinable to
`cycle_events.cycle_id`. `audit_log.session_id` must remain joinable to
`sessions.session_id`. Any migration that renames any of these three fields breaks the
behavioral provenance chain irretrievably for historical records.

> **CORRECTED — 2026-04-22**: Original wording said "`audit_log.session_id` must remain
> joinable to `cycle_events.cycle_id`" — that direct join does not exist. The chain is
> two hops via `sessions`. See corrected Seam 5.

---

### Invariant 3 — `observations.phase` Must Remain Indexed and Not Dropped

`observations.phase` captures workflow phase at the time each tool call was observed via UDS hooks. `idx_observations_topic_phase ON observations (topic_signal, phase)` is confirmed. [CODE: `db.rs:824`, `db.rs:839-840`]

**Invariant**: `observations.phase` must not be dropped. It is the phase signal for behavioral context analysis. Dropping it destroys this signal irretrievably for historical records.

---

### Invariant 4 — Future `task_log` Table Anchor Requirements

Tasks are currently invisible to Unimatrix (no `task_log` table). [CODE: verified no `task_log` in `db.rs`]

When future work adds task tracking, `task_log` must:
1. Have a `session_id` column that joins to `audit_log.session_id` and `cycle_events.cycle_id`.
2. Have a `timestamp` column for temporal ordering with `audit_log` entries.
3. Use the `prefix_session_id("mcp", sid)` convention for MCP-path sessions.

Without these anchors, tasks cannot be ordered relative to audit entries, and the goal → task → action → outcome behavioral provenance chain cannot be queried.

---

### Invariant 5 — Audit Log Is Append-Only: Never UPDATE, Never DELETE

**Current enforcement**: Application-level convention only. No DDL enforcement. [CODE: verified no triggers in current DDL]

**Invariant**: No UPDATE or DELETE must ever be issued against `audit_log`. The trigger enforcement from Section 4 makes this DDL-enforced from the migration date forward. Until installed, it is a documented code invariant enforced by code review. Any PR that issues non-INSERT SQL against `audit_log` is a rejection-criterion violation.

**Compliance note**: SOC 2 CC7.1 requires tamper-evident audit logs. ISO 27001 Annex A 8.15 requires logs to be "protected." A row modifiable after the fact is not compliant evidence.

---

### Invariant 6 — `audit_log.session_id` Must Never Default to Empty String at Call Sites

Multiple `AuditEvent` literals in `tools.rs` currently construct with `session_id: String::new()`. [CODE: `tools.rs:1383-1385`, `tools.rs:1416-1418`] This breaks the first hop of the provenance chain (`audit_log.session_id → sessions.session_id`).

**Invariant**: All `AuditEvent` construction must populate `session_id` from
`ToolContext.audit_ctx.session_id` (agent-declared) when a session context exists.
`String::new()` is acceptable only when no session is active. The fix must use the
agent-declared session_id — NOT the rmcp UUID — because `sessions.session_id` is
keyed on the agent-declared value. Code review gate: any PR adding an `AuditEvent`
literal must document why `session_id` is empty if it is.

> **CORRECTED — 2026-04-22**: Original wording said this "breaks the join to
> `cycle_events`" — the join is two hops via `sessions`, not direct. The fix approach
> (rmcp UUID in OQ-03) was wrong; agent-declared session_id is the correct value.

---

### Invariant 7 — `agent_id` Tool Parameter Must Never Be the Sole Capability Gate

**Current state**: `agent_id` tool param drives both security (capability check) and attribution (audit log) — a confirmed design error.

**Invariant**: After W2-2/W2-3 lands, capability gating must be driven by `ResolvedIdentity` from the validated auth path (bearer token / JWT), never from the self-declared `agent_id` tool param. The tool param `agent_id` is permanently classified as attribution metadata. This invariant applies to the HTTPS transport path only; STDIO remains unchanged.

---

## 7. Resolved Questions

**OQ-01 and OQ-03 RESOLVED** — See `product/research/ass-050/FINDINGS-OQ01.md` for full evidence.

**OQ-01: `clientInfo.name` capture in HTTPS transport path — ANSWERED**

`clientInfo.name` is accessible at tool call dispatch time via `RequestContext.peer.peer_info()`, NOT via `RequestContext.extensions` as originally hypothesized. The exact access path:

```rust
// ctx is RequestContext<RoleServer> — available in all 12 Unimatrix tool handlers today
let client_name: Option<&str> = ctx
    .peer
    .peer_info()          // Option<&InitializeRequestParams>
    .map(|ci| ci.client_info.name.as_str());
```

`ClientInfo` is a type alias for `InitializeRequestParams` (rmcp `src/model.rs:785`). The `peer` field on `RequestContext` carries the full MCP initialize result, stored at handshake time before any tool call is dispatched. No rmcp upstream changes required. Implementation work is entirely in Unimatrix's `server.rs` (Seam 2 overload). [CODE: rmcp source read, empirical]

**OQ-03: rmcp session UUID accessibility — ANSWERED**

The rmcp-assigned session UUID (server-assigned at `initialize`, not client-controlled) is accessible at tool call time via `RequestContext.extensions.get::<http::request::Parts>()` + reading the `Mcp-Session-Id` header. rmcp's own documentation (`tower.rs:62-69`) names this as the intended mechanism for HTTP-level data in tool handlers:

```rust
fn extract_rmcp_session_id(extensions: &rmcp::model::Extensions) -> Option<String> {
    extensions
        .get::<http::request::Parts>()
        .and_then(|parts| parts.headers.get("mcp-session-id"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned())
}
```

This is distinct from the `session_id` tool parameter: the rmcp UUID is server-assigned and non-spoofable; the tool param is client-chosen and spoofable.

> **CORRECTED — 2026-04-22**: The original conclusion — "the rmcp UUID can populate
> `audit_log.session_id` automatically, enabling the `cycle_events` join" — was wrong
> on both counts. (1) The rmcp UUID is the correct key for `client_type_map` lookup
> (vnc-014 design), not for `audit_log.session_id`. (2) `audit_log.session_id` does
> not join directly to `cycle_events.cycle_id` — the chain goes via `sessions`. Using
> the rmcp UUID for `audit_log.session_id` would break the first hop because
> `sessions.session_id` is keyed on the agent-declared value. The `String::new()` fix
> must use `ToolContext.audit_ctx.session_id` (agent-declared).

No rmcp upstream changes required. [CODE: rmcp source read, empirical]

**vnc-014 implementation path is now fully unblocked.** Both data sources confirmed accessible in rmcp 0.16 as shipped. All required work is in Unimatrix.

---

## 8. Remaining Unanswered Questions

**1. `observations` rows under `crt-036` retention policy**

Invariant 3 asserts `observations.phase` is retained. The index is confirmed [CODE: `db.rs:839-840`]. However, the `crt-036` retention policy (referenced in `main.rs` as `retention_config`) may delete old `observations` rows. Whether the deletion window removes data needed for behavioral provenance analysis is not determined by this spike. [Requires: read of `crt-036` retention implementation]

**3. Append-only triggers — pre-migration row vulnerability window**

Trigger enforcement applies from migration time forward. Pre-Wave-2 rows remain mutable by a database admin. For SOC 2 Type I readiness this is acceptable (point-in-time controls). For Type II certification, the trigger must be installed before the audit period begins — timing must be coordinated with the SOC 2 audit window.

---

## 8. Out-of-Scope Discoveries

1. **`write_count_since()` accuracy under attribution model shift**: When `audit_log.agent_id` is populated from `clientInfo.name` rather than per-call tool params, all calls from "Claude Code" appear as one agent. In multi-tenant enterprise scenarios this creates rate-limiting accuracy problems. Flag for ASS-042 enterprise rate-limiting design.

2. **`observations.session_id` join fragility**: Populated from UDS hook events; same "omitted session_id = empty string" fragility applies for the first hop. ~~Both gaps resolved when session-pinned identity arrives.~~ *(CORRECTED 2026-04-22: `observations.session_id` does not join to `cycle_events.cycle_id` directly either — same two-hop chain via `sessions` applies.)*

3. **`SessionWrite` capability is defined but unused**: `schema.rs:275` defines `Capability::SessionWrite = 4`; no `require_cap(..., Capability::SessionWrite)` call exists anywhere in `tools.rs`. Must be documented as reserved and not removed — removing it would shift the integer values of subsequent capability variants.

4. **`agent_registry` ABAC columns are present but dormant**: `AgentRecord.allowed_topics` and `allowed_categories` (`schema.rs:303-305`) are `None` for all bootstrapped agents and checked by no current capability logic. These are a partially-implemented ABAC foundation. Must not be dropped — they are the seam for future attribute-based access control, which ASS-048 identifies as a post-Wave-2 requirement.

---

## 9. Recommendations Summary

| Topic | Recommendation |
|-------|----------------|
| **Implementation audit** | Execute one breaking migration before Wave 2 ships: `AuditEvent` struct + `audit_log` DDL (4 new columns, append-only triggers, 2 indexes). All other required changes are additive or non-breaking. Do not defer the migration. |
| **OSS personal cloud** | Bearer token = authorization. Any client presenting a valid token has full access — no enrollment, no agent_id pre-registration. `agent_id` tool param is reclassified as persona metadata. `AgentRegistry` retained for attribution analytics. The `"human"` agent_id hack for Admin-gated tools is eliminated by full-capability `ResolvedIdentity` on bearer-authenticated clients. |
| **`BearerValidator` trait** | Create `BearerValidator` trait in `unimatrix-server::infra::auth`. Ship `StaticTokenAuth` OSS impl (constant-time token compare, full capability return). Enterprise ships `JwtBearerAuth` in private repo. Injected via tower middleware layer — no `ServerBuilder` needed for Wave 2. |
| **Enterprise capability gating** | `require_cap()` calls in tool handlers are unchanged — they become enterprise RBAC enforcement points. Enterprise injects role-scoped `ResolvedIdentity` through `JwtBearerAuth`; OSS always provides full caps. RBAC logic never enters tool handler code. |
| **`EnterpriseAuditWriter` trait** | Add as additive optional field on `UnimatrixServer`. OSS path unchanged. Enterprise populates it at startup; dual write (SQLite + SIEM) is post-dispatch. |
| **Audit log schema** | Add 4 fields: `credential_type`, `capability_used`, `agent_attribution`, `metadata`. One migration. Add append-only triggers. Add 2 new indexes. This is a one-way decision — do not patch incrementally. |
| **Seam 1 (`extract_agent_id`)** | Add `extract_agent_id_with_context()` overload now. Low cost. Enables `clientInfo.name` priority for attribution without touching existing call sites. |
| **Seam 2 (`build_context`)** | Add `build_context_with_external_identity()` overload now. Low cost now; high cost to retrofit later across 12 tool handlers. |
| **Seam 5 (session linkage)** | Do not rename `cycle_events.cycle_id`, `sessions.feature_cycle`, or `audit_log.session_id` — these are the three anchor fields of the two-hop provenance chain. Fix the `String::new()` session_id bug using `ToolContext.audit_ctx.session_id` (agent-declared), NOT the rmcp UUID. The rmcp UUID is the `client_type_map` key (vnc-014), not the session anchor. Separately: fix `context_cycle` to write `cycle_events` and update `sessions.feature_cycle` from the MCP handler (issue #574) — currently only the hook path does this, breaking provenance for Codex/Gemini. |
| **Don't-foreclose invariants** | Codify all 7 invariants as code review gates. Priority: Invariant 5 (append-only triggers) and Invariant 6 (session_id empty string) address immediate correctness gaps. Invariants 1–4 and 7 are forward-looking preservation constraints. |
