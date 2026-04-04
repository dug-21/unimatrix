# crt-046 Researcher Report

## Summary

Researched the problem space for behavioral signal delivery (ASS-040 Group 6). SCOPE.md written to `product/features/crt-046/SCOPE.md`.

## Key Findings

### Schema state (v21, current)

- `cycle_events`: `goal_embedding BLOB` column confirmed present. Encoding helpers `encode_goal_embedding` / `decode_goal_embedding` are `pub` in `unimatrix-store::embedding`. `get_cycle_start_goal_embedding` store method does NOT exist yet — must be added (analogous to `get_cycle_start_goal`).
- `observations`: `phase TEXT` column and `idx_observations_topic_phase` composite index confirmed present.
- `graph_edges`: columns are `id, source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only, metadata`. UNIQUE constraint on `(source_id, target_id, relation_type)`. The attribution field is `source` — not `signal_origin` (the roadmap used that term but no such column exists).
- `goal_clusters` table does NOT exist yet — must be created at v22.

### context_cycle_review hook point

Step 8a (store_cycle_review) runs after full pipeline, before step 11 (audit). New step 8b should insert between these. The memoisation cache-hit early return (step 2.5) bypasses 8b — this is an open question for the human (see OQ-06 in SCOPE.md).

### Co-access pair recovery

No in-memory structure survives to review time. The durable record is `observations.input` JSON for `tool='context_get'` rows. `load_sessions_for_feature` + `load_observations_for_sessions` is the proven two-hop pattern already used by the retrospective pipeline. The entry ID is in the `id` field of the input JSON.

### context_briefing extension point

`derive_briefing_query` step 2 already reads `session_state.current_goal`. Goal embedding for the current cycle is stored in `cycle_events` (requires a DB read per briefing call). Alternative: cache in `SessionState.current_goal_embedding`. This is an open question (OQ-05).

### Briefing blending

`IndexBriefingService::index()` currently calls `SearchService.search()` once and filters. Goal-cluster blending must inject IDs before or after that call. Cold-start path (NULL embedding, empty table) must be a zero-branch passthrough.

## Scope Boundaries

**Ships in crt-046**:
- Schema v22: `goal_clusters` table + index
- `get_cycle_start_goal_embedding` store method
- `insert_goal_cluster` store method
- `query_goal_clusters_by_embedding` store method (cosine top-K)
- Step 8b in `context_cycle_review`: edge emission + goal-cluster insert
- `context_briefing` goal-conditioned blending with cold-start fallback

**Deferred**:
- Phase-stratified briefing weighting (S6/S7 — future feature)
- Retention policy for `goal_clusters` (Group 7 / Group 8 territory)
- `context_status` surfacing of goal-cluster count

## Risks

1. `mcp/tools.rs` is already large; step 8b logic must be extracted to a service module to stay under 500-line file limit.
2. The `UNIQUE(source_id, target_id, relation_type)` constraint means a behavioral edge for a pair already owned by NLI is silently dropped — behavioral weighting is never applied. Human should decide if this is acceptable (OQ-02).
3. Parsing `observations.input` JSON is best-effort. Malformed rows must be skipped without failing the handler.
4. Per-cycle pair cap (proposed 200) prevents pathological O(N^2) cases but needs a concrete constant.

## Open Questions for Human

Seven open questions are documented in SCOPE.md (OQ-01 through OQ-07). The two most consequential:
- **OQ-01**: observations vs audit_log as the source for context_get entry IDs.
- **OQ-02**: INSERT OR IGNORE vs INSERT OR REPLACE on Informs edge conflict with NLI.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 18 entries; most relevant: #3397 (derive_briefing_query ADR), #3409 (SubagentStart goal-present branch), #3937 (NLI neutral-zone pattern). Confirmed no prior work on behavioral edge emission or goal-cluster retrieval.
- Stored: entry #4108 "Recovering behavioral co-access pairs at cycle review time via observations + INSERT OR IGNORE for additive Informs edges" via /uni-store-pattern
