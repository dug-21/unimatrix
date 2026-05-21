# FINDINGS-RAW: Security Model Review — OSS + Enterprise Foundation

**Spike**: ASS-050
**Date**: 2026-04-22
**Approach**: audit + design
**Confidence**: high (matches required level)

All recommendations are marked:
- `[CODE]` = confirmed by code read
- `[PRIOR]` = derived from prior spike finding (ASS-041, ASS-048, or ASS-049)
- `[INFERENCE]` = reasoned inference — confidence level stated inline

---

## Phase 1 — Component Audit

### 1.1 `infra/registry.rs` — `AgentRegistry`

**What was this built to do?**
`AgentRegistry` was built as an identity-and-capability store: it enrolls agents, resolves their trust level and capabilities, and gates operations via `require_capability`. It also auto-enrolls unknown agents (`permissive = true` mode gives `[Read, Write, Search]`; `permissive = false` gives `[Read, Search]`). It bootstraps three protected default agents: `system` (System trust, full caps), `human` (Privileged trust, full caps), `cortical-implant` (Internal trust, Read+Search only). [CODE: `infra/registry.rs`, `unimatrix-store/src/registry.rs`]

**What does it assume about identity?**
Per-call declared identity only. The `AgentRegistry` is queried by string `agent_id` on every tool call — the string is the identity. There is no connection-level or session-level identity anchor. [CODE: `identity.rs::resolve_identity()` calls `registry.resolve_or_enroll(agent_id)` with a string from tool params]

**What breaks or becomes redundant if `agent_id` is no longer a per-call security mechanism?**
Under the OSS bearer-token model where any valid token = full access:
- `require_capability()` calls in tool handlers become no-ops — checks still run and pass, but add latency and DB round-trips for no security value.
- `auto_enroll` logic becomes vestigial: runs to create a record for the declared `agent_id` string, but the declared string is no longer a security boundary.
- `context_enroll` as a security gate disappears: in OSS tier, Admin is implicit for any authenticated caller. No one needs to be explicitly enrolled.
- `PROTECTED_AGENTS` (`["system", "human"]`) loses security significance — still needed if `AgentRegistry` persists for attribution, but not a security boundary.
- `enroll_agent()` self-lockout prevention becomes meaningless.

**What is load-bearing and should be preserved?**
- The `agent_id` string as an attribution field in `AuditEvent` (persona tracking). [CODE: `schema.rs:AuditEvent.agent_id`]
- `AgentRegistry.last_seen_at` update — analytics value, not security.
- The `permissive` / `session_caps` config as a control for future stricter modes.
- `bootstrap_defaults()` idempotency — safe to keep for backward compat.

**What was compensating for lost session-pinned identity?**
The `resolve_or_enroll` auto-enrollment with permissive defaults was explicitly a bandaid. In permissive mode, any caller gets full `[Read, Write, Search]` capabilities automatically — no real gating. The system relies on the assumption that clients are trusted (STDIO local context), and the capability checks are documentation rather than security enforcement. [CODE: comment in `registry.rs:AgentRegistry::new`: "unknown agents auto-enroll with Write capability (dsn-001, AC-06)"]

**The `context_quarantine` and `context_enroll` Admin gating detail:**
These two tools require `Capability::Admin`. In STDIO mode, the only way to get `Admin` is to be the bootstrapped `human` agent. The operator passes `agent_id: "human"` to these tools — `human` is bootstrapped with Admin, so the check passes. This is security-through-obscurity. [CODE: `tools.rs:1353`, `tools.rs:1457`; `registry.rs:PROTECTED_AGENTS`]

---

### 1.2 `infra/audit.rs` — `AuditLog`

**What was this built to do?**
Append-only record of tool calls. Wraps `AuditEvent` structs and writes them to the `audit_log` SQLite table via `SqlxStore`. Provides both sync (`log_event` via `block_in_place`) and async (`log_event_async`) write paths to avoid GH #302 write-pool starvation. [CODE: `infra/audit.rs`]

**What does it assume about identity?**
Records `agent_id` as a freeform string — whatever was passed as the `agent_id` tool parameter (or `"anonymous"` if absent). No token fingerprint, no credential type, no `session_id` link to `cycle_events` (session_id is stored but populated from `String::new()` in several call sites). [CODE: `schema.rs:AuditEvent`, `tools.rs:1383-1385`, `tools.rs:1416-1418`]

**Current `audit_log` DDL** (8 fields):
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
)
```
Indexes: `agent_id`, `timestamp`. [CODE: `unimatrix-store/src/db.rs:792-810`]

**Gaps vs. compliance requirements:**
- No `credential_type` field — no way to distinguish STDIO local (no auth), OSS bearer token, or enterprise JWT.
- No `capability_used` field — the capability gate evaluated is not recorded.
- No `agent_attribution` field distinct from `agent_id` — in OSS tier, attribution comes from `clientInfo.name`, not from the `agent_id` tool param.
- No `metadata` JSON field for AI system attributes required by ISO 42001.
- `session_id` present but often `""` at call sites.

**Append-only enforcement:**
Application-level convention only. No DDL trigger-based enforcement prevents DELETE or UPDATE. [CODE: verified DDL has no triggers]

---

### 1.3 `mcp/identity.rs` — `ResolvedIdentity`

**What was this built to do?**
Extracts `agent_id` from tool call parameters, normalizes it (trim whitespace, default to `"anonymous"`), resolves against `AgentRegistry` to produce a `ResolvedIdentity` with `trust_level` and `capabilities`. [CODE: `mcp/identity.rs`]

**What does it assume about identity?**
Per-call declared identity only. `extract_agent_id()` reads from `Option<String>` in tool params. No mechanism exists to accept identity from connection context, request extensions, or any source outside the tool payload. [CODE: `identity.rs:22-34`]

**What breaks if `agent_id` is no longer a security mechanism?**
- `resolve_identity()` still needed for audit attribution — `agent_id` in `AuditEvent` comes from `ctx.agent_id`.
- The capability resolution path (`registry.resolve_or_enroll → record.capabilities`) becomes vestigial overhead in OSS tier.
- `extract_agent_id()` must be extended or replaced with a function that can also accept identity from connection context (`clientInfo.name` from MCP handshake, or JWT `sub` from request extensions). Currently no such mechanism exists. [INFERENCE: high confidence]

**Is `ResolvedIdentity` sufficient to carry both OSS and enterprise cases?**
The current struct:
```rust
pub struct ResolvedIdentity {
    pub agent_id: String,
    pub trust_level: TrustLevel,
    pub capabilities: Vec<Capability>,
}
```
For OSS bearer token: `agent_id = clientInfo.name` or default, `trust_level = Privileged`, `capabilities = full set`. Works.
For enterprise JWT: `agent_id = JWT sub claim`, `trust_level` from role lookup, `capabilities` from role. Also works — IF the struct is populated from middleware layer rather than from tool params.

`ResolvedIdentity` is structurally sufficient but the population path is wrong: it currently reads from tool params, not from connection context. A seam is needed to inject identity from outside `identity.rs`. [INFERENCE: high confidence]

---

### 1.4 `main.rs` — Startup Wiring

**Current constructor pattern:**
`AgentRegistry` and `AuditLog` are constructed as `Arc<T>` and injected into `UnimatrixServer::new()`. There is no `BearerValidator` trait or auth plugin hook — the server has no concept of a bearer token. Identity comes entirely from tool params at dispatch time. [CODE: `main.rs:540-548`]

**Can the current pattern support `BearerValidator` injection without a `ServerBuilder` abstraction?**
Yes. The pattern used for `observation_registry`, `inference_config`, and `store_config` (post-construction field assignment: `server.field = value`) works for any new auth validator. The `BearerValidator` is injected into the tower middleware layer, not directly into `UnimatrixServer` — the server itself does not need to know about it. The middleware resolves identity and writes it to request extensions; tool handlers read from extensions. [CODE: `main.rs:700-708`, `server.rs:200-243`; PRIOR: ASS-041 flow diagram]

**Key dependency**: `UnimatrixServer` is `Clone` (required by rmcp). Any injected validator in the middleware must be `Arc<dyn Validator + Send + Sync>`. [CODE: `server.rs:190`]

---

### 1.5 `server.rs` — `UnimatrixServer::build_context()`

**What `build_context()` does:**
1. Extracts `agent_id` from `Option<String>` tool param
2. Calls `resolve_agent()` → `identity::extract_agent_id` → `identity::resolve_identity`
3. Produces `ToolContext` with `agent_id`, `trust_level`, `format`, `audit_ctx`, `caller_id`
4. Tool handler then calls `require_cap(&ctx.agent_id, Capability::X)` [CODE: `server.rs:368-410`]

This is the critical seam. All 12 tool handlers go through `build_context()`. If modified to accept identity from an external source (e.g., request extensions from middleware), all tool handlers benefit simultaneously. This is the highest-value injection point.

**`agent_id` in tool parameter structs:**
All 12 tool handler parameter structs include `pub agent_id: Option<String>`. [CODE: `tools.rs` lines 59, 97, 125, 143, 175, 190, 207, 220, 244, 264, 275, 314] This field currently drives security AND attribution. After the fix, it drives attribution only (persona hint).

---

## Output 1: Implementation Audit — Change Categorization

| Component | Required Change | Category |
|-----------|-----------------|----------|
| `BearerValidator` trait | Create new trait in `unimatrix-server::infra::auth` | **(a) Additive** |
| `StaticTokenAuth` impl | Create in `unimatrix-server` | **(a) Additive** |
| Token file generation at startup | Add to daemon/stdio startup paths | **(a) Additive** |
| Tower auth middleware layer | `BearerAuthLayer` wrapping `StreamableHttpService` | **(a) Additive** |
| `build_context()` in `server.rs` | Add optional external identity parameter path | **(b) Non-breaking** — existing param path still works |
| `extract_agent_id()` in `identity.rs` | Add `extract_agent_id_with_context()` overload | **(a) Additive** — keep existing fn |
| `AuditEvent` struct | Add 4 fields: `credential_type`, `capability_used`, `agent_attribution`, `metadata` | **(c) Breaking** — struct change + all call sites |
| `audit_log` DDL | Add 4 columns + append-only triggers + 2 indexes | **(c) Breaking** — schema migration required |
| `AgentRegistry::require_capability()` call sites | No change — keep as-is for enterprise compatibility | No change |
| `context_enroll` / `context_quarantine` Admin gate | No change — bearer-auth clients have Admin via full-cap `ResolvedIdentity` | No change |
| `write_count_since()` | No change for OSS; token fingerprint index additive for enterprise | **(a) Additive** for enterprise |

**Breaking change blast radius — `AuditEvent` struct and `audit_log` DDL:**
- `AuditEvent` is in `unimatrix-store/src/schema.rs`. Used in `unimatrix-server/src/infra/audit.rs`, `unimatrix-store/src/audit.rs`, and at minimum 20 `AuditEvent` literal construction sites in `tools.rs`.
- Every call site constructing an `AuditEvent` literal must be updated to provide the new fields.
- `audit_log` DDL migration: SQLite `ALTER TABLE ... ADD COLUMN` supports adding nullable/defaulted columns. All new columns have defaults so existing rows are valid.
- `read_audit_event()` and `log_audit_event()` SQL must be updated.
- `write_count_since()` query does not reference new fields — no change there.

**STDIO mode**: No bearer token in STDIO mode; `BearerValidator` is only invoked on the HTTPS path. STDIO remains unchanged. [INFERENCE: high confidence — confirmed by ASS-041 flow diagram]

---

## Output 2: OSS Personal Cloud Security Model

### Token Lifecycle
[PRIOR: ASS-041; CODE: main.rs startup pattern confirmed]

1. **Generation**: First run (token file absent at `{data_volume}/token`): generate 32 bytes via `rand::rngs::OsRng`, hex-encode as 64 lowercase chars, write with mode 0600.
2. **First-run print**: Print once to stdout: `[UNIMATRIX TOKEN] <hex>`. Only appearance in output.
3. **Subsequent runs**: Read file silently into `Arc<String>`. No print.
4. **In-memory**: `Arc<String>` in `StaticTokenAuth` middleware struct.
5. **Validation**: `subtle::ConstantTimeEq` comparison on every bearer token header.
6. **Rotation**: Stop → delete `{data_volume}/token` → restart.

### Where Token Validation Happens

`StaticTokenAuth` tower middleware wraps `StreamableHttpService<UnimatrixServer>`. Intercepts every HTTP request before it reaches the rmcp service layer. On success: writes a full-capability `ResolvedIdentity` to request extensions. On failure: returns HTTP 401 immediately. **Purely additive** — no changes to tool dispatch. [PRIOR: ASS-041]

### `agent_id` as Optional Attribution Metadata

In the OSS personal cloud tier, `agent_id` from tool params is **observation metadata only, not a security mechanism**. Security decision is made by middleware based on bearer token alone.

Attribution source hierarchy (OSS tier):
1. **Primary**: `clientInfo.name` from MCP `initialize` handshake — non-spoofable per-session source. [PRIOR: ASS-049 confirms availability]
2. **Secondary**: `agent_id` tool parameter — agent persona (architect, researcher, etc.) — useful for analytics.
3. **Fallback**: `"anonymous"`.

**Can existing tool implementations treat `agent_id` as optional metadata without breaking?**
Yes. In OSS tier with bearer auth, any bearer-authenticated caller auto-resolves to full capabilities. `require_cap()` passes unconditionally. The only behavioral change is that audit log `agent_id` field should prefer `clientInfo.name`. The `agent_id` parameter in all 12 tool structs does NOT need to be removed — it remains an optional persona hint. [CODE + PRIOR]

### `AgentRegistry` and `context_enroll` Disposition in OSS Tier

**Hypothesis assessment**: Partially confirmed — the registry becomes a no-op for security but retains value for attribution.

- `AgentRegistry` retained for persona persistence and `last_seen_at` analytics.
- Auto-enrollment with permissive defaults is correct: any `clientInfo.name`-derived agent_id gets full caps on first appearance.
- `context_enroll` no longer required for security. Available but not a user-facing onboarding step.
- **Zero-enrollment-friction confirmed**: Bearer token auth means any client with the token gets full access immediately, no `context_enroll` call needed. [INFERENCE: high confidence]
- The `"human"` agent_id hack for Admin-gated tools is eliminated: any bearer-authenticated client has Admin capability via the full-capability `ResolvedIdentity` returned by `StaticTokenAuth`.

### Audit Log at OSS Tier

The current `AuditLog` struct **cannot record token fingerprint, credential_type, or agent_attribution** as distinct from `agent_id`. Schema changes required (see Output 4).

At OSS tier, each audit record must include:
- `token_fingerprint`: SHA-256 of bearer token (new field, in `metadata` JSON per Output 4 design)
- `credential_type`: `"static_token"` (new field)
- `agent_attribution`: `clientInfo.name` when available (new field)
- `capability_used`: capability gate evaluated (new field)
- Existing: `operation`, `timestamp`, `session_id`, `outcome`, `detail`

---

## Output 3: Enterprise Extension Surface

### `BearerValidator` Trait

**In crate**: `unimatrix-server`, module: `infra::auth`

```rust
pub trait BearerValidator: Send + Sync {
    /// Validate a bearer token and return resolved identity on success.
    /// Called on every HTTP request before tool dispatch.
    async fn validate(&self, token: &str) -> Result<ResolvedIdentity, AuthError>;
}

pub enum AuthError {
    MissingToken,      // no Authorization header
    InvalidToken,      // bad signature, wrong format
    TokenExpired,      // valid signature but exp claim failed
    InsufficientScope, // valid token but required capability not in scopes
    Internal(String),  // JWKS fetch failure, etc.
}
```

**OSS default impl — `StaticTokenAuth`:**
```rust
pub struct StaticTokenAuth {
    token: Arc<String>,        // plaintext for constant-time compare
    token_fingerprint: String, // SHA-256 hex for audit records
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

**Enterprise impl contract — `JwtBearerAuth`** (lives in private repo `unimatrix-compliance`):
- Decode bearer token as JWT. Validate `exp`, `iss`, `aud` (configured Unimatrix audience), signature (RS256/ES256 via JWKS cache). [PRIOR: ASS-041]
- Extract `sub` claim as agent identifier.
- Perform `AgentRegistry` lookup: `sub` → `ResolvedIdentity` with role-scoped capabilities.
- Return distinct errors: `AuthError::TokenExpired` for expired tokens, `AuthError::InvalidToken` for signature failures.
- JWKS cache: background refresh tick + per-validation-failure cache miss fallback.
- Library: `jsonwebtoken` for JWT decode/verify. [PRIOR: ASS-041]

**Is `ResolvedIdentity` sufficient for both cases?** Yes — confirmed structurally. The population path changes (middleware vs. tool params); the struct itself is unchanged.

### Capability Gating

**OSS tier**: `require_cap()` calls in tool handlers remain unchanged. In OSS mode, `ResolvedIdentity.capabilities` always contains all caps, so all checks pass. They are no-ops for access control but serve as documentation of which capability each tool requires.

**Enterprise tier**: `JwtBearerAuth` returns a `ResolvedIdentity` with role-scoped capabilities:
- Admin role: `[Read, Write, Search, Admin]`
- Operator role: `[Read, Write, Search]`
- Auditor role: `[Read, Search]`

The tool handlers are **unchanged** — only the `ResolvedIdentity` population path changes. This is the critical design property: enterprise capability enforcement is injected through the identity layer, not through RBAC logic in tool handlers. [CODE + PRIOR]

### Startup Plugin Registration

**No `ServerBuilder` needed for Wave 2.** The `BearerValidator` is injected into the tower middleware layer at startup, not into `UnimatrixServer` directly. Pattern:

```rust
// OSS startup (main.rs):
let validator: Arc<dyn BearerValidator> = Arc::new(
    StaticTokenAuth::load_or_create(&paths.token_path)?
);
let auth_layer = BearerAuthLayer::new(Arc::clone(&validator));
// auth_layer wraps StreamableHttpService<UnimatrixServer> in the HTTPS bind
```

Enterprise binary replaces `StaticTokenAuth::load_or_create` with `JwtBearerAuth::new(config)`. No other code changes. [CODE: confirmed by examining `main.rs` post-construction field pattern; INFERENCE: high confidence]

### `AuditLogWriter` / `EnterpriseAuditWriter` Trait

An enterprise compliance audit log (SIEM-exportable, retention-policy enforced) differs from the current `AuditLog`. The recommended approach for Wave 2:

- Keep `Arc<AuditLog>` in `UnimatrixServer` (SQLite-backed, existing behavior).
- Add `enterprise_audit: Option<Arc<dyn EnterpriseAuditWriter>>` as an optional field on `UnimatrixServer` (initially `None`; enterprise startup populates it).
- Tool dispatch (or a post-dispatch hook) calls both when the enterprise writer is set.

**Trait signature** (to be placed in `unimatrix-server::infra::audit`):
```rust
pub trait EnterpriseAuditWriter: Send + Sync {
    fn write_compliance_event(&self, event: &ComplianceAuditEvent);
}

pub struct ComplianceAuditEvent {
    pub event_id: u64,
    pub timestamp: u64,
    pub session_id: String,
    pub credential_type: String,   // "jwt"
    pub agent_attribution: String, // JWT sub claim
    pub token_fingerprint: String, // SHA-256 of JWT
    pub operation: String,
    pub capability_used: String,
    pub target_ids: Vec<u64>,
    pub outcome: String,
    pub detail: String,
    pub metadata: serde_json::Value, // ISO 42001 extensible field
}
```

This is **(a) additive** — the OSS path is unchanged; enterprise adds on top.

---

## Output 4: Audit Log Schema Recommendation

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

    -- New fields
    credential_type   TEXT    NOT NULL DEFAULT 'none',
    -- Values: 'none' (STDIO), 'static_token' (OSS HTTPS), 'jwt' (enterprise)

    capability_used   TEXT    NOT NULL DEFAULT '',
    -- Capability gate evaluated: 'read', 'write', 'search', 'admin', 'session_write'
    -- Empty only when no capability check ran

    agent_attribution TEXT    NOT NULL DEFAULT '',
    -- clientInfo.name (OSS HTTPS), JWT sub claim (enterprise), or agent_id tool param
    -- Non-spoofable: populated from connection/auth layer, not tool param

    metadata          TEXT    NOT NULL DEFAULT '{}'
    -- JSON object for AI system attributes
    -- Minimum shape when known: {"model": str, "agent_role": str, "context_version": int}
    -- ISO 42001 extensibility: new attributes added to JSON without schema migration
    -- token_fingerprint stored here: {"token_fingerprint": "<sha256 hex>"}
);

-- Additional indexes
CREATE INDEX IF NOT EXISTS idx_audit_log_session ON audit_log(session_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_cred    ON audit_log(credential_type);

-- Append-only enforcement triggers (apply in same migration)
CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: UPDATE not permitted'); END;

CREATE TRIGGER audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: DELETE not permitted'); END;
```

### Field-by-Field Rationale

| Field | Required by | Rationale |
|-------|------------|-----------|
| `credential_type` | SOC 2 CC6.1, ISO 27001 5.15 | Distinguishes auth tier. Enables "show all JWT-authenticated actions" queries. Weak now (always 'static_token'), strong when enterprise adds JWT. |
| `capability_used` | SOC 2 CC6.3 | Duty segregation audit: "prove write operations were performed only by authorized roles." Current schema has no record of which capability gate was evaluated. |
| `agent_attribution` | ISO 42001, SOC 2 CC7.1 | Non-spoofable attribution anchor. `agent_id` (tool param) is self-declared and spoofable; `agent_attribution` comes from MCP handshake or JWT sub — not changeable per call. |
| `metadata` | ISO 42001 AI governance | AI system attributes for the "AI agent X with model Y called tool T" audit trail. JSON avoids migration cost as AI attribute tracking matures. Includes `token_fingerprint`. |

### Migration Classification

| Change | Category | Migration Path |
|--------|----------|---------------|
| Add `credential_type TEXT DEFAULT 'none'` | **(c) Breaking** | `ALTER TABLE audit_log ADD COLUMN credential_type TEXT NOT NULL DEFAULT 'none'` |
| Add `capability_used TEXT DEFAULT ''` | **(c) Breaking** | `ALTER TABLE audit_log ADD COLUMN capability_used TEXT NOT NULL DEFAULT ''` |
| Add `agent_attribution TEXT DEFAULT ''` | **(c) Breaking** | `ALTER TABLE audit_log ADD COLUMN agent_attribution TEXT NOT NULL DEFAULT ''` |
| Add `metadata TEXT DEFAULT '{}'` | **(c) Breaking** | `ALTER TABLE audit_log ADD COLUMN metadata TEXT NOT NULL DEFAULT '{}'` |
| Add `session_id` index | **(a) Additive** | `CREATE INDEX` only |
| Add `credential_type` index | **(a) Additive** | `CREATE INDEX` only |
| Add UPDATE/DELETE triggers | **(a) Additive** | `CREATE TRIGGER` — no data change |

**Recommended execution**: One schema version bump. All 4 `ALTER TABLE` statements in one migration. Existing rows get valid defaults. `AuditEvent` Rust struct gains 4 fields with `#[serde(default)]`. `log_audit_event()` SQL INSERT updated to include new columns. All `AuditEvent` literal call sites updated.

---

## Output 5: Seam Map

Five critical seams where identity resolution must remain injectable. Ordered by blast radius (hardest to retrofit first).

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

**Cost now**: Low — new function alongside existing, existing call sites unchanged.
**Cost of retrofitting later**: Medium — would require touching all `build_context()` call sites (12 tool handlers).

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

**Pattern that would foreclose this seam**: Making `build_context()` a sealed trait method or making tool handlers private such that wrapping is impossible.

---

### Seam 3 — `AuditEvent::agent_id` Field — Attribution Record

**Current state**: `AuditEvent.agent_id` is populated from `ctx.agent_id` which comes from the tool parameter. No separate `agent_attribution` field. [CODE: `schema.rs:AuditEvent`]

**Injectable now?** Resolved by the schema change in Output 4 (add `agent_attribution` field). Once distinct fields exist, downstream analytics choose which to use for non-spoofable attribution.

**Cost of not doing it now**: Every audit record permanently loses the distinction between "who the connection said it was" and "what agent persona the call declared." Cannot be reconstructed from existing records.

---

### Seam 4 — `AgentRegistry::resolve_or_enroll()` — Identity Grounding

**Current state**: All capability resolution goes through `resolve_or_enroll(agent_id: &str)`. [CODE: `registry.rs:67-78`]

**Injectable now?** Yes — no changes to `AgentRegistry` needed. If `build_context()` is extended to accept an external `ResolvedIdentity` (Seam 2), the `resolve_or_enroll` call is bypassed entirely for that code path. The bypass condition is in `resolve_agent()` in `server.rs`.

**Pattern that would foreclose this seam**: Forcing all identity resolution through `resolve_or_enroll` without a bypass path. The current abstraction has `resolve_agent()` as the call site — this is the correct bypass location.

---

### Seam 5 — `cycle_events` / `audit_log` Session Linkage — Behavioral Provenance Chain

**Current state**: `cycle_events.cycle_id` and `audit_log.session_id` share the same value (prefixed with `"mcp::"` on MCP path) when clients supply a `session_id` tool parameter. [CODE: `server.rs:384-398`; `db.rs:627-645`, `db.rs:792-810`]

**Is the linkage queryable today?**
```sql
SELECT ae.*, ce.goal, ce.goal_embedding
FROM audit_log ae
LEFT JOIN cycle_events ce
    ON ce.cycle_id = ae.session_id
   AND ce.event_type = 'cycle_start'
WHERE ae.session_id = 'mcp::some-session-id'
```
The index `idx_cycle_events_cycle_id` makes this O(log N). **But**: the join works only when the client passes a consistent `session_id`. If the client omits `session_id` (many do), `audit_log.session_id = ""` and the join produces no results. This is a design gap, not a code bug. [INFERENCE: high confidence]

**Pattern that would foreclose this seam**: Renaming `cycle_events.cycle_id` or `audit_log.session_id`, or compacting the `cycle_events` table in a way that drops `goal_embedding` rows. Do not allow these in any schema migration.

**Session-pinned identity would fix this**: A non-spoofable session ID at the connection level would populate `audit_log.session_id` automatically on every call. This seam must remain injectable — the `session_id` in `AuditEvent` must be settable from connection context, not only from tool params.

---

## Output 6: Don't-Foreclose List

Seven behavioral provenance invariants.

### Invariant 1 — `audit_log.detail` Must Never Be Truncated or Compressed

**Data**: `detail` field contains the human-readable record of what was stored/corrected. For write operations, this includes content summaries or actual written content.

**Why**: Future goal-action alignment analysis requires actual action payloads. If `detail` is compressed or truncated in any schema optimization, the behavioral provenance record is incomplete and future alignment analysis is impossible.

**Invariant**: `audit_log.detail` must never be truncated, compressed, or replaced with a summary. If storage pressure demands it, add a separate `summary` column and keep `detail` full.

**Current state**: No truncation today. [CODE: `audit.rs` — `log_audit_event` writes `detail` as provided]

---

### Invariant 2 — `cycle_events.goal_embedding` Must Remain Indexed and Joinable to `audit_log`

**Data**: `cycle_events.goal_embedding` (BLOB, bincode-encoded `Vec<f32>`, populated for `cycle_start` events with non-empty `goal`). [CODE: `db.rs:637`]

**Current indexing**: `idx_cycle_events_cycle_id ON cycle_events (cycle_id)` — enables O(log N) lookup. [CODE: `db.rs:643`]

**Invariant**: `cycle_events.cycle_id` must never be renamed or repurposed. `audit_log.session_id` must remain joinable to `cycle_events.cycle_id`. Any migration that renames either field breaks the behavioral provenance chain.

---

### Invariant 3 — `observations.phase` Must Remain Indexed and Not Dropped

**Data**: `observations.phase` captures the workflow phase at the time each tool call was observed via UDS hooks. [CODE: `db.rs:824`]

**Current indexing**: `idx_observations_topic_phase ON observations (topic_signal, phase)`. [CODE: `db.rs:839-840`]

**Invariant**: `observations.phase` must not be dropped. It is the phase signal for behavioral context analysis — "which development phase was the agent in when this action was taken." Dropping it destroys this signal irretrievably for historical records.

---

### Invariant 4 — Future `task_log` Anchor Requirements

Tasks are currently invisible to Unimatrix (no `task_log` table exists). [CODE: `db.rs` — verified no task_log table]

When future work adds task tracking, `task_log` must:
1. Have a `session_id` column that joins to `audit_log.session_id` and `cycle_events.cycle_id`
2. Have a `timestamp` column for temporal ordering with `audit_log` entries
3. Use the `prefix_session_id("mcp", sid)` convention for MCP-path sessions

---

### Invariant 5 — Audit Log Is Append-Only: Never UPDATE, Never DELETE

**Current enforcement**: Application-level convention only. No DDL enforcement. [CODE: verified — no triggers in current DDL]

**Invariant**: No UPDATE or DELETE must ever be issued against `audit_log`. The trigger enforcement from Output 4 makes this DDL-enforced from the migration date forward. Until installed, it is a documented code invariant enforced by code review.

**Compliance note**: SOC 2 CC7.1 requires tamper-evident audit logs. An `audit_log` row that can be modified after the fact is not compliant evidence. ISO 27001 Annex A 8.15 requires logs to be "protected."

---

### Invariant 6 — `audit_log.session_id` Must Never Default to Empty String at Call Sites

**Current state**: Multiple `AuditEvent` literals in `tools.rs` construct with `session_id: String::new()`. [CODE: `tools.rs:1383-1385`, `tools.rs:1416-1418`] This breaks the join to `cycle_events`.

**Invariant**: All `AuditEvent` construction must populate `session_id` from `ToolContext.audit_ctx.session_id` when a session context exists. `String::new()` is only acceptable when no session is active. Code review gate: any PR adding an `AuditEvent` literal must document why `session_id` is empty if it is.

---

### Invariant 7 — `agent_id` Tool Parameter Must Never Be the Sole Capability Gate

**Current state**: `agent_id` tool param drives both security (capability check) and attribution (audit log). The SCOPE.md correctly identifies this as a design error.

**Invariant**: After W2-2/W2-3 lands, capability gating must be driven by `ResolvedIdentity` from the validated auth path (bearer token / JWT), never from the self-declared `agent_id` tool param. The tool param `agent_id` is permanently classified as attribution metadata. This invariant applies to the HTTPS transport path; STDIO remains unchanged (no bearer token).

---

## Unanswered Questions

**1. `clientInfo.name` capture in HTTPS transport path**

ASS-049 confirmed `clientInfo.name` is available at MCP `initialize` handshake. Whether rmcp 0.16's `StreamableHttpService` exposes `clientInfo.name` to tool handler dispatch is not confirmed. Must be verified against rmcp 0.16 `StreamableHttpService` session management internals before implementing `agent_attribution` population from this source. [REQUIRES: rmcp source read — out of scope for this spike]

**2. `observations` rows under `crt-036` retention policy**

SCOPE.md asserts `observations.phase` is retained. The index is confirmed [CODE: `db.rs:839-840`]. However, `crt-036` retention policy (referenced in `main.rs` as `retention_config`) may delete old `observations` rows. Whether the deletion window removes data needed for behavioral provenance analysis is not confirmed by this spike. [REQUIRES: read of `crt-036` retention implementation]

**3. Append-only triggers — pre-migration row vulnerability window**

Trigger enforcement applies from migration time forward. Existing pre-Wave-2 rows remain mutable by a database admin. For SOC 2 Type I readiness this is acceptable (point-in-time controls). For Type II, the trigger must be installed before the audit period begins.

---

## Out-of-Scope Discoveries

1. **`write_count_since()` accuracy degrades under attribution model shift**: When `audit_log.agent_id` is populated from `clientInfo.name` rather than per-call tool params, all calls from "Claude Code" appear as one agent. In multi-tenant enterprise scenarios this would be a rate-limiting problem. Flag for ASS-042 enterprise rate-limiting design.

2. **`observations.session_id` join to `cycle_events.cycle_id` has same fragility as `audit_log` join**: Populated from UDS hook events; join has the same "omitted session_id = empty string" fragility. Both gaps resolved when session-pinned identity arrives.

3. **`SessionWrite` capability exists but no tool uses it**: `schema.rs:275` defines `Capability::SessionWrite = 4`; no `require_cap(..., Capability::SessionWrite)` call exists anywhere in `tools.rs`. Reserved for future use. Must be documented as reserved, not removed.

4. **`agent_registry` ABAC columns are unused but present**: `AgentRecord.allowed_topics` and `allowed_categories` (defined in `schema.rs:303-305`) are `None` for all bootstrapped agents and checked by no current capability logic. These are a partially-implemented ABAC foundation. They should not be dropped — they are the seam for future attribute-based access control (ASS-048 identified ABAC as post-Wave-2 requirement).

---

## Recommendations Summary

| Topic | Recommendation |
|-------|---------------|
| Implementation audit | `BearerValidator` trait + `StaticTokenAuth`: additive. `build_context()` extension: non-breaking. `AuditEvent` struct + `audit_log` DDL: one breaking migration before Wave 2 ships — do not defer. |
| OSS personal cloud | Bearer token = authorization. Any client with valid token has full access, no enrollment required. `agent_id` tool param is persona metadata. `AgentRegistry` retained for attribution. `context_enroll` and `context_quarantine` Admin requirement satisfied by any bearer-authenticated client. `"human"` agent_id hack eliminated. |
| Enterprise extension surface | `BearerValidator` trait confirmed viable — OSS ships `StaticTokenAuth`, enterprise ships `JwtBearerAuth` in private repo. `require_cap()` calls in tool handlers unchanged — they become enterprise RBAC enforcement points. No `ServerBuilder` needed for Wave 2. `EnterpriseAuditWriter` trait: additive optional field on `UnimatrixServer`. |
| Audit log schema | Add 4 fields: `credential_type`, `capability_used`, `agent_attribution`, `metadata`. One migration. Add append-only triggers. Add 2 new indexes. Do not patch incrementally. |
| Seam map | Seam 1 (`extract_agent_id`) + Seam 2 (`build_context`): make injectable now, low cost, high future value. Seam 3 (`agent_attribution` field): resolved by schema migration. Seam 4 (`AgentRegistry` bypass): resolved by Seam 2 extension. Seam 5 (`cycle_events` / `audit_log` linkage): fragile today, do not close this seam. |
| Don't-foreclose | Never truncate `audit_log.detail`. Never rename `cycle_events.cycle_id` or `audit_log.session_id`. Never remove `observations.phase`. Future `task_log` needs `session_id` + `timestamp`. `agent_id` tool param must never be sole capability gate after W2-3 lands. |
