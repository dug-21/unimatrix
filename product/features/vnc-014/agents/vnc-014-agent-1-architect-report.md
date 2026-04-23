# Agent Report: vnc-014-agent-1-architect

## Status: COMPLETE

## Output Files

- `/workspaces/unimatrix/product/features/vnc-014/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/vnc-014/architecture/ADR-001-client-type-map-storage.md` (Unimatrix #4355)
- `/workspaces/unimatrix/product/features/vnc-014/architecture/ADR-002-initialize-override.md` (Unimatrix #4356)
- `/workspaces/unimatrix/product/features/vnc-014/architecture/ADR-003-build-context-seam2-overload.md` (Unimatrix #4357)
- `/workspaces/unimatrix/product/features/vnc-014/architecture/ADR-004-four-column-migration-idempotency.md` (Unimatrix #4358)
- `/workspaces/unimatrix/product/features/vnc-014/architecture/ADR-005-append-only-trigger-remediation.md` (Unimatrix #4359)
- `/workspaces/unimatrix/product/features/vnc-014/architecture/ADR-006-capability-used-string-constants.md` (Unimatrix #4360)
- `/workspaces/unimatrix/product/features/vnc-014/architecture/ADR-007-two-field-attribution-model.md` (Unimatrix #4361)

## Key Decisions Summary

1. **ADR-001 (#4355)**: `client_type_map: Arc<Mutex<HashMap<String,String>>>` on `UnimatrixServer`. Key on `Mcp-Session-Id` for HTTP, `""` for stdio. `Arc`-wrapped for rmcp `Clone` compatibility. `DashMap` deferred to W2-2.

2. **ADR-002 (#4356)**: Override `ServerHandler::initialize` on `UnimatrixServer`. Extract `request.client_info.name` directly from `InitializeRequestParams` (not via extensions). Return `Ok(self.get_info())` — identical to default.

3. **ADR-003 (#4357)**: Ship `build_context_with_external_identity()` with full Seam 2 signature including `Option<&ResolvedIdentity>` (always `None` in vnc-014; W2-3 wires this). Remove `build_context()` after migration — compile enforcement is the completeness gate (SR-04). 10 call sites in tools.rs confirmed. Non-tool-call `AuditEvent` sites use `..AuditEvent::default()`.

4. **ADR-004 (#4358)**: v24→v25 migration. Run all 4 `pragma_table_info` checks before any `ALTER TABLE`. `CREATE TRIGGER/INDEX IF NOT EXISTS` are idempotent. Schema version cascade checklist (#4125) applies.

5. **ADR-005 (#4359)**: `BEFORE DELETE` trigger breaks `gc_audit_log()` (retention.rs) and `DELETE FROM audit_log` (import/mod.rs). Both removed. `AuditEvent` gains `impl Default`. No test infrastructure changes — fresh TempDir DBs are unaffected.

6. **ADR-006 (#4360)**: Add `Capability::as_audit_str() -> &'static str` (exhaustive match, lowercase). All tool-call sites use this for `capability_used`. Non-gated sites use `""`.

7. **ADR-007 (#4361)**: `agent_id` = agent-declared, spoofable, for routing. `agent_attribution` = transport-attested, for compliance. Both fields required; never merged. W2-3 upgrades `agent_attribution` source to JWT via Seam 2 without schema change.

## Critical Findings for Delivery Agent

- **AuditEvent construction sites**: 13 in tools.rs + 3 in background.rs + 1 in uds/listener.rs + 2 in infra/audit.rs tests = ~19 total sites needing the four new fields.
- **SR-07 confirmed RESOLVED**: Only two production DELETE sites on `audit_log` (retention.rs and import/mod.rs). Zero test sites. Both removals are correct semantics.
- **Seam 2 type already exists**: `ResolvedIdentity` in `mcp/identity.rs` — no new type needed.
- **`clientInfo.name` access at initialize**: `request.client_info.name` (direct field, not via extensions).
- **`Mcp-Session-Id` access at tool call**: `context.extensions.get::<http::request::Parts>().and_then(|p| p.headers.get("mcp-session-id")).and_then(|v| v.to_str().ok()).unwrap_or("")`.

## Open Questions for Delivery

- **OQ-1**: Does any CI/test scenario use rmcp stateless mode (no session manager)? If so, `""` fallback means HTTP stateless is treated as stdio — document as limitation.
- **OQ-2**: Confirm background.rs audit sites at lines 1197, 1252, 2267 are the only non-tool-call sites; use `..AuditEvent::default()` for all four new fields.
- **OQ-3**: Run `cargo test --workspace` immediately after bumping `CURRENT_SCHEMA_VERSION` to 25 — catch cascade failures before writing the new migration test file.
