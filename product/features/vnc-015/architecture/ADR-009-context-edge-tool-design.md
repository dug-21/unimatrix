## ADR-009: context_edge Tool Design — Handler Structure, Validation Pipeline, Atomic Operations

### Context

The original vnc-015 scope (Phase 2a) added an `edges` parameter on `context_store` and
`context_correct` for attaching edges at entry creation/correction time. This covers the
creation path only. A critical gap remains: retargeting or retracting an edge on an existing
entry without creating a new version of the source entry. The canonical use case is supersession:
when Goal A is corrected to A', all entries that previously declared `Advances → A` should be
retargetable to `Advances → A'` without bumping their content version.

The scope addition approved after Phase 2a adds `context_edge` as the 13th MCP tool:

```
context_edge(
  mode:          "add" | "remove" | "redirect",
  source_id:     u64,
  edge_type:     String,
  target_id:     u64,
  new_target_id: u64  // redirect only
)
```

Decisions required:
1. Where does the handler live — in `tools.rs` or a new file?
2. What is the validation pipeline and its ordering?
3. How are `remove` and `redirect` implemented, especially for bidirectional `Contradicts`?
4. Is `redirect` atomic?
5. What does ownership mean on an existing-entry operation?

### Decision

**Handler placement:**

`context_edge` handler lives in `tools.rs`. It follows the same structural pattern as the 12
existing tool handlers. No new file is created for the handler itself — the handler is ~80–120
lines, consistent with other tool handlers, and the edge logic it calls delegates to
`edge_write.rs`.

`tools.rs` already has an extraction boundary via `edge_write.rs` (ADR-005). New functions
added to `edge_write.rs` for `remove` and `redirect` keep the per-file size within bounds.

**Validation pipeline (ordered):**

```
1. Capability gate:       agent_id must have Capability::Write
2. Source fetch:          fetch entry by source_id — not found → error
3. Source status:         source_entry.status must not be Quarantined or Deprecated → error
4. Self-ref check:        source_id != target_id — equal → error
5. Edge type resolution:  RelationType::from_str(&edge_type) — None → error
6. Target validation:     target_id must exist and not be quarantined (ADR-010)
   (for redirect: also validate new_target_id same way)
```

Steps 1–5 require no additional DB reads beyond the source fetch. Step 6 requires one SELECT
per target being validated (target_id for add/remove/redirect; new_target_id additionally for
redirect). All validation steps run before any mutation.

**No ownership check:**

`agent_id` is not a reliable ownership anchor in this RBAC model. The security gate is
`Capability::Write` plus source entry status (not quarantined, not deprecated). Any agent
holding `Capability::Write` may operate on any non-frozen source entry. This differs from
early drafts of this ADR that included an ownership check — that check was dropped because
multiple agents collaborate on shared knowledge and `created_by` does not reliably represent
current stewardship in a multi-agent RBAC model.

**Source entry status:**

Source must not be Quarantined or Deprecated. Editing edges on a frozen (quarantined) or
retracted (deprecated) entry is semantically incoherent — quarantined entries are hidden from
retrieval, and deprecated entries have been superseded. The restriction matches the SCOPE.md
Goal 12 requirement.

**remove mode:**

```rust
pub(crate) async fn delete_graph_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
) -> Result<(), EdgeDeleteError>;
```

Executes: `DELETE FROM graph_edges WHERE source_id = ?1 AND target_id = ?2 AND relation_type = ?3`

For `Contradicts`, executes both:
- `DELETE ... WHERE source_id = A AND target_id = B AND relation_type = 'Contradicts'`
- `DELETE ... WHERE source_id = B AND target_id = A AND relation_type = 'Contradicts'`

Both deletes run in the same fire-and-forget sequence (not background ticks) before the handler
returns. **Idempotent**: if the edge does not exist, the DELETE affects 0 rows. Success is
returned regardless (no "edge not found" error for remove — callers should not need to pre-check
existence).

A new `delete_graph_edge` function is added to `edge_write.rs`, calling `store.write_pool_server()`
directly (same pool as `write_graph_edge`). The existing `write_graph_edge` is for INSERT; a
separate function for DELETE is appropriate rather than overloading the existing function.

**redirect mode:**

Redirect = remove old edge + add new edge, **atomically** in a single SQLite transaction.

```rust
pub(crate) async fn redirect_graph_edge(
    store: &Store,
    source_id: u64,
    old_target_id: u64,
    new_target_id: u64,
    relation_type: &str,
    created_at: u64,
) -> Result<(), EdgeRedirectError>;
```

Transaction body:
```sql
BEGIN IMMEDIATE;
DELETE FROM graph_edges WHERE source_id=?1 AND target_id=?2 AND relation_type=?4;
INSERT OR IGNORE INTO graph_edges (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only, metadata)
VALUES (?1, ?3, ?4, 1.0, ?5, 'agent', 'agent', 0, '');
COMMIT;
```

For `Contradicts`, all four rows are managed in one transaction:
```sql
BEGIN IMMEDIATE;
DELETE FROM graph_edges WHERE source_id=A AND target_id=B AND relation_type='Contradicts';
DELETE FROM graph_edges WHERE source_id=B AND target_id=A AND relation_type='Contradicts';
INSERT OR IGNORE INTO graph_edges ... (A, new_B, Contradicts, ...);
INSERT OR IGNORE INTO graph_edges ... (new_B, A, Contradicts, ...);
COMMIT;
```

**Rationale for transaction on redirect (not remove):** Remove is a single destructive
operation — partial success (one direction deleted, other failed) is recoverable by re-calling
remove. Redirect is a non-idempotent compound: if the old edge is deleted and then the insert
fails, the entry has no edge at all. This is a data loss scenario for the caller. The atomicity
guarantee on redirect prevents that loss. SQLite's serialized write_pool_server writer makes
the transaction safe without concurrency issues.

**Pure graph operation:**

`context_edge` triggers no embedding recompute, no confidence update, no duplicate detection,
and no usage recording. It is a direct graph mutation only. This is consistent with the SCOPE.md
Goal 10 "pure graph operation" requirement.

**Tool count tests:**

Any test asserting exact MCP tool count must be updated from 12 to 13. The known test is
`test_default_rules_has_22_rules` — that test counts detection rules, not tools. Tool count
is likely asserted in server initialization tests or tool registration tests; the spec must
identify and update these.

### Consequences

Easier: supersession retargeting (the primary use case) is possible without creating a new
entry version. Atomic redirect prevents the half-retargeted state that a non-transactional
remove+add would leave. Any `Capability::Write` agent can operate on non-frozen entries,
enabling collaborative graph maintenance without requiring the original author to be present.

Harder: `redirect` requires a transaction on `write_pool_server()`, adding a new DB interaction
pattern to `edge_write.rs`. The `delete_graph_edge` and `redirect_graph_edge` functions must
be carefully integrated with the existing `write_graph_edge` pattern (ADR-003 partial-write
posture does not apply here — redirect is fully transactional).

Supersedes: none.
Related: ADR-001 (validation-first), ADR-002 (failure posture), ADR-003 (partial-write; note
that redirect is an exception — it IS transactional), ADR-005 (edge_write.rs module), ADR-010
(target validation).
