# VNC-014 Implementation Brief
## ASS-050 audit_log 4-Column Migration + Server-Side MCP Client Attribution via clientInfo.name

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-014/SCOPE.md |
| Architecture | product/features/vnc-014/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-014/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-014/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-014/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| UnimatrixServer (server.rs) | pseudocode/server.md | test-plan/server.md |
| ToolContext (mcp/context.rs) | pseudocode/tool-context.md | test-plan/tool-context.md |
| AuditEvent + audit.rs (unimatrix-store) | pseudocode/audit-event.md | test-plan/audit-event.md |
| Schema migration (migration.rs + db.rs) | pseudocode/migration.md | test-plan/migration.md |
| Tool handlers migration (mcp/tools.rs) | pseudocode/tools.md | test-plan/tools.md |
| Capability::as_audit_str (infra/registry.rs) | pseudocode/capability.md | test-plan/capability.md |
| Append-only remediation (retention.rs + import/mod.rs) | pseudocode/remediation.md | test-plan/remediation.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

VNC-014 closes the audit attribution gap for non-Claude-Code MCP clients (Codex CLI, Gemini CLI)
by capturing `clientInfo.name` from the MCP `initialize` handshake at the server, keying it on
the rmcp transport session ID, and propagating it through `ToolContext` to every `audit_log` write
in that session. Simultaneously it delivers the full ASS-050 four-column schema migration
(`credential_type`, `capability_used`, `agent_attribution`, `metadata`) with append-only DDL
triggers (schema v24 → v25), and ships the `build_context_with_external_identity()` Seam 2 overload
required by the W2-3 bearer-auth feature.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| `client_type_map` storage mechanism | `Arc<Mutex<HashMap<String, String>>>` on `UnimatrixServer`; HTTP key = `Mcp-Session-Id` UUID, stdio key = `""`. `DashMap` deferred to W2-2. | ADR-001 (#4355) | architecture/ADR-001-client-type-map-storage.md |
| `ServerHandler::initialize` override | Override in `server.rs`; extracts `request.client_info.name` directly (not via context); returns `Ok(self.get_info())`. | ADR-002 (#4356) | architecture/ADR-002-initialize-override.md |
| `build_context()` supersession and Seam 2 overload | `build_context_with_external_identity()` added with full W2-3 Seam 2 signature; old `build_context()` removed (compile-time enforcement of migration completeness). | ADR-003 (#4357) | architecture/ADR-003-build-context-seam2-overload.md |
| Four-column migration idempotency | All four `pragma_table_info` checks run before any ALTER; triggers and indexes use `IF NOT EXISTS` (inherently idempotent); single v24→v25 bump. | ADR-004 (#4358) | architecture/ADR-004-four-column-migration-idempotency.md |
| Append-only trigger remediation | `gc_audit_log()` removed from `retention.rs` and background tick; `DELETE FROM audit_log` removed from `drop_all_data()` in `import/mod.rs`. No test infrastructure changes needed. | ADR-005 (#4359) | architecture/ADR-005-append-only-trigger-remediation.md |
| `capability_used` string derivation | `Capability::as_audit_str()` exhaustive match on the existing enum; no free-form strings at call sites. | ADR-006 (#4360) | architecture/ADR-006-capability-used-string-constants.md |
| Two-field attribution model | `agent_id` = spoofable routing identity; `agent_attribution` = transport-attested compliance field. Both coexist permanently. W2-3 upgrades `agent_attribution` source to JWT sub without schema change. | ADR-007 (#4361) | architecture/ADR-007-two-field-attribution-model.md |
| `ResolvedIdentity` crate placement | Stays in `unimatrix-server/mcp/identity.rs` (already exists there). Not promoted to `unimatrix-core`. W2-3 uses `pub use` re-export if cross-crate access materialises. | ADR-008 (#4362) | architecture/ADR-008-resolved-identity-placement.md |

---

## Files to Create / Modify

### unimatrix-store

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/schema.rs` | Modify | Add four new fields to `AuditEvent` with `#[serde(default)]`; add `impl Default for AuditEvent` |
| `crates/unimatrix-store/src/audit.rs` | Modify | Update `log_audit_event` INSERT (`?9`–`?12`) and `read_audit_event` SELECT for four new columns |
| `crates/unimatrix-store/src/migration.rs` | Modify | Add v24→v25 migration block with pre-flight pragma checks, four ALTERs, two indexes, two triggers; bump `CURRENT_SCHEMA_VERSION` to 25 |
| `crates/unimatrix-store/src/db.rs` | Modify | Update `create_tables_if_needed()` `audit_log` DDL to include four new columns and append-only triggers |
| `crates/unimatrix-store/src/retention.rs` | Modify | Remove `gc_audit_log()` function and add explanatory comment about append-only model |

### unimatrix-server

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/server.rs` | Modify | Add `client_type_map` field; add `ServerHandler::initialize` override; add `build_context_with_external_identity()`; remove `build_context()` |
| `crates/unimatrix-server/src/mcp/context.rs` | Modify | Add `client_type: Option<String>` field to `ToolContext` |
| `crates/unimatrix-server/src/mcp/tools.rs` | Modify | Migrate all 12 tool handlers from `build_context()` to `build_context_with_external_identity()`; populate four new `AuditEvent` fields at all construction sites |
| `crates/unimatrix-server/src/infra/registry.rs` | Modify | Add `Capability::as_audit_str()` method |
| `crates/unimatrix-server/src/import/mod.rs` | Modify | Remove `DELETE FROM audit_log;` from `drop_all_data()`; add explanatory comment |
| `crates/unimatrix-server/src/background.rs` | Modify | Update all three `AuditEvent` construction sites (lines ~1197, ~1252, ~2267) to supply four new fields using `..AuditEvent::default()` |
| `crates/unimatrix-server/src/uds/listener.rs` | Modify | Update any direct `AuditEvent` construction sites to supply four new fields |

---

## Data Structures

### `AuditEvent` (post-migration, 12 fields)

```rust
pub struct AuditEvent {
    // existing 8 fields unchanged:
    pub event_id:          u64,
    pub timestamp:         u64,
    pub session_id:        String,   // agent-declared, sentinel: ""
    pub agent_id:          String,   // agent-declared, spoofable, for routing
    pub operation:         String,
    pub target_ids:        Vec<u64>,
    pub outcome:           Outcome,
    pub detail:            String,

    // ASS-050 / vnc-014 additions:
    #[serde(default)]
    pub credential_type:   String,   // sentinel: "none" (code); SQL DEFAULT 'none'
    #[serde(default)]
    pub capability_used:   String,   // sentinel: "" (no gate)
    #[serde(default)]
    pub agent_attribution: String,   // transport-attested; non-spoofable; sentinel: ""
    #[serde(default)]
    pub metadata:          String,   // JSON object; sentinel: "{}" (code); SQL DEFAULT '{}'
}

impl Default for AuditEvent {
    fn default() -> Self {
        AuditEvent {
            // ... existing defaults ...
            credential_type:   "none".to_string(),
            capability_used:   String::new(),
            agent_attribution: String::new(),
            metadata:          "{}".to_string(),
        }
    }
}
```

**Two-field attribution model (ADR-007):**

| Field | Source | Spoofable | Purpose |
|-------|--------|-----------|---------|
| `agent_id` | Tool parameter (agent-declared) | Yes | Routing, session keying, per-agent behavior |
| `agent_attribution` | `clientInfo.name` via MCP `initialize` | No | Compliance, non-repudiation, ISO 42001 evidence |

### `ToolContext` (new field)

```rust
pub struct ToolContext {
    // existing fields unchanged ...
    pub client_type: Option<String>,  // clientInfo.name from client_type_map, or None
}
```

### `client_type_map` on `UnimatrixServer`

```rust
pub struct UnimatrixServer {
    // existing fields ...
    pub client_type_map: Arc<Mutex<HashMap<String, String>>>,
    // Key: Mcp-Session-Id UUID (HTTP) or "" (stdio)
    // Value: clientInfo.name, truncated to 256 chars
}
```

### `credential_type` canonical values

| Value | Transport | Set by |
|-------|-----------|--------|
| `"none"` | Stdio, all vnc-014 connections | vnc-014 (all rows) |
| `"static_token"` | HTTP bearer token | W2-2 |
| `"jwt"` | Enterprise JWT | W2-3 |

### `capability_used` canonical values

| `Capability` variant | `as_audit_str()` | Tools |
|----------------------|-----------------|-------|
| `Capability::Search` | `"search"` | `context_search`, `context_lookup`, `context_briefing` |
| `Capability::Read` | `"read"` | `context_get`, `context_status`, `context_retrospective` |
| `Capability::Write` | `"write"` | `context_store`, `context_correct`, `context_deprecate`, `context_quarantine`, `context_cycle` |
| `Capability::Admin` | `"admin"` | `context_enroll` |

---

## Function Signatures

### `ServerHandler::initialize` override

```rust
fn initialize(
    &self,
    request: InitializeRequestParams,
    context: RequestContext<RoleServer>,
) -> impl Future<Output = Result<InitializeResult, McpError>> + Send + '_
```

Behavior: extract `request.client_info.name`; if non-empty, truncate to 256 chars (WARN if truncated), extract `Mcp-Session-Id` header from `context.extensions.get::<http::request::Parts>()` (fallback `""` for stdio), insert into `self.client_type_map`; return `std::future::ready(Ok(self.get_info()))`.

### `build_context_with_external_identity`

```rust
pub(crate) async fn build_context_with_external_identity(
    &self,
    params_agent_id: &Option<String>,
    format: &Option<String>,
    session_id: &Option<String>,
    request_context: &RequestContext<RoleServer>,
    external_identity: Option<&ResolvedIdentity>,  // always None in vnc-014; W2-3 wires this
) -> Result<ToolContext, rmcp::ErrorData>
```

Behavior: extract rmcp session key from `request_context.extensions`; look up `client_type` in `self.client_type_map`; when `external_identity` is `Some`, bypass `resolve_agent()` (W2-3 path); when `None`, call `resolve_agent()` as before; return `ToolContext` with `client_type` populated.

### `Capability::as_audit_str`

```rust
impl Capability {
    pub fn as_audit_str(&self) -> &'static str {
        match self {
            Capability::Read   => "read",
            Capability::Write  => "write",
            Capability::Search => "search",
            Capability::Admin  => "admin",
        }
    }
}
```

Match is exhaustive — no wildcard arm. Future variant additions will produce a compile error.

### rmcp session ID extraction (shared access path)

```rust
request_context.extensions
    .get::<http::request::Parts>()
    .and_then(|p| p.headers.get("mcp-session-id"))
    .and_then(|v| v.to_str().ok())
    .unwrap_or("")   // "" for stdio or header-absent cases
```

> **SESSION ID NAMESPACE WARNING (Unimatrix #4363):** `AuditEvent.session_id` must be populated
> from `ctx.audit_ctx.session_id` (the agent-declared `mcp::`-prefixed parameter) — exactly as
> today. Do NOT use the `Mcp-Session-Id` UUID that flows through
> `build_context_with_external_identity()`. The rmcp UUID is the `client_type_map` lookup key only;
> it is an internal routing key that never surfaces in audit records. These are two distinct
> namespaces. A wrong assignment compiles and passes tests silently.

### `AuditEvent` field population at tool-call sites

```rust
AuditEvent {
    // existing fields ...
    credential_type:   "none".to_string(),
    capability_used:   Capability::X.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata: match ctx.client_type.as_deref().filter(|s| !s.is_empty()) {
        Some(ct) => serde_json::json!({"client_type": ct}).to_string(),
        None     => "{}".to_string(),
    },
}
```

`serde_json::json!` is mandatory for `metadata` construction (FR-10). Format-string concatenation
is explicitly prohibited regardless of escaping applied.

### `AuditEvent` field population at non-tool-call sites (background, UDS listener)

```rust
AuditEvent {
    // existing fields ...
    ..AuditEvent::default()
    // yields: credential_type="none", capability_used="", agent_attribution="", metadata="{}"
}
```

---

## Migration SQL (v24 → v25)

```sql
-- Pre-flight pragma checks (run all four before any ALTER)
-- ALTER TABLE only if column does not yet exist (pragma_table_info guard)
ALTER TABLE audit_log ADD COLUMN credential_type   TEXT NOT NULL DEFAULT 'none';
ALTER TABLE audit_log ADD COLUMN capability_used   TEXT NOT NULL DEFAULT '';
ALTER TABLE audit_log ADD COLUMN agent_attribution TEXT NOT NULL DEFAULT '';
ALTER TABLE audit_log ADD COLUMN metadata          TEXT NOT NULL DEFAULT '{}';

-- Idempotent index and trigger creation
CREATE INDEX IF NOT EXISTS idx_audit_log_session ON audit_log(session_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_cred    ON audit_log(credential_type);
CREATE TRIGGER IF NOT EXISTS audit_log_no_update BEFORE UPDATE ON audit_log
    BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: UPDATE not permitted'); END;
CREATE TRIGGER IF NOT EXISTS audit_log_no_delete BEFORE DELETE ON audit_log
    BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: DELETE not permitted'); END;
```

`CURRENT_SCHEMA_VERSION` bumps from 24 to 25. `db.rs` `create_tables_if_needed()` DDL updated
byte-identical to the migration (R-11 mitigation).

---

## Constraints

- **rmcp 0.16.0 pinned**: `ServerHandler::initialize` signature must match exactly. No version bump.
- **Single schema version bump**: All four ALTERs in one v24→v25 step. No splitting across bumps.
- **`metadata` never empty**: Minimum value is `"{}"`. `NOT NULL DEFAULT '{}'` at DDL level.
- **`serde_json::json!` mandatory**: Format-string construction of `metadata` JSON is prohibited (SEC-02).
- **`build_context()` removed**: Compile-time enforcement via removal (not deprecation). No thin wrapper.
- **`agent_attribution` is connection-layer only**: Never populated from tool parameters.
- **`pragma_table_info` pre-flight order**: All four column checks execute before any ALTER.
- **`Capability::as_audit_str()` exhaustive match**: No wildcard arm; compile error on new variants.
- **`client_type_map` poison recovery**: All `Mutex::lock()` calls use `unwrap_or_else(|e| e.into_inner())`.
- **`clientInfo.name` truncation**: 256 Unicode scalar values (not bytes); WARN logged if truncated.
- **No tool schema changes**: No `#[tool]` attribute struct gains a new field.
- **Schema cascade checklist**: `CURRENT_SCHEMA_VERSION = 25`, `sqlite_parity.rs` column count = 12, migration test file renamed, cascade test files updated.
- **`ResolvedIdentity` location**: `unimatrix-server/mcp/identity.rs` (already exists there; not moved).

---

## Dependencies

| Crate / Component | Role | Change |
|-------------------|------|--------|
| `unimatrix-store` | `AuditEvent` struct, `audit_log` DDL, migration, `gc_audit_log` removal | Modified |
| `unimatrix-server` | `UnimatrixServer`, `tools.rs`, `server.rs`, `import/mod.rs` | Modified |
| `rmcp 0.16.0` | `ServerHandler::initialize`, `RequestContext`, `InitializeRequestParams` | Read-only (no version change) |
| `http` (via rmcp/tower) | `http::request::Parts` for header extraction | Existing transitive dep |
| `serde_json` | `json!` macro for metadata construction | Existing dep (mandatory use for FR-10) |
| `std::collections::HashMap` | `client_type_map` backing store | std |
| `std::sync::{Arc, Mutex}` | Thread-safe shared state | std |

No new external dependencies. All crates are already in the workspace.

---

## NOT in Scope

- `cycle_events` gap for Codex CLI — `cycle_events` populated only via the hook path, not addressed here.
- vnc-013 hook normalization (Gemini canonical event names) — separate parallel work stream.
- OAuth JWT identity (`credential_type = "jwt"`) — enterprise tier, W2-3 deliverable.
- `client_type` as a tool parameter — attribution is server-side and transparent to agents.
- Retention, querying, or reporting on `client_type` — field is written; analytics deferred.
- `DashMap` or high-throughput concurrency for `client_type_map` — deferred to W2-2 (SR-01 accepted).
- `context_cycle_review` or `context_status` behavioral changes.
- Stateless HTTP mode (`Mcp-Session-Id` absent on all requests) — documented limitation, not supported.

---

## Alignment Status

Both WARNs from the ALIGNMENT-REPORT.md are resolved:

**WARN-1 (Seam 2 Scope Addition — OQ-A)**: Resolved by ADR-008. `ResolvedIdentity` already exists in `unimatrix-server/mcp/identity.rs` and stays there. No crate boundary change. No open question remains.

**WARN-2 (SEC-02 JSON Injection — FR-10 gap)**: Resolved by FR-10 update. The specification now explicitly mandates `serde_json::json!` (or equivalent proper serializer) and prohibits format-string construction for `metadata`. The prohibition is load-bearing: delivery agents must use `serde_json::json!({"client_type": ct}).to_string()`.

No open variances remain. All 12 functional requirements, 8 non-functional requirements, and 12 acceptance criteria are addressed by the architecture and specification.

**Vision alignment**: Feature directly advances two product vision non-negotiables: "Audit log is append-only and complete" (DDL triggers enforce this at the SQLite layer for the first time) and "Every operation attributed and logged" (`agent_attribution` provides transport-attested, non-spoofable client identity). Correctly positioned as W2-3 prerequisite infrastructure.
