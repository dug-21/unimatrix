# VNC-014 Pseudocode Overview

## Feature Summary

VNC-014 delivers two tightly coupled changes in a single PR:

1. **ASS-050 schema migration** (v24→v25): four new columns on `audit_log`,
   two indexes, two append-only DDL triggers.
2. **Server-side client attribution**: `clientInfo.name` captured at MCP
   `initialize`, keyed by `Mcp-Session-Id` in a `client_type_map`, propagated
   through `ToolContext` to every `AuditEvent` construction.

Neither is useful without the other: the migration creates columns that the
attribution machinery writes to.

---

## Component Inventory

| File | Component | Role |
|------|-----------|------|
| `server.md` | `UnimatrixServer` (server.rs) | New field, new `initialize` override, new `build_context_with_external_identity`, removal of `build_context` |
| `tool-context.md` | `ToolContext` (mcp/context.rs) | New `client_type: Option<String>` field |
| `audit-event.md` | `AuditEvent` + `audit.rs` (unimatrix-store) | Four new fields on struct; INSERT and SELECT updated |
| `migration.md` | `migration.rs` + `db.rs` (unimatrix-store) | v24→v25 migration block; `create_tables_if_needed` DDL parity |
| `tools.md` | All 12 tool handlers (mcp/tools.rs) | Migrate from `build_context` to Seam 2; populate four new `AuditEvent` fields |
| `capability.md` | `Capability::as_audit_str` (infra/registry.rs) | New method returning canonical lowercase audit strings |
| `remediation.md` | `gc_audit_log` (retention.rs) + `drop_all_data` (import/mod.rs) | Remove DELETE paths that conflict with append-only triggers |

---

## Data Flow

```
MCP Client
  |
  | initialize(clientInfo.name="codex-mcp-client", Mcp-Session-Id="<uuid>")
  v
UnimatrixServer::initialize()                         [server.rs]
  truncate name to 256 chars
  extract session_key from Mcp-Session-Id header (or "" for stdio)
  client_type_map.lock().insert(session_key, name)
  return Ok(self.get_info())

  | tool call (same Mcp-Session-Id header)
  v
UnimatrixServer::build_context_with_external_identity()   [server.rs]
  extract session_key from request_context extensions
  client_type = client_type_map.lock().get(session_key).cloned()
  resolve_agent() as before (external_identity=None in vnc-014)
  return ToolContext { ..., client_type: Option<String> }
  |
  v
Tool handler                                          [mcp/tools.rs]
  ctx.client_type consumed to build AuditEvent:
    credential_type   = "none"
    capability_used   = Capability::X.as_audit_str().to_string()
    agent_attribution = ctx.client_type.clone().unwrap_or_default()
    metadata          = serde_json::json!({"client_type": ct}).to_string()
                        OR "{}" when client_type is None/empty
  |
  v
SqlxStore::log_audit_event()                          [unimatrix-store/audit.rs]
  INSERT with ?9..?12 for four new fields
  |
  v
audit_log table (12 columns, append-only triggers)
```

---

## Shared Types Introduced or Modified

### `AuditEvent` (unimatrix-store/schema.rs) — 4 new fields

```
credential_type:   String   // sentinel "none"; #[serde(default)] -> ""
capability_used:   String   // sentinel ""; #[serde(default)] -> ""
agent_attribution: String   // sentinel ""; #[serde(default)] -> ""
metadata:          String   // sentinel "{}"; #[serde(default)] -> ""
```

`Default` impl returns sentinels, NOT serde defaults:
- `credential_type  = "none".to_string()`
- `capability_used  = String::new()`
- `agent_attribution = String::new()`
- `metadata         = "{}".to_string()`

IMPORTANT: `#[serde(default)]` gives `""` for all four (String::default).
This is correct for legacy JSON deserialization compatibility.
`AuditEvent::default()` gives the sentinels above. These two paths are
distinct and must be tested separately (R-13).

### `ToolContext` (mcp/context.rs) — 1 new field

```
client_type: Option<String>   // clientInfo.name from client_type_map; None if absent
```

### `UnimatrixServer` (server.rs) — 1 new field

```
client_type_map: Arc<Mutex<HashMap<String, String>>>
  // Key: Mcp-Session-Id UUID (HTTP) or "" (stdio)
  // Value: clientInfo.name, truncated to 256 Unicode scalar values
```

---

## Sequencing Constraints

1. **`audit-event.md` first** — struct and SQL changes that all other components depend on.
2. **`migration.md` second** — schema must be in place before server changes can be tested.
3. **`capability.md` third** — `as_audit_str()` must exist before `tools.md` can reference it.
4. **`tool-context.md` fourth** — `ToolContext.client_type` must exist before `server.md` can return it.
5. **`server.md` fifth** — `build_context_with_external_identity` depends on `ToolContext`.
6. **`tools.md` sixth** — all 12 handlers call the new Seam 2 method.
7. **`remediation.md` last** — removal is safe once triggers are in place (migration step done).

All seven components can be implemented by a single delivery agent in sequence.
The compile gate after step 6 (removing `build_context`) enforces migration completeness.

---

## Critical Cross-Cutting Constraints

- `AuditEvent.session_id` MUST come from `ctx.audit_ctx.session_id` (agent-declared,
  `mcp::`-prefixed). NEVER from the rmcp `Mcp-Session-Id` header. These are two
  completely different namespaces. (#4363)
- `metadata` construction MUST use `serde_json::json!`. Format-string concatenation
  is prohibited regardless of escaping (FR-10, SEC-02).
- All four `pragma_table_info` checks run BEFORE any `ALTER TABLE` (ADR-004, SR-02).
- `build_context()` is removed entirely — not wrapped, not deprecated (ADR-003).
- All `Mutex::lock()` calls use `unwrap_or_else(|e| e.into_inner())` (poison recovery).
- `clientInfo.name` truncated to 256 Unicode scalar values via `chars().take(256)` —
  byte-level truncation is forbidden (NFR-02).
