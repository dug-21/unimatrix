# Agent Report — crt-058-agent-1-pseudocode

## Task

Per-component pseudocode for eager agent-authored `graph_edges` cleanup at `context_deprecate`,
resolving the delivery-time closure items (R-03 atomicity, SR-06 placement, LOCKED predicate,
non-fatal handling, `edges_removed` plumbing).

## Deliverables

- `product/features/crt-058/pseudocode/OVERVIEW.md`
- `product/features/crt-058/pseudocode/eager-delete-helper.md`
- `product/features/crt-058/pseudocode/deprecate-handler.md`
- `product/features/crt-058/pseudocode/response-formatter.md`
- `product/features/crt-058/pseudocode/audit-emit.md`

## Components Covered

| Component | Site | Change |
|-----------|------|--------|
| eager-delete-helper | `mcp/edge_write.rs` | NEW `delete_agent_edges_for_entry` + `RemovedEdge`; LOCKED `DELETE … RETURNING` |
| deprecate-handler | `mcp/tools.rs:1413` | NEW step 6.5 (after step-6 flip, line 1472) |
| response-formatter | `mcp/response/mutations.rs:16` | `edges_removed: Option<u64>` added to `format_status_change` + `format_deprecate_success` |
| audit-emit | `server.rs:650` | NEW `emit_edge_cleanup_audit` helper beside `audit_fire_and_forget` |
| tick backstop | `background.rs:805` | UNCHANGED (subset-test dependency only) |

## Closure Items Resolved

- **R-03 atomicity** — helper uses a single `DELETE … RETURNING` with one `fetch_all`; count and
  audit tuples both derive from that one returned `Vec<RemovedEdge>`. No delete-then-SELECT window.
- **SR-06 placement** — confirmed against the real handler: step 6.5 goes after the step-6 flip
  (`tools.rs:1472`), past the step-5 guard (line 1442). Step-7 `confidence.recompute` (line 1475) is
  independent fire-and-forget; ordering vs 6.5 immaterial.
- **LOCKED predicate** — `(source_id=?1 OR target_id=?1) AND source=?2 RETURNING source_id,
  target_id, relation_type` on `write_pool_server()`, `?2 = EDGE_SOURCE_AGENT`. No relation_type
  widening, no runtime `superseded_by` clause.
- **Non-fatal** — helper `Err` → `warn!` (entry id + error), `edges_removed = None`, normal success.
  `Ok(tuples)` → `Some(tuples.len() as u64)`; audit only when `!tuples.is_empty()`.
- **`edges_removed: Option<u64>`** — threaded through `format_status_change` +
  `format_deprecate_success`; quarantine/restore and the idempotent early-return pass `None`;
  `Some(0)` renders a literal `0`.

## Implementer Flags (not gaps — surfaced for Wave A/B and the tester)

1. **Identity-move hazard (handler):** step-6 flip audit construction (`tools.rs:1459`) MOVES
   `ctx.agent_id`. Clone `agent_id` / `session_id` / `client_type` BEFORE that line so the
   edge_cleanup audit can reuse them. Documented in `deprecate-handler.md`.
2. **Existing formatter unit tests break at compile time:** `mcp/response/mod.rs:700–990` call the
   OLD arities of `format_deprecate_success` (3-arg) and `format_status_change` (6-arg). Must be
   updated to the new signature — cumulative test infra. The arity break is the intended tripwire.
3. **`RemovedEdge` derives `Serialize`** so its field names ARE the audit metadata JSON keys — one
   source of truth for the `{source_id, target_id, relation_type}` shape.
4. **Json `None` omission:** build the Json object as a mutable `serde_json::Value` and insert
   `edges_removed` only in the `Some` branch, so `None` omits the key rather than emitting `null`.

## Open Questions

None. R-04 (zero-case) was resolved pre-Stage-3a (ADR-004: `Some(0)` renders `0`) and is reflected
throughout. All interface names trace to architecture / existing code (verified against
`edge_write.rs`, `tools.rs`, `mutations.rs`, `server.rs`, `background.rs`).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (crt-058 pseudocode task) — surfaced the four crt-058
  ADRs (#5458–5460), #4425 (`EDGE_SOURCE_AGENT` + `created_by` convention), #3910 (all cleanup passes
  on a table must use identical status filters — the eager ⊆ tick basis), #4041 (`write_graph_edge`
  three-case `rows_affected` contract — informs the tuples-len-not-rows_affected choice). Findings
  applied directly; no novel generalizable pattern to store (this is a read-only tier).
- Deviations from established patterns: none. Helper mirrors `delete_graph_edge`'s
  `write_pool_server()` pattern; audit helper mirrors the existing `AuditEvent` construction
  (`tools.rs:1455`) and `audit_fire_and_forget` discipline; predicate is a strict subset of the
  unchanged tick.
