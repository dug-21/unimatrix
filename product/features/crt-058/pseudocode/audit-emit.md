# Component: audit-emit

**File:** `crates/unimatrix-server/src/server.rs:650` — NEW helper method beside `audit_fire_and_forget`
(reuses it). Called from the deprecate-handler (step 6.5) on a non-empty successful cleanup.

## Purpose

Build the one `context_deprecate.edge_cleanup` `AuditEvent` — entry id, count summary, and the
removed-edge tuples serialized as JSON metadata (reconstructability of an irreversible delete, SR-01 /
ADR-002) — and hand it to the existing `audit_fire_and_forget`. Keeps tuple→JSON serialization beside
the audit infra and out of the handler.

## New Method

```rust
pub(crate) fn emit_edge_cleanup_audit(
    &self,
    entry_id: u64,
    removed: &[RemovedEdge],
    session_id: String,
    agent_id: String,
    agent_attribution: String,
)
```

Distinct from the flip's own `"context_deprecate"` audit (R-08): this is a SECOND, separate record.

## Pseudocode

```
FUNCTION emit_edge_cleanup_audit(self, entry_id, removed, session_id, agent_id, agent_attribution):
    # Caller guards !removed.is_empty(); guard defensively here too.
    IF removed.is_empty():
        RETURN

    count = removed.len()

    # metadata: JSON ARRAY of tuples via the JSON encoder — NEVER string interpolation, and must
    # NOT fall through to the "{}" sentinel on non-empty removals (security: relation_type strings
    # are encoder-escaped, not interpolated).
    metadata =
        MATCH serde_json::to_string(removed):     # RemovedEdge derives Serialize -> [{"source_id":..,
            Ok(s)  => s                            #   "target_id":.., "relation_type":".."}, …]
            Err(e) =>                              # practically impossible for these types
                tracing::warn!(entry = entry_id, error = %e,
                               "edge_cleanup audit metadata serialization failed")
                RETURN                             # skip the event rather than emit a "{}" sentinel

    detail = "eager edge cleanup: removed {count} agent-authored edge(s) for deprecated entry #{entry_id}"

    event = AuditEvent {
        event_id:          0,                                  # assigned by the audit layer
        timestamp:         0,                                  # assigned by the audit layer
        session_id:        session_id,
        agent_id:          agent_id,
        operation:         "context_deprecate.edge_cleanup",   # DISTINCT from the flip's operation
        target_ids:        vec![entry_id],
        outcome:           Outcome::Success,
        detail:            detail,
        credential_type:   "none".to_string(),
        capability_used:   Capability::Write.as_audit_str().to_string(),
        agent_attribution: agent_attribution,
        metadata:          metadata,                           # the tuple JSON array
    }

    self.audit_fire_and_forget(event)     # existing fire-and-forget spawn (server.rs:650)
```

Field set mirrors the flip `AuditEvent` built at `tools.rs:1455–1468` (same struct shape, same
`credential_type`/`capability_used` conventions) so no new audit plumbing is introduced (FR-08 / C-06).

## Error Handling

- Serialization error (near-impossible for `u64`/`String`) → `warn!` with entry id, skip the event.
  Never emit the `"{}"` sentinel with a non-empty removal (would defeat reconstructability).
- The audit WRITE itself is fire-and-forget: a dropped write is degraded observability, not data loss
  (the delete already committed, the caller already has its count). This is the only fire-and-forget
  part of step 6.5 — the DELETE is synchronous.

## Data Flow

- **In:** `entry_id`, `&[RemovedEdge]` (from the helper), identity strings (cloned in the handler
  before the flip audit event consumes `ctx.agent_id`).
- **Out:** none (fire-and-forget). Side effect: one queued `AuditEvent`.
- **Transform:** `&[RemovedEdge]` → JSON array string in `AuditEvent.metadata`.

## Key Test Scenarios (hints)

- AC-03 / AC-11: deprecate an entry with N known agent edges → read back the record filtered on
  `operation == "context_deprecate.edge_cleanup"`; assert `target_ids == [entry]`, count N in `detail`,
  and `metadata` parses to exactly the N `(source_id, target_id, relation_type)` tuples (set equality
  vs the pre-delete edge set) — not a count-only check.
- R-08: assert this record is distinct from the flip's `"context_deprecate"` record; idempotent
  re-deprecate emits neither cleanup event.
- Security: an unusual `relation_type` string is encoder-escaped (well-formed JSON), never corrupts
  the record or falls to `"{}"`.
- Empty removal: helper returns empty → this method is not called (no event on zero) — assert no
  `edge_cleanup` record for a zero-agent-edge deprecation, while `edges_removed = Some(0)` still renders.
