# crt-058 Architecture — Eager Agent-Authored Edge Cleanup at `context_deprecate`

## System Overview

`context_deprecate` performs a bare, successor-less status flip (`Active → Deprecated`, `superseded_by` stays NULL). After the flip, agent-authored `graph_edges` rows touching the entry become live dangling references to retired knowledge with no successor to follow. Today they are removed only by the periodic `EveryTick` orphaned-edge compaction (`run_orphaned_edge_compaction`, `background.rs:805`), up to ~900s later.

This feature pulls the tick's blanket delete **forward for the one entry being deprecated**: one synchronous, non-fatal, indexed `DELETE` of that entry's agent-authored edges, executed inside the `context_deprecate` handler immediately after the flip. The caller is told inline how many edges were removed; the removal is audited with the removed edge tuples. The `EveryTick` compaction is unchanged and remains the backstop.

This is deterministic graph maintenance — **not** self-learning, drift-adaptation, detection, or a governance nudge. It resolves the dangling-reference condition (deletes) rather than flagging it. It adds no table, migration, or lifecycle: it reuses `graph_edges`, `write_pool_server()`, and the existing endpoint indexes.

## Component Breakdown

| Component | File / Site | Responsibility (this feature) |
|-----------|-------------|-------------------------------|
| `context_deprecate` handler | `crates/unimatrix-server/src/mcp/tools.rs:1413` | Orchestration: after the step-5 idempotency return and the step-6 flip, invoke the eager delete, fire the removal audit, thread the count into the response. |
| Eager-delete helper (NEW) | `crates/unimatrix-server/src/mcp/edge_write.rs` (beside `delete_graph_edge:244`) | Single `DELETE ... RETURNING` of the entry's agent-authored edges (both directions) on `write_pool_server()`; returns the removed tuples. |
| Response formatter | `crates/unimatrix-server/src/mcp/response/mutations.rs:16` | `format_status_change` / `format_deprecate_success` gain an optional `edges_removed` advisory, additive across Summary/Markdown/Json. |
| Audit path | `crates/unimatrix-server/src/server.rs:650` (`audit_fire_and_forget`) | Emit one fire-and-forget `AuditEvent` recording entry id, count, and removed tuples. |
| `run_orphaned_edge_compaction` | `crates/unimatrix-server/src/background.rs:805` | UNCHANGED. Standing backstop; the eager set must remain a strict subset of what this removes (see Invariant). |

The eager delete is a **new single statement**, not a reuse of `delete_graph_edge` (which is per-`(source_id,target_id,relation_type)` triple) or `redirect_graph_edge` (per-edge, transactional). No by-endpoint delete helper exists; this adds the first one.

## Insertion Point (exact)

In `context_deprecate` (`tools.rs:1413`), the current steps are: 1 identity, 2 validate, 3 extract id, 4 get entry, 5 idempotency early-return (`if entry.status == Deprecated { return … }`, ~line 1442), 6 `deprecate_with_audit` (flip), 7 `confidence.recompute` (fire-and-forget), 8 `format_deprecate_success`.

**Insert a new step 6.5, between the step-6 flip and step-8 format** (after step-6, before or after step-7 — step 7 is independent fire-and-forget, so ordering vs it is immaterial; place 6.5 immediately after step 6 to keep the flip → delete → count → audit → format flow contiguous):

```
6.   let deprecated = self.deprecate_with_audit(entry_id, reason, audit_event).await?;   // flip → non-Active
6.5. let removed = delete_agent_edges_for_entry(&self.store, entry_id).await;             // NON-FATAL
     //   Ok(tuples)  -> edges_removed = Some(tuples.len()); fire removal audit with tuples
     //   Err(e)      -> warn!(...); edges_removed = None (advisory omitted); tick backstops
7.   self.services.confidence.recompute(&[deprecated.id]);
8.   Ok(format_deprecate_success(&deprecated, reason, edges_removed, ctx.format));
```

Ordering constraints satisfied (SR-06):
- **After the flip (step 6):** the delete predicate keys on the entry id being non-Active — it must run after the status flip so the entry is Deprecated. (The predicate does not itself test status; running post-flip is what makes it a subset of the tick — see Invariant.)
- **After the step-5 idempotency guard:** a re-deprecation of an already-Deprecated entry returns at step 5 and never reaches 6.5 — no redundant delete, no advisory (AC-07).
- **Synchronous before return:** the `DELETE` is `await`ed inline (AC-09). Only the audit *write* is fire-and-forget; the edges are gone before the response is formatted.

## Data Flow

```
context_deprecate(id)
  ├─ step 5: already Deprecated? ── yes ─► return success (no delete, no advisory)   [AC-07]
  │                                └ no
  ├─ step 6: deprecate_with_audit ─► entry flipped Active→Deprecated (superseded_by NULL)
  ├─ step 6.5: delete_agent_edges_for_entry(store, id)
  │      DELETE FROM graph_edges
  │        WHERE (source_id = ?id OR target_id = ?id) AND source = 'agent'
  │        RETURNING source_id, target_id, relation_type          [write_pool_server()]
  │      ├─ Ok(tuples) ─► count = tuples.len()
  │      │                 audit_fire_and_forget(edge-cleanup event { id, count, tuples })  [AC-03]
  │      │                 edges_removed = Some(count)                                       [AC-02, AC-05]
  │      └─ Err(e)   ─► warn!(entry=id, error=e, "eager edge cleanup failed");               [AC-06]
  │                     edges_removed = None    (advisory omitted; tick will sweep on next pass)
  ├─ step 7: confidence.recompute (fire-and-forget, independent)
  └─ step 8: format_deprecate_success(entry, reason, edges_removed, format)                  [AC-02]
```

## The eager ⊆ tick Invariant (SR-02, primary constraint)

**Claim.** For a bare-deprecated entry `e`, the set of edges the eager delete removes is a strict subset of the set the `EveryTick` compaction would remove for `e`.

**Why it holds.** After the step-6 flip, `e` is non-Active with `superseded_by` NULL.
- Tick Phase 1 (`repoint_deprecated_target_edges`) only repoints inbound agent edges when the target is Deprecated **with a successor** (`superseded_by IS NOT NULL`). A bare deprecation has no successor, so Phase 1 repoints/keeps **nothing** for `e`.
- Tick Phase 2 blanket-deletes every edge with a non-Active endpoint, **all sources** — so every edge touching `e` (inbound or outbound) is deleted by the tick.
- The eager delete removes only the `source='agent'` edges touching `e` — a strict subset of "all edges touching `e`."

**The one way it breaks:** if the eager delete ever runs on a Deprecated entry that **has a successor**, Phase 1 would repoint an inbound agent edge (keep it) that the eager delete destroys → eager ⊄ tick → lost referrer + divergence. The chokepoint prevents this structurally: `context_deprecate` → `deprecate_with_audit` never sets `superseded_by`; the successor-setting path (`correct_entry`) is excluded and already repoints its own inbound edges. The eager delete therefore only ever runs on successor-less entries.

**Enforcement mechanism (testable, not prose) — ADR-003.** The invariant is made executable by a test that runs **both real functions** against parallel fixtures:
1. Seed `e` with one edge per (direction × source) — inbound/outbound × {`agent`, `nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8`}.
2. On fixture A: bare-deprecate `e`, run the eager helper, capture removed tuple set `R`.
3. On identical fixture B: bare-deprecate `e`, run `run_orphaned_edge_compaction`, capture removed tuple set `T`.
4. Assert `R ⊆ T` **and** `R` equals exactly the two `agent` edges (per-source discrimination, also covers SR-03).

Because the test invokes the actual eager predicate and the actual tick, any future widening of the eager predicate (e.g. adding a machine source) or narrowing of the tick (e.g. the tick gaining a `source` filter that keeps agent edges) breaks `R ⊆ T` or the exact-set assertion and fails the test. A companion assertion pins the structural guard: `context_deprecate` leaves `superseded_by` NULL, so the eager path never sees a successor-bearing entry. Cite bugfix-458 (#3910), bugfix-879 (#5417).

## Error / Non-Fatal Handling (SR-05)

- The eager delete is an optimization; a failure must never affect the deprecation result (Constraint, AC-06). On `Err`, log once at `warn!` (with entry id + error), set `edges_removed = None`, and continue to a normal success response.
- Mirror the established fire-and-forget discipline: `confidence.recompute` (`services/confidence.rs:131`) and `audit_fire_and_forget` (`server.rs:650`). The `warn!` is a real failure signal, **not** an expected-suppressed error — do not downgrade it to `debug` (#3448 fire-and-forget log discipline).
- **Standing coupling (SR-05):** correctness of the swallowed failure depends on `run_orphaned_edge_compaction` remaining the blanket backstop. This is a dependency invariant: any future change that removes or narrows the compaction must re-verify that a swallowed eager-delete failure still gets swept. Recorded in ADR-001.

## Response-Surface Contract (SR-04)

`format_status_change` (`mutations.rs:16`) gains one parameter; the change is additive and backward-compatible.

```rust
pub fn format_status_change(
    entry: &EntryRecord,
    action: &str,
    status_key: &str,
    status_display: &str,
    reason: Option<&str>,
    edges_removed: Option<u64>,   // NEW — None = advisory omitted; Some(n) = n edges removed (incl. Some(0))
    format: ResponseFormat,
) -> CallToolResult
```

`Option<u64>` encodes **ran-vs-failed**, the value encodes **count**:
- `Some(n)` — the eager delete ran; render the count in every format (including `Some(0)`, per AC-05: a no-edge deprecation reports a zero count).
- `None` — the eager delete failed or the path was not run (quarantine/restore); omit the advisory entirely.

Per-format rendering (all three MUST surface `Some(n)` and MUST omit `None`):

| Format | `Some(n)` rendering | `None` |
|--------|--------------------|--------|
| Summary | append ` \| {n} edges removed` to the existing line | line unchanged |
| Markdown | add line `**Edges removed:** {n}` | no line |
| Json | add field `"edges_removed": n` | field absent |

Call-site updates: `format_deprecate_success` gains an `edges_removed: Option<u64>` param and forwards it; `format_quarantine_success` and `format_restore_success` pass `None` (they delete no edges); the `context_deprecate` handler passes `Some(count)` / `None`. SR-04 test discipline: a **behavioral per-format matrix** asserting the count appears in each of the three formats for `Some(n)`, is absent for `None`, and renders `0` for `Some(0)` — plus an audit-record-content assertion. Not a call-count or string-presence-only check (#5427).

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `context_deprecate` handler | `async fn context_deprecate(&self, Parameters<DeprecateParams>, RequestContext) -> Result<CallToolResult, ErrorData>` | `mcp/tools.rs:1413` |
| Step-5 idempotency return | `if entry.status == Status::Deprecated { return Ok(format_deprecate_success(...)) }` | `mcp/tools.rs:1442` |
| Flip | `deprecate_with_audit(entry_id, reason, audit_event) -> Result<EntryRecord, ServerError>` → `change_status_with_audit` | `server.rs:949` → `server.rs:1089` |
| Eager-delete helper (NEW) | `async fn delete_agent_edges_for_entry(store: &Store, entry_id: u64) -> Result<Vec<RemovedEdge>, EdgeDeleteError>` | `mcp/edge_write.rs` (new, beside `delete_graph_edge:244`) |
| `RemovedEdge` (NEW) | `struct RemovedEdge { source_id: u64, target_id: u64, relation_type: String }` | `mcp/edge_write.rs` (new) |
| Eager predicate (LOCKED) | `DELETE FROM graph_edges WHERE (source_id = ?1 OR target_id = ?1) AND source = ?2 RETURNING source_id, target_id, relation_type` | `mcp/edge_write.rs` (new) |
| Provenance constant | `EDGE_SOURCE_AGENT: &str = "agent"` (the single agent/human-authored `source` value) | `mcp/edge_write.rs:28` |
| Endpoint indexes | `idx_graph_edges_source_id` (`db.rs:969`), `idx_graph_edges_target_id` (`db.rs:972`) — cover the OR-union | schema |
| Write pool | `store.write_pool_server()` — same pool as compaction DELETE and `delete_graph_edge` | — |
| Formatter (CHANGED) | `format_status_change(..., edges_removed: Option<u64>, format)`; `format_deprecate_success(entry, reason, edges_removed, format)` | `mcp/response/mutations.rs:16,54` |
| Audit emit | `self.audit_fire_and_forget(event: AuditEvent)` | `server.rs:650` |
| `AuditEvent` | struct with `operation: String`, `target_ids: Vec<u64>`, `detail: String`, `metadata: String` (JSON, sentinel `"{}"`) | `unimatrix-store/schema.rs:360` |
| Tick backstop (UNCHANGED) | `run_orphaned_edge_compaction(store: &Store)`; Phase 2 predicate `source_id NOT IN Active OR target_id NOT IN Active` | `background.rs:805` |

### Audit record shape

One `AuditEvent` via `audit_fire_and_forget`, emitted only when `Ok(tuples)` and `!tuples.is_empty()`:
- `operation`: `"context_deprecate.edge_cleanup"` (distinct from the flip's `"context_deprecate"` audit)
- `target_ids`: `[entry_id]`
- `detail`: `"eager edge cleanup: removed {count} agent-authored edge(s) for deprecated entry #{id}"`
- `metadata`: JSON array of removed tuples `[{"source_id":..,"target_id":..,"relation_type":".."}, ...]` for reconstructability (SR-01, ADR-002)

## Constraints Restated (traceability)

- Non-fatal, synchronous, `write_pool_server()`, single indexed statement both directions agent-only, after step-5 guard, after flip, additive response surface, chokepoint-only, no new table/migration/tick change — all satisfied above (AC-01…AC-09).
- Predicate LOCKED to exactly `(source_id=? OR target_id=?) AND source='agent'` (SR-01); provenance enumeration-bound and subset-safe (SR-03, ADR-001).

## Open Questions

None blocking. Two items flagged forward (not blockers):
- **SR-05 standing dependency:** any future change to `run_orphaned_edge_compaction` must re-verify the backstop guarantee. Captured as a coupling note in ADR-001 — recommend the delivery tester leave a comment linking the eager helper to the compaction so the coupling is discoverable.
- **SR-03 provenance drift:** if a new `EDGE_SOURCE_*` for human/other agent-authored edges is ever added, the inclusive `source='agent'` eager filter will not cover it (subset-safe — the tick still sweeps). The per-source discrimination test (ADR-003) will surface the new source as "not eagerly removed," prompting a conscious decision. No action now.
