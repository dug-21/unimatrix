# Test Plan: AuditEvent + audit.rs (unimatrix-store)

## Component Summary

`AuditEvent` in `unimatrix-store/src/schema.rs` gains four new fields with `#[serde(default)]`
and a new `impl Default`. The `log_audit_event` INSERT and `read_audit_event` SELECT in
`audit.rs` are updated to bind/read `?9`–`?12`.

Key correctness requirements:
- `Default::default()` yields `credential_type = "none"`, `metadata = "{}"` (not empty string)
- `#[serde(default)]` yields `""` for missing fields (intentionally different from `Default`)
- Round-trip via `log_audit_event` → `read_audit_event` is lossless for all four fields

---

## Unit Tests

### AE-U-01: `AuditEvent::default()` — correct sentinel values for all four fields

**Risk**: R-06, R-13
**Arrange**: Call `AuditEvent::default()`.
**Assert**:
- `ae.credential_type == "none"` (NOT empty string)
- `ae.capability_used == ""`
- `ae.agent_attribution == ""`
- `ae.metadata == "{}"` (NOT empty string)

---

### AE-U-02: `#[serde(default)]` — deserializing 8-field legacy JSON gives empty-string defaults

**Risk**: R-13, AC-05 (serde path)
**Arrange**: Prepare a JSON string representing an 8-field `AuditEvent` (no `credential_type`,
`capability_used`, `agent_attribution`, or `metadata` keys). This simulates a pre-v25 audit
record deserialization.
**Act**: `serde_json::from_str::<AuditEvent>(&json)`
**Assert**:
- Deserializes successfully (no error)
- `ae.credential_type == ""` (serde default = `String::default()`, NOT `"none"`)
- `ae.capability_used == ""`
- `ae.agent_attribution == ""`
- `ae.metadata == ""` (serde default = `String::default()`, NOT `"{}"`)

**Important distinction**: The serde defaults (`""`) differ from the construction defaults
(`"none"`, `"{}"`). This is intentional and must be documented. Tests must NOT assert that
serde-deserialized fields equal the `Default` impl values.

---

### AE-U-03: `AuditEvent` struct has exactly 12 fields

**Risk**: R-02, R-12
**Assert**: The `AuditEvent` struct definition contains exactly 12 named fields. This is
verified by code inspection and confirmed by the column count test in migration.md.

---

### AE-U-04: `Default` impl used at non-tool-call sites — struct update syntax

**Risk**: R-12
**Arrange**: Verify that `AuditEvent { ..., ..AuditEvent::default() }` syntax is used at
background.rs and uds/listener.rs construction sites.
**Assert**: Compilation succeeds. The four new fields are not explicitly listed at those sites
(they come from `..AuditEvent::default()`). If the `Default` impl is wrong, this test catches
it via AE-U-01.

---

## Integration Tests (unimatrix-store)

### AE-I-01: `log_audit_event` → `read_audit_event` round-trip — all four fields

**Risk**: R-06, AC-05
**Arrange**: Open an in-memory store. Construct an `AuditEvent` with:
- `credential_type = "none"`
- `capability_used = "write"`
- `agent_attribution = "codex-mcp-client"`
- `metadata = r#"{"client_type":"codex-mcp-client"}"#`
**Act**: `store.log_audit_event(event).await`; then `store.read_audit_event(event_id).await`.
**Assert**:
- Returned event has `credential_type == "none"`
- `capability_used == "write"`
- `agent_attribution == "codex-mcp-client"`
- `metadata == r#"{"client_type":"codex-mcp-client"}"#`
- `serde_json::from_str::<serde_json::Value>(&returned.metadata).is_ok()`

---

### AE-I-02: Round-trip with `metadata = "{}"` — minimum value preserved

**Risk**: R-06, NFR-06
**Arrange**: Construct `AuditEvent` with `metadata = "{}"`.
**Act**: Log and read back.
**Assert**:
- `returned.metadata == "{}"`
- `serde_json::from_str::<serde_json::Value>(&returned.metadata).is_ok()`

---

### AE-I-03: Round-trip with `AuditEvent::default()` — all sentinel defaults preserved

**Risk**: R-06, R-12
**Arrange**: `let event = AuditEvent { operation: "test_op".to_string(), ..AuditEvent::default() };`
**Act**: Log and read back.
**Assert**:
- `returned.credential_type == "none"`
- `returned.capability_used == ""`
- `returned.agent_attribution == ""`
- `returned.metadata == "{}"`

---

### AE-I-04: INSERT binds `?9`–`?12` correctly — no column count mismatch

**Risk**: R-12
**Arrange**: Open an in-memory store (fresh v25 schema). Construct a fully-populated
`AuditEvent` with all 12 fields.
**Act**: `store.log_audit_event(event).await`
**Assert**:
- Returns `Ok(_)` — no "expected N bind parameters, got M" error
- Row count in `audit_log` increments by 1

---

### AE-I-05: `metadata` JSON injection resistance — serde_json::json! handles special chars

**Risk**: R-08, SEC-02
**Arrange**: Test each of the following `client_type` values:
1. `r#"client"with"quotes"#` — embedded double quotes
2. `r"client\with\backslash"` — backslashes
3. `"client\nwith\nnewline"` — literal newlines
4. `r#"a","b":"c"#` — JSON injection attempt (EC-06)
**Act**: For each, construct `metadata` via `serde_json::json!({"client_type": ct}).to_string()`.
**Assert** for each:
- `serde_json::from_str::<serde_json::Value>(&metadata).is_ok()` — valid JSON
- Parsed `metadata["client_type"]` equals the original `ct` string exactly
- The JSON does NOT contain `{"client_type":"a","b":"c"}` for case 4 — the entire string
  is treated as one value

---

### AE-I-06: Empty string `clientInfo.name` — `metadata = "{}"`

**Risk**: R-06, AC-02
**Arrange**: `client_type = None` (empty `clientInfo.name` means no map entry).
**Act**: Construct metadata: `if ct.is_empty() { "{}".to_string() } else { serde_json::json!(...) }`.
**Assert**:
- `metadata == "{}"`
- No `client_type` key present in parsed JSON
