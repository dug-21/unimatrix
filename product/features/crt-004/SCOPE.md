# crt-004: Co-Access Boosting

## Problem Statement

When an agent searches for "error handling conventions," they receive a ranked list of entries based on embedding similarity and confidence. But the system has no awareness that certain entries are *naturally retrieved together* -- that "error handling conventions" is almost always retrieved alongside "Result type patterns" and "logging conventions." An experienced human would know these entries form a coherent knowledge cluster and proactively surface them. Unimatrix does not.

The raw data for this insight already exists. Every retrieval operation (`context_search`, `context_lookup`, `context_get`, `context_briefing`) records which entry IDs were returned together via the AUDIT_LOG `target_ids` field (vnc-001) and updates `access_count` via the usage pipeline (crt-001). But this co-retrieval signal is discarded -- no structure accumulates the fact that entries A and B appear in the same result set repeatedly.

Co-access boosting is the lightweight version of PageRank on the access graph. Instead of computing authority scores across a full link graph, it tracks pairwise co-retrieval frequency and uses that to boost entries that are frequently retrieved alongside a given search result. This captures 80% of the value (surfacing related knowledge that agents consistently need together) at 20% of the complexity (no iterative convergence, no full graph materialization).

Without crt-004, search results are isolated -- each entry is scored independently. The system cannot answer "what else do agents typically need when they retrieve this entry?" This is the difference between a search engine (returns what you asked for) and a knowledge engine (returns what you need).

## Goals

1. **Track pairwise co-access frequency** -- Record which entries are retrieved together in the same tool call. When a search/lookup/briefing returns entries {A, B, C}, record co-access pairs (A,B), (A,C), (B,C) with an incrementing counter. Use a dedicated CO_ACCESS table in redb for O(1) lookup of co-access partners for any entry.

2. **Session-scoped deduplication for co-access recording** -- Prevent the same agent from inflating co-access counts by retrieving the same set of entries repeatedly within a session. Extend the existing `UsageDedup` mechanism to track co-access pairs already recorded per agent per session.

3. **Boost search results with co-access signal** -- After the existing search pipeline produces results (similarity + confidence re-ranking), apply a co-access boost that promotes entries frequently co-retrieved with the top-ranked results. The boost is additive to the existing rerank score, weighted to keep similarity as the dominant signal.

4. **Expose co-access data in context_status** -- Extend `StatusReport` to include co-access statistics: total co-access pairs tracked, top co-access clusters (groups of entries frequently retrieved together), and co-access coverage (what percentage of active entries have co-access data).

5. **Decay stale co-access relationships** -- Co-access pairs that have not been reinforced within a configurable time window should decay, preventing historical retrieval patterns from permanently influencing results when usage patterns change. Use a simple last-updated timestamp per pair with a staleness threshold.

6. **Co-access as a confidence factor** -- Add a seventh factor to the confidence composite formula: entries that are frequently co-accessed with high-confidence entries should receive a co-access affinity boost. This captures the "guilt by association" signal -- entries that travel in good company are more likely to be useful.

## Non-Goals

- **No full graph materialization.** Co-access is tracked as pairwise counts, not as a graph structure. No graph database, no adjacency lists, no PageRank iteration. The "80/20 PageRank" framing from the vision means pairwise frequency, not actual iterative authority computation.
- **No transitive co-access.** If A co-occurs with B and B co-occurs with C, that does NOT imply A co-occurs with C. Only direct pairwise co-retrieval is tracked.
- **No cross-session co-access.** Co-access is computed within a single tool call's result set. Entries retrieved in separate tool calls by the same agent in the same session are not co-accessed. The unit of co-access is the result set, not the session.
- **No new tools.** Co-access data is surfaced through existing tools (`context_search` result boosting, `context_status` reporting). No `context_coaccessed` tool.
- **No UI for co-access visualization.** That is mtx-002 (Knowledge Explorer) territory.
- **No background co-access computation.** All recording is inline (fire-and-forget in the usage pipeline). No batch jobs or scheduled tasks.
- **No per-agent co-access profiles.** Co-access is global across all agents. Different agents retrieving the same pair reinforces the same counter.
- **No co-access influence on non-search retrieval.** `context_lookup` (deterministic) and `context_get` (by ID) return exactly what was requested. Co-access boosting applies only to `context_search` (semantic) and `context_briefing` (compiled orientation).

## Background Research

### Existing Infrastructure

**AUDIT_LOG (vnc-001):** Every tool call is audited with an `AuditEvent` containing `target_ids: Vec<u64>` -- the entry IDs returned by the operation. This is the raw co-access signal. However, using AUDIT_LOG as the source of co-access data would require scanning the entire log and cross-referencing events. A dedicated co-access table is more efficient for O(1) lookups.

**Usage pipeline (crt-001):** The `record_usage_for_entries` method in `server.rs` is called for every retrieval with the full list of returned entry IDs. This is the natural integration point for co-access recording -- the IDs are already collected, dedup is already applied, and the fire-and-forget pattern is established.

**UsageDedup (crt-001):** Session-scoped in-memory dedup using `HashSet<(String, u64)>` for access counting and `HashMap<(String, u64), bool>` for vote tracking. Co-access dedup would use `HashSet<(u64, u64)>` (ordered pairs) keyed by agent_id, tracking which pairs have already been recorded this session.

**Confidence formula (crt-002):** Six-factor additive weighted composite: base (0.20) + usage (0.15) + freshness (0.20) + helpfulness (0.15) + correction (0.15) + trust (0.15). Weights sum to 1.0. Adding a seventh factor requires redistributing weights. The function pointer pattern in `record_usage_with_confidence` passes `compute_confidence` as `Option<&dyn Fn(&EntryRecord, u64) -> f32>`, keeping the store crate formula-independent.

**Search re-ranking (crt-002):** `rerank_score(similarity, confidence) = 0.85 * similarity + 0.15 * confidence`. Co-access boosting would add a third term or modify the blend.

**redb table patterns:** All existing tables use simple key types: `u64`, `&str`, `(&str, u64)`, `(u8, u64)`, `(u64, u64)`. A co-access table would use `(u64, u64)` keys (ordered pair of entry IDs) mapping to co-access metadata (count, last_updated timestamp).

**Schema evolution:** New fields on EntryRecord require scan-and-rewrite migration. However, co-access data is relational (pairwise), not per-entry, so it belongs in a separate table, not on EntryRecord. No schema migration needed.

### Scale Considerations

For a knowledge base with n active entries, the maximum number of co-access pairs is n*(n-1)/2. At Unimatrix's expected scale (100-2000 entries), this is 4,950-1,999,000 theoretical pairs. In practice, only a fraction will have co-access data -- entries that are never retrieved together have no pair. With typical search results returning 5-10 entries, and assuming 10-50 unique searches per session, the number of active co-access pairs will be orders of magnitude smaller than the theoretical maximum.

Storage per co-access pair: 8 bytes (entry_id_a) + 8 bytes (entry_id_b) + 4 bytes (count) + 8 bytes (last_updated) = 28 bytes. At 10,000 active pairs: ~280KB. Negligible.

### Design Choice: Co-Access as Confidence Factor vs. Search Boost

Two options for integrating co-access signal into search results:

**Option A: Seventh confidence factor.** Add co-access affinity to the confidence formula. Requires: defining what "co-access affinity" means for a single entry (average co-access count with its partners? weighted by partner confidence?). Pro: integrates naturally with existing re-ranking. Con: the confidence formula is per-entry and computed at retrieval time -- but co-access is relational (between pairs), not an intrinsic property of an entry. Forcing it into per-entry confidence is architecturally awkward.

**Option B: Post-ranking search boost.** After the existing similarity+confidence re-ranking, apply a co-access boost that promotes entries frequently co-retrieved with already-high-ranking results. Pro: co-access stays relational, applied at query time. Con: adds a third ranking step.

**Decision: Option B as primary, with a lightweight Option A component.** The primary co-access signal is applied as a post-ranking boost in `context_search`. A lightweight per-entry co-access score (number of co-access partners weighted by partner confidence) is included as a seventh confidence factor for secondary use in briefing assembly. This avoids forcing relational data into a per-entry formula while still capturing the "entries that travel in good company" signal.

## Proposed Approach

### CO_ACCESS Table

New redb table: `(u64, u64) -> &[u8]` where the key is an ordered entry ID pair (smaller ID first) and the value is a bincode-serialized `CoAccessRecord`:

```rust
struct CoAccessRecord {
    count: u32,          // number of times this pair was co-retrieved
    last_updated: u64,   // unix timestamp of most recent co-retrieval
}
```

Ordered pairs ensure deduplication: (A, B) and (B, A) map to the same key with `min(A,B), max(A,B)`.

### Co-Access Recording

In `record_usage_for_entries`, after the existing usage recording, emit co-access pairs from the returned entry IDs. For a result set of size k, this produces k*(k-1)/2 pairs. At k=5, that is 10 pairs. At k=10, that is 45 pairs. Cap the result set size for co-access recording (e.g., top 10 results) to bound the quadratic growth.

### Session Dedup

Extend `UsageDedup` with a `HashSet<(u64, u64)>` tracking ordered pairs already recorded this session per agent. Prevents the same agent from inflating co-access counts by running the same search repeatedly.

### Search Boost

After step 9b (similarity+confidence re-ranking) in `context_search`:
1. Take the top result(s) as "anchor" entries
2. Look up their co-access partners in CO_ACCESS
3. For each result entry that is also a co-access partner of an anchor, add a small boost to its rerank score
4. Re-sort by boosted score

The boost is capped and weighted to preserve similarity as the dominant signal.

### Confidence Factor

Add a seventh factor `co_access_score(entry_id, co_access_partners)` to the confidence composite. This requires the store to provide a lookup function for co-access partners. The function pointer pattern allows passing this as an optional closure, keeping the store crate independent of the co-access logic. Weight redistribution: reduce existing weights proportionally to accommodate the new factor (target weight: 0.05-0.10).

### Staleness Decay

Co-access pairs with `last_updated` older than a configurable threshold (default: 30 days) are excluded from boost calculations. A periodic cleanup (piggybacked on `context_status` or a new maintenance operation) removes stale pairs from the table.

### StatusReport Extension

Extend `StatusReport` with:
- `total_co_access_pairs: u64` -- count of tracked pairs
- `active_co_access_pairs: u64` -- count of non-stale pairs
- `top_co_access_clusters: Vec<CoAccessCluster>` -- top N groups of frequently co-accessed entries

## Acceptance Criteria

- AC-01: `CO_ACCESS` redb table exists with `(u64, u64) -> &[u8]` schema, where keys are ordered entry ID pairs (min, max)
- AC-02: `CoAccessRecord` struct contains `count: u32` and `last_updated: u64`, serializable via bincode serde path
- AC-03: Co-access pairs are recorded during `record_usage_for_entries` when the result set contains 2+ entries
- AC-04: Co-access recording produces k*(k-1)/2 pairs for a result set of k entries, capped at a configurable maximum result set size (default: 10) for pair generation
- AC-05: Co-access pairs use ordered keys (min(a,b), max(a,b)) to deduplicate symmetric pairs
- AC-06: Co-access count is incremented atomically when a pair is re-encountered
- AC-07: `last_updated` timestamp is set to current time on every co-access count increment
- AC-08: Session-scoped deduplication prevents the same agent from inflating co-access counts for the same pair within a session
- AC-09: `context_search` applies a co-access boost after the existing similarity+confidence re-ranking
- AC-10: The co-access boost is additive and capped, with similarity remaining the dominant ranking signal
- AC-11: Co-access boost uses the top result(s) as anchors and promotes their co-access partners in the result set
- AC-12: Co-access pairs with `last_updated` older than a configurable staleness threshold (default: 30 days) are excluded from boost calculations
- AC-13: `context_status` reports include `total_co_access_pairs`, `active_co_access_pairs`, and top co-access clusters
- AC-14: Confidence formula expanded to seven factors with a co-access affinity component
- AC-15: Confidence weight redistribution preserves the sum-to-1.0 invariant
- AC-16: Co-access affinity for a single entry is computed from its co-access partners' confidence scores
- AC-17: `context_briefing` applies direct co-access boosting with a very small influence weight, in addition to the indirect confidence factor effect
- AC-18: Stale co-access pairs are cleaned up during `context_status` execution (piggybacked maintenance)
- AC-19: All new code has unit tests; integration tests verify co-access recording, dedup, boost, and staleness
- AC-20: Existing tests continue to pass (no regressions from confidence weight redistribution)
- AC-21: Co-access recording is fire-and-forget (does not block tool response)
- AC-22: `#![forbid(unsafe_code)]`, no new crate dependencies beyond what is already in the workspace

## Constraints

- **Confidence weights must sum to 1.0.** Adding a seventh factor requires reducing existing weights. This affects all confidence computations and must be done carefully to avoid degrading existing entry quality signals.
- **Function pointer pattern for confidence.** The `compute_confidence` function is passed as `Option<&dyn Fn(&EntryRecord, u64) -> f32>` to `record_usage_with_confidence`. Co-access affinity is computed at query time (not stored on EntryRecord), so the confidence function signature must be extended to accept co-access data, or the co-access factor must be applied separately at query time.
- **No new crate dependencies.** All functionality must use existing workspace crates (redb, bincode, tokio).
- **`#![forbid(unsafe_code)]`**, edition 2024, MSRV 1.89.
- **Object-safe traits.** Any trait extensions must maintain object safety.
- **Fire-and-forget pattern.** Co-access recording must not block tool responses. Use the same `spawn_blocking` pattern as usage recording.
- **Test infrastructure is cumulative.** Build on existing test fixtures in unimatrix-store and unimatrix-server.
- **CO_ACCESS table must be created in Store::open.** Follow the same pattern as other tables (open in begin_write during init).
- **No EntryRecord schema change.** Co-access affinity is computed at query time. No new field on EntryRecord, no schema migration.
- **Quadratic pair generation.** For a result set of size k, co-access recording generates k*(k-1)/2 pairs. At k=20, that is 190 pairs per call. Must cap k for co-access to keep write volume bounded.

## Resolved Questions

1. **Co-access affinity is computed at query time, not stored on EntryRecord.** No schema migration. No new field on EntryRecord. Co-access affinity is looked up from the CO_ACCESS table at search/briefing time. If performance becomes a concern, this decision can be revisited.

2. **Co-access confidence factor weight follows crt-002 research guidance.** The weight is determined by the ASS-008/crt-002 research recommendations for adding signals to the composite formula. The architect should reference that research when selecting the weight value.

3. **`context_briefing` receives direct co-access boosting with very small influence.** Co-access boosting applies to briefing assembly in addition to the indirect confidence effect, but with a deliberately small weight to avoid irrelevant entries being promoted by association alone.

4. **Staleness threshold is configurable, defaulting to 30 days.** Exposed as a named constant (same pattern as other configurable thresholds like `FRESHNESS_HALF_LIFE_HOURS`). Can be adjusted via future configuration (vnc-004).

## Tracking

https://github.com/dug-21/unimatrix/issues/36
