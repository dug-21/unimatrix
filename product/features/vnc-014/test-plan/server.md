# Test Plan: UnimatrixServer (server.rs)

## Component Summary

`UnimatrixServer` gains three changes:
1. New field `client_type_map: Arc<Mutex<HashMap<String, String>>>`
2. `ServerHandler::initialize` override that captures `clientInfo.name`
3. `build_context_with_external_identity()` replacing `build_context()`

The removed `build_context()` method is enforced at compile time (ADR-003).

---

## Unit Tests

### SRV-U-01: `client_type_map` initialized empty

**Risk**: R-01 (setup)
**Arrange**: Construct `UnimatrixServer` via its test constructor.
**Act**: Lock `client_type_map` and read its length.
**Assert**: `client_type_map.lock().unwrap().len() == 0`

---

### SRV-U-02: `initialize` inserts non-empty `clientInfo.name` under session key

**Risk**: R-03, AC-01, AC-08
**Arrange**: Construct server; prepare a mock `InitializeRequestParams` with
`client_info.name = "codex-mcp-client"` and a mock `RequestContext` that injects
`Mcp-Session-Id: "test-uuid-001"` into extensions.
**Act**: Call `server.initialize(request, context).await`.
**Assert**:
- Returns `Ok(_)`
- `client_type_map.lock()["test-uuid-001"] == "codex-mcp-client"`
- Map contains exactly 1 entry

---

### SRV-U-03: `initialize` does NOT insert empty `clientInfo.name`

**Risk**: AC-02
**Arrange**: Construct server; prepare `client_info.name = ""`.
**Act**: Call `server.initialize(request, context).await`.
**Assert**:
- Returns `Ok(_)`
- `client_type_map.lock().is_empty() == true`

---

### SRV-U-04: `initialize` uses `""` key for stdio (no Mcp-Session-Id header)

**Risk**: AC-08, R-03
**Arrange**: Construct server; prepare `client_info.name = "claude-code"` and a
`RequestContext` with no `http::request::Parts` in extensions (stdio path).
**Act**: Call `server.initialize(request, context).await`.
**Assert**:
- `client_type_map.lock()[""] == "claude-code"`

---

### SRV-U-05: `clientInfo.name` truncated at 256 chars with WARN log

**Risk**: R-03, AC-10, EC-02
**Arrange**: Construct server; prepare `client_info.name` of exactly 300 ASCII characters.
**Act**: Call `server.initialize(request, context).await` with WARN log capture.
**Assert**:
- Stored value in `client_type_map` has exactly 256 chars (`value.chars().count() == 256`)
- WARN log was emitted containing the truncation notice

---

### SRV-U-06: `clientInfo.name` of exactly 256 chars NOT truncated, no WARN

**Risk**: AC-10, EC-01
**Arrange**: Construct server; prepare `client_info.name` of exactly 256 ASCII characters.
**Act**: Call `server.initialize(request, context).await`.
**Assert**:
- Stored value has exactly 256 chars
- No WARN log emitted

---

### SRV-U-07: `initialize` returns `Ok(self.get_info())` — InitializeResult parity

**Risk**: R-14, AC-06
**Arrange**: Construct server.
**Act**: Call `server.initialize(request, context).await` AND `server.get_info()` on the same
server instance.
**Assert**:
- Both return identical `InitializeResult` structs (compare field by field: `server_info`,
  `capabilities`, `protocol_version`, `instructions`)
- The override does NOT add extra capabilities or alter the protocol version

---

### SRV-U-08: Stdio key `""` overwritten on second `initialize` — WARN emitted

**Risk**: R-10
**Arrange**: Construct server; call `initialize` with `client_info.name = "first-client"` (key `""`).
**Act**: Call `initialize` again with `client_info.name = "second-client"` (key `""`).
**Assert**:
- `client_type_map.lock()[""] == "second-client"` (second value wins)
- A WARN-level log was emitted on the second call indicating overwrite of the `""` key

---

### SRV-U-09: `build_context_with_external_identity` with `external_identity = None` populates `client_type`

**Risk**: R-05, R-07, AC-01
**Arrange**: Construct server; pre-populate `client_type_map["test-uuid"] = "gemini-cli-mcp-client"`;
prepare `RequestContext` with `Mcp-Session-Id: "test-uuid"`.
**Act**: Call `build_context_with_external_identity(..., None).await`.
**Assert**:
- Returns `Ok(ToolContext { client_type: Some("gemini-cli-mcp-client"), ... })`
- `resolve_agent()` was called (normal auth path)

---

### SRV-U-10: `build_context_with_external_identity` with no map entry returns `client_type = None`

**Risk**: R-03, AC-03
**Arrange**: Construct server with empty `client_type_map`; prepare any `RequestContext`.
**Act**: Call `build_context_with_external_identity(..., None).await`.
**Assert**:
- Returns `Ok(ToolContext { client_type: None, ... })`
- Call succeeds without error

---

### SRV-U-11: `build_context_with_external_identity` with `external_identity = Some(_)` bypasses `resolve_agent`

**Risk**: R-07 (Seam 2 W2-3 path)
**Arrange**: Construct server; prepare a `ResolvedIdentity` stub value.
**Act**: Call `build_context_with_external_identity(..., Some(&identity)).await`.
**Assert**:
- Returns `Ok(_)` — does NOT panic or `unreachable!()`
- `resolve_agent()` was NOT called (bypassed when identity is provided)

---

### SRV-U-12: `client_type_map` Mutex poison recovery

**Risk**: SEC-03
**Arrange**: Construct server; poison the Mutex by calling `std::panic::catch_unwind` on a
closure that acquires the lock and panics.
**Act**: Call `server.initialize(request, context).await` on the poisoned server.
**Assert**:
- Does NOT panic at `lock()` call site
- The `unwrap_or_else(|e| e.into_inner())` recovery pattern executes
- A value is inserted into the map (poison recovery succeeds)

---

### SRV-U-13: `Mcp-Session-Id` header present but invalid UTF-8 — fallback to `""`

**Risk**: EC-05
**Arrange**: Construct server; prepare `client_info.name = "test-client"` and a
`RequestContext` with a `mcp-session-id` header containing invalid UTF-8 bytes.
**Act**: Call `server.initialize(request, context).await`.
**Assert**:
- `client_type_map.lock()[""] == "test-client"` (falls back to stdio key)
- No panic or error

---

### SRV-U-14: `build_context()` removed — compile-time enforcement

**Risk**: R-05, AC-12
**Arrange**: N/A (compile-time).
**Assert**: `grep -r "build_context[^_]" crates/unimatrix-server/src/` returns zero matches.
In CI, verify the symbol `build_context` (without `_with_external_identity` suffix) does not
exist in `server.rs`.

---

### SRV-U-15: Multi-byte Unicode name truncated at char boundary (not byte boundary)

**Risk**: EC-03, NFR-02
**Arrange**: Construct server; prepare `client_info.name` = 255 ASCII chars + one 4-byte
Unicode character (e.g., U+1F600 GRINNING FACE), total 256 chars / 259 bytes.
**Act**: Call `server.initialize(request, context).await`.
**Assert**:
- Stored value has exactly 256 chars: `value.chars().count() == 256`
- Value ends with the 4-byte character, not truncated mid-codepoint
- No WARN log emitted (exactly 256 chars, not over limit)

---

## Integration Tests

### SRV-I-01: Tool call with no prior `initialize` succeeds — EC-04

**Risk**: AC-03, EC-04
Implemented in `tools.md` (cross-reference). No `initialize` called before tool invocation;
`client_type` is `None`; tool succeeds; audit row has `agent_attribution = ""`.
