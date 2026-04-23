# Risk-Based Test Strategy: vnc-014

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Append-only triggers break existing DELETE paths — `gc_audit_log` and `drop_all_data` issue DELETE on `audit_log`; if not remediated before triggers land, production code fails at runtime | High | High | Critical |
| R-02 | Schema version cascade missed — 7+ touchpoints (sqlite_parity.rs column count, test files, `CURRENT_SCHEMA_VERSION` assertion) must all advance to v25; any missed touchpoint causes test or runtime failure | High | High | Critical |
| R-03 | Cross-session attribution bleed — `client_type_map` keyed on `Mcp-Session-Id`; if the key extraction returns `""` for an HTTP session (header absent or not-UTF-8), two distinct HTTP sessions collapse to the same map entry | High | Med | Critical |
| R-04 | Partially-migrated database on crash re-run — if process dies between first ALTER and schema_version bump, re-run must use pre-flight pragma_table_info guards; missing any guard causes `ALTER TABLE` to fail on already-added column | High | Med | High |
| R-05 | Missed `build_context()` call site — 10+ tool handlers must migrate; any site that still calls the old function silently uses the pre-migration path (if retained as wrapper) or fails to compile (if removed); the risk is architectural regression, not just missed attribution | High | Med | High |
| R-06 | `metadata` field written as empty string — `AuditEvent.metadata` must never be `""` (minimum `"{}"`); SQLite column default covers SQL-inserted rows but code-constructed `AuditEvent` values rely on the `Default` impl being correct | High | Med | High |
| R-07 | `ResolvedIdentity` stub breaks W2-3 seam — the stub type must be defined in the correct crate (`unimatrix-server` vs `unimatrix-core`); if defined in the wrong crate, W2-3 must move it, causing a breaking change in the Seam 2 signature | Med | Med | High |
| R-08 | `clientInfo.name` JSON injection in `metadata` — the format string `{"client_type":"<value>"}` escapes `"` but does not escape `\`, `\n`, or other JSON-special characters; a client name containing a backslash or newline produces invalid JSON | High | Med | High |
| R-09 | `Capability::as_audit_str()` exhaustive match — if a new `Capability` variant is added without updating `as_audit_str`, the match compiles only if the enum is `non_exhaustive`; a wildcard arm or future variant returning a wrong string is a silent correctness failure | Med | Low | Med |
| R-10 | Stdio key `""` overwrite silences second stdio client — a test or CI scenario that creates two sequential stdio connections overwrites attribution silently; only a WARN is logged; no error is returned | Low | Med | Med |
| R-11 | `db.rs` `create_tables_if_needed` DDL divergence — fresh databases use `create_tables_if_needed`; if it is not updated byte-identical to the migration DDL (four columns + triggers), fresh DB schema differs from migrated DB schema | High | Med | High |
| R-12 | Non-tool-call `AuditEvent` sites omit new fields — `background.rs` (lines 1197, 1252, 2267) and `uds/listener.rs` construct `AuditEvent` directly without `RequestContext`; if they do not populate the four new fields, INSERT binds `?9`–`?12` to incorrect or missing values | Med | Med | Med |
| R-13 | `serde(default)` insufficient for round-trip — `AuditEvent.metadata` `serde(default)` gives `""` (not `"{}"`); deserializing a legacy JSON audit event record and then re-inserting it would write `""` as metadata, violating NFR-06 | Med | Med | Med |
| R-14 | `initialize` override returns wrong `InitializeResult` — if the override does not call `self.get_info()` exactly, it may omit capabilities, change protocol version, or alter instruction text, breaking client negotiation | Med | Low | Med |
| R-15 | Stateless HTTP mode misclassified as stdio — in rmcp stateless mode (no session manager, no `Mcp-Session-Id` header), all tool calls fall back to key `""`; if a stdio session was already registered under `""`, HTTP stateless sessions inherit its attribution | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Append-Only Triggers Break Existing DELETE Paths

**Severity**: High
**Likelihood**: High
**Impact**: `gc_audit_log` or `drop_all_data` called post-migration raises SQLite `ABORT` error in production. Background tick crashes or import resets fail silently.

**Test Scenarios**:
1. After migration, call `gc_audit_log()` and verify it returns `Ok(0)` without issuing any DELETE (no-op or removed); confirm no SQLite ABORT error.
2. After migration, call `drop_all_data()` and verify it completes without error; confirm `audit_log` row count is unchanged (not reset to zero).
3. After migration, attempt raw `DELETE FROM audit_log WHERE event_id = 1` via `sqlx::query` and assert the error message contains `"audit_log is append-only: DELETE not permitted"`.
4. After migration, attempt raw `UPDATE audit_log SET detail = 'x' WHERE event_id = 1` and assert the error message contains `"audit_log is append-only: UPDATE not permitted"`.

**Coverage Requirement**: All DELETE and UPDATE paths on `audit_log` must be enumerated and tested. Triggers must be verified to fire before any production code path can silently succeed.

---

### R-02: Schema Version Cascade Missed

**Severity**: High
**Likelihood**: High
**Impact**: `test_schema_version_initialized_to_current_on_fresh_db` fails; sqlite_parity column count assertions fail; pre-existing migration range tests fail. Runtime: server panics on schema assertion.

**Test Scenarios**:
1. `CURRENT_SCHEMA_VERSION` constant in `migration.rs` equals 25 — asserted by the existing `test_schema_version_initialized_to_current_on_fresh_db` test.
2. `pragma_table_info('audit_log')` on a fresh database returns exactly 12 columns (8 existing + 4 new).
3. The existing v23→v24 migration test file (if using `at_least_N` naming pattern) is renamed correctly so the test suite range covers v24→v25.
4. `sqlite_parity.rs` column count assertion for `audit_log` matches 12.

**Coverage Requirement**: Schema version constant, fresh-DB column count, and cascade-file naming must all be exercised in CI.

---

### R-03: Cross-Session Attribution Bleed

**Severity**: High
**Likelihood**: Med
**Impact**: Codex session audit rows carry Gemini's `agent_attribution` (or vice versa), invalidating compliance evidence and audit integrity.

**Test Scenarios**:
1. Concurrent test: initialize two HTTP sessions with distinct `Mcp-Session-Id` UUIDs and distinct `clientInfo.name` values (`"codex-mcp-client"` and `"gemini-cli-mcp-client"`); execute a tool call on each; assert session A rows have `agent_attribution = "codex-mcp-client"` and session B rows have `agent_attribution = "gemini-cli-mcp-client"` with no cross-contamination.
2. Simulate header extraction failure (header absent, non-UTF-8 header value): assert fallback to `""` does not overwrite an existing HTTP session's attribution with the stdio sentinel.
3. A tool call with no prior `initialize` (empty `client_type_map`) produces `agent_attribution = ""` and `metadata = "{}"` — no error, no bleed.

**Coverage Requirement**: AC-07 concurrent session test is mandatory. The fallback path (`""` key for header-absent HTTP calls) must have its own scenario distinct from the stdio case.

---

### R-04: Partially-Migrated Database on Crash Re-Run

**Severity**: High
**Likelihood**: Med
**Impact**: Re-running migration after a crash between ALTER-1 and schema_version bump fails with `"duplicate column name"` if pragma_table_info guards are missing or run in wrong order.

**Test Scenarios**:
1. Directly execute the ALTER for `credential_type` on a v24 test database, then run `migrate_if_needed` — verify it completes successfully without error, adding only the remaining three columns.
2. Execute all four ALTERs on a v24 test database (simulating fully-applied DDL without version bump), then run `migrate_if_needed` — verify all four pragma checks skip their respective ALTERs and only the version bump and index/trigger creation proceed.
3. Run `migrate_if_needed` twice on the same v24 database — idempotent; second run is a no-op for the ALTER steps.

**Coverage Requirement**: Partial application states for each column position (1 of 4, 2 of 4, 3 of 4, all 4 applied) must each be safe on re-run.

---

### R-05: Missed `build_context()` Call Site

**Severity**: High
**Likelihood**: Med
**Impact**: Any tool handler retaining the old `build_context()` call silently omits `client_type` from `ToolContext`, producing `agent_attribution = ""` for all audit rows from that tool regardless of session.

**Test Scenarios**:
1. Compile-time check: `build_context()` must not exist in `server.rs` (removed per ADR-003) or must be `#[deprecated]` — verified by inspecting the final binary/source in CI.
2. For each of the 12 tool handlers, a unit or integration test that calls the tool with a populated `client_type_map` entry verifies `agent_attribution` is non-empty in the resulting audit row.
3. A grep/lint step confirms no production call site references `build_context` by name (can be asserted in a compile-time test or CI script check).

**Coverage Requirement**: All 12 tool handlers must have at least one test verifying correct `agent_attribution` propagation. The old function must be provably absent from production call sites.

---

### R-06: `metadata` Field Written as Empty String

**Severity**: High
**Likelihood**: Med
**Impact**: `metadata` constraint is `NOT NULL DEFAULT '{}'`; an empty string `""` satisfies NOT NULL but violates NFR-06's minimum value requirement and breaks downstream JSON parsing.

**Test Scenarios**:
1. `AuditEvent::default()` produces `metadata = "{}"` — not `""`. Unit test on the `Default` impl.
2. Round-trip test: `log_audit_event` with `metadata = "{}"` then `read_audit_event` returns `"{}"`, not `""` or `NULL`.
3. For the no-session path (tool call with no `client_type`): audit row `metadata` column contains `"{}"` parsed as valid JSON with no `client_type` key.
4. For the session path (tool call with `client_type = "codex-mcp-client"`): audit row `metadata` parses as valid JSON with `client_type = "codex-mcp-client"`.

**Coverage Requirement**: Every code path that constructs `AuditEvent.metadata` (session path, no-session path, non-tool-call default path) must be verified to produce valid, non-empty JSON.

---

### R-07: `ResolvedIdentity` Stub Breaks W2-3 Seam

**Severity**: Med
**Likelihood**: Med
**Impact**: If `ResolvedIdentity` is defined in `unimatrix-server` but W2-3 needs it in `unimatrix-core` (shared across crates), W2-3 must relocate the type and update `build_context_with_external_identity`'s signature, causing a breaking change.

**Test Scenarios**:
1. `ResolvedIdentity` is defined in its correct final crate (per architect decision in OQ-A); the `build_context_with_external_identity` function compiles with `external_identity: None` passed from all 12 handlers.
2. `external_identity: Some(identity)` code path (W2-3 activation) compiles and routes to a stub or bypasses `resolve_agent()` — even if the path is unreachable in vnc-014, the `Some` arm must compile correctly.

**Coverage Requirement**: The Seam 2 function must compile with both `None` and `Some` for `external_identity`. The `Some` arm must not `unreachable!()` — it must have compilable, if minimal, logic.

---

### R-08: `clientInfo.name` JSON Injection in `metadata`

**Severity**: High
**Likelihood**: Med
**Impact**: A `clientInfo.name` containing `\` or `"` produces invalid JSON in `metadata`, breaking downstream JSON parsing by compliance tooling. A name containing `\n` produces a JSON string with a raw newline, which is invalid per JSON spec.

**Test Scenarios**:
1. `clientInfo.name = "client\"with\"quotes"` → `metadata` parses as valid JSON with `client_type` value `"client\"with\"quotes"` (quotes correctly escaped).
2. `clientInfo.name = "client\\with\\backslash"` → `metadata` parses as valid JSON (backslash escaped as `\\`).
3. `clientInfo.name = "client\nwith\nnewline"` → `metadata` parses as valid JSON (newline escaped as `\n` in the JSON string).
4. `clientInfo.name = ""` → `metadata = "{}"` (no `client_type` key, valid JSON).

**Coverage Requirement**: The `metadata` construction function must be tested against all JSON-special characters that the simple `replace('"', "\\\"")` in the SCOPE.md pseudocode does NOT handle. A property-based or parametric test is appropriate here.

---

### R-09: `Capability::as_audit_str()` Exhaustive Match

**Severity**: Med
**Likelihood**: Low
**Impact**: A future `Capability` variant added without updating `as_audit_str` produces a compile error (desired) — but only if the match is exhaustive. If a wildcard arm exists, wrong values silently appear in `capability_used` audit fields.

**Test Scenarios**:
1. `Capability::Read.as_audit_str()` returns `"read"`, `Capability::Write.as_audit_str()` returns `"write"`, `Capability::Search.as_audit_str()` returns `"search"`, `Capability::Admin.as_audit_str()` returns `"admin"` — unit test per variant.
2. Confirm no wildcard (`_`) arm in the match expression — code review or compile-time test via `#[deny(unreachable_patterns)]`.
3. `capability_used` audit rows for each of the 12 tools contain the expected canonical string from the domain model table (SPECIFICATION.md § `capability_used` canonical values).

**Coverage Requirement**: All four `Capability` variants must be explicitly tested. The match must be exhaustive with no wildcard.

---

### R-10: Stdio Key `""` Overwrite in Test Scenarios

**Severity**: Low
**Likelihood**: Med
**Impact**: Test creates two sequential stdio connections in the same process, second overwrites `client_type_map[""]` silently; WARN is logged but no test assertion catches attribution from first connection being lost.

**Test Scenarios**:
1. Simulate two sequential stdio `initialize` calls (same server instance, key `""`): assert WARN log is emitted on second call; assert `client_type_map[""]` holds the second client name after overwrite.
2. After overwrite, a tool call reads the second client name, not the first — correct post-overwrite behavior.

**Coverage Requirement**: The overwrite scenario must be explicitly tested and the WARN emission verified (not just the final value).

---

### R-11: `create_tables_if_needed` DDL Divergence

**Severity**: High
**Likelihood**: Med
**Impact**: A fresh database created by `create_tables_if_needed` has a different schema than a migrated database — missing columns, different defaults, or missing triggers. Tests using fresh DBs pass while migrated DBs fail in production.

**Test Scenarios**:
1. Create a fresh database via `create_tables_if_needed`; run `pragma_table_info('audit_log')` — verify all four new columns present with correct defaults.
2. Create a fresh database; attempt `DELETE FROM audit_log` — verify trigger fires and returns ABORT error.
3. Create a fresh database; run `migrate_if_needed` (already at v25) — no-op; schema is unchanged.
4. Compare fresh-DB schema to migrated-DB schema via `pragma_table_info` — column names, types, defaults, and notnull constraints must be identical.

**Coverage Requirement**: Fresh-DB and migrated-DB schema equivalence must be explicitly verified, not assumed.

---

### R-12: Non-Tool-Call `AuditEvent` Sites Omit New Fields

**Severity**: Med
**Likelihood**: Med
**Impact**: Background tick `AuditEvent` construction sites in `background.rs` (at minimum 3 sites) and `uds/listener.rs` omit `?9`–`?12` bindings, causing INSERT to fail or bind NULL to NOT NULL columns.

**Test Scenarios**:
1. Trigger a background tick operation that produces an `AuditEvent`; verify the resulting `audit_log` row has `credential_type = "none"`, `capability_used = ""`, `agent_attribution = ""`, `metadata = "{}"`.
2. If `uds/listener.rs` constructs `AuditEvent` directly, verify the same four-field defaults in the resulting row.
3. Compile-time: `AuditEvent { ..., ..AuditEvent::default() }` struct update syntax is used at non-tool-call sites — if any site uses the exhaustive struct literal without the four new fields, it fails to compile.

**Coverage Requirement**: Every non-tool-call `AuditEvent` construction site must be enumerated and tested.

---

### R-13: `serde(default)` Produces Wrong Default for `metadata`

**Severity**: Med
**Likelihood**: Med
**Impact**: `#[serde(default)]` on `metadata: String` gives `String::default()` = `""`, not `"{}"`. Deserializing a pre-migration `AuditEvent` JSON record (missing `metadata` key) produces `metadata = ""`. If this deserialized record is re-inserted, it violates NFR-06.

**Test Scenarios**:
1. Deserialize an 8-field `AuditEvent` JSON (no `metadata`, `credential_type`, `capability_used`, `agent_attribution` fields) — verify deserialized struct has `credential_type = ""`, `capability_used = ""`, `agent_attribution = ""`, `metadata = ""` (the serde defaults, not the SQL defaults).
2. Verify that the `Default` impl (code-side) returns `metadata = "{}"` and `credential_type = "none"` — these are the construction defaults, distinct from the serde defaults.
3. Document the `""` vs `"{}"` distinction in the `AuditEvent` struct comments so future implementers do not conflate serde deserialization paths with construction paths.

**Coverage Requirement**: The serde path (legacy deserialization) and the construction path (`AuditEvent::default()`, explicit field assignment) must be tested separately.

---

### R-14: `initialize` Override Returns Wrong `InitializeResult`

**Severity**: Med
**Likelihood**: Low
**Impact**: The override omits capabilities, changes protocol version, or alters `instructions` text — MCP clients negotiating capabilities see a different server than before vnc-014.

**Test Scenarios**:
1. Call `server.initialize(request, context)` and compare the returned `InitializeResult` field-by-field to `server.get_info()` — must be bit-identical.
2. A client connecting before and after this change sees the same capability set.

**Coverage Requirement**: AC-06 — the comparison test is mandatory and must cover all fields of `InitializeResult`.

---

## Integration Risks

**IR-01: rmcp `initialize` firing semantics in stateless mode.** Architecture OQ-1 notes that stateless HTTP mode never calls `initialize`. If any CI or test uses stateless mode, `client_type_map` is empty and all audit rows get `agent_attribution = ""`. This is correct behavior, but tests asserting on `agent_attribution` must not run against stateless-mode server instances.

**IR-02: `http::request::Parts` injection by rmcp.** The session ID extraction relies on rmcp injecting `http::request::Parts` into `RequestContext.extensions` for every HTTP POST. If a specific rmcp tool call path (e.g., SSE notification, stateless POST) does not inject these parts, the key falls back to `""` silently. The injection contract with rmcp 0.16.0 must be verified empirically, not assumed.

**IR-03: `AuditContext` vs `ToolContext` field routing.** `client_type` is held on `ToolContext` (not `AuditContext`). If any audit construction site reads from `AuditContext` rather than the full `ToolContext`, it will not find `client_type`. The distinction must be verified across all 12 tool handlers.

**IR-04: `import/drop_all_data` transaction model.** Removing `DELETE FROM audit_log` from `drop_all_data` changes the semantics of a full import reset. Integration tests for import that rely on a clean `audit_log` post-reset must be updated or will fail with stale audit rows from prior test operations.

---

## Edge Cases

**EC-01: `clientInfo.name` of exactly 256 characters.** Must not be truncated. Must not emit WARN. The boundary is `> 256`, not `>= 256`.

**EC-02: `clientInfo.name` of 257 characters.** Must be truncated to exactly 256 scalar values. WARN must be emitted. Stored value must be 256 chars.

**EC-03: `clientInfo.name` containing multi-byte Unicode characters near the 256-char boundary.** Byte-level truncation is forbidden (NFR-02). Must truncate by `chars().take(256).collect()`. A name of 255 ASCII + 1 four-byte character must produce 256 chars (not 255 + partial bytes).

**EC-04: Tool call arrives before `initialize`.** `client_type_map` has no entry. `build_context_with_external_identity` returns `client_type = None`. Audit row has `agent_attribution = ""`, `metadata = "{}"`. No panic, no error returned to client.

**EC-05: `Mcp-Session-Id` header present but not valid UTF-8.** `to_str().ok()` returns `None`; fallback to `""`. HTTP session is treated as stdio. This is a recoverable misclassification, not a failure.

**EC-06: `initialize` called with `clientInfo.name` that is a valid JSON injection string.** For example: `"a","b":"c"` as the name value. The JSON construction in `metadata` must not produce `{"client_type":"a","b":"c"}`. The entire client name must be treated as a single string value, properly escaped.

**EC-07: Schema v24 database with zero rows in `audit_log`.** Migration runs successfully; table has four new columns with correct defaults; no rows to update. Fresh-table case for the migration test.

**EC-08: Schema v24 database with existing `audit_log` rows.** Migration runs; existing rows get valid defaults for all four new columns (`credential_type = 'none'`, others = column default). Row count unchanged. AC-09.

---

## Security Risks

**SEC-01: `agent_attribution` as non-spoofable field.** The entire compliance value of `agent_attribution` rests on it being populated only from `client_type_map`, which is populated only from the transport layer. If any code path allows a tool parameter to write to `agent_attribution` (even indirectly), the non-spoofability claim is false.
- Untrusted input: tool call parameters (agent-declared `agent_id`, `session_id`)
- Blast radius: all compliance audit evidence for the session is invalidated if spoofed
- Test: no tool parameter anywhere in `tools.rs` can influence `AuditEvent.agent_attribution` — verified by code inspection and confirmed by AC-05 round-trip showing attribution comes from `ctx.client_type`, which comes only from `client_type_map`

**SEC-02: `metadata` JSON injection.** `clientInfo.name` is an opaque, externally-supplied string. It is embedded into a JSON object via string concatenation. Incomplete escaping produces invalid JSON or, in contexts where `metadata` is parsed and re-serialized, can influence the JSON structure.
- Untrusted input: `clientInfo.name` from any MCP client
- Blast radius: malformed `metadata` breaks downstream JSON parsers reading `audit_log`; in a worst case, a crafted name with `"}` could terminate the `client_type` value and inject additional keys
- Mitigation required: use a proper JSON serializer (e.g., `serde_json::json!`) rather than format string concatenation — the format string approach in SCOPE.md pseudocode escapes only `"` and is insufficient
- Test: EC-06 above; property test against arbitrary client name strings

**SEC-03: `client_type_map` Mutex poisoning.** If a thread panics while holding the `Mutex<HashMap>`, the mutex becomes poisoned. The spec requires `unwrap_or_else(|e| e.into_inner())` for recovery (consistent with `CategoryAllowlist` usage). Without this, a panicking initialize handler permanently disables client attribution for all subsequent sessions.
- Blast radius: all tool calls after a panic-in-initialize return `client_type = None` → `agent_attribution = ""`
- Test: verify `unwrap_or_else(|e| e.into_inner())` is used at all `client_type_map.lock()` call sites

**SEC-04: Audit log integrity after trigger installation.** The append-only triggers are the only enforcement mechanism for audit log immutability. If `CREATE TRIGGER IF NOT EXISTS` silently fails (e.g., trigger already exists with different definition), the immutability guarantee is absent.
- Test: after migration, verify trigger existence via `SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='audit_log'`; assert both trigger names are present

---

## Failure Modes

**FM-01: `initialize` override Mutex lock fails (panics).** If the Mutex is poisoned, `lock().unwrap()` panics. Mitigation: use `unwrap_or_else(|e| e.into_inner())`. Expected: lock succeeds with poisoned map; WARN may be logged; session setup continues.

**FM-02: `migrate_if_needed` called on a database with unrecognized schema version (> 25).** No migration step runs; `CURRENT_SCHEMA_VERSION` check passes. Expected: no-op; no regression.

**FM-03: `migrate_if_needed` called on v24 database with all four columns already present.** All four pragma checks return `true`; all four ALTERs skipped; indexes and triggers created (idempotent). Expected: clean migration with version bump; no error.

**FM-04: `log_audit_event` called with `metadata = ""`.** The NOT NULL constraint is satisfied (empty string is not NULL), but NFR-06 is violated. Expected behavior: must not reach this state — callers must supply `"{}"` as the minimum. Defensive option: assert or map `""` → `"{}"` inside `log_audit_event`.

**FM-05: Background tick fires `AuditEvent` construction during migration.** Migration runs inside a transaction; background tick's audit fire is on a separate connection. Background tick's INSERT may fail if it reads the schema mid-migration (column count mismatch). Expected: fire-and-forget path handles the error silently; no crash; audit row may be lost for that tick event.

**FM-06: `build_context_with_external_identity` called from a handler not yet migrated (if `build_context` retained as wrapper).** Both paths produce `ToolContext`; the old path produces `client_type = None`. Expected: audit row gets `agent_attribution = ""` — silent regression. Mitigation: remove `build_context()` entirely (ADR-003 decision).

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (Mutex contention on `client_type_map`) | R-03 (cross-session bleed), NFR-01 | Accepted for vnc-014: `Mutex` held only for HashMap ops, no I/O. DashMap deferred to W2-2. AC-07 concurrent test documents the bound. |
| SR-02 (non-idempotent ALTER TABLE without pragma guards) | R-04 | Fully addressed: ADR-004 mandates all four pre-flight pragma checks before any ALTER. Pattern #4092 applied. |
| SR-03 (rmcp `initialize` trait signature fragility) | R-14 | Addressed: feature pins rmcp 0.16.0; `std::future::ready()` return avoids async machinery; AC-06 verifies `InitializeResult` parity. |
| SR-04 (missed `build_context()` call sites) | R-05 | Fully addressed: ADR-003 removes `build_context()` (compile-time enforcement); AC-12 verifies removal. |
| SR-05 (`capability_used` free-form string divergence) | R-09 | Fully addressed: ADR-006 mandates `Capability::as_audit_str()` exhaustive match; AC-11 verifies canonical values. |
| SR-06 (stdio `""` key overwrite) | R-10 | Addressed: WARN log on overwrite; debug assertion for test scenarios. Explicit test scenario in AC-08 / R-10. |
| SR-07 (append-only triggers break test fixtures) | R-01, R-11 | Fully addressed: ADR-005 removes `gc_audit_log` and strips `DELETE FROM audit_log` from `drop_all_data`. Tests confirmed to use TempDir databases (not DELETE for teardown). |
| SR-08 (semantic ambiguity `agent_id` vs `agent_attribution`) | SEC-01, R-07 | Fully addressed: ADR-007 documents two-field attribution model; `AuditEvent` comments mandate the distinction; R-07 covers W2-3 seam risk. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | 10 scenarios |
| High | 5 (R-04, R-05, R-06, R-08, R-11) | 16 scenarios |
| Med | 7 (R-07, R-09, R-10, R-12, R-13, R-14, SEC-01–04) | 14 scenarios |
| Low | 1 (R-15) | 1 scenario (document, no dedicated test) |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `lesson-learned failures gate rejection` — found gate-3c non-negotiable test name validation (#2758), cascading rework from single-pass gate validation (#1203), tautological assertion lesson (#4177). Applied: R-05 and R-11 scenarios designed to be explicitly verifiable, not structurally-always-passing.
- Queried: `/uni-knowledge-search` for `SQLite migration schema column audit_log` — found pattern #4092 (idempotent ALTER TABLE multi-column guard), pattern #681 (create-new-then-swap migration), entry #4358 (ADR-004 vnc-014 directly), #4182 (ADR-004 crt-047 atomicity). Applied: R-04 explicitly references the pre-flight ordering requirement from #4092.
- Queried: `/uni-knowledge-search` for `risk pattern` in category `pattern` — results were low relevance (formatter regression, background tick dedup, entropy NaN). No directly applicable pattern to store or reference.
- Stored: nothing novel to store — SEC-02 (JSON injection via format-string `metadata` construction) is feature-specific, not a cross-feature pattern at this time.
