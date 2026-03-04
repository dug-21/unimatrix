## ADR-004: Session ID Prefix at Boundary, Strip Before Storage

### Context

vnc-009 introduces transport-prefixed session IDs (`mcp::{id}` vs `uds::{id}`) to prevent cross-contamination between MCP and UDS sessions (security surface analysis finding X-05).

The question is where the prefix lives:

1. **Prefix everywhere**: All code sees `mcp::abc` or `uds::sess-123`. Storage writes include the prefix. This changes the data format for existing injection logs and co-access pairs (breaking change for existing data).

2. **Prefix at boundary, strip before storage**: Transports add the prefix when passing session IDs to services. Services and audit see prefixed IDs for cross-transport safety. Storage writes strip the prefix to maintain backward compatibility with existing data.

3. **Prefix in audit only**: Only AuditContext carries the prefix. Services and storage see raw IDs. This limits the benefit — rate limiting and logging cannot distinguish transport origins.

Existing data analysis:
- `INJECTION_LOG` entries use raw UDS session IDs (UUID format)
- `CO_ACCESS` pairs reference entry IDs, not session IDs directly
- `SESSIONS` table uses raw session IDs
- No existing MCP session data exists (MCP has no sessions pre-vnc-009)

### Decision

Prefix at the transport-service boundary. Strip before storage writes.

**Transport layer**:
- MCP `build_context()`: if `params.session_id` is `Some(sid)`, set `audit_ctx.session_id = Some(format!("mcp::{sid}"))`
- UDS `handle_connection()`: set `audit_ctx.session_id = Some(format!("uds::{sid}"))`

**Service layer**:
- Services see prefixed session IDs in AuditContext
- Rate limiting uses CallerId (not session_id), so prefix does not affect rate limiting
- Audit events record the prefixed session_id (new events are distinguishable by transport)

**Storage layer**:
- `UsageService` strips prefix before calling `insert_injection_log_batch`, `record_co_access_pairs`
- Helper: `fn strip_session_prefix(prefixed: &str) -> &str` returns everything after `::` (or the full string if no prefix)
- Existing data continues to work unchanged

**Prefix format**: `{transport}::{raw_id}` where transport is `mcp` or `uds`. The `::` delimiter is chosen because it cannot appear in UUID session IDs and is visually distinct.

### Consequences

**Easier**:
- Zero migration for existing data
- Audit trail shows transport origin for new events
- Services can distinguish transport origin when needed (e.g., logging, debugging)
- Future transports add their own prefix

**Harder**:
- Historical audit events (pre-vnc-009) have unprefixed session IDs — mixed format in audit trail
- Must remember to strip prefix before storage writes (encapsulated in UsageService, not spread across codebase)
- `strip_session_prefix` is a simple string operation but adds a processing step
