# VNC-014: Specification
## Server-Side MCP Client Attribution via clientInfo.name + ASS-050 audit_log Schema Migration

---

## Objective

VNC-014 fixes the audit attribution gap for non-Claude-Code MCP clients (e.g., Codex CLI, Gemini
CLI) by capturing `clientInfo.name` from the MCP `initialize` handshake, binding it to the
rmcp transport session ID, and propagating it to every subsequent `audit_log` write in that
session. Simultaneously, the feature delivers the full four-column ASS-050 schema migration to
`audit_log` (schema version 24 → 25), adding `credential_type`, `capability_used`,
`agent_attribution`, and `metadata` columns with append-only DDL triggers. The feature ships
`build_context_with_external_identity()` (Seam 2) as the new overload on `UnimatrixServer`,
enabling bearer-validated identity passthrough for the W2-3 enterprise tier with zero impact
on vnc-014's OSS scope.

---

## Functional Requirements

### FR-01: `initialize` Handler Override

`UnimatrixServer` MUST override `ServerHandler::initialize` from the rmcp 0.16.0 trait. The
override MUST:

1. Extract `request.client_info.name` directly from the `InitializeRequestParams` parameter
   (not from `context` extensions — the peer_info path applies at tool-call time, not here).
2. Determine the session key:
   - HTTP transport: read the `Mcp-Session-Id` header from
     `context.extensions.get::<http::request::Parts>()` and use its string value as the key.
   - Stdio transport: use the empty string `""` as the key.
   - If the header is absent or not valid UTF-8, fall back to `""`.
3. Truncate the client name to 256 characters (by Unicode scalar value, not bytes) before
   storing. If truncation occurs, log a WARN-level message.
4. If `client_info.name` is non-empty after truncation, insert `(session_key, client_name)`
   into `self.client_type_map`.
5. Return `Ok(self.get_info())` — the `InitializeResult` MUST be identical to the default
   provided-method behavior.

**Testable**: Override compiles, returns the same `InitializeResult` as `get_info()`, populates
`client_type_map` for HTTP sessions, and does not error on empty `clientInfo.name`.

---

### FR-02: `client_type_map` on `UnimatrixServer`

`UnimatrixServer` MUST carry a `client_type_map: Arc<Mutex<HashMap<String, String>>>` field.

1. The map is initialized as empty on server construction.
2. The key MUST be the rmcp transport-level `Mcp-Session-Id` UUID string (from the HTTP header)
   for HTTP sessions, and the empty string `""` for the stdio session.
3. The value MUST be the truncated `clientInfo.name` string.
4. All accesses (read and write) MUST use `Mutex::lock().unwrap_or_else(|e| e.into_inner())`
   (poison recovery pattern, consistent with existing `CategoryAllowlist` usage).
5. The stdio key `""` represents a singleton: if `initialize` is called again on the same
   stdio server instance (reconnect scenario), the value is overwritten. A debug-level log
   entry MUST be emitted when the `""` key is overwritten.

**Testable**: Map is present and accessible; concurrent insert and lookup from two goroutines
produces no data race.

---

### FR-03: `build_context_with_external_identity()` — Seam 2 Overload

A new method MUST be added to `UnimatrixServer`:

```rust
pub(crate) async fn build_context_with_external_identity(
    &self,
    params_agent_id: &Option<String>,
    format: &Option<String>,
    session_id: &Option<String>,
    request_context: &RequestContext<RoleServer>,
    external_identity: Option<&ResolvedIdentity>,
) -> Result<ToolContext, rmcp::ErrorData>
```

Behavior:

1. Extract the rmcp session key from `request_context` using the same header extraction
   logic as FR-01 (HTTP: `Mcp-Session-Id` header; missing/stdio: `""`).
2. Look up `client_type` in `self.client_type_map` using the extracted session key. The
   result is `Option<String>` — `None` when no entry exists.
3. When `external_identity` is `Some`, bypass `resolve_agent()` entirely and use the
   provided identity (W2-3 activation path). When `None`, call `resolve_agent()` exactly
   as the existing `build_context()` does.
4. Attach `client_type` (as `Option<String>`) to the returned `ToolContext`.
5. The existing `build_context()` MUST either be removed or marked `#[deprecated]` after
   all 12 call sites are migrated to `build_context_with_external_identity()`. The
   choice between removal and deprecation is left to the implementation agent, with the
   constraint that any remaining `build_context()` call site MUST produce a compile-time
   error or warning sufficient to detect missed migrations.

**Testable**: The method compiles with `external_identity = None`, produces `ToolContext` with
correct `client_type`, and the old `build_context()` path is fully superseded.

---

### FR-04: All 12 Tool Handlers Migrated to Seam 2

Every tool handler in `tools.rs` MUST call `build_context_with_external_identity()` in place
of `build_context()`. The migration is mechanical (O(n)) — each handler passes
`&tool_call_context.request_context` and `None` for `external_identity`.

There are exactly 12 tools: `context_search`, `context_lookup`, `context_get`, `context_store`,
`context_correct`, `context_deprecate`, `context_status`, `context_briefing`,
`context_quarantine`, `context_enroll`, `context_retrospective`, `context_cycle`.

**Testable**: The old `build_context()` function is absent or deprecated; all 12 tools compile
and pass existing tests.

---

### FR-05: ASS-050 Four-Column Schema Migration (v24 → v25)

The migration logic in `migration.rs` MUST add a new migration step for schema version 24 → 25
that executes the following SQL operations in order, each guarded by a `pragma_table_info`
existence check (idempotency pattern — see established codebase pattern):

**Column additions (each preceded by pragma_table_info guard):**

```sql
ALTER TABLE audit_log ADD COLUMN credential_type   TEXT NOT NULL DEFAULT 'none';
ALTER TABLE audit_log ADD COLUMN capability_used   TEXT NOT NULL DEFAULT '';
ALTER TABLE audit_log ADD COLUMN agent_attribution TEXT NOT NULL DEFAULT '';
ALTER TABLE audit_log ADD COLUMN metadata          TEXT NOT NULL DEFAULT '{}';
```

**Index additions:**

```sql
CREATE INDEX IF NOT EXISTS idx_audit_log_session ON audit_log(session_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_cred    ON audit_log(credential_type);
```

**Append-only DDL triggers:**

```sql
CREATE TRIGGER IF NOT EXISTS audit_log_no_update BEFORE UPDATE ON audit_log
    BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: UPDATE not permitted'); END;

CREATE TRIGGER IF NOT EXISTS audit_log_no_delete BEFORE DELETE ON audit_log
    BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: DELETE not permitted'); END;
```

`CURRENT_SCHEMA_VERSION` in `migration.rs` MUST be incremented from 24 to 25. The
`create_tables_if_needed()` DDL in `db.rs` MUST also be updated to include all four new
columns in the `CREATE TABLE IF NOT EXISTS audit_log` statement, so fresh databases are
created with the full schema.

**All four column additions MUST land in a single schema version bump.** Do not split them
across multiple migration steps.

**Testable**: After migration, `pragma_table_info('audit_log')` returns all four new columns;
existing rows have the documented default values; no data loss.

---

### FR-06: `AuditEvent` Struct — Four New Fields

The `AuditEvent` struct in `unimatrix-store/src/schema.rs` MUST gain four new fields:

```rust
#[serde(default)]
pub credential_type: String,    // sentinel: "none"

#[serde(default)]
pub capability_used: String,    // sentinel: ""

#[serde(default)]
pub agent_attribution: String,  // sentinel: ""

#[serde(default)]
pub metadata: String,           // sentinel: "{}"
```

All four fields use `#[serde(default)]` (empty string / "none" / "{}" from `Default::default()`)
to ensure forward-compatibility when deserializing `AuditEvent` records that predate this
migration. No nullable fields — this is consistent with the existing `session_id` pattern.

**Testable**: Existing `AuditEvent` deserialization from an 8-field JSON record succeeds with
default values for all four new fields.

---

### FR-07: `log_audit_event` — Updated INSERT

The `log_audit_event` method in `audit.rs` MUST be updated to:

1. Include all four new columns in the INSERT column list.
2. Bind all four new field values as positional parameters `?9` through `?12`.
3. The `metadata` value MUST never be empty string — callers that supply no metadata MUST
   pass `"{}"`.

**Testable**: A round-trip test (`log_audit_event` then `read_audit_event`) confirms all four
fields are stored and retrieved correctly.

---

### FR-08: `read_audit_event` — Updated SELECT

The `read_audit_event` method in `audit.rs` MUST be updated to:

1. Include all four new column names in the SELECT projection.
2. Map each column to its corresponding `AuditEvent` field.
3. Use the correct column name strings (`"credential_type"`, `"capability_used"`,
   `"agent_attribution"`, `"metadata"`).

**Testable**: `read_audit_event` on a newly-inserted row returns the correct values for all
four new fields.

---

### FR-09: `AuditEvent` Construction — All Four Fields Populated

At every site in `tools.rs` where `AuditEvent { ... }` is constructed, all four new fields
MUST be populated:

| Field | vnc-014 value | Notes |
|-------|---------------|-------|
| `credential_type` | `"none"` | Literal string. W2-2 updates this to `"static_token"` for HTTP bearer sessions. |
| `capability_used` | Capability gate string constant for this tool | See canonical values in Domain Models. |
| `agent_attribution` | `ctx.client_type.clone().unwrap_or_default()` | `clientInfo.name` from connection layer, or `""`. |
| `metadata` | `{"client_type":"<value>"}` or `"{}"` | See FR-10. |

**Testable**: Audit rows for each of the 12 tool operations contain the correct
`capability_used` value for that operation.

---

### FR-10: `metadata` JSON Construction

The `metadata` field value MUST be constructed using a proper JSON serializer — specifically
`serde_json::json!` macro or an equivalent `serde_json` API call. Format-string construction
(e.g., `format!(r#"{{"client_type":"{}"}}"#, ct)`) is EXPLICITLY PROHIBITED regardless of
any escaping applied, because format strings cannot safely encode all valid Unicode string
values (backslash sequences, control characters, embedded `"}` sequences, and similar inputs
produce invalid or injected JSON that evades single-character `"` → `\"` escaping).

Construction MUST follow this pattern:

- If `ctx.client_type` is `Some(ct)` and `ct` is non-empty:
  - Construct: `serde_json::json!({"client_type": ct}).to_string()`
  - This produces a well-formed JSON object with `ct` correctly serialized as a JSON string
    value, regardless of its content.
- Otherwise: use the literal string `"{}"`.

The `metadata` field MUST be a valid JSON object string. It MUST NOT be `NULL` or an empty
string. The initial key is `client_type`; the format is extensible for W2-2 (`token_fingerprint`).

**Testable**: The `metadata` string parses as valid JSON for all inputs, including values
containing backslashes, newlines, embedded quotes, and `"}` sequences; the `client_type` key
is present and its deserialized value equals the original `ct` string exactly.

---

### FR-11: Append-Only Enforcement — `gc_audit_log` Replacement

The `gc_audit_log` method in `retention.rs` issues a `DELETE FROM audit_log WHERE timestamp < ...`
statement. After the append-only triggers are installed, this DELETE will be rejected by the
`audit_log_no_delete` trigger.

`gc_audit_log` MUST be updated to use `DROP TABLE + CREATE TABLE + re-insert` or, preferably,
deactivated: the method body MUST be replaced with a no-op returning `Ok(0)` and a `tracing::warn!`
noting that `audit_log` is now append-only and time-based GC is deferred to a future retention
policy feature.

The `import::drop_all_data` function in `unimatrix-server/src/import/mod.rs` issues
`DELETE FROM audit_log` as part of a bulk data drop for the import path. This MUST be replaced
with a pattern that recreates the `audit_log` table (DROP + re-initialize) or uses a test-only
flag to disable the trigger temporarily. The implementation agent MUST choose one of:

- **Option A (preferred)**: Replace the `DELETE FROM audit_log` in `drop_all_data` with
  `DROP TABLE audit_log; <create_tables DDL>` (or call a store method that does this).
- **Option B**: Add a `disable_audit_triggers(conn)` helper used only in test/import contexts,
  guarded by a `#[cfg(test)]` or explicit import-mode flag.

The choice is implementation-agent's decision; it MUST be documented in the delivery PR.

All existing tests that issue raw `INSERT INTO audit_log` (in `retention.rs` tests) MUST be
updated to not use the `DELETE` path for setup/teardown. Test isolation MUST use in-memory
databases (`:memory:`) which are reset by database destruction, not row deletion.

**Testable**: After migration, attempting `DELETE FROM audit_log` on a non-test connection raises
`SqliteError` with message containing `audit_log is append-only`.

---

### FR-12: Concurrent HTTP Session Isolation

When two concurrent HTTP sessions with different `clientInfo.name` values are active
simultaneously, `agent_attribution` in audit rows MUST reflect the correct client for each
session. There MUST be no cross-session attribution bleed.

**Testable**: A concurrent test with two sessions (different `Mcp-Session-Id` UUIDs, different
`clientInfo.name` values) confirms that audit rows from each session carry the correct
`agent_attribution`.

---

## Non-Functional Requirements

### NFR-01: Concurrency Safety

`client_type_map` uses `Arc<Mutex<HashMap<String, String>>>`. The `Mutex` ensures safety under
concurrent HTTP session initialization. Lock acquisition MUST use `unwrap_or_else(|e| e.into_inner())`
for poison recovery. This is acceptable for vnc-014's concurrency bound (low concurrent session
count in current deployments). High-throughput HTTP concurrency (`DashMap` or per-session state)
is deferred to W2-2 per SR-01.

### NFR-02: `clientInfo.name` Truncation

`clientInfo.name` values exceeding 256 Unicode scalar values MUST be truncated. The truncation
MUST be character-boundary-safe (iterate chars, take 256, collect). Byte-level truncation is
forbidden (risk of splitting multi-byte sequences). Values that are truncated MUST be logged at
WARN level before insertion.

### NFR-03: Zero Behavioral Regression for Existing Tools

Tools that receive no HTTP session context (stdio path, or HTTP calls where no entry exists in
`client_type_map`) MUST continue to function correctly. The absence of a `client_type_map` entry
MUST result in:
- `agent_attribution = ""`
- `metadata = "{}"`
- `credential_type = "none"`
- `capability_used = <tool's capability gate string>`

No tool call MUST fail or produce an error due to missing session context.

### NFR-04: Migration Idempotency

Each `ALTER TABLE audit_log ADD COLUMN` statement MUST be preceded by a
`pragma_table_info('audit_log')` existence check, following the established pattern in
`migration.rs` (lines 184, 215, 315, 464, 512, 538, 563). All four checks MUST execute
before any `ALTER TABLE` statement executes, so that a partially-migrated database (crash
between first and second ALTER) will correctly skip already-added columns on re-run rather
than failing.

### NFR-05: Schema Version Integrity

`CURRENT_SCHEMA_VERSION` in `migration.rs` MUST be 25 after this feature. The existing
`test_schema_version_initialized_to_current_on_fresh_db` test validates this automatically
by comparing the counter value to `CURRENT_SCHEMA_VERSION`.

### NFR-06: `metadata` is Never NULL

The `metadata` column is `NOT NULL DEFAULT '{}'`. The `AuditEvent.metadata` field MUST never
hold an empty string or NULL-equivalent at the time of INSERT. The minimum value is `"{}"`.
This diverges from the `graph_edges.metadata` column (which is `DEFAULT NULL`) — the audit
log demands stricter non-nullability for compliance auditability.

### NFR-07: `InitializeResult` Parity

The overridden `initialize` method MUST return `Ok(self.get_info())`. No additional capability
negotiation, session registration, or response mutation is permitted. The override adds only
the `client_type_map` side-effect.

### NFR-08: No Tool Schema Changes

No `#[tool]` attribute struct in `tools.rs` gains a new field. The agent-facing MCP tool
parameter schema is unchanged. Attribution is transport-layer only.

---

## Acceptance Criteria

### AC-01: HTTP Session Attribution — Named Client

**Verification**: Integration test.

When an MCP client sends `initialize` with `clientInfo.name = "codex-mcp-client"` over an HTTP
session with `Mcp-Session-Id = "<uuid>"`, subsequent tool calls in the same session (same
`Mcp-Session-Id` header) produce `audit_log` rows where:
- `agent_attribution = "codex-mcp-client"`
- `metadata` column value, parsed as JSON, contains key `"client_type"` with value
  `"codex-mcp-client"`.

---

### AC-02: Empty `clientInfo.name` — No Attribution Written

**Verification**: Unit test on `initialize` handler.

When `clientInfo.name` is `""` (empty string), no entry is inserted into `client_type_map`.
Subsequent audit rows for that session have `agent_attribution = ""` and `metadata = "{}"`.
No `client_type` key appears in the `metadata` JSON object.

---

### AC-03: No Session Context — Zero Regression

**Verification**: Existing test suite passes unmodified (except infrastructure changes for
append-only triggers).

When no rmcp session context is available (stdio path where `client_type_map[""]` has no
entry, or a tool call with no matching session key), audit rows have:
- `agent_attribution = ""`
- `metadata = "{}"`
- `credential_type = "none"`

Tool calls do not error. All 12 tools remain functional.

---

### AC-04: Four Columns Present After Migration

**Verification**: Migration integration test using `pragma_table_info`.

After running `migrate_if_needed` on a schema-version-24 database:
- `pragma_table_info('audit_log')` returns rows for `credential_type`, `capability_used`,
  `agent_attribution`, and `metadata`.
- All four columns are `NOT NULL`.
- Default values match: `credential_type` → `'none'`, `capability_used` → `''`,
  `agent_attribution` → `''`, `metadata` → `'{}'`.
- Existing rows' new columns carry the documented defaults (no data loss, no NULL rows).

---

### AC-05: `AuditEvent` Round-Trip

**Verification**: Unit test in `audit.rs` (or `store` integration tests).

`log_audit_event` followed by `read_audit_event` correctly round-trips all four new fields.
Specifically:
- `credential_type = "none"` is stored and retrieved as `"none"`.
- `capability_used = "write"` is stored and retrieved as `"write"`.
- `agent_attribution = "codex-mcp-client"` is stored and retrieved as `"codex-mcp-client"`.
- `metadata = "{\"client_type\":\"codex-mcp-client\"}"` is stored and retrieved as the same
  string and parses as valid JSON.

---

### AC-05b: Append-Only Triggers Installed and Enforced

**Verification**: Integration test.

After migration, executing `DELETE FROM audit_log WHERE event_id = 1` via `sqlx::query` MUST
return an `Err` whose message contains `"audit_log is append-only: DELETE not permitted"`.
Executing `UPDATE audit_log SET detail = 'x' WHERE event_id = 1` MUST return an `Err` whose
message contains `"audit_log is append-only: UPDATE not permitted"`.

This criterion ALSO requires that `gc_audit_log` (FR-11) has been updated to not issue DELETE
statements, and `import::drop_all_data` has been updated per FR-11.

**Risk SR-07 mitigation**: All test sites that previously used `DELETE FROM audit_log` for
setup/teardown MUST be replaced with in-memory database recreation before this AC can pass.

---

### AC-06: `InitializeResult` Unchanged

**Verification**: Unit test comparing `initialize()` return value to `get_info()` return value.

The `initialize` override returns `Ok(server_info)` where `server_info` is bit-for-bit
identical to the result of calling `self.get_info()` directly. No capability fields, protocol
version, or instruction text differ.

---

### AC-07: Concurrent HTTP Session Isolation

**Verification**: Concurrent integration test.

Two simultaneous HTTP sessions with distinct `Mcp-Session-Id` UUIDs and distinct `clientInfo.name`
values (e.g., `"codex-mcp-client"` and `"gemini-cli-mcp-client"`) produce audit rows where:
- Session A rows have `agent_attribution = "codex-mcp-client"`.
- Session B rows have `agent_attribution = "gemini-cli-mcp-client"`.
- No row from session A carries session B's attribution, and vice versa.

**Risk SR-01 mitigation documented**: The test MUST include a comment that `Mutex` is acceptable
for current concurrency levels and that `DashMap` migration is tracked for W2-2.

---

### AC-08: Stdio Session Attribution

**Verification**: Integration test using stdio-mode server construction.

When the stdio transport is in use and `initialize` is called with a non-empty `clientInfo.name`,
the value is stored in `client_type_map` under key `""`. Subsequent tool calls (all keyed to
the same `""` entry) produce audit rows with `agent_attribution` set to the captured
`clientInfo.name`.

---

### AC-09: Schema Version Bumped, No Data Loss

**Verification**: Existing `test_schema_version_initialized_to_current_on_fresh_db` passes.
Additional migration test on an existing v24 database.

After migration on a database with pre-existing `audit_log` rows:
- `SELECT value FROM counters WHERE name = 'schema_version'` returns `25`.
- Row count in `audit_log` is unchanged.
- All pre-existing rows have valid (non-NULL, default-value) data in the four new columns.

---

### AC-10: `clientInfo.name` Truncated at 256 Characters

**Verification**: Unit test.

When `clientInfo.name` contains 300 characters (all ASCII for simplicity), the value stored in
`client_type_map` and subsequently in `agent_attribution` is exactly 256 characters. A WARN-level
log entry is emitted during `initialize` handling. The original 300-character value is not stored.

---

### AC-11: `capability_used` Values Are Canonical

**Verification**: Code review and unit test per tool.

For each of the 12 tool handlers, `AuditEvent.capability_used` contains the canonical lowercase
string corresponding to the `Capability` enum variant the tool gate checks (see Domain Models,
`credential_type` enum and `capability_used` canonical values). No ad-hoc or free-form strings
are used. The implementation MUST derive these strings from a shared constant or via
`Capability::as_str()` (a method to be added to the `Capability` enum), not from inline
string literals that can diverge.

**Risk SR-05 mitigation**: `Capability::as_str()` (or an equivalent constant table) MUST be
added to prevent string drift across tool sites.

---

### AC-12: `build_context()` Fully Superseded

**Verification**: Compile-time (removal or `#[deprecated]`).

After migration of all 12 tool handlers to `build_context_with_external_identity()`, the
original `build_context()` function is either:
- Removed entirely (preferred — compile-time enforcement), or
- Marked `#[deprecated(note = "use build_context_with_external_identity")]` producing a
  compiler warning for any remaining call site.

No production call site invokes the old `build_context()` function.

---

## Domain Models

### `AuditEvent` Struct (Post-Migration, 12 Fields)

```
AuditEvent {
    event_id:          u64     // auto-assigned monotonic counter
    timestamp:         u64     // Unix seconds, set at log time
    session_id:        String  // agent-declared session_id tool param, prefixed "mcp::"
                               // sentinel: "" (no session declared)
    agent_id:          String  // agent-declared identity (spoofable; for routing)
    operation:         String  // tool name, e.g. "context_store"
    target_ids:        Vec<u64> // entry IDs affected
    outcome:           Outcome  // Success | Failure | Error
    detail:            String  // human-readable outcome detail

    // ASS-050 / vnc-014 additions:
    credential_type:   String  // how the caller authenticated; sentinel: "none"
    capability_used:   String  // capability gate checked; sentinel: ""
    agent_attribution: String  // transport-attested client identity (non-spoofable)
                               // source: clientInfo.name (OSS); JWT sub (enterprise, W2-3)
                               // NOT the tool-param agent_id
                               // sentinel: ""
    metadata:          String  // JSON object, extensible; minimum value: "{}"
                               // vnc-014 keys: {"client_type":"<name>"}
                               // W2-2 keys: {"client_type":"...", "token_fingerprint":"..."}
}
```

**Semantic distinction — two attribution fields:**

| Field | Source | Spoofable? | Purpose |
|-------|--------|-----------|---------|
| `agent_id` | Tool parameter, agent-declared | Yes | Routing, session keying, Unimatrix logic |
| `agent_attribution` | Transport layer (`clientInfo.name` or JWT sub) | No | Compliance, audit, ISO 42001 evidence |

These fields MUST NOT be conflated. Downstream consumers (audit queries, compliance tooling)
MUST use `agent_attribution` for compliance evidence and `agent_id` for routing/logic.

---

### `credential_type` — Canonical Values

| Value | Meaning | When set |
|-------|---------|----------|
| `"none"` | Stdio transport; no credential | All vnc-014 connections (OSS default) |
| `"static_token"` | HTTP bearer token (OSS HTTPS) | W2-2 bearer middleware |
| `"jwt"` | Enterprise JWT (sub+aud) | W2-3 enterprise tier |

vnc-014 MUST write `"none"` for all rows. The string `"none"` is the canonical sentinel
(not empty string) to distinguish "credential system present, no credential" from
"field not populated".

---

### `capability_used` — Canonical Values

Derived from `Capability` enum (`unimatrix-store/src/schema.rs`). Values are lowercase:

| `Capability` variant | `capability_used` string | Tools |
|----------------------|--------------------------|-------|
| `Capability::Search` | `"search"` | `context_search`, `context_lookup`, `context_briefing` |
| `Capability::Read` | `"read"` | `context_get`, `context_status`, `context_retrospective` |
| `Capability::Write` | `"write"` | `context_store`, `context_correct`, `context_deprecate`, `context_quarantine`, `context_cycle` |
| `Capability::Admin` | `"admin"` | `context_enroll` |

The mapping MUST be enforced via `Capability::as_str()` (a new method on the `Capability` enum)
or a shared constant array — not inline string literals at each tool site.

---

### `client_type_map` Lifecycle

```
Server startup
    └── client_type_map = Arc::new(Mutex::new(HashMap::new()))

initialize(request, context) [per session]
    ├── extract client_name = request.client_info.name
    ├── if non-empty:
    │       truncate to 256 chars
    │       session_key = Mcp-Session-Id header OR "" (stdio)
    │       map.lock().insert(session_key, client_name)
    └── return get_info()

tool_call(params, request_context)
    ├── session_key = Mcp-Session-Id header OR ""
    ├── client_type = map.lock().get(session_key).cloned()
    └── build_context_with_external_identity(..., client_type → ToolContext)

ToolContext.client_type: Option<String>
    └── AuditEvent.agent_attribution = client_type.unwrap_or_default()
    └── AuditEvent.metadata = {"client_type":"<value>"} or "{}"
```

---

### `SessionRegistry` — Not Modified

`SessionRegistry` is keyed on the agent-declared `session_id` tool parameter (prefixed `mcp::`).
This is a distinct namespace from the rmcp `Mcp-Session-Id`. VNC-014 does NOT modify
`SessionRegistry` or `SessionState`. `client_type` is resolved directly from `client_type_map`
using the rmcp session ID at `build_context_with_external_identity()` call time.

---

### Known `clientInfo.name` Values

| Client | `clientInfo.name` value | Source |
|--------|------------------------|--------|
| Claude Code | `"claude-code"` | Confirmed: v2.1.117 binary (OQ-02 resolved) |
| Codex CLI | `"codex-mcp-client"` | Inferred; live capture needed for confirmation |
| Gemini CLI | `"gemini-cli-mcp-client"` | Inferred; live capture needed for confirmation |

The implementation MUST treat `clientInfo.name` as an opaque string — no allowlist or
normalization. Whatever the client sends is stored (after truncation).

---

## User Workflows

### Workflow A: Codex CLI Tool Call

1. Codex CLI connects to Unimatrix MCP server over HTTP.
2. Codex sends `initialize` with `clientInfo.name = "codex-mcp-client"` and receives a new
   `Mcp-Session-Id` UUID in the response header.
3. `UnimatrixServer::initialize` stores `("uuid", "codex-mcp-client")` in `client_type_map`.
4. Codex sends `context_search` with `Mcp-Session-Id: uuid` header.
5. `build_context_with_external_identity` reads `"codex-mcp-client"` from `client_type_map`.
6. `AuditEvent` is constructed with `agent_attribution = "codex-mcp-client"` and
   `metadata = '{"client_type":"codex-mcp-client"}'`.
7. The audit row is written to `audit_log`. Compliance tooling can now identify Codex sessions
   without relying on agent-declared `agent_id`.

### Workflow B: Claude Code Stdio Session

1. Claude Code connects to Unimatrix MCP server over stdio.
2. `initialize` is called with `clientInfo.name = "claude-code"`.
3. `UnimatrixServer::initialize` stores `("", "claude-code")` in `client_type_map`.
4. All subsequent tool calls (no HTTP session header) look up key `""` and find `"claude-code"`.
5. Audit rows carry `agent_attribution = "claude-code"`.

### Workflow C: No `initialize` Before Tool Call (Edge Case)

1. A test or partial client sends a tool call without a prior `initialize`.
2. `client_type_map` has no entry for the session key.
3. `build_context_with_external_identity` returns `client_type = None`.
4. `AuditEvent` is constructed with `agent_attribution = ""` and `metadata = "{}"`.
5. The tool call proceeds normally. No error is returned.

---

## Constraints

### C-01: rmcp 0.16.0 API Surface

The `ServerHandler::initialize` trait method signature is:
```rust
fn initialize(
    &self,
    request: InitializeRequestParams,
    context: RequestContext<RoleServer>,
) -> impl Future<Output = Result<InitializeResult, McpError>> + Send + '_;
```
The override MUST match this signature exactly. Lifetime bounds and `Send` requirement are
non-negotiable. `clientInfo.name` is accessed as `request.client_info.name` (a `String` field).

### C-02: Stdio vs HTTP Session Semantics

Stdio has no `Mcp-Session-Id` header and no HTTP session concept. The `""` key sentinel
represents a single-connection invariant — one stdio server serves one client for its lifetime.
If `client_type_map[""]` would be overwritten (reconnect), a debug-level log MUST be emitted.
This edge case does not require error handling beyond logging.

### C-03: `serde(default)` Requirement

All four new `AuditEvent` fields MUST carry `#[serde(default)]`. This ensures that
`AuditEvent` values deserialized from JSON representations that predate this migration
(e.g., export/import JSONL files, test fixtures) do not fail deserialization.

### C-04: `metadata` Non-Nullable

`metadata` is `NOT NULL DEFAULT '{}'` at the DDL level and `String` (never `Option<String>`) in
the `AuditEvent` struct. The minimum valid value is the string `"{}"`. This diverges from
`graph_edges.metadata` (which is nullable). The difference is intentional: audit records are
compliance artifacts and MUST NOT have null fields.

### C-05: `agent_attribution` Is Connection-Layer Only

`agent_attribution` MUST NOT be populated from any tool parameter. It is set only from
`client_type_map`, which is populated only from `ServerHandler::initialize`. There is no
mechanism for an agent to influence `agent_attribution` via tool call parameters.

### C-06: `ResolvedIdentity` Stub

`ResolvedIdentity` is introduced as a type for the Seam 2 function signature. Its definition
in vnc-014 MUST be a minimal stub sufficient to compile (e.g., an empty struct or a
zero-field type alias). W2-3 will replace this stub with the full JWT validation type.
The architect must decide whether to define it in `unimatrix-server` or `unimatrix-core`.

### C-07: Single Schema Version Bump

All four `ALTER TABLE` statements MUST be part of the single v24 → v25 migration. They MUST
NOT be split across two version bumps. Rationale: ASS-050 specifies these as a coordinated set;
partial migration creates an intermediate schema state that no code version supports.

---

## Dependencies

### Crates

| Crate | Role | Change |
|-------|------|--------|
| `unimatrix-store` | `AuditEvent` struct, `audit_log` DDL, migration, `gc_audit_log` | Modified |
| `unimatrix-server` | `UnimatrixServer`, `tools.rs`, `server.rs` | Modified |
| `rmcp 0.16.0` | `ServerHandler::initialize`, `RequestContext`, `InitializeRequestParams` | Read-only (no version change) |
| `http` (via rmcp/tower) | `http::request::Parts` for header extraction | Existing transitive dep |
| `std::collections::HashMap` | `client_type_map` backing store | std |
| `std::sync::{Arc, Mutex}` | Thread-safe shared state | std |

### External Services

None. This feature is entirely within the Unimatrix server binary.

### Existing Components

| Component | Dependency |
|-----------|-----------|
| `migration.rs` `run_main_migrations()` | Must add v24→v25 step |
| `db.rs` `create_tables_if_needed()` | Must add four new columns to DDL |
| `audit.rs` `log_audit_event`, `read_audit_event` | Must be updated (FR-07, FR-08) |
| `retention.rs` `gc_audit_log` | Must be updated (FR-11) |
| `import/mod.rs` `drop_all_data` | Must be updated (FR-11) |
| `server.rs` `build_context` | Must be superseded by `build_context_with_external_identity` |
| `tools.rs` (all 12 handlers) | Must migrate to Seam 2 (FR-04, FR-09) |
| `schema.rs` `Capability` | Must add `as_str()` method (AC-11) |

---

## NOT In Scope

- **`cycle_events` gap for Codex CLI.** `cycle_events` is populated only via the hook path.
  Server-side attribution via `clientInfo.name` does not populate `cycle_events` for clients
  that do not fire hooks. This gap is explicit non-goal.
- **vnc-013 hook normalization.** The Gemini canonical event name layer is a separate feature.
- **OAuth JWT identity (`credential_type = "jwt"`).** The enterprise tier JWT sub+aud
  superseding `clientInfo.name` is a W2-3 deliverable.
- **`client_type` as a tool parameter.** Attribution is server-side and transparent to agents.
  No tool parameter schema changes.
- **Retention, querying, or reporting on `client_type`.** The field is written; analytics are
  deferred.
- **`Mcp-Session-Id` awareness on stdio transport beyond the `""` sentinel.** Stdio has no
  per-connection UUID.
- **`DashMap` or other high-throughput concurrency for `client_type_map`.** Deferred to W2-2
  per SR-01 acceptance.
- **`context_cycle_review` or `context_status` behavioral changes.** This feature adds columns
  to `audit_log` only; no changes to query logic, health metrics, or retrospective pipelines.

---

## Open Questions

**OQ-A (for architect):** Where should `ResolvedIdentity` be defined — `unimatrix-server` as
a server-internal type, or `unimatrix-core` as a shared domain type? `unimatrix-core` is
preferable if W2-3 needs it in multiple crates; `unimatrix-server` keeps scope minimal for
vnc-014. Decision affects crate dependency graph.

**OQ-B (for architect):** For FR-11 (`gc_audit_log` replacement), should time-based audit log
GC be deferred entirely (returning `Ok(0)`) or should it be replaced with an alternative
mechanism that does not violate append-only? The current callers of `gc_audit_log` (if any,
beyond tests) need to be audited. If no production caller invokes it today, no-op is safe.

**OQ-C (for implementation agent):** The `drop_all_data` function in `import/mod.rs` is used
for full import resets. With append-only triggers, this function breaks. Option A (DROP +
recreate) is preferred but requires access to the full CREATE TABLE DDL from within the import
crate. Confirm the import crate has a dependency path to the DDL initialization logic, or
whether a new `SqlxStore::reset_for_import()` method in `unimatrix-store` is required.

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned 14 entries. Most relevant: entry #4047
  (AuditEvent 5-surface update pattern), entry #317 (AuditContext construction pattern),
  entry #296 (transport-agnostic service extraction procedure). Confirmed alignment with
  established codebase conventions for schema migration (pragma_table_info guard),
  AuditEvent extension, and session prefixing patterns.
