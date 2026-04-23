# Component: UnimatrixServer (server.rs)

## Purpose

Three changes to `UnimatrixServer`:
1. New field: `client_type_map: Arc<Mutex<HashMap<String, String>>>`
2. New method: `ServerHandler::initialize` override — captures `clientInfo.name`
3. New method: `build_context_with_external_identity()` — Seam 2 overload
4. Removal: `build_context()` — deleted entirely after all call sites migrate

**File modified:** `crates/unimatrix-server/src/server.rs`

---

## New Field

### `client_type_map` on `UnimatrixServer`

Add to the `UnimatrixServer` struct after `store_config`:

```
/// Maps rmcp session ID → clientInfo.name (vnc-014, ADR-001).
///
/// Key: Mcp-Session-Id UUID string (HTTP) or "" (stdio singleton).
/// Value: clientInfo.name truncated to 256 Unicode scalar values.
///
/// Arc satisfies rmcp's Clone requirement on UnimatrixServer.
/// Mutex is poison-recovered via unwrap_or_else(|e| e.into_inner())
/// at every lock site (NFR-01, SEC-03).
pub client_type_map: Arc<Mutex<HashMap<String, String>>>,
```

Initialize in `UnimatrixServer::new()`:

```
// In the struct literal at the end of new():
client_type_map: Arc::new(Mutex::new(HashMap::new())),
```

---

## New Method: `ServerHandler::initialize` Override

This is an override of the rmcp 0.16.0 `ServerHandler` trait method. The
default provided-method implementation returns `Ok(self.get_info())`. This
override adds the `client_type_map` side-effect before returning the same result.

```
fn initialize(
    &self,
    request: InitializeRequestParams,
    context: RequestContext<RoleServer>,
) -> impl Future<Output = Result<InitializeResult, McpError>> + Send + '_:

    // 1. Extract clientInfo.name from the request parameter.
    let client_name_raw = request.client_info.name;

    // 2. Only proceed if the name is non-empty.
    if !client_name_raw.is_empty():

        // 3. Truncate to 256 Unicode scalar values (chars, not bytes) (NFR-02).
        let truncated: String =
            if client_name_raw.chars().count() > 256:
                let truncated = client_name_raw.chars().take(256).collect::<String>()
                tracing::warn!(
                    original_len = client_name_raw.chars().count(),
                    "clientInfo.name truncated to 256 chars"
                )
                truncated
            else:
                client_name_raw

        // 4. Extract the rmcp session key from request context extensions.
        //    HTTP: Mcp-Session-Id header value.
        //    Stdio or absent/non-UTF-8 header: "" (empty string sentinel).
        let session_key: String = context.extensions
            .get::<http::request::Parts>()
            .and_then(|p| p.headers.get("mcp-session-id"))
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()

        // 5. Insert into client_type_map with poison recovery.
        let mut map = self.client_type_map
            .lock()
            .unwrap_or_else(|e| e.into_inner())

        // 6. If stdio key "" is being overwritten, emit a debug log (FR-02, C-02).
        if session_key.is_empty() && map.contains_key(""):
            tracing::debug!(
                existing = map.get("").map(String::as_str).unwrap_or(""),
                new = %truncated,
                "stdio client_type_map entry overwritten (reconnect or second initialize)"
            )

        map.insert(session_key, truncated)
        drop(map)  // release lock immediately

    // 7. Return identical result to default implementation (NFR-07).
    return std::future::ready(Ok(self.get_info()))
```

**rmcp signature constraint (C-01)**: The return type is
`impl Future<Output = Result<InitializeResult, McpError>> + Send + '_`.
Using `std::future::ready(...)` satisfies this without an `async fn` body
which would require a pinned future. This is the correct pattern for rmcp 0.16.0.

---

## New Method: `build_context_with_external_identity`

Replaces `build_context()`. Accepts `RequestContext<RoleServer>` to extract
the rmcp session key for `client_type_map` lookup. Accepts
`Option<&ResolvedIdentity>` for the W2-3 bearer-auth seam (always `None`
in vnc-014).

```
pub(crate) async fn build_context_with_external_identity(
    &self,
    params_agent_id: &Option<String>,
    format: &Option<String>,
    session_id: &Option<String>,
    request_context: &RequestContext<RoleServer>,
    external_identity: Option<&ResolvedIdentity>,   // always None in vnc-014
) -> Result<ToolContext, rmcp::ErrorData>:

    use crate::mcp::context::ToolContext
    use crate::services::{AuditContext, AuditSource, CallerId, prefix_session_id}

    // 1. Resolve identity.
    //    When external_identity is Some (W2-3 activation path), bypass resolve_agent
    //    entirely and use the provided identity.
    //    When None (vnc-014 path), call resolve_agent exactly as build_context did.
    let identity: ResolvedIdentity = match external_identity:
        Some(ext) => ext.clone()
        None => self.resolve_agent(params_agent_id).await.map_err(rmcp::ErrorData::from)?

    // 2. Parse format.
    let format = crate::mcp::response::parse_format(format).map_err(rmcp::ErrorData::from)?

    // 3. Session ID: validate (S3) and prefix with mcp::
    let prefixed_session = if let Some(sid) = session_id:
        Self::validate_session_id(sid).map_err(rmcp::ErrorData::from)?
        Some(prefix_session_id("mcp", sid))
    else:
        None

    // 4. Build AuditContext (unchanged from build_context).
    let audit_ctx = AuditContext {
        source: AuditSource::Mcp {
            agent_id:    identity.agent_id.clone(),
            trust_level: identity.trust_level,
        },
        caller_id:     identity.agent_id.clone(),
        session_id:    prefixed_session,
        feature_cycle: None,
    }

    // 5. Extract rmcp session key for client_type lookup.
    //    Same extraction logic as in initialize().
    let rmcp_session_key: &str = request_context.extensions
        .get::<http::request::Parts>()
        .and_then(|p| p.headers.get("mcp-session-id"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")

    // 6. Look up client_type from client_type_map.
    //    Returns None when no entry exists (no initialize called, or
    //    missing session header). This is the correct behavior for
    //    NFR-03 (zero regression for tool calls without session context).
    let client_type: Option<String> = {
        let map = self.client_type_map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
        map.get(rmcp_session_key).cloned()
    }  // lock released here

    // 7. Build and return ToolContext with client_type populated.
    let caller_id = CallerId::Agent(identity.agent_id.clone())

    return Ok(ToolContext {
        agent_id:    identity.agent_id,
        trust_level: identity.trust_level,
        format,
        audit_ctx,
        caller_id,
        client_type,    // new field
    })
```

---

## Removed Method: `build_context`

Delete the `build_context()` method entirely after all 12 call sites in
`tools.rs` have been migrated. Do not replace it with a wrapper or
deprecation — the compile error when a call site remains is the enforcement
mechanism (ADR-003, SR-04).

The delivery agent migrates all 12 tool handlers first, then removes
`build_context`. Running `cargo build --workspace` after removal confirms
no remaining call sites.

---

## Initialization Sequence

The `new()` constructor gains one new field initialization:

```
UnimatrixServer {
    // ... all existing fields unchanged ...
    client_type_map: Arc::new(Mutex::new(HashMap::new())),
}
```

`HashMap::new()` allocates an empty map — no entries until `initialize` is called.

---

## Imports Required

Add to the top of `server.rs` (may already be present in some form):
```
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
// http crate is a transitive dep via rmcp/tower — no Cargo.toml change needed
```

The `Future` trait import for the `initialize` override return type:
```
use std::future::ready;   // or use as std::future::ready(...)
```

---

## Error Handling

- `initialize`: cannot return `Err` in vnc-014 scope. `Mutex` lock uses
  poison recovery. Truncation logs a WARN. Returns `Ok(self.get_info())`.
- `build_context_with_external_identity`: same error surface as the existing
  `build_context()` — errors from `resolve_agent` and `parse_format` propagate
  as `rmcp::ErrorData`. No new error paths.

---

## Key Test Scenarios

1. **HTTP session map insert (FR-01, AC-01)**: Call `initialize` with
   `clientInfo.name = "codex-mcp-client"` and a simulated
   `Mcp-Session-Id` header. Assert `client_type_map` contains
   `("<uuid>", "codex-mcp-client")` after the call.

2. **Empty clientInfo.name (AC-02)**: Call `initialize` with `clientInfo.name = ""`.
   Assert `client_type_map` is empty after the call (no insert for empty name).

3. **Stdio session (AC-08)**: Call `initialize` with non-empty `clientInfo.name`
   and no `Mcp-Session-Id` header. Assert `client_type_map[""]` = name.

4. **Stdio overwrite (R-10, FR-02)**: Call `initialize` twice on same server
   with key `""`. Assert WARN log on second call and map holds second name.

5. **Truncation at 257 chars (AC-10, EC-02)**: `clientInfo.name` of 257 chars.
   Map holds 256-char value. WARN logged. No byte-boundary split.

6. **Exact 256 chars (EC-01)**: Name of exactly 256 chars. Not truncated. No WARN.

7. **Multi-byte Unicode truncation (EC-03)**: Name of 255 ASCII + 1 four-byte char.
   Truncation by `chars().take(256)` gives 256 chars. No half-byte split.

8. **InitializeResult parity (AC-06, NFR-07)**: Return value equals
   `self.get_info()` field-by-field.

9. **Concurrent HTTP sessions (AC-07, FR-12)**: Two concurrent calls to
   `initialize` with distinct UUIDs and names. Assert no cross-contamination
   in `client_type_map`. Assert `build_context_with_external_identity` returns
   correct `client_type` for each session key.

10. **build_context_with_external_identity, no session entry (NFR-03)**:
    Call with a request_context that has no matching key in `client_type_map`.
    `ToolContext.client_type = None`. No error.

11. **Poison recovery (SEC-03, FM-01)**: `Mutex::lock()` recovery path must
    use `unwrap_or_else(|e| e.into_inner())` at both the `initialize` and
    `build_context_with_external_identity` lock sites. Verify by code inspection
    and/or test with a simulated poisoned mutex.
