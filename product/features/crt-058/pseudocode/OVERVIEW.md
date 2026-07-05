# crt-058 Pseudocode Overview — Eager Agent-Authored Edge Cleanup at `context_deprecate`

One crate (`unimatrix-server`). Four delivery components plus one UNCHANGED dependency
(`run_orphaned_edge_compaction`) that the subset test invokes as a real function.

## Components

| Component | File / Site | Pseudocode |
|-----------|-------------|-----------|
| eager-delete-helper | `mcp/edge_write.rs` (new fn + `RemovedEdge`, beside `delete_graph_edge:244`) | `eager-delete-helper.md` |
| deprecate-handler | `mcp/tools.rs:1413` `context_deprecate` (new step 6.5) | `deprecate-handler.md` |
| response-formatter | `mcp/response/mutations.rs:16` (`format_status_change` + wrappers) | `response-formatter.md` |
| audit-emit | `server.rs:650` (new helper beside `audit_fire_and_forget`) | `audit-emit.md` |
| tick backstop (UNCHANGED) | `background.rs:805` `run_orphaned_edge_compaction` | no change; subset-test dependency |

## Data Flow (flip → delete → count → audit → format)

```
context_deprecate(id)
  step 5  already Deprecated? ── yes ─► format_deprecate_success(entry, reason, None, fmt)   [AC-07: no delete]
  step 6  deprecate_with_audit(id) ─► entry flipped Active→Deprecated, superseded_by NULL
  step 6.5 removed = delete_agent_edges_for_entry(store, id).await          [eager-delete-helper]
          ├─ Ok(tuples) ─► if !tuples.is_empty(): emit_edge_cleanup_audit(id, &tuples, …)  [audit-emit]
          │                edges_removed = Some(tuples.len() as u64)                        [incl. Some(0)]
          └─ Err(e)     ─► warn!(entry=id, error=e, "eager edge cleanup failed")
                           edges_removed = None
  step 7  confidence.recompute(&[id])                                        [independent fire-and-forget]
  step 8  format_deprecate_success(entry, reason, edges_removed, fmt)        [response-formatter]
```

`edges_removed` and the audit tuples both derive from ONE source of truth: the `Vec<RemovedEdge>`
returned by the single `DELETE … RETURNING` statement. Count = `tuples.len()`, never `rows_affected()`.

## Shared Types

```rust
// mcp/edge_write.rs (NEW) — serialize keys match the audit metadata JSON shape exactly
#[derive(Debug, serde::Serialize)]
pub(crate) struct RemovedEdge {
    pub source_id: u64,
    pub target_id: u64,
    pub relation_type: String,
}
```

`edges_removed: Option<u64>` — threaded caller-side through the formatter. `Option` encodes
**ran-vs-failed**, the value encodes **count**:
- `Some(n)` — eager delete ran, removed `n` (incl. `Some(0)` → renders literal `0`).
- `None` — eager delete failed, OR the path does not delete (quarantine / restore / idempotent
  re-deprecate early-return). Advisory omitted in all three formats.

Reused (unchanged): `EDGE_SOURCE_AGENT` (`edge_write.rs:28`), `EdgeDeleteError` (`edge_write.rs:76`),
`AuditEvent` (`unimatrix-store` schema; fields exercised in `tools.rs:1455`), `Outcome::Success`.

## The eager ⊆ tick Relationship (ADR-003, executable invariant)

The helper's LOCKED predicate `(source_id=?1 OR target_id=?1) AND source=?2` removes strictly the
`agent`-source subset of what the tick's Phase-2 blanket delete
(`source_id NOT IN Active OR target_id NOT IN Active`, all sources, `background.rs:810`) removes for
the same now-non-Active entry. Because the chokepoint (`deprecate_with_audit`) never sets
`superseded_by`, the entry is successor-less, so the tick's Phase-1 repoint
(`repoint_deprecated_target_edges`, needs `superseded_by IS NOT NULL`) rescues nothing for it — the
subset holds. The invariant is enforced by a test invoking BOTH real functions over parallel
fixtures (`R ⊆ T` and `R` == exactly the two agent edges), NOT by any runtime `superseded_by` clause
in the helper. Do not add such a clause; the predicate stays LOCKED.

## Sequencing Constraints (delivery waves)

- **Wave A** — eager-delete-helper (`edge_write.rs`) and response-formatter (`mutations.rs`) are
  independent of each other; both can land first.
- **Wave B** — deprecate-handler (`tools.rs`) + audit-emit (`server.rs`) depend on both Wave A
  components (handler calls the helper, threads `Some(count)`/`None` into the formatter, calls the
  audit helper). Single PR; waves are ordering only.

## Cross-cutting flags for implementers / tester

- Existing formatter unit tests (`mcp/response/mod.rs:700–990`) call the OLD arities of
  `format_deprecate_success` (3-arg) and `format_status_change` (6-arg). The signature change breaks
  them at compile time (Rust arity — good). They MUST be updated to pass the new `edges_removed`
  slot. Cumulative test infra — extend, do not fork.
- Quarantine / restore call sites (`tools.rs:1976, 2008, 2046`) and the step-5 idempotent
  early-return (`tools.rs:1443`) all pass `None`.
