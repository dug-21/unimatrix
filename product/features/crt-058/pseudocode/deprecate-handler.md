# Component: deprecate-handler

**File:** `crates/unimatrix-server/src/mcp/tools.rs:1413` — `context_deprecate`. Insert step 6.5
between the step-6 flip and the step-8 format.

## Purpose

Orchestrate the eager cleanup: after the flip, call the helper, derive `edges_removed`, fire the
tuple audit on a non-empty success, and thread the count into the response. Owns the non-fatal
policy (C-01): a helper `Err` never propagates.

## Placement (SR-06 confirmed against the real handler)

Current steps (`tools.rs:1413–1483`): 1 identity/cap, 2 validate, 3 extract id, 4 get entry,
5 idempotency early-return (`if entry.status == Deprecated`, line 1442), 6 `deprecate_with_audit`
flip (line 1469), 7 `confidence.recompute` (line 1475, fire-and-forget), 8 `format_deprecate_success`
(line 1478).

- Insert **6.5 immediately after the step-6 flip completes (after line 1472)**, before step 7.
- Step 7 `confidence.recompute` is an independent fire-and-forget; ordering vs 6.5 is immaterial.
  Placing 6.5 right after 6 keeps flip → delete → count → audit → format contiguous (C-03).
- 6.5 sits past the step-5 guard, so an already-Deprecated re-deprecate returns at step 5 and never
  deletes (AC-07 / FR-07). The step-5 early-return (line 1443) must pass `None` for `edges_removed`.

## Identity capture (implementation detail — ordering hazard)

At step 6 the flip's `AuditEvent` construction (line 1459) MOVES `ctx.agent_id` and clones
`session_id` / `client_type`. The edge_cleanup audit (step 6.5) needs the same identity. Capture
clones BEFORE building the flip audit event:

```
agent_id_for_cleanup    = ctx.agent_id.clone()          # before line 1459 moves ctx.agent_id
session_id_for_cleanup  = ctx.audit_ctx.session_id.clone().unwrap_or_default()
attribution_for_cleanup = ctx.client_type.clone().unwrap_or_default()
```

## Pseudocode (steps 5–8)

```
# step 5 (existing) — idempotent early-return, now threads None
IF entry.status == Status::Deprecated:
    RETURN Ok(format_deprecate_success(&entry, reason.as_deref(), None, ctx.format))   # AC-07

# ... capture identity clones (above) ...

# step 6 (existing) — flip
deprecated = self.deprecate_with_audit(entry_id, reason.clone(), flip_audit_event).await?   # non-Active now

# step 6.5 (NEW) — eager cleanup, NON-FATAL (C-01)
edges_removed: Option<u64> =
    MATCH delete_agent_edges_for_entry(&self.store, entry_id).await:      # C-04 awaited inline
        Ok(tuples) =>
            IF NOT tuples.is_empty():                                     # audit only on non-empty
                self.emit_edge_cleanup_audit(                            # -> audit-emit component
                    entry_id,
                    &tuples,
                    session_id_for_cleanup,
                    agent_id_for_cleanup,
                    attribution_for_cleanup,
                )
            Some(tuples.len() AS u64)                                     # incl. Some(0); AC-02/AC-05
        Err(e) =>
            tracing::warn!(entry = entry_id, error = %e,                  # NFR-05: warn, not debug (#3448)
                           "eager edge cleanup failed")
            None                                                          # AC-06: advisory omitted; tick backstops

# step 7 (existing) — independent fire-and-forget
self.services.confidence.recompute(&[deprecated.id])

# step 8 (existing, changed arity) — thread the count
RETURN Ok(format_deprecate_success(&deprecated, reason.as_deref(), edges_removed, ctx.format))
```

## Error Handling

- Helper `Err` → `warn!` with entry id + error, `edges_removed = None`, normal success returned. The
  flip stands; the tick sweeps within ≤900s (AC-06 / FR-05). Never `?`-propagate the helper error.
- `Ok(empty)` → `Some(0)`, no audit event (guarded by `!is_empty()`), advisory renders `0` (AC-05).
- Concurrent tick already swept (R-07) → `Ok(empty)` → `Some(0)`, no panic, no audit.

## Data Flow

- **In:** `entry_id` (agent-supplied, validated, range-checked at step 2/3), flipped `deprecated`
  record, identity clones.
- **Out:** `CallToolResult` from `format_deprecate_success` with the threaded `edges_removed`.
- **Side effects:** at most one `context_deprecate.edge_cleanup` audit event (fire-and-forget).

## Key Test Scenarios (hints)

- AC-09 synchronous: on return (no tick, no sleep) the agent edges are already absent.
- AC-07 / R-11 idempotency: re-deprecate an already-Deprecated entry with a fresh agent edge → step-5
  return, fresh edge survives, no `edge_cleanup` audit event.
- AC-06 / R-03: inject a helper `Err` → success returned, entry Deprecated, `warn` emitted carrying
  entry id, `edges_removed = None`, agent edges still present; then run the compaction → swept.
- R-11 ordering: assert the delete matches because the entry is non-Active at 6.5 (flip precedes delete).
- R-08 double-audit: assert the flip audit (`"context_deprecate"`) and the cleanup audit
  (`"context_deprecate.edge_cleanup"`) are two distinct records.
- AC-10 chokepoint-exclusion: drive `context_correct` (successor-bearing) → no `edge_cleanup` audit
  event, inbound edge survives (helper never invoked on the repointable path).
