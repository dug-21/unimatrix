# Agent Report: vnc-014-agent-9-tools

## Task

Migrate all 12 tool handlers in `crates/unimatrix-server/src/mcp/tools.rs` from `build_context()` to `build_context_with_external_identity()`, and populate the 4 new `AuditEvent` fields at every construction site.

## Files Modified

- `crates/unimatrix-server/src/mcp/tools.rs`

## Changes Made

### 1. `build_context` → `build_context_with_external_identity` (10 call sites)

Added `request_context: rmcp::service::RequestContext<rmcp::RoleServer>` as a second parameter to all 10 tool handlers that used `build_context`:

- `context_search` — `Capability::Search`
- `context_lookup` — `Capability::Read`
- `context_store` — `Capability::Write`
- `context_get` — `Capability::Read`
- `context_correct` — `Capability::Write`
- `context_deprecate` — `Capability::Write`
- `context_status` — `Capability::Read`
- `context_briefing` — `Capability::Search`
- `context_quarantine` — `Capability::Write`
- `context_enroll` — `Capability::Admin`

Each call now passes `(&params.agent_id, &params.format, &params.session_id_or_none, &request_context, None)`.

`RequestContext<RoleServer>` is extracted via `FromContextPart` (rmcp 0.16.0 — `impl<C: AsRequestContext> FromContextPart<C> for RequestContext<RoleServer>`), which allows adding it as a named parameter to the `#[tool]`-annotated handler functions.

### 2. AuditEvent 4-field population (8 explicit construction sites)

At each `AuditEvent { ..AuditEvent::default() }` site inside tool handler scope (where `ctx.client_type` is available), replaced `..AuditEvent::default()` with explicit 4-field values:

```rust
credential_type:   "none".to_string(),
capability_used:   Capability::X.as_audit_str().to_string(),
agent_attribution: ctx.client_type.clone().unwrap_or_default(),
metadata:          <serde_json::json! or "{}">
```

The `metadata_json` is computed before each `AuditEvent` construction using:

```rust
let metadata_json = match ctx.client_type.as_deref().filter(|s| !s.is_empty()) {
    Some(ct) => serde_json::json!({"client_type": ct}).to_string(),
    None => "{}".to_string(),
};
```

Affected sites: `context_lookup`, `context_get`, `context_deprecate`, `context_status`, `context_briefing`, `context_quarantine` (2 arms: quarantine + restore), `context_enroll`.

### 3. Non-tool-call AuditEvent sites — unchanged

`context_cycle_review` (3 sites) and `context_cycle` (1 site) use `resolve_agent` directly (no `ToolContext`). These retain `..AuditEvent::default()` per the pseudocode spec for non-tool-call sites. The `write_lesson_learned` helper also retains `..AuditEvent::default()`.

### 4. `context_briefing` capability gate correction

The original `require_cap` call used `Capability::Read`. The spec (IMPLEMENTATION-BRIEF.md capability table, tools.md, test plan TOOL-U-04) maps `context_briefing` to `Capability::Search`. Corrected to `Capability::Search` in both `require_cap` and `capability_used`.

### 5. Unit tests added

Added `mod vnc014_audit_field_tests` at end of file covering TOOL-U-03 through TOOL-U-10:
- `credential_type` is literal "none" (not empty)
- `capability_used` values for all 4 capability groups
- `agent_attribution` populated from `client_type`, not `agent_id`
- `metadata` is "{}" when `client_type` is None or empty
- `metadata` contains `client_type` key when present
- JSON safety for embedded quotes, backslashes, newlines, injection attempts, nested-JSON strings

## Build Result

`cargo build --workspace` — clean, zero errors (18 pre-existing warnings, unchanged).

## Tests

`cargo test --workspace` compiles all non-server test targets cleanly.

The `unimatrix-server` lib test binary has 2 pre-existing errors in `server.rs` test-only code introduced by the Wave 1 server.rs agent (`serve_client` function absent, `ClientInfo` missing `meta` field). These are outside the scope of `mcp/tools.rs` and did not exist before the current feature branch. The errors prevent running my unit tests in isolation, but do not indicate a regression in tools.rs.

Production lib (`cargo check -p unimatrix-server --lib`) compiles clean.

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in non-test code
- [x] All modified files within scope (tools.rs only)
- [x] Error handling uses project error type with context via `?`, no `.unwrap()` in non-test code
- [x] New test functions have descriptive TOOL-U-NN names matching test plan
- [x] Code follows validated pseudocode — no silent deviations
- [x] Test cases match component test plan expectations (TOOL-U-03 to TOOL-U-10)
- [x] `AuditEvent.session_id` populated from `ctx.audit_ctx.session_id` (never from Mcp-Session-Id UUID) — verified by inspection
- [x] `metadata` uses `serde_json::json!` macro exclusively (FR-10/SEC-02)
- [x] `cargo fmt` applied

## Issues / Deviations

1. **`context_briefing` capability gate**: Original code used `Capability::Read` in `require_cap`. Corrected to `Capability::Search` per spec. This is not a silent deviation — it is documented here and in the PR.

2. **`context_quarantine` capability gate**: Original code used `Capability::Admin`. Per pseudocode/tools.md: `capability gate: Capability::Write`. Corrected to `Capability::Write` in `require_cap`. Documented.

3. **server.rs test compilation failures**: Pre-existing — introduced by Wave 1 server.rs agent. Not caused by this agent. Files: `src/server.rs:3194` (`ClientInfo` missing `meta`) and `src/server.rs:3209` (`serve_client` not found). Out of scope for this agent.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced #4363 (session_id namespace warning), #4357 (build_context Seam 2 ADR), #4047 (AuditEvent 5-surface update pattern). All applied.
- Queried: `mcp__unimatrix__context_search` (pattern + decision categories) — returned #4363, #4356, #4355. Applied.
- Stored: nothing novel to store — the IR-02 integration risk (how `RequestContext` reaches tool handlers) resolved empirically by reading rmcp 0.16.0 source: `FromContextPart<C> for RequestContext<RoleServer> where C: AsRequestContext` is implemented in `handler/server/common.rs`. Pattern already documented implicitly in vnc-014 pseudocode. The rmcp API discovery is not novel enough to warrant a separate entry.
