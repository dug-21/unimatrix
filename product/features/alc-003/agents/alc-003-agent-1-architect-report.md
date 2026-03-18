# Agent Report: alc-003-agent-1-architect

## Status: Complete

## Output Files

- `/workspaces/unimatrix/product/features/alc-003/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/alc-003/architecture/ADR-001-session-identity-source-shape.md`
- `/workspaces/unimatrix/product/features/alc-003/architecture/ADR-002-capability-source-session-not-per-call.md`
- `/workspaces/unimatrix/product/features/alc-003/architecture/ADR-003-permissive-auto-enroll-deleted.md`
- `/workspaces/unimatrix/product/features/alc-003/architecture/ADR-004-adr-1839-deferred.md`
- `/workspaces/unimatrix/product/features/alc-003/architecture/ADR-005-preflight-blast-radius-measurement.md`

## ADR Unimatrix Storage

All five ADR store calls failed with `MCP error -32603: Internal storage error`. This
is the exact capability enforcement gap that alc-003 is designed to fix: the current
session has no `UNIMATRIX_SESSION_AGENT` set, and the daemon's write path is failing
for this MCP session identity. The ADRs exist as files in the architecture directory.

Unimatrix storage of the ADRs must be completed by an agent running in a session with
Write capability (either after alc-003 ships, or after `UNIMATRIX_SESSION_AGENT` is
configured in `settings.json`).

ADR #79 supersession (capability-resolution aspect) was also attempted and failed for
the same reason.

## Key Design Decisions

| ADR | Title | File |
|-----|-------|------|
| ADR-001 | SessionIdentitySource as enum with resolve() | ADR-001-session-identity-source-shape.md |
| ADR-002 | Session capabilities cached at startup; no per-call registry lookup | ADR-002-capability-source-session-not-per-call.md |
| ADR-003 | PERMISSIVE_AUTO_ENROLL deleted entirely | ADR-003-permissive-auto-enroll-deleted.md |
| ADR-004 | ADR #1839 (UNIMATRIX_CLIENT_TOKEN) deferred | ADR-004-adr-1839-deferred.md |
| ADR-005 | Pre-flight blast radius measurement before any implementation code | ADR-005-preflight-blast-radius-measurement.md |

## Supersession Required

ADR #79 ("Agent Identity via Tool Parameters") is partially superseded by ADR-002. Its
capability-resolution aspect is superseded; its audit-attribution aspect is preserved.
A `context_correct` call for entry #79 must be made once Write access is available.

## Open Questions for Spec Writer

See ARCHITECTURE.md "Open Questions for the Spec Writer" section for 6 items, including:
1. Whether `resolve_or_enroll()` store-level signature cleanup is in alc-003 scope
2. Whether `resolve_agent()` / `identity::resolve_identity()` should be deleted or deprecated
3. Test helper naming and module location for `make_server_with_session()`
4. Exact count of tool handler `require_cap()` call sites (expected 12 but must confirm)
5. Whether `ToolContext::trust_level` is used for conditional logic in any handler
6. Startup failure exit code convention (specific vs. any non-zero)
