# VNC-014 Test Plan Overview
## ASS-050 audit_log Migration + MCP Client Attribution

---

## Test Strategy

VNC-014 introduces two tightly coupled changes: a four-column schema migration to `audit_log`
with append-only DDL triggers, and server-side session attribution via `clientInfo.name`.
Testing is organized at three levels:

1. **Unit tests** (in `#[cfg(test)]` blocks or `tests/` files) — pure logic: struct defaults,
   method return values, serde round-trips, enum exhaustiveness.
2. **Integration tests** (migration test files in `crates/unimatrix-store/tests/`) — database
   migrations, DDL parity, trigger enforcement, round-trip persistence. These follow the
   established `migration_vN_to_vM.rs` pattern.
3. **Integration harness tests** (infra-001, Python pytest) — MCP-level: `initialize` side
   effects, `agent_attribution` propagation, concurrent session isolation, tool attribution.

The critical ordering constraint is: **R-01 remediation (remove DELETE paths) must be
implemented before trigger DDL lands**. All test plans enforce this ordering at the assertion
level — trigger tests assert both that triggers fire AND that `gc_audit_log` and `drop_all_data`
no longer issue DELETEs.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Test File(s) | Scenarios |
|---------|----------|-------------|-------------|-----------|
| R-01 | Critical | remediation.md | `test-plan/remediation.md` | R-01-S1–S4 |
| R-02 | Critical | migration.md | `test-plan/migration.md` | R-02-S1–S4 |
| R-03 | Critical | server.md, tool-context.md | `test-plan/server.md`, infra-001 | R-03-S1–S3 |
| R-04 | High | migration.md | `test-plan/migration.md` | R-04-S1–S3 |
| R-05 | High | tools.md | `test-plan/tools.md` | R-05-S1–S3 |
| R-06 | High | audit-event.md | `test-plan/audit-event.md` | R-06-S1–S4 |
| R-07 | High | server.md | `test-plan/server.md` | R-07-S1–S2 |
| R-08 | High | tools.md, audit-event.md | `test-plan/tools.md`, infra-001 | R-08-S1–S4 |
| R-09 | Med | capability.md | `test-plan/capability.md` | R-09-S1–S3 |
| R-10 | Med | server.md | `test-plan/server.md` | R-10-S1–S2 |
| R-11 | High | migration.md | `test-plan/migration.md` | R-11-S1–S4 |
| R-12 | Med | tools.md | `test-plan/tools.md` | R-12-S1–S3 |
| R-13 | Med | audit-event.md | `test-plan/audit-event.md` | R-13-S1–S3 |
| R-14 | Med | server.md | `test-plan/server.md` | R-14-S1–S2 |
| SEC-01 | Med | tools.md | `test-plan/tools.md` | SEC-01 |
| SEC-02 | High | tools.md, audit-event.md | `test-plan/audit-event.md`, infra-001 | SEC-02-S1–S4 |
| SEC-03 | Med | server.md | `test-plan/server.md` | SEC-03 |
| SEC-04 | Med | migration.md, remediation.md | `test-plan/migration.md` | SEC-04 |

---

## Cross-Component Test Dependencies

1. `AuditEvent` defaults (audit-event.md) are a prerequisite for tool handler tests (tools.md).
   The four-field `Default` impl must be verified before any round-trip test.
2. Schema migration (migration.md) must succeed before any trigger enforcement test
   (remediation.md) can pass.
3. `Capability::as_audit_str()` (capability.md) must be verified before `capability_used`
   assertions in tool handler tests (tools.md).
4. `ToolContext.client_type` population (tool-context.md) is a prerequisite for attribution
   assertions in tool handler tests (tools.md).
5. `ServerHandler::initialize` override (server.md) must be verified before concurrent
   session isolation tests can run.

---

## AC-to-Test Mapping

| AC-ID | Component Plan | Test Scenario |
|-------|---------------|---------------|
| AC-01 | server.md, tools.md | HTTP session attribution round-trip |
| AC-02 | server.md | Empty clientInfo.name no-op |
| AC-03 | tools.md | No session context — all 12 tools pass |
| AC-04 | migration.md | pragma_table_info 12 columns post-migration |
| AC-05 | audit-event.md | log/read round-trip all four fields |
| AC-05b | remediation.md, migration.md | Trigger enforcement + gc/import remediation |
| AC-06 | server.md | InitializeResult parity with get_info() |
| AC-07 | server.md, infra-001 | Concurrent session isolation (no bleed) |
| AC-08 | server.md | Stdio session attribution under key "" |
| AC-09 | migration.md | schema_version=25, row count unchanged |
| AC-10 | server.md | 256-char truncation boundary |
| AC-11 | capability.md, tools.md | Canonical capability_used per tool |
| AC-12 | tools.md | build_context() absent from production code |

---

## Integration Harness Plan (infra-001)

### Mandatory Smoke Gate

Run before any other suite:

```bash
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60
```

The smoke gate verifies that the server binary starts, completes `initialize`, responds to
tool calls, and shuts down gracefully. All of these exercised after the schema migration runs
on startup.

### Existing Suite Coverage

| Suite | Relevance to vnc-014 | What it validates |
|-------|---------------------|-------------------|
| `protocol` | High | `initialize` handshake still works; capability negotiation unchanged (AC-06) |
| `tools` | High | All 12 tools functional after handler migration; no regressions (AC-03, R-05) |
| `lifecycle` | High | Store→search chains still work; schema migration does not break persistence |
| `security` | Med | Tool capability enforcement unchanged; `agent_attribution` not injectable via params (SEC-01) |
| `edge_cases` | Med | Unicode handling, boundary values — `clientInfo.name` edge cases align with EC-01–EC-08 |

### Gaps Requiring New Integration Tests

The following behaviors are visible only through the MCP interface and are not fully covered
by any existing suite. New tests should be added to the indicated suites.

#### 1. `initialize` with `clientInfo.name` — `suites/test_tools.py`

**Scenario**: Send `initialize` with `clientInfo.name = "codex-mcp-client"` then call
`context_store`. Assert the resulting audit row (if readable via status or a test helper)
carries `agent_attribution = "codex-mcp-client"`.

**Problem**: The existing `server` fixture may not expose `clientInfo.name` injection in
`initialize`. The `conftest.py` server fixture will need `clientInfo` param support, OR a
new fixture `attributed_server` that initializes with a known `clientInfo.name`.

**Test function**: `test_initialize_client_info_name_stored(server)`

#### 2. Concurrent session isolation — `suites/test_tools.py` or `test_lifecycle.py`

**Scenario**: Two concurrent server instances (or two sessions if HTTP transport is available
in the harness) each initialized with distinct `clientInfo.name` values. Assert audit row
isolation per session.

**Note**: If infra-001 uses stdio (single-session), concurrent isolation must be covered at
the unit level in `server.rs` tests (R-03-S1). The harness test can validate
single-session correctness. Document the stdio limitation in the test.

**Test function**: `test_single_session_attribution_roundtrip(server)`

#### 3. Append-only trigger visible through MCP — `suites/test_security.py`

**Scenario**: After migration, the `gc_audit_log` no-op and `drop_all_data` audit-log
preservation are not directly testable through MCP. However, the import tool (`context_store`
chain) can verify that audit rows from prior operations are still present after a hypothetical
reset sequence.

**Assessment**: Limited MCP-level testability for trigger enforcement. Trigger tests are
better placed in unit/integration tests (migration_v24_to_v25.rs). Flag as "unit-covered,
no new harness test needed" for trigger enforcement.

#### 4. `clientInfo.name` truncation and JSON safety — `suites/test_edge_cases.py`

**Scenario**: Send `initialize` with a 300-char `clientInfo.name` and a name containing
backslash, newline, and embedded quote sequences. Assert server starts and responds to
tool calls without error. Assert no server crash.

**Test function**: `test_long_client_name_no_crash(server)`
**Test function**: `test_special_chars_client_name_no_crash(server)`

### New Tests Summary

| Suite | Function | Covers |
|-------|----------|--------|
| `test_tools.py` | `test_initialize_client_info_name_stored` | AC-01, AC-08 |
| `test_tools.py` | `test_single_session_attribution_roundtrip` | R-03, AC-07 |
| `test_edge_cases.py` | `test_long_client_name_no_crash` | AC-10, EC-01, EC-02 |
| `test_edge_cases.py` | `test_special_chars_client_name_no_crash` | SEC-02, EC-06 |

### Suite Selection for Stage 3c Execution

Per the suite selection table, vnc-014 touches:
- Tool logic and store/retrieval behavior: `tools`, `lifecycle`, `edge_cases`
- Schema change: `lifecycle`, `volume`
- Security boundary: `security`
- Any change at all: `smoke` (mandatory gate)

**Required suites for Stage 3c**:

```bash
python -m pytest suites/ -v -m smoke --timeout=60          # mandatory gate
python -m pytest suites/test_protocol.py -v --timeout=60   # initialize parity (R-14, AC-06)
python -m pytest suites/test_tools.py -v --timeout=60      # all 12 tools (R-05, AC-03, AC-11)
python -m pytest suites/test_lifecycle.py -v --timeout=60  # persistence, migration (R-02, R-11)
python -m pytest suites/test_security.py -v --timeout=60   # SEC-01
python -m pytest suites/test_edge_cases.py -v --timeout=60 # SEC-02, EC-01–EC-08
```

---

## Out-of-Scope for This Test Plan

- `context_cycle_review` and `context_status` behavioral testing — not modified by vnc-014.
- OAuth/JWT credential_type values (`"static_token"`, `"jwt"`) — W2-2 and W2-3 deliverables.
- Analytics or querying on `agent_attribution` — field is written, analytics deferred.
- Retention policy reporting — `gc_audit_log` is removed, no new retention policy.
