# Agent Report: vnc-014-agent-1-pseudocode

## Task

Produce per-component pseudocode for all 7 components in VNC-014:
client attribution via `clientInfo.name`, ASS-050 four-column schema
migration, append-only trigger remediation, Seam 2 `build_context`
overload.

## Output

All 8 files written to `product/features/vnc-014/pseudocode/`:

| File | Component |
|------|-----------|
| `OVERVIEW.md` | Component interaction, data flow, sequencing |
| `audit-event.md` | `AuditEvent` struct + `audit.rs` INSERT/SELECT |
| `migration.md` | `migration.rs` v24→v25 + `db.rs` DDL parity |
| `capability.md` | `Capability::as_audit_str()` in `schema.rs` |
| `tool-context.md` | `ToolContext.client_type` field addition |
| `server.md` | `client_type_map`, `initialize` override, `build_context_with_external_identity`, removal of `build_context` |
| `tools.md` | 12-handler Seam 2 migration + `AuditEvent` population |
| `remediation.md` | `gc_audit_log` no-op, `drop_all_data` audit_log removal |

## Open Questions Found

**OQ-tools-1 (context_lookup capability)**:
The specification domain model maps `context_lookup` to `Capability::Search`
("search"). The current implementation uses `Capability::Read` in its
`require_cap` call. The `tools.md` pseudocode uses `Capability::Read` (matching
current code) but flags this for delivery agent confirmation. The
`capability_used` value in the audit row must match the capability actually
gated — not the one in the spec table.

**OQ-tools-2 (RequestContext availability in tool handlers)**:
The pseudocode assumes `RequestContext<RoleServer>` is accessible from inside
rmcp `#[tool]` attribute functions (via `tool_call_context.request_context`
or as a direct parameter). The exact injection mechanism for rmcp 0.16.0
must be confirmed by the delivery agent before writing code. This is IR-02
from the Risk-Test Strategy.

**OQ-retention-1 (retention.rs test updates)**:
The existing `test_gc_audit_log_retention_boundary` and `test_gc_audit_log_epoch_row_deleted`
tests use raw INSERT to populate audit_log and then assert on deletion counts.
With `gc_audit_log` becoming a no-op, these tests must be rewritten. The
remediation pseudocode identifies this but does not prescribe the exact
replacement test content — that is left to the delivery agent.

**OQ-db-1 (existing idx_audit_log_timestamp index)**:
The current `create_tables_if_needed` DDL may already create an
`idx_audit_log_timestamp` index. The migration pseudocode adds two new indexes
but does not include or remove an existing timestamp index. Delivery agent must
inspect the current DDL and preserve any existing indexes in the updated DDL.

## Deviations from Established Patterns

None identified. The pseudocode follows the existing codebase patterns for:
- `pragma_table_info` guard before ALTER TABLE (matches every prior migration block)
- `unwrap_or_else(|e| e.into_inner())` for Mutex poison recovery (matches `CategoryAllowlist`)
- `#[serde(default)]` on new struct fields (matches `EntryRecord` extension pattern)
- `..AuditEvent::default()` for non-tool-call construction sites (new pattern, consistent with Rust idioms)
- `serde_json::json!` for JSON construction (FR-10 mandate, no prior audit event used this)

The `serde_json::json!` requirement for `metadata` is a new pattern in audit event construction — prior events used no metadata field. This deviation is intentional and mandated by FR-10/SEC-02.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 13 entries. Most relevant: #4363 (session_id namespace warning — directly embedded as CRITICAL in server.md and tools.md), #317 (ToolContext construction pattern — confirmed existing structure before adding field), #4357/#4358/#4359/#4360 (ADR entries directly from this feature — confirmed all design decisions are reflected).
- Queried: `mcp__unimatrix__context_search` category=pattern for "audit log session_id AuditEvent" — found #4363 (same), #4047 (5-surface update pattern for AuditEvent extension — applied: all 5 surfaces are covered across the 7 component files).
- Queried: `mcp__unimatrix__context_search` category=decision topic=vnc-014 — found ADRs #4355, #4356, #4362 — all reflected in pseudocode.
- Deviations from established patterns: none (see above).
