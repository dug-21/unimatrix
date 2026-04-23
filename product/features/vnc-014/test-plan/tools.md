# Test Plan: Tool Handlers (mcp/tools.rs)

## Component Summary

All 12 tool handlers in `tools.rs` migrate from `build_context()` to
`build_context_with_external_identity()`. Every `AuditEvent` construction site in tool handlers
gains four new fields: `credential_type`, `capability_used`, `agent_attribution`, `metadata`.

Non-tool-call `AuditEvent` sites (`background.rs`, `uds/listener.rs`) use `..AuditEvent::default()`.

---

## Unit Tests (in tools.rs `#[cfg(test)]`)

### TOOL-U-01: `build_context()` absent from production source

**Risk**: R-05, AC-12
**Assert**: `cargo grep -n "build_context[^_]" crates/unimatrix-server/src/` returns 0 results
from production code paths. Specifically, no `fn build_context(` or `self.build_context(` call
site exists. This is enforced by removal (ADR-003 decision).

Verified by: successful `cargo build --workspace` (removal causes compile error at any
missed call site).

---

### TOOL-U-02: All 12 handlers compile with `build_context_with_external_identity`

**Risk**: R-05
**Assert**: `cargo build --workspace` succeeds with zero errors related to handler migration.
This is a compile-time proof of completeness.

---

### TOOL-U-03: `credential_type = "none"` in all tool handler audit rows

**Risk**: R-09 (cascade: constant must be literal string, not variant)
**Arrange**: For a representative tool (e.g., `context_store`), construct a `ToolContext`
with no `client_type`.
**Act**: Execute the tool handler in test mode.
**Assert**: The constructed `AuditEvent.credential_type == "none"` (literal, not derived from
`Capability::as_audit_str()`).

---

### TOOL-U-04: `capability_used` — each tool uses its canonical Capability string

**Risk**: R-09, AC-11
Verify via unit test for each tool group:

| Tool | Expected `capability_used` | Capability Variant |
|------|--------------------------|-------------------|
| `context_search` | `"search"` | `Capability::Search` |
| `context_lookup` | `"search"` | `Capability::Search` |
| `context_briefing` | `"search"` | `Capability::Search` |
| `context_get` | `"read"` | `Capability::Read` |
| `context_status` | `"read"` | `Capability::Read` |
| `context_retrospective` | `"read"` | `Capability::Read` |
| `context_store` | `"write"` | `Capability::Write` |
| `context_correct` | `"write"` | `Capability::Write` |
| `context_deprecate` | `"write"` | `Capability::Write` |
| `context_quarantine` | `"write"` | `Capability::Write` |
| `context_cycle` | `"write"` | `Capability::Write` |
| `context_enroll` | `"admin"` | `Capability::Admin` |

**Assert for each**: Inspect the `AuditEvent` constructed during the tool's execution
and verify `capability_used == expected_string`.

Implementation note: Unit tests that call handlers directly need a constructed `ToolContext`
with appropriate capability permissions. Use the existing test helper infrastructure in
the server crate.

---

### TOOL-U-05: `agent_attribution` populated from `ctx.client_type`, not from tool params

**Risk**: SEC-01
**Arrange**: Construct a `ToolContext` where `client_type = Some("codex-mcp-client")`.
**Act**: Execute any tool handler (e.g., `context_store` with valid params).
**Assert**:
- `AuditEvent.agent_attribution == "codex-mcp-client"`
- The `agent_id` tool parameter (agent-declared) does NOT appear in `agent_attribution`
- These are confirmed to be distinct fields from separate sources

---

### TOOL-U-06: `agent_attribution = ""` when `ctx.client_type = None`

**Risk**: R-05, AC-03
**Arrange**: `ToolContext` with `client_type = None`.
**Act**: Execute any tool handler.
**Assert**: `AuditEvent.agent_attribution == ""`

---

### TOOL-U-07: `metadata` contains `client_type` key when attribution is present

**Risk**: R-08, R-06
**Arrange**: `ToolContext` with `client_type = Some("gemini-cli-mcp-client")`.
**Act**: Execute tool handler; inspect constructed `AuditEvent.metadata`.
**Assert**:
- `serde_json::from_str::<serde_json::Value>(&event.metadata).is_ok()`
- `parsed["client_type"] == "gemini-cli-mcp-client"`

---

### TOOL-U-08: `metadata = "{}"` when no attribution

**Risk**: R-06, NFR-06
**Arrange**: `ToolContext` with `client_type = None`.
**Act**: Execute tool handler.
**Assert**: `AuditEvent.metadata == "{}"`

---

### TOOL-U-09: `metadata` with JSON-special `clientInfo.name` — FR-10 / SEC-02

**Risk**: R-08, SEC-02
**Test cases** (using `serde_json::json!` macro, not format strings):

1. `client_type = r#"client"with"quotes"#`:
   - `metadata` parses as valid JSON
   - `parsed["client_type"]` equals original string with embedded quotes

2. `client_type = r"client\with\backslash"`:
   - `metadata` parses as valid JSON
   - `parsed["client_type"]` equals original string with backslash

3. `client_type = "client\nwith\nnewline"`:
   - `metadata` parses as valid JSON
   - `parsed["client_type"]` equals original string with literal newline

4. `client_type = r#"a","b":"c"#` (injection attempt):
   - `metadata` parses as valid JSON
   - Only one key (`client_type`) is present — no injection
   - `parsed["client_type"]` equals `r#"a","b":"c"#` as a string value

5. `client_type = r#"{"nested":"json"}"#`:
   - `metadata` parses as valid JSON
   - `parsed["client_type"]` equals the nested-looking string as a single string value

**Assert for all**: `serde_json::from_str::<serde_json::Value>(&metadata).is_ok()` AND
the `client_type` value in parsed JSON equals the original input exactly.

---

### TOOL-U-10: No tool parameter can influence `AuditEvent.agent_attribution`

**Risk**: SEC-01, C-05
**Arrange**: Construct a `ToolContext` with `client_type = None`. Provide a tool call with
`agent_id = "attacker-injected"` parameter.
**Act**: Execute tool handler.
**Assert**:
- `AuditEvent.agent_attribution == ""` (from `client_type`, not from `agent_id`)
- `AuditEvent.agent_id == "attacker-injected"` (agent_id still stored, but in the separate field)

---

## Non-Tool-Call Sites

### TOOL-U-11: Background.rs `AuditEvent` sites compile with `..AuditEvent::default()`

**Risk**: R-12
**Assert**: `cargo build --workspace` succeeds. The `..AuditEvent::default()` syntax fills
the four new fields at the three background.rs sites (approx. lines 1197, 1252, 2267) and
any `uds/listener.rs` sites.

**Integration verification**: Trigger a background tick that produces an audit event; verify
the resulting row has `credential_type = "none"`, `capability_used = ""`,
`agent_attribution = ""`, `metadata = "{}"`.

---

### TOOL-U-12: `import/drop_all_data` does not issue `DELETE FROM audit_log`

**Risk**: R-01, AC-05b, IR-04
**Arrange**: Open a store with pre-existing `audit_log` rows. Install the v25 triggers.
**Act**: Call `import::drop_all_data(pool)` (or the updated equivalent).
**Assert**:
- Returns `Ok(_)` — does NOT raise trigger ABORT error
- `audit_log` row count is NOT zero after the call (audit history preserved per ADR-005)

---

## Acceptance Criteria Coverage

| AC-ID | Test(s) |
|-------|---------|
| AC-01 | TOOL-U-05, TOOL-U-07, and SRV integration test |
| AC-03 | TOOL-U-06, TOOL-U-08, TOOL-U-10 |
| AC-05 | AE-I-01 (cross-component) |
| AC-11 | TOOL-U-04 (all 12 tools) |
| AC-12 | TOOL-U-01, TOOL-U-02 |
