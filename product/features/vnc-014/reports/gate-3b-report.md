# Gate 3b Report: vnc-014

> Gate: 3b (Code Review)
> Date: 2026-04-22
> Result: REWORKABLE FAIL
> Iteration 2 Date: 2026-04-23
> Iteration 2 Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | FAIL | `context_cycle` and `context_cycle_review` not migrated to Seam 2; use `..AuditEvent::default()` instead of explicit 4-field population |
| Architecture compliance | PASS | All ADR decisions followed; `build_context()` removed; migration idempotency pattern correct |
| Interface implementation | PASS | `ToolContext.client_type`, `AuditEvent` 4 new fields, `Capability::as_audit_str()` all correct |
| Test case alignment | WARN | TOOL-U-04 lists `context_lookup → "read"` but spec domain model says `"search"`; both documented in pseudocode and agent report as a known discrepancy |
| Code quality — compiles | PASS | `cargo test --workspace` all pass; zero errors |
| Code quality — no stubs | PASS | No `todo!()`, `unimplemented!()`, `TODO`, `FIXME` in production code |
| Code quality — no unwrap | PASS | No `.unwrap()` in non-test production code paths for vnc-014 changes |
| Code quality — file size | WARN | All over-500-line files are pre-existing (schema.rs 792, migration.rs 2114, server.rs 3406, tools.rs 8160, retention.rs 1404, import/mod.rs 956); none created by this feature |
| Security — metadata JSON | PASS | `serde_json::json!` macro used at all tool sites; FR-10 satisfied |
| Security — no secrets | PASS | No hardcoded credentials |
| Security — input validation | PASS | `clientInfo.name` truncated at 256 chars (char boundary safe), poison recovery on all Mutex locks |
| Security — path traversal | N/A | No new file paths introduced |
| Security — trigger enforcement | PASS | BEFORE DELETE/UPDATE triggers installed; `gc_audit_log` no-op; `drop_all_data` excludes `audit_log` |
| Knowledge stewardship — implementation agents | WARN | `vnc-014-agent-1-architect-report.md` missing `## Knowledge Stewardship` section; all other agent reports (8-server, 9-tools, 4-migration, 3-audit-event, 5-capability, 6-remediation, 7-tool-context) have correct Queried/Stored entries |

## Detailed Findings

### Pseudocode Fidelity

**Status**: FAIL

**Evidence**: The pseudocode `tools.md` states:

> Migrate all 12 tool handlers from `build_context()` to `build_context_with_external_identity()`.

The specification FR-04 says:

> Every tool handler in `tools.rs` MUST call `build_context_with_external_identity()` in place of `build_context()`.

The specification FR-09 says:

> At every site in `tools.rs` where `AuditEvent { ... }` is constructed, all four new fields MUST be populated.

**Actual implementation** (confirmed by code inspection and agent report `vnc-014-agent-9-tools-report.md`, section 3):

- **10 handlers migrated** to Seam 2: `context_search`, `context_lookup`, `context_get`, `context_store`, `context_correct`, `context_deprecate`, `context_status`, `context_briefing`, `context_quarantine`, `context_enroll`.

- **2 handlers NOT migrated** (`context_cycle_review` at lines 1643–2618 and `context_cycle` at lines 2689–2818):
  - Both call `self.resolve_agent()` directly instead of `build_context_with_external_identity`.
  - Both use `..AuditEvent::default()` for the 4 new fields, producing:
    - `credential_type = "none"` (correct from Default)
    - `capability_used = ""` (WRONG — should be `"read"` for `context_cycle_review`, `"write"` for `context_cycle`)
    - `agent_attribution = ""` (always empty — session context never used)
    - `metadata = "{}"` (always empty JSON — session context never used)

The agent rationale (section 3 of the tools report): "These retain `..AuditEvent::default()` per the pseudocode spec for non-tool-call sites." This reasoning is incorrect — `context_cycle` and `context_cycle_review` ARE tool handlers registered via `#[tool]` and receive `RequestContext<RoleServer>` through rmcp's `FromContextPart` mechanism (the same mechanism the other 10 handlers use). The pseudocode's "non-tool-call sites" refers to `background.rs` and `uds/listener.rs`, not to tool handlers.

**Effect**: For these two tools, audit rows will always show `capability_used = ""` and `agent_attribution = ""` even when a named client connected. This violates AC-11 (`capability_used` canonical for each tool) and AC-01 (attribution propagated to audit rows).

---

### Architecture Compliance

**Status**: PASS

All ADR decisions are correctly implemented:
- ADR-001: `client_type_map: Arc<Mutex<HashMap<String, String>>>` on `UnimatrixServer` (line 251, server.rs), initialized in `new()` (line 352)
- ADR-002: `ServerHandler::initialize` override (lines 1027–1090, server.rs) extracts `request.client_info.name`, truncates at 256 chars (char-boundary safe), inserts into map with poison recovery
- ADR-003: `build_context()` removed entirely; `build_context_with_external_identity()` present (lines 387–460, server.rs); compile-time enforcement verified by test suite passing
- ADR-004: All 4 `pragma_table_info` checks run before any `ALTER TABLE` in migration.rs (lines 1128–1170)
- ADR-005: `gc_audit_log` is a no-op returning `Ok(0)` with `tracing::warn!` (retention.rs:271); `drop_all_data` excludes `audit_log` with comment referencing ADR-005 (import/mod.rs:241–248)
- ADR-006: `Capability::as_audit_str()` exhaustive match in schema.rs (lines 278–292)

---

### Interface Implementation

**Status**: PASS

All specified interfaces are implemented correctly:

- `ToolContext.client_type: Option<String>` — present in `mcp/context.rs:35` with correct doc comment
- `AuditEvent` 4 new fields — present in `schema.rs:377–390` with `#[serde(default)]` and correct `Default` impl sentinels (`credential_type="none"`, `capability_used=""`, `agent_attribution=""`, `metadata="{}"`)
- `log_audit_event` INSERT — binds `?9`–`?12` for all 4 new fields (audit.rs:47–65)
- `read_audit_event` SELECT — reads all 4 new columns by name (audit.rs:97–131)
- `Capability::as_audit_str()` — exhaustive match, no wildcard arm (schema.rs:283–292), `SessionWrite → "session_write"` correctly added
- `build_context_with_external_identity` — signature matches spec FR-03 (server.rs:387–393)

---

### Test Case Alignment

**Status**: WARN

**Evidence**:

The test plan TOOL-U-04 table (test-plan/tools.md) lists `context_lookup → "search" / Capability::Search` but the implementation maps it to `"read" / Capability::Read`. The test `test_tool_u04_capability_used_read_tools()` in `tools.rs:8029–8033` includes a comment listing `context_lookup` under "read tools". The actual `AuditEvent.capability_used` for `context_lookup` at line 551 uses `Capability::Read.as_audit_str()`.

This discrepancy between the spec domain model table and the implementation is documented in the pseudocode `tools.md` note on `context_lookup`:

> NOTE: Specification domain model maps `context_lookup` to `Capability::Search` ("search"). The current handler uses `Capability::Read` in `require_cap`. Delivery agent must inspect the current `require_cap` call in `context_lookup` and use whatever capability is actually gated there. Document the choice in the PR.

The agent report `vnc-014-agent-9-tools-report.md` does not explicitly document this choice (it documents the `context_briefing` and `context_quarantine` corrections but omits the `context_lookup` deviation). This is a WARN — the deviation is from the spec table, the pseudocode documents it, but the agent report should have listed it as a deviation for clarity.

All migration integration tests (`migration_v24_to_v25.rs`) are present and cover all 10 scenarios from the migration test plan. Append-only trigger tests (`test_v25_append_only_triggers_fire_on_delete`, `test_v25_append_only_triggers_fire_on_update`) are present and pass.

---

### Code Quality

**Status**: PASS (with file size WARN)

- Build: all tests pass — `cargo test --workspace` shows zero failures across all crates
- No `todo!()`, `unimplemented!()`, `TODO`, `FIXME` in production code
- No `.unwrap()` in production code paths modified by this feature
- `metadata` construction uses `serde_json::json!` macro at all 8 explicit tool handler sites (FR-10 / SEC-02 satisfied)
- Pre-existing file size violations (all files listed were at these sizes before vnc-014; not introduced by this feature)

---

### Security

**Status**: PASS

- `clientInfo.name` truncation is char-boundary safe: `chars().take(256).collect()` (server.rs:1048–1059)
- Mutex poison recovery via `unwrap_or_else(|e| e.into_inner())` at both lock sites in `initialize` and `build_context_with_external_identity` (server.rs:1072, 449)
- `metadata` JSON injection prevention via `serde_json::json!` macro — 4 injection-resistance unit tests in `audit.rs:343–385` and 5 more in `tools.rs:8088–8138`
- BEFORE DELETE / BEFORE UPDATE triggers installed — `gc_audit_log` is a no-op, `drop_all_data` excludes `audit_log`

---

### Knowledge Stewardship

**Status**: WARN

The architect agent report (`vnc-014-agent-1-architect-report.md`) is missing a `## Knowledge Stewardship` section. This is an active-storage agent (produced ADRs #4355–#4362) and must have this block per gate rules. The report shows the ADR IDs were stored, but the stewardship block is absent from the report document itself.

All 8 implementation agent reports (server, tools, migration, audit-event, capability, remediation, tool-context, spec/testplan) contain the required block with `Queried:` and `Stored:` / "nothing novel to store" entries.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| `context_cycle_review` not migrated to Seam 2 | rust-dev (tools specialist) | Add `request_context: rmcp::service::RequestContext<rmcp::RoleServer>` parameter; call `build_context_with_external_identity`; populate 4 fields with `Capability::Read.as_audit_str()` for `capability_used`. Multiple `AuditEvent` sites in the handler (lines 1797, 1809, 2576, 2598) need explicit field values from `ctx.client_type`. |
| `context_cycle` not migrated to Seam 2 | rust-dev (tools specialist) | Add `request_context: rmcp::service::RequestContext<rmcp::RoleServer>` parameter; call `build_context_with_external_identity`; populate 4 fields with `Capability::Write.as_audit_str()` for `capability_used`. AuditEvent site is line 2793. The `write_lesson_learned` helper at line 3227 is genuinely a non-tool-call site and may retain `..AuditEvent::default()`. |
| Architect agent report missing Knowledge Stewardship block | (documentation fix, not code) | Add `## Knowledge Stewardship` section to `vnc-014-agent-1-architect-report.md` documenting the ADR stores and any queries made. |

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` (lesson-learned, "Seam migration tool handler skipped") — no prior lesson exists. Verified via search before attempting store.
- Stored: nothing stored — write capability unavailable in validator agent context. Lesson to store (coordinator action required):
  - Title: "Seam migration skips tool handlers with complex branching logic"
  - Topic: validation, Category: lesson-learned
  - Tags: gate-3b, gate-failure, mcp, seam-migration, rework, vnc-014
  - Content: Gate 3b caught that context_cycle_review and context_cycle were excluded from the build_context → build_context_with_external_identity Seam 2 migration. Both are #[tool]-annotated handlers that can receive RequestContext<RoleServer> via FromContextPart. Root cause: handlers with multiple internal code paths (cache-hit, full-pipeline, error) were underestimated as migration effort and skipped. Takeaway: gate-3b validators must verify each named tool handler has exactly one build_context_with_external_identity call — counting total call sites is insufficient when handlers branch internally.

---

## Iteration 2 — Recheck (2026-04-23)

**Scope**: Verify rework committed after iteration 1 REWORKABLE FAIL. Checked only the previously failing items.

### FR-04 / FR-09: context_cycle and context_cycle_review migrated to Seam 2

**Status**: PASS

`context_cycle_review` (tools.rs:1643): now declares `request_context: rmcp::service::RequestContext<rmcp::RoleServer>` parameter and calls `build_context_with_external_identity` at line 1654. All three `AuditEvent` construction sites within the handler have explicit 4-field population:

- Purged-signals path (line 1808): `credential_type: "none"`, `capability_used: Capability::Read.as_audit_str().to_string()`, `agent_attribution: ctx.client_type.clone().unwrap_or_default()`, `metadata: metadata_json` from `ctx.client_type`.
- Cache-hit / memoization path (line 2594): same 4-field explicit population.
- Full-pipeline path (line 2623): same 4-field explicit population.

`context_cycle` (tools.rs:2717): now declares `request_context: rmcp::service::RequestContext<rmcp::RoleServer>` parameter and calls `build_context_with_external_identity` at line 2724. The single `AuditEvent` site (line 2830) has explicit 4-field population with `Capability::Write.as_audit_str()` for `capability_used`, `ctx.client_type.clone().unwrap_or_default()` for `agent_attribution`, and `metadata_json` from `ctx.client_type`.

`write_lesson_learned` helper (line 3267): retains `..AuditEvent::default()` — correct, this is a background non-`#[tool]` site with no `RequestContext` available.

No `..AuditEvent::default()` present in any `#[tool]`-annotated handler's `AuditEvent` construction.

### Iteration 1 warnings — still acceptable

- `context_lookup` using `Capability::Read` vs "search" in domain model: unchanged, documented deviation, WARN retained.
- Architect agent report missing Knowledge Stewardship section: informational WARN, not a code issue, retained.

### Test Suite

`cargo test --workspace`: all crates pass, zero failures across all test result lines.

**Iteration 2 Result: PASS**
