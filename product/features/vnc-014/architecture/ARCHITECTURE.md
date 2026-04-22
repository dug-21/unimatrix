# VNC-014 Architecture: MCP Client Attribution via clientInfo.name + ASS-050 Schema Migration

## System Overview

VNC-014 closes the audit attribution gap for non-Claude-Code MCP clients (Codex, Gemini CLI)
that do not fire client-side hooks. It does this by capturing `clientInfo.name` from the MCP
`initialize` handshake at the server and propagating it to every `audit_log` row in that
session, without requiring any change to the tool parameter schema.

The feature delivers two tightly coupled changes:

1. **ASS-050 schema migration**: Four new columns on `audit_log`, two new indexes, and
   append-only DDL triggers — all in a single schema version bump (v24 → v25).

2. **Server-side session attribution**: `UnimatrixServer` captures `clientInfo.name` at
   `initialize` time, stores it keyed on the rmcp session ID, and propagates it through
   `ToolContext` to every `AuditEvent` construction in the tool handlers.

These two are inseparable: the migration creates the columns that the attribution machinery
writes to. Neither is useful without the other.

## Component Breakdown

### 1. `UnimatrixServer` (unimatrix-server / server.rs)

**New field**: `client_type_map: Arc<Mutex<HashMap<String, String>>>`

Maps rmcp-level session ID → `clientInfo.name`. The session ID is the `Mcp-Session-Id` UUID
header for HTTP sessions; the empty string `""` for the stdio transport. The `Arc` wrapper
satisfies rmcp's `Clone` requirement on `UnimatrixServer`.

**New method override**: `ServerHandler::initialize`

Captures `request.client_info.name` from the `InitializeRequestParams`, truncates to 256 chars
(AC-10), and inserts into `client_type_map`. Returns `Ok(self.get_info())` — identical
to the default behavior.

**New method**: `build_context_with_external_identity()`

Replaces `build_context()` on the tool-call path. Accepts `RequestContext<RoleServer>` to
extract the `Mcp-Session-Id` header and look up `client_type`. Accepts
`Option<&ResolvedIdentity>` for the Seam 2 W2-3 bearer-auth path (always `None` in vnc-014).
Populates `ToolContext.client_type`.

**Removed method**: `build_context()`

Removed after all tool handler call sites are migrated. Compile-time enforcement ensures no
missed call sites (SR-04 mitigation, ADR-003).

### 2. `ToolContext` (unimatrix-server / mcp/context.rs)

**New field**: `client_type: Option<String>`

Populated by `build_context_with_external_identity()` from the `client_type_map` lookup.
`None` when no rmcp session context is available. Consumed at `AuditEvent` construction in each
tool handler to populate `agent_attribution` and `metadata`.

### 3. `AuditEvent` (unimatrix-store / schema.rs)

**Four new fields** with `#[serde(default)]` and `impl Default for AuditEvent`:

```rust
#[serde(default)]
pub credential_type: String,    // Default::default() = ""; code must supply "none"
#[serde(default)]
pub capability_used: String,    // "" = no gate
#[serde(default)]
pub agent_attribution: String,  // "" = no transport-attested identity
#[serde(default)]
pub metadata: String,           // Default::default() = ""; code must supply "{}"
```

`Default` impl sets: `credential_type: "none"`, `capability_used: ""`,
`agent_attribution: ""`, `metadata: "{}"`. Construction sites that have
`ToolContext.client_type` override `agent_attribution` and `metadata` explicitly.

### 4. `SqlxStore::log_audit_event` + `read_audit_event` (unimatrix-store / audit.rs)

**Updated INSERT**: adds `?9`, `?10`, `?11`, `?12` bindings for the four new fields.

**Updated SELECT**: reads all four new columns in `read_audit_event`.

### 5. Schema Migration (unimatrix-store / migration.rs + db.rs)

**`CURRENT_SCHEMA_VERSION`**: bumped from 24 to 25.

**Migration block `if current_version < 25`**: pre-flight `pragma_table_info` checks for all
four columns before any ALTER executes. Four `ALTER TABLE ADD COLUMN`, two `CREATE INDEX IF
NOT EXISTS`, two `CREATE TRIGGER IF NOT EXISTS` statements.

**`db.rs` `create_tables_if_needed()`**: updated with the new columns in the `audit_log` DDL
and the trigger DDL (byte-identical to migration).

### 6. `Capability::as_audit_str()` (unimatrix-server / infra/registry.rs)

New method returning lowercase string constants per variant. All tool handler AuditEvent
construction sites use this method for `capability_used`. Non-capability-gated construction
sites use `""`.

### 7. Removed: `gc_audit_log` (unimatrix-store / retention.rs)

The `BEFORE DELETE` trigger makes time-based GC impossible. `gc_audit_log()` is removed and
its background tick call site is removed. Audit log accumulates indefinitely (append-only
by design). See ADR-005.

### 8. Updated: `drop_all_data` (unimatrix-server / import/mod.rs)

`DELETE FROM audit_log;` is removed from the import reset path. Audit history is preserved
across import operations. See ADR-005.

## Component Interactions

```
MCP Client (Codex/Gemini/Claude Code)
    |
    | HTTP POST /initialize  (InitializeRequestParams)
    v
UnimatrixServer::initialize()           [server.rs]
    |-- extract client_info.name
    |-- extract Mcp-Session-Id from extensions
    |-- client_type_map.insert(session_id, name)
    |
    | HTTP POST /rpc  (tool call)
    v
rmcp tool dispatch
    v
build_context_with_external_identity()  [server.rs]
    |-- extract Mcp-Session-Id from request_context.extensions
    |-- client_type_map.get(session_id)  -> Option<String>
    |-- resolve_agent() (or bypass if external_identity Some)
    |-- returns ToolContext { ..., client_type: Option<String> }
    v
Tool handler  [mcp/tools.rs]
    |-- require_cap()
    |-- build AuditEvent {
    |       credential_type:   "none",
    |       capability_used:   Capability::X.as_audit_str(),
    |       agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    |       metadata:          <JSON or "{}">
    |   }
    |-- audit_fire_and_forget(event)
    v
AuditLog::log_event_async()             [infra/audit.rs]
    v
SqlxStore::log_audit_event()            [unimatrix-store/audit.rs]
    v
audit_log table (12 columns, append-only)
```

## Technology Decisions

| ADR | Title | Unimatrix ID |
|-----|-------|--------------|
| ADR-001 | client_type_map as Arc<Mutex<HashMap>> on UnimatrixServer | #4355 |
| ADR-002 | ServerHandler::initialize override for clientInfo.name capture | #4356 |
| ADR-003 | build_context_with_external_identity() Seam 2 overload; build_context() removed | #4357 |
| ADR-004 | Four-column audit_log migration idempotency via pragma_table_info | #4358 |
| ADR-005 | Append-only trigger remediation — remove gc_audit_log and import DELETE | #4359 |
| ADR-006 | capability_used via Capability::as_audit_str() enum-derived constants | #4360 |
| ADR-007 | Two-field attribution model: agent_id (spoofable) vs agent_attribution (transport-attested) | #4361 |
| ADR-008 | ResolvedIdentity crate placement — unimatrix-server only, not unimatrix-core | #4362 |

- **`Arc<Mutex<HashMap>>` not `DashMap`**: Current session concurrency is low. Mutex is
  held only for HashMap insert/lookup (no I/O). DashMap is deferred to the W2-2 HTTP scale
  work if benchmarks show contention. See ADR-001 (#4355).
- **`build_context()` removed, not wrapped**: Compile-time enforcement is the only reliable
  way to guarantee complete migration of 10+ call sites. See ADR-003 (#4357), SR-04.
- **All four columns in one version bump**: ASS-050 mandates it; they are semantically
  interdependent as part of the compliance record. See ADR-004 (#4358).
- **Append-only triggers remove the GC path**: This is the correct semantic for a compliance
  audit log. GC was a size-management measure inconsistent with append-only requirements.
  See ADR-005 (#4359).

## Integration Points

### Existing Components Modified

| Component | File | Change |
|-----------|------|--------|
| `UnimatrixServer` | `server.rs` | New field, new `initialize` override, new `build_context_with_external_identity`, remove `build_context` |
| `ToolContext` | `mcp/context.rs` | New `client_type: Option<String>` field |
| `AuditEvent` | `unimatrix-store/schema.rs` | Four new fields + `Default` impl |
| `log_audit_event` | `unimatrix-store/audit.rs` | Bind four new fields in INSERT |
| `read_audit_event` | `unimatrix-store/audit.rs` | Read four new fields in SELECT |
| `migration.rs` | `unimatrix-store/migration.rs` | New v24→v25 migration block |
| `db.rs` | `unimatrix-store/db.rs` | Updated `audit_log` DDL + triggers in `create_tables_if_needed` |
| `Capability` | `infra/registry.rs` | New `as_audit_str()` method |
| `retention.rs` | `unimatrix-store/retention.rs` | Remove `gc_audit_log` |
| `import/mod.rs` | `unimatrix-server/import/mod.rs` | Remove `DELETE FROM audit_log` |
| All tool handlers | `mcp/tools.rs` | Migrate from `build_context` to `build_context_with_external_identity`; supply four new AuditEvent fields |
| Background audit sites | `background.rs`, `uds/listener.rs` | Supply four new AuditEvent fields directly |

### Seam 2 Forward-Compatibility Surface (W2-3)

`build_context_with_external_identity()` accepts `Option<&ResolvedIdentity>`. In vnc-014 it
is always `None`. W2-3 activates this seam by passing the bearer-validated identity. The
function signature does not change between vnc-014 and W2-3.

`ResolvedIdentity` is defined in `unimatrix-server/mcp/identity.rs` and stays there. It is not promoted to `unimatrix-core` for vnc-014. If W2-3 requires it from another crate, a `pub use` re-export is the migration path at that time. See ADR-008 (#4362).

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|----------------|--------|
| `UnimatrixServer.client_type_map` | `Arc<Mutex<HashMap<String, String>>>` | `server.rs` (new field) |
| `ServerHandler::initialize` override | `fn initialize(&self, InitializeRequestParams, RequestContext<RoleServer>) -> impl Future<Output=Result<InitializeResult, McpError>> + Send + '_` | `server.rs` |
| `build_context_with_external_identity` | `async fn(&self, &Option<String>, &Option<String>, &Option<String>, &RequestContext<RoleServer>, Option<&ResolvedIdentity>) -> Result<ToolContext, ErrorData>` | `server.rs` |
| `ToolContext.client_type` | `Option<String>` | `mcp/context.rs` (new field) |
| `AuditEvent.credential_type` | `String`, default `"none"` | `unimatrix-store/schema.rs` |
| `AuditEvent.capability_used` | `String`, default `""` | `unimatrix-store/schema.rs` |
| `AuditEvent.agent_attribution` | `String`, default `""` | `unimatrix-store/schema.rs` |
| `AuditEvent.metadata` | `String`, default `"{}"` | `unimatrix-store/schema.rs` |
| `Capability::as_audit_str()` | `fn(&self) -> &'static str` | `infra/registry.rs` (new method) |
| rmcp session ID access path | `context.extensions.get::<http::request::Parts>().and_then(\|p\| p.headers.get("mcp-session-id")).and_then(\|v\| v.to_str().ok()).unwrap_or("")` | `server.rs` (used in both initialize and build_context_with_external_identity) |
| `clientInfo.name` access at initialize | `request.client_info.name` (field on `InitializeRequestParams`) | rmcp 0.16.0 |
| `clientInfo.name` access at tool call | `ctx.peer.peer_info().map(\|ci\| ci.client_info.name.as_str())` | rmcp 0.16.0 (NOT used — map lookup preferred) |

## Attribution Population Rules

At `AuditEvent` construction in tool handlers:

```
agent_attribution = ctx.client_type.clone().unwrap_or_default()
metadata = if let Some(ct) = ctx.client_type.as_deref().filter(|s| !s.is_empty()) {
    format!(r#"{{"client_type":"{}"}}"#, ct.replace('"', "\\\""))
} else {
    "{}".to_string()
}
credential_type = "none"   // STDIO/OSS; "static_token" once W2-2 bearer lands
capability_used = Capability::X.as_audit_str()
```

At non-tool-call construction sites (background tick, UDS listener):
```
credential_type = "none"
capability_used = ""
agent_attribution = ""
metadata = "{}"
```

## Open Questions

**OQ-1 (stateless HTTP mode)**: In rmcp's stateless mode (no session manager, no `Mcp-Session-Id`
header), `initialize` is never called and the header is absent on all requests. The fallback to
`""` as the session key means stateless HTTP traffic is treated as stdio. This is documented as
a known limitation — stateless mode is not a supported Unimatrix deployment in vnc-014 scope.
Delivery agent should confirm whether any test or CI scenario uses stateless mode.

**OQ-2 (background.rs audit sites)**: There are AuditEvent construction sites in `background.rs`
at lines 1197, 1252, and 2267. These are not on the tool-call path and do not have
`RequestContext` available. They should use `..AuditEvent::default()` for the four new fields.
Delivery agent should confirm these are the only non-tool-call sites.

**OQ-3 (schema version cascade)**: The cascade checklist (pattern #4125) applies. Delivery agent
must run `cargo test --workspace` immediately after bumping `CURRENT_SCHEMA_VERSION` to 25 to
catch all cascade failures before writing the new migration test file.
