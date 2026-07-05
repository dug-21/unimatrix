# crt-058 — Eager Inbound-Edge Cleanup at Deprecation Time

## Problem Statement

`context_deprecate` is a rare, terminal operation: an agent uses it when an entry is no longer needed or is causing harm, with no intent to correct or replace it (`superseded_by` stays NULL). After the flip, some Active entries may still hold `Prerequisite`/other edges pointing at the now-deprecated entry — live dangling references to retired knowledge with no successor to follow.

`context_correct` already handles the analogous condition for its case: it repoints inbound edges onto the Active successor (`repoint_deprecated_target_edges`, `background.rs:838`). Deprecation has **no successor**, so the parallel corrective action is to **delete** those inbound edges rather than repoint them.

Today those edges are only removed later, by the `EveryTick` orphaned-edge compaction (`run_orphaned_edge_compaction`, `background.rs:805`, Phase 2 blanket delete). Between the deprecation and the next tick (~900s), live entries keep dangling references to the dead entry.

This feature makes a product decision: **clean those edges up eagerly, at the deprecation event, for this specific entry** — pulling the tick's blanket delete forward for the one entry being deprecated — instead of waiting for the periodic sweep. It is an **enhancement**, not a bug fix (#891 already retired the earlier, tick-starved detection rule/metric).

## Value / Framing

Integrity / data-accuracy: eagerly **resolve** dangling references by deleting them at the source event, keeping the graph's dependency data accurate the instant an entry is retired rather than up to a tick later. This is a *resolution*, not a flag — the mechanism-without-outcome concern raised in the #895 zero review is moot because we act on the condition (delete) rather than merely reporting it.

Per project memory, integrity is a documented rationale, not a product goal/vision pillar. Keep the feature minimal: one synchronous delete, an inline count to the caller, and an audit record. No new persistence, no defensive scaffolding.

Explicitly **not self-learning / not drift-adaptation** — this is deterministic graph maintenance, feeding no model, confidence, or adaptation path.

## Goals

1. At `context_deprecate` time, after the entry is flipped to Deprecated, synchronously delete the **agent/human-authored** graph edges touching it — inbound (`target_id = entry`) and outbound (`source_id = entry`), filtered to `source = 'agent'` — so no live entry retains an agent-declared dangling reference to the retired entry. System/machine-generated edges are left to the EveryTick compaction (they are disposable derived data, never regenerated to a non-Active endpoint).
2. Warn the calling agent inline (option a) with the count of edges removed.
3. Record the removal in the audit log (option b).
4. Keep the eager delete non-fatal — a failure must never affect the deprecation result; the EveryTick compaction remains the backstop.
5. Add no new table, schema migration, or prune lifecycle; reuse the existing `graph_edges` write path and indexes.

## Non-Goals

- **Not self-learning / not drift-adaptation.** Deterministic graph maintenance only.
- **Not detection / not a governance nudge.** There is no interpretation, validation, filtering by edge semantics, or capped referrer list. The condition is resolved (edges deleted), not flagged.
- **No relation-type filtering; provenance-filtered to agent/human-authored.** Agent-authored edges are removed regardless of relation type (`Prerequisite`, `Supports`, `Contradicts`, etc.) — no relation semantics are interpreted. But the delete IS filtered by provenance to `source = 'agent'`.
- **System/machine edges are NOT eagerly deleted — left to the tick.** `nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8` edges are disposable derived data: the EveryTick compaction blanket-deletes them and never regenerates them to a non-Active endpoint (every generator's candidate query is Active-allowlisted on both endpoints). Eagerly deleting them buys nothing and risks divergence; the tick owns them.
- **Not a revival of `DependencyOnDeprecatedRule` or the cohesion metric** (retired in #891). No `context_cycle_review` metric, no findings table.
- **`context_correct` / successor path — out of scope.** Correction already repoints inbound edges to the Active successor (`repoint_deprecated_target_edges`); it is not a bare deprecation and needs no delete.
- **Not a change to the EveryTick compaction.** Compaction is unchanged and remains the backstop for anything the eager delete misses (entries deprecated before this feature shipped, and any eager-delete failure).
- **CoAccess cold-start migration — out of scope (accepted).** Unrelated to this cleanup; `co_access` counts do not transfer on `context_correct`. Separate future issue only.

## Background Research

Grounded in the crt-058 worktree.

**Current behavior — `context_deprecate` deletes no edges today (confirmed).** `change_status_with_audit` (`server.rs:1089-1170`) only calls `update_entry_status_extended` (flips the `entries.status` row) and fire-and-forget audit logging — no `graph_edges` operation. `deprecate_with_audit` merely delegates to it, and handler steps 5-8 (`tools.rs`) touch no edges. So both inbound and outbound edge removal for a deprecated entry is currently deferred entirely to the EveryTick compaction (~900s later). This feature adds the only edge-deletion at deprecation time.

**Chokepoint is complete.** `context_deprecate` handler (`tools.rs:1413`): step 5 is the idempotency early-return (`if entry.status == Status::Deprecated { return format_deprecate_success(...) }`, ~line 1442); step 6 calls `deprecate_with_audit`; step 7 fires `confidence.recompute` (fire-and-forget); step 8 formats. `deprecate_with_audit` (`server.rs:949` → `change_status_with_audit`, `server.rs:1089`) is the only production path to a bare Deprecated flip. `context_correct` uses a separate successor-setting path. So the eager delete has exactly one insertion site, after step 5.

**The tick this pulls forward.** `run_orphaned_edge_compaction` (`background.rs:805`) Phase 2: `DELETE FROM graph_edges WHERE source_id NOT IN (Active) OR target_id NOT IN (Active)`, run on `write_pool_server()`. Once the entry is Deprecated (non-Active), **both** its inbound (`target_id = entry`) and outbound (`source_id = entry`) edges become eligible for that blanket delete on the next tick. Phase 1 repoint only rescues targets that have a successor (`superseded_by IS NOT NULL`); a bare deprecation is never repointed, so all its edges are doomed — the eager delete simply does now what the tick will do in ≤900s.

**No existing by-endpoint delete helper to reuse.** `delete_graph_edge` (`edge_write.rs:244`) deletes a *single* edge by `(source_id, target_id, relation_type)` on `write_pool_server()`; `redirect_graph_edge` is per-edge/transactional. Neither deletes by endpoint. The eager cleanup is a **new single indexed statement** modeled on the compaction's blanket predicate, scoped to one entry id.

**Provenance (`source` column) values — enumerated.** The `graph_edges.source` column takes exactly: `agent` (`EDGE_SOURCE_AGENT`, `edge_write.rs:28`) for all agent/human-directed edge writes, and the machine generators `nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8` (`read.rs:1751-1803`). There is **no distinct `human` value** in this column — the `"human"` strings elsewhere are the agent-registry bootstrap identity (`registry.rs:46`) and cycle-review attribution, not `graph_edges.source`. Every agent/human-directed edge write binds `EDGE_SOURCE_AGENT`. So the agent/human-authored set is the single value `source = 'agent'`. This is the same "trusted agent fact" class the compaction's Phase 1 repoint filters on (`background.rs:849`, F2: filter on the source column, never a relation-type blocklist).

**Indexes and pool.** `idx_graph_edges_target_id` (`db.rs:972`) covers `WHERE target_id = ?`; `idx_graph_edges_source_id` (`db.rs:969`) covers `WHERE source_id = ?`. This is now a **write** — it must use `write_pool_server()` (the pool the compaction DELETE and `delete_graph_edge` already use). The earlier read-pool consideration is obsolete.

**Non-fatal precedent.** `confidence.recompute` (`services/confidence.rs:131`) and the audit path (`audit_fire_and_forget`, `server.rs:650`; detached-spawn in `change_status_with_audit`, `server.rs:~1163`) are the fire-and-forget templates. The audit pipeline is already wired end-to-end, so option (b) is near-zero incremental cost.

**Response surface.** `format_deprecate_success` → `format_status_change` (`mcp/response/mutations.rs:16`) currently has no advisory slot in any of the three formats (Summary/Markdown/Json). The inline count (option a) requires threading an optional `edges_removed` value through this function — a small, additive, backward-compatible change.

## Proposed Approach

In the `context_deprecate` handler, **after the step-5 idempotency early-return** and after the status flip, run a single synchronous, non-fatal, indexed `DELETE FROM graph_edges WHERE (source_id = ?entry OR target_id = ?entry) AND source = 'agent'` on `write_pool_server()` — removing agent/human-authored edges touching the entry in both directions. Capture `rows_affected()` as the removed-edge count. Then:
- **(a) inline:** thread the count into `format_deprecate_success` so the caller sees "N edges to this entry removed."
- **(b) audit:** emit an `AuditEvent` recording the removal (entry id + count) via the already-wired audit path.

On any delete error: log at `warn`, report zero / omit the advisory, and return the normal deprecation success — never propagate. The EveryTick compaction remains the backstop.

Placement note: the delete predicate keys on the entry id (now non-Active), so it must run after the flip. Simplest correct order: flip via `deprecate_with_audit`, then delete, then format.

## Acceptance Criteria

- **AC-01:** Deprecating an entry removes the agent/human-authored (`source = 'agent'`) edges touching it — both inbound (`target_id = entry`, other entries pointing at it) and outbound (`source_id = entry`, the entry's own edges) — synchronously within the `context_deprecate` call.
- **AC-02:** The `context_deprecate` response reports the count of edges removed (inline, option a).
- **AC-03:** The removal is recorded in the audit log with the entry id and count (option b).
- **AC-04:** Only agent/human-authored edges (`source = 'agent'`) are removed; machine-generated edges (`nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8`) are left untouched by the eager delete and remain the tick's responsibility. No relation-type filtering is applied within the agent-authored set.
- **AC-05:** Deprecating an entry with no such edges removes nothing, reports a zero count, and leaves the existing deprecation behavior unchanged.
- **AC-06:** A failure in the eager delete is non-fatal: `context_deprecate` still returns its normal success result; the failure is logged, not propagated; the EveryTick compaction still removes the edges on its next pass.
- **AC-07:** Re-deprecating an already-Deprecated entry (idempotent path) performs no delete and reports no removal — the cleanup sits past the step-5 early-return.
- **AC-08:** No new table, schema migration, or EveryTick-path change is introduced; the delete reuses `graph_edges`, `write_pool_server()`, and existing indexes.
- **AC-09:** The delete completes synchronously before `context_deprecate` returns (no async that would leave the edges present after the response).

## Constraints

- **Non-fatal** — the eager delete must never propagate an error into the `context_deprecate` result (mirror `confidence.recompute` / audit fire-and-forget). The delete is an optimization; the EveryTick compaction is the explicit backstop.
- **After the idempotency early-return** — placed past the handler step-5 already-Deprecated guard (`tools.rs:~1442`) so re-deprecation performs no redundant delete.
- **Synchronous** — the edges must be gone before the response returns; do not defer to an async.
- **Write path** — this is a WRITE: use `write_pool_server()` (the pool used by the compaction DELETE and `delete_graph_edge`). No read-pool.
- **Single indexed statement, both directions, agent-authored only** — one `DELETE FROM graph_edges WHERE (source_id = ?entry OR target_id = ?entry) AND source = 'agent'`, served by `idx_graph_edges_source_id` + `idx_graph_edges_target_id` (SQLite OR-by-index-union). Filter on the `source` column (F2 discipline, `background.rs:849`), never a relation-type blocklist. No per-edge loop.
- **Eager-path / tick-path alignment: eager ⊆ tick (bugfix-458 — multi-pass cleanup on the same table must not diverge).** The eager delete keys on the entry id and provenance (`(source_id = ?entry OR target_id = ?entry) AND source = 'agent'`); the tick keys on status (`source_id NOT IN Active OR target_id NOT IN Active`, all sources). Because the entry is non-Active immediately after the flip, every edge the eager delete removes is also removed by the tick — the eager set is a strict **subset** of the tick set for this entry (agent edges now; machine edges + any agent edges the eager path missed still swept by the tick). Eager ⊆ tick means no divergence and no ghost records: the eager path can only ever do a subset of the backstop's work early. This is load-bearing — flag for the architect to preserve. If the eager predicate is ever widened beyond the tick's (e.g. removing an edge the tick would repoint/keep), the subset invariant breaks; keep the eager filter a subset of the tick's status predicate.
- **No new helper indirection required** — no by-endpoint delete helper exists; add the statement inline or as a small `edge_write.rs` function beside `delete_graph_edge`, reusing its `write_pool_server()` pattern.
- **Response surface** — add an optional `edges_removed` advisory to `format_status_change`, additive and backward-compatible across Summary/Markdown/Json.
- **Chokepoint-only** — bare deprecation via `context_deprecate`; `correct_entry` excluded (already repoints inbound).

## Open Questions

None. Both open design choices are resolved by the human:
- **Direction: both** — inbound (`target_id = entry`) and outbound (`source_id = entry`).
- **Provenance: agent/human-authored only** — `source = 'agent'` (the single agent-authored value; no distinct `human` value exists in the edge `source` column). Machine-generated edges are left to the tick.

Final eager-delete predicate: `DELETE FROM graph_edges WHERE (source_id = ?entry OR target_id = ?entry) AND source = 'agent'`, run synchronously and non-fatally at `context_deprecate` after the idempotency early-return; warn the caller inline with the count (a); audit the removal (b); the EveryTick compaction remains the backstop (eager ⊆ tick).

## Tracking

GitHub Issue #895 (enhancement): https://github.com/dug-21/unimatrix/issues/895
