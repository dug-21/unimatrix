# crt-058-researcher — Agent Report

## Task
Explore the problem space for crt-058 (#895) and author SCOPE.md. Scope was reframed twice mid-task by the human (via coordinator): detect-and-nudge → eager edge cleanup at deprecation time → both-directions → agent/human-authored-only. Final scope below.

## Deliverable
- `product/features/crt-058/SCOPE.md` (settled — no open questions)

## Final Scope (as-settled)
Eagerly delete agent/human-authored graph edges touching a deprecated entry at `context_deprecate` time, both directions, filtered to `source = 'agent'`; warn the caller inline (a); audit the removal (b); non-fatal; tick is the backstop; chokepoint-only.

Final predicate: `DELETE FROM graph_edges WHERE (source_id = ?entry OR target_id = ?entry) AND source = 'agent'` on `write_pool_server()`, after the step-5 idempotency early-return in the `context_deprecate` handler.

## Key Findings (verified against the crt-058 worktree)
- **`context_deprecate` deletes zero edges today.** `change_status_with_audit` (server.rs:1089-1170) only flips `entries.status` via `update_entry_status_extended` + fire-and-forget audit; `deprecate_with_audit` delegates to it; handler steps 5-8 touch no edges. All edge removal for a deprecated entry is currently deferred to the EveryTick compaction (~900s).
- **Chokepoint complete.** `deprecate_with_audit` (server.rs:949) is reached only via the `context_deprecate` handler (tools.rs:1413); idempotency early-return at step 5 (~line 1442) precedes the insertion site. `context_correct` uses a separate successor-setting path (already repoints inbound via `repoint_deprecated_target_edges`, background.rs:838).
- **Tick pulled forward.** `run_orphaned_edge_compaction` Phase 2 (background.rs:805) blanket-deletes `source_id NOT IN Active OR target_id NOT IN Active` on `write_pool_server()`; a bare deprecation (no successor) is never repointed by Phase 1, so all its edges are doomed — the eager delete does a subset early.
- **Provenance values enumerated.** `graph_edges.source` ∈ {`agent` (EDGE_SOURCE_AGENT, edge_write.rs:28), `nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8` (read.rs:1751-1803)}. No distinct `human` value in the edge source column (the `"human"` strings are registry bootstrap identity + cycle-review attribution). All agent/human-directed edge writes bind `EDGE_SOURCE_AGENT`. Filter = `source = 'agent'`, matching the compaction Phase 1 repoint's F2 trusted-agent class (background.rs:849).
- **No by-endpoint delete helper exists.** `delete_graph_edge` (edge_write.rs:244) is single-edge by (source,target,relation) on `write_pool_server()`; the eager cleanup is a new single indexed statement (or a small sibling fn), reusing that pool. Indexes `idx_graph_edges_source_id` (db.rs:969) + `idx_graph_edges_target_id` (db.rs:972) serve the OR predicate.
- **Non-fatal precedent.** `confidence.recompute` (confidence.rs:131) and `audit_fire_and_forget` (server.rs:650) are the fire-and-forget templates; audit pipeline already wired (option b near-zero cost).
- **Response surface.** `format_status_change` (mutations.rs:16) has no advisory slot — inline count needs a small additive `edges_removed` thread-through across Summary/Markdown/Json.
- **Alignment (bugfix-458): eager ⊆ tick.** Eager predicate (id + `source='agent'`) removes a strict subset of the tick predicate (status, all sources) for the entry, since the entry is non-Active post-flip. No divergence, no ghost records. Flagged for the architect to preserve.

## Open Questions
None — scope settled.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search (#891/#895, DependencyOnDeprecatedRule, cohesion metric, context_cycle_review, compaction). Surfaced and applied: vnc-015 ADR-004 #4421 (the retired rule's origin — now a non-goal); col-029 ADR-003 #3595 / lesson #3619 (read_pool vs write_pool — informed the corrected write-pool constraint once scope became a write); the compaction Phase 1 repoint F2 discipline (source-column filter) informed the `source='agent'` provenance filter. No prior entry contradicts the settled scope; #891's retirement is reflected as a non-goal.
- Stored: attempted to store a pattern "Resolve-at-source-event with eager cleanup ⊆ periodic backstop" — store failed (anonymous agent lacks Write capability; non-blocking). Text preserved here for a capable agent to persist:
  > When a periodic maintenance tick blanket-cleans a table by endpoint-status, a rare terminal source event (e.g. context_deprecate) can pull that cleanup forward for the single affected id, provided: a COMPLETE chokepoint (one production caller), an index already serving the by-id predicate, and the eager predicate kept a strict SUBSET of the tick's predicate (eager ⊆ tick — no divergence, no ghost records, tick stays the backstop). Do it after the idempotency early-return, NON-FATAL (mirror confidence.recompute / audit_fire_and_forget — errors log+emit-nothing, never propagate), synchronous, on write_pool_server(). Filter provenance on the source COLUMN (source='agent'), not a relation-type blocklist (F2, background.rs:849). Contrast with capture-before-destroy (new table on the tick path) and with a starved on-demand read at a later batch point (why vnc-015's DependencyOnDeprecatedRule was retired in #891).
