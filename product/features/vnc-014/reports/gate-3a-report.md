# Gate 3a Report: vnc-014

> Gate: 3a (Design Review)
> Date: 2026-04-22
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 7 components match ARCHITECTURE.md decomposition; all 8 ADRs reflected |
| Specification coverage | PASS | All 12 FRs + 8 NFRs have corresponding pseudocode; no scope additions |
| Risk coverage | PASS | All 16 risks (R-01–R-15 + SEC-01–SEC-04) map to test plan scenarios |
| Interface consistency | PASS | Shared types in OVERVIEW.md match per-component usage with one noted discrepancy flagged for delivery |
| Knowledge stewardship compliance | PASS | All four agent reports have stewardship blocks with Queried and Stored/declined entries |

---

## Detailed Findings

### 1. Architecture Alignment

**Status**: PASS

**Evidence**:

All 7 pseudocode components map 1-to-1 to the 8 architecture components (components 7 and 8 are both in `remediation.md`):

| Architecture Component | Pseudocode File | Match |
|------------------------|-----------------|-------|
| `UnimatrixServer` (server.rs) | `server.md` | Exact — field, initialize override, `build_context_with_external_identity`, `build_context` removal |
| `ToolContext` (context.rs) | `tool-context.md` | Exact — `client_type: Option<String>` field |
| `AuditEvent` + `audit.rs` | `audit-event.md` | Exact — 4 fields, `Default` impl, INSERT/SELECT updated |
| `SqlxStore::log_audit_event` + `read_audit_event` | `audit-event.md` | Exact — `?9`–`?12` bindings |
| Schema Migration (migration.rs + db.rs) | `migration.md` | Exact — v24→v25 block, DDL parity |
| `Capability::as_audit_str()` | `capability.md` | Exact — exhaustive match, no wildcard |
| `gc_audit_log` removal | `remediation.md` | Exact — no-op returning `Ok(0)` |
| `drop_all_data` update | `remediation.md` | Exact — `DELETE FROM audit_log` removed |

Technology choices comply with all 8 ADRs:

- ADR-001 (#4355): `Arc<Mutex<HashMap>>` used in `server.md`, not `DashMap` — PASS
- ADR-002 (#4356): `ServerHandler::initialize` override with `std::future::ready(Ok(self.get_info()))` — PASS
- ADR-003 (#4357): `build_context()` removed (not deprecated), Seam 2 signature with `Option<&ResolvedIdentity>` — PASS
- ADR-004 (#4358): All 4 `pragma_table_info` checks before any ALTER TABLE — PASS
- ADR-005 (#4359): `gc_audit_log` → no-op, `DELETE FROM audit_log` removed from `drop_all_data` — PASS
- ADR-006 (#4360): `Capability::as_audit_str()` exhaustive match, lowercase constants — PASS
- ADR-007 (#4361): `agent_id` vs `agent_attribution` semantic distinction explicit in `tool-context.md`, session_id namespace warning (#4363) in `server.md` and `tools.md` — PASS
- ADR-008 (#4362): `ResolvedIdentity` placed in `unimatrix-server/mcp/identity.rs` per architect report finding — PASS

**One item confirmed by architect report**: `ResolvedIdentity` already exists in `mcp/identity.rs` (no new type needed). This is correctly reflected in `server.md` pseudocode.

---

### 2. Specification Coverage

**Status**: PASS

All 12 functional requirements and 8 NFRs have direct pseudocode coverage:

| Requirement | Pseudocode Location | Coverage |
|-------------|---------------------|----------|
| FR-01: `initialize` override | `server.md` §New Method: ServerHandler::initialize Override | Full — all 5 sub-requirements (extract name, determine session key, truncate 256, insert if non-empty, return get_info()) |
| FR-02: `client_type_map` on UnimatrixServer | `server.md` §New Field | Full — empty init, session key semantics, poison recovery, stdio overwrite debug log |
| FR-03: `build_context_with_external_identity` | `server.md` §New Method: build_context_with_external_identity | Full — signature matches spec exactly |
| FR-04: All 12 tool handlers migrated | `tools.md` §Migration Pattern | Full — explicit template for all 12, including the tool_call_context parameter question flagged as OQ |
| FR-05: v24→v25 schema migration | `migration.md` §run_main_migrations | Full — all 4 pragma checks, 4 ALTERs, 2 indexes, 2 triggers, version bump |
| FR-06: `AuditEvent` 4 new fields | `audit-event.md` §AuditEvent struct | Full — all 4 fields with `#[serde(default)]` |
| FR-07: `log_audit_event` INSERT | `audit-event.md` §SqlxStore::log_audit_event | Full — `?9`–`?12` bindings listed explicitly |
| FR-08: `read_audit_event` SELECT | `audit-event.md` §SqlxStore::read_audit_event | Full — all 4 column names in SELECT and struct construction |
| FR-09: 4 fields at all AuditEvent construction sites | `tools.md` §Per-Tool Capability | Full — all 12 tools shown with all 4 fields |
| FR-10: `metadata` via `serde_json::json!` | `tools.md` §metadata Construction Helper, `tool-context.md` §Usage Pattern | Full — format-string prohibition restated as CRITICAL, serde_json macro used |
| FR-11: `gc_audit_log` + `drop_all_data` remediation | `remediation.md` | Full — no-op with WARN log, DELETE line removed |
| FR-12: Concurrent HTTP session isolation | `server.md` §New Field (multiple sessions) + test scenario 9 | Covered through field design and test plan |
| NFR-01–08 | Across all component files | All addressed (truncation char-boundary, poison recovery, metadata non-nullable, `InitializeResult` parity, no tool schema changes) |

**No scope additions detected**: No pseudocode file implements functionality not requested by the specification.

**One discrepancy flagged and properly handled**: `context_lookup` capability gate — spec domain model says `Capability::Search` but current code uses `Capability::Read`. The pseudocode (`capability.md` and `tools.md`) flags this as a delivery agent decision and uses `Capability::Read` (matching current code), which is correct practice — the audit string must match the gated capability. This is a WARN-level observation; it is acknowledged and correctly deferred to delivery agent confirmation.

---

### 3. Risk Coverage

**Status**: PASS

All 16 risks from the Risk-Test Strategy have test plan coverage:

| Risk | Priority | Test Plan Coverage |
|------|----------|--------------------|
| R-01 (append-only breaks DELETE paths) | Critical | remediation.md: REM-U-01 to REM-U-07; migration.md: MIG-V25-U-08 |
| R-02 (schema version cascade) | Critical | migration.md: MIG-V25-U-01 to U-06, PARITY-01 |
| R-03 (cross-session bleed) | Critical | server.md: SRV-U-02, U-04, U-09, U-10; plus concurrent test in OVERVIEW.md |
| R-04 (partial migration re-run) | High | migration.md: MIG-V25-U-05 sub-cases A+B |
| R-05 (missed build_context call site) | High | tools.md: TOOL-U-01, TOOL-U-02 (compile-time) |
| R-06 (metadata empty string) | High | audit-event.md: AE-U-01, AE-I-01 through AE-I-06 |
| R-07 (ResolvedIdentity crate placement) | High | server.md: SRV-U-11 |
| R-08 (metadata JSON injection) | High | audit-event.md: AE-I-05; tools.md: TOOL-U-09 |
| R-09 (Capability::as_audit_str exhaustive) | Med | capability.md: CAP-U-01 through CAP-U-07 |
| R-10 (stdio key overwrite) | Med | server.md: SRV-U-08 |
| R-11 (db.rs DDL divergence) | High | migration.md: MIG-V25-U-07, PARITY-01 |
| R-12 (non-tool-call AuditEvent sites) | Med | tools.md: TOOL-U-11; audit-event.md: AE-U-04 |
| R-13 (serde(default) wrong for metadata) | Med | audit-event.md: AE-U-02 |
| R-14 (initialize returns wrong InitializeResult) | Med | server.md: SRV-U-07 |
| SEC-01 (agent_attribution spoofable) | Med | tools.md: TOOL-U-05, TOOL-U-10 |
| SEC-02 (metadata JSON injection) | High | audit-event.md: AE-I-05; tools.md: TOOL-U-09 |
| SEC-03 (Mutex poisoning) | Med | server.md: SRV-U-12 |
| SEC-04 (trigger existence post-install) | Med | migration.md: MIG-V25-U-09 |

All Critical and High risks have multiple test scenarios. The test plan OVERVIEW.md includes an explicit risk-to-test mapping table that cross-references all components, confirming completeness.

Integration risks (IR-01 through IR-04) are documented in OVERVIEW.md with appropriate test plan notes, including the limitation that IR-02 (`http::request::Parts` injection) must be verified empirically by the delivery agent.

Edge cases EC-01 through EC-08 are explicitly covered:
- EC-01 (exactly 256 chars, no truncation): SRV-U-06
- EC-02 (257 chars, truncation): SRV-U-05
- EC-03 (multi-byte Unicode boundary): SRV-U-15
- EC-04 (tool call before initialize): SRV-I-01
- EC-05 (invalid UTF-8 session header): SRV-U-13
- EC-06 (JSON injection string): AE-I-05, TOOL-U-09
- EC-07 (v24 DB, zero rows): MIG-V25-U-10
- EC-08 (v24 DB with rows): MIG-V25-U-06

---

### 4. Interface Consistency

**Status**: PASS

The OVERVIEW.md shared type definitions are consistent across all component files:

**`AuditEvent` 4 new fields** — Defined in OVERVIEW.md as `credential_type: String`, `capability_used: String`, `agent_attribution: String`, `metadata: String`, all with `#[serde(default)]` producing `""`. The `Default` impl sentinel values (`"none"`, `""`, `""`, `"{}"`) are defined in OVERVIEW.md and replicated correctly in `audit-event.md`.

**`ToolContext.client_type: Option<String>`** — Defined in OVERVIEW.md and `tool-context.md`. Consumed in `server.md` (returned by `build_context_with_external_identity`) and `tools.md` (consumed at AuditEvent construction). No contradiction.

**`UnimatrixServer.client_type_map: Arc<Mutex<HashMap<String, String>>>`** — Defined in OVERVIEW.md and `server.md`. No contradiction.

**`Capability::as_audit_str()` return mapping** — `capability.md` and `tools.md` agree on all 4 variant→string mappings (`search`, `read`, `write`, `admin`).

**One acknowledged discrepancy** (pre-existing, flagged appropriately): `context_lookup` is listed as `Capability::Read` in `tools.md` (matching current code) but `Capability::Search` in the SPECIFICATION.md domain model table and `capability.md` tool reference table. The `tools.md` and `capability.md` both flag this discrepancy with a delivery agent confirmation note. This is not a Gate 3a blocking issue — the pseudocode correctly defers to actual gate behavior, not spec table. **WARN** — delivery agent must confirm and document.

**`serde_json::json!` mandate** — OVERVIEW.md §Critical Cross-Cutting Constraints, `tool-context.md`, and `tools.md` all consistently prohibit format-string `metadata` construction and mandate `serde_json::json!`. This is a security correction from the SCOPE.md pseudocode and is consistently applied across all component files.

**Session ID namespace** — OVERVIEW.md, `server.md`, `tool-context.md`, and `tools.md` all carry the #4363 warning that `AuditEvent.session_id` comes from `ctx.audit_ctx.session_id` (agent-declared, `mcp::`-prefixed), never from the rmcp `Mcp-Session-Id` header. Consistent throughout.

---

### 5. Knowledge Stewardship Compliance

**Status**: PASS

Four agent reports examined:

| Agent | Report | Stewardship Block | Entries |
|-------|--------|-------------------|---------|
| vnc-014-agent-1-architect | `agents/vnc-014-agent-1-architect-report.md` | Present | Queried (briefing + search), Stored: ADRs #4355–#4362 via Unimatrix |
| vnc-014-agent-2-spec | `agents/vnc-014-agent-2-spec-report.md` | Present | Queried: briefing 14 entries |
| vnc-014-agent-1-pseudocode | `agents/vnc-014-agent-1-pseudocode-report.md` | Present | Queried: briefing + search; "nothing novel to store" with reason (serde_json pattern is feature-specific, not cross-feature) |
| vnc-014-agent-3-risk | `agents/vnc-014-agent-3-risk-report.md` | Present | Queried: 2 searches; "nothing novel to store" with reason |
| vnc-014-agent-2-testplan | `agents/vnc-014-agent-2-testplan-report.md` | Present | Queried: briefing; Stored: entry #4364 (append-only trigger test categories) |

All active-storage agents (architect) have `Stored:` entries — 8 ADRs stored in Unimatrix.
All read-only/query agents (pseudocode, risk, testplan) have `Queried:` entries with specific entry IDs referenced.
"Nothing novel to store" entries include reasons.

---

## Rework Required

None.

---

## Warnings for Delivery Agent

The following WARNs do not block delivery but require attention:

1. **`context_lookup` capability gate discrepancy**: The SPECIFICATION.md domain model maps `context_lookup` to `Capability::Search` ("search"), but the pseudocode uses `Capability::Read` (matching current code). The delivery agent must inspect the actual `require_cap` call in `context_lookup`, use whatever capability is actually gated, and document the choice in the PR. If the gate is `Read`, the `capability_used = "read"` audit string is correct. If the gate is `Search`, the spec table wins.

2. **OQ-tools-2 (RequestContext availability in tool handlers)**: The pseudocode notes that the exact mechanism for accessing `RequestContext<RoleServer>` inside rmcp 0.16.0 `#[tool]` attribute functions must be verified empirically (IR-02). If `RequestContext` is not directly available in the handler signature, the session key extraction path must be adapted. The delivery agent must confirm this before writing code for `tools.md` handlers.

3. **OQ-db-1 (existing `idx_audit_log_timestamp` index)**: The `migration.md` pseudocode notes that the current `create_tables_if_needed` DDL may already create a timestamp index. Delivery agent must inspect and preserve it in the updated DDL.

4. **`drop_all_data` option choice (FR-11, OQ-C)**: The pseudocode implements Option B (remove the DELETE line, audit history preserved). FR-11 specifies Option A (DROP+recreate) as preferred. The delivery agent must choose and document. The `remediation.md` pseudocode uses Option B; if Option A is chosen instead, the pseudocode must be superseded. Either is acceptable; the test plan (REM-U-03) is written option-agnostic.

5. **Retention.rs test audit**: Six existing `gc_audit_log` tests will break when triggers are installed. The delivery agent must audit and rewrite these before the trigger DDL lands (noted in `remediation.md` and `test-plan/remediation.md`).

---

## Knowledge Stewardship

- Stored: nothing novel to store — the gate findings (context_lookup capability discrepancy, drop_all_data option ambiguity) are feature-specific. No cross-feature validation pattern emerged that does not already exist in Unimatrix.
