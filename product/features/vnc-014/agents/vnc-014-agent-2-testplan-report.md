# Agent Report: vnc-014-agent-2-testplan

**Phase**: Stage 3a — Test Plan Design
**Agent ID**: vnc-014-agent-2-testplan
**Feature**: vnc-014

---

## Deliverables

All 8 test plan files written to `product/features/vnc-014/test-plan/`:

| File | Component | Tests |
|------|-----------|-------|
| `OVERVIEW.md` | Strategy, risk mapping, integration harness plan | — |
| `server.md` | `UnimatrixServer` — initialize, client_type_map, Seam 2 | 15 unit, 1 integration |
| `tool-context.md` | `ToolContext.client_type` propagation | 6 unit, 2 integration |
| `audit-event.md` | `AuditEvent` struct defaults, round-trip, JSON safety | 3 unit, 6 integration |
| `migration.md` | v24→v25 migration, DDL parity, trigger installation | 1 unit, 10 integration |
| `tools.md` | All 12 tool handlers, attribution, JSON safety | 12 unit |
| `capability.md` | `Capability::as_audit_str()` exhaustive match | 7 unit |
| `remediation.md` | `gc_audit_log` no-op, `drop_all_data` audit preservation | 7 unit |

Total: ~62 named test scenarios across all plans.

---

## Risk Coverage

| Risk | Priority | Covered By | Coverage |
|------|----------|-----------|----------|
| R-01 | Critical | remediation.md (REM-U-01–U-07), migration.md (MIG-V25-U-08) | Full |
| R-02 | Critical | migration.md (MIG-V25-U-01–U-06, PARITY-01) | Full |
| R-03 | Critical | server.md (SRV-U-02–U-04, U-09–U-10), OVERVIEW integration harness | Full |
| R-04 | High | migration.md (MIG-V25-U-05 sub-cases A+B) | Full |
| R-05 | High | tools.md (TOOL-U-01–U-02) | Full (compile-time) |
| R-06 | High | audit-event.md (AE-U-01, AE-I-01–I-06) | Full |
| R-07 | High | server.md (SRV-U-11) | Full |
| R-08 | High | audit-event.md (AE-I-05), tools.md (TOOL-U-09) | Full |
| R-09 | Med | capability.md (CAP-U-01–U-07) | Full |
| R-10 | Med | server.md (SRV-U-08) | Full |
| R-11 | High | migration.md (MIG-V25-U-07, PARITY-01) | Full |
| R-12 | Med | tools.md (TOOL-U-11), audit-event.md (AE-U-04) | Full |
| R-13 | Med | audit-event.md (AE-U-02) | Full |
| R-14 | Med | server.md (SRV-U-07) | Full |
| SEC-01 | Med | tools.md (TOOL-U-10) | Full |
| SEC-02 | High | audit-event.md (AE-I-05), tools.md (TOOL-U-09) | Full |
| SEC-03 | Med | server.md (SRV-U-12) | Full |
| SEC-04 | Med | migration.md (MIG-V25-U-09) | Full |

---

## Integration Harness Plan (infra-001)

**Mandatory gate**: `pytest -m smoke`

**Required suites for Stage 3c**:
- `test_protocol.py` — AC-06, R-14
- `test_tools.py` — R-05, AC-03, AC-11, plus 2 new tests
- `test_lifecycle.py` — R-02, R-11
- `test_security.py` — SEC-01
- `test_edge_cases.py` — SEC-02, EC-01–EC-08, plus 2 new tests

**New harness tests planned** (Stage 3c to implement):

| Suite | Function | Covers |
|-------|----------|--------|
| `test_tools.py` | `test_initialize_client_info_name_stored` | AC-01, AC-08 |
| `test_tools.py` | `test_single_session_attribution_roundtrip` | R-03, AC-07 |
| `test_edge_cases.py` | `test_long_client_name_no_crash` | AC-10, EC-01–02 |
| `test_edge_cases.py` | `test_special_chars_client_name_no_crash` | SEC-02, EC-06 |

**Note on concurrent session testing**: The infra-001 harness uses stdio transport, which is
single-session. AC-07 concurrent HTTP session isolation (R-03) must be covered at the unit
level in `server.rs` (SRV-U-02 + SRV-U-09 for two separate sessions). The harness tests
validate single-session correctness and server stability, not concurrent HTTP isolation.

---

## Open Questions for Stage 3b

1. **Retention test suite audit**: The delivery agent must audit which existing `retention.rs`
   tests use `gc_audit_log` and raw `audit_log` DELETEs, and rewrite them before installing
   triggers. Six specific tests were identified in `remediation.md`.

2. **`drop_all_data` option**: The delivery agent must choose Option A (DROP+recreate) or
   Option B (import-mode flag) per FR-11/OQ-C and document the decision in the PR.

3. **Existing v23→v24 schema_version assertions**: The `migration_v23_to_v24.rs` file uses
   `schema_version >= 24`. After bumping to v25, verify these assertions still use `>= N`
   (not `== N`) so they remain valid. Pattern #4125 covers this cascade requirement.

4. **background.rs audit sites**: Confirm lines 1197, 1252, 2267 are the only non-tool-call
   `AuditEvent` construction sites, and that `uds/listener.rs` has no additional sites.
   (OQ-2 from ARCHITECTURE.md.)

5. **`clientInfo.name` harness injection**: The infra-001 `conftest.py` server fixture does
   not currently support `clientInfo.name` injection in `initialize`. The new harness tests
   (`test_initialize_client_info_name_stored`, etc.) will need to either extend the fixture
   or use a direct MCP JSON-RPC call with a custom `clientInfo` payload.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — 13 entries returned. Most relevant: #4357
  (ADR-003 build_context removal), #4359 (ADR-005 append-only remediation), #317
  (ToolContext pattern), #4125 (schema cascade checklist pattern), #378 (old-schema migration
  test lesson). Applied: migration test file naming follows established `migration_vN_to_vM.rs`
  pattern; cascade checklist applied to MIG-V25 test IDs; serde vs Default distinction
  called out explicitly in AE-U-02.
- Queried: `context_search` for "append-only trigger test pattern SQLite" in category
  `pattern` — no prior pattern found covering trigger test triage.
- Stored: entry #4364 "Append-only trigger adoption requires three test categories:
  enforcement, call-site remediation, and existing-test audit" via `context_store`.
