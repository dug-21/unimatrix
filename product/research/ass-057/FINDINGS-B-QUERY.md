# FINDINGS: ASS-057 Track B — Query APIs, Traversal & Write Path

**Spike**: ass-057 (Track B)
**Date**: 2026-05-14
**Approach**: investigation
**Confidence**: validated (all answers grounded in direct codebase evidence with file/line references)

---

## 1. Traversal API Feasibility Table

### Summary

| API | Classification | Storage change needed | resolve_supersessions impact |
|-----|---------------|----------------------|------------------------------|
| `context_neighbors` | New MCP tool, existing storage | None | New hop-level logic; entries.superseded_by walk |
| `context_subgraph` | New MCP tool, in-memory BFS | None for topology; OQ-B-1 for edge properties | Same hop-level logic × depth |
| `context_inverse` | New MCP tool, SQL antijoin | None (existing indexes sufficient at 3k) | N/A (no traversal) |
| `context_supersession_chain` | New MCP tool, recursive CTE on entries fields | None | This IS the supersession query |
| `context_current` | New MCP tool or context_get parameter | None | This IS the supersession query |
| `context_path` | New MCP tool, petgraph BFS in-memory | None | Per-hop superseded_by substitution |
| `context_batch_write` | New MCP tool + new synchronous write path | Requires synchronous bypass of analytics queue | N/A (write operation) |
| `context_filter` | New MCP tool, correlated SQL subquery | None (bounded at 3k) | N/A (no traversal) |

---

### context_neighbors(id, edge_types, direction, depth, resolve_supersessions)

**Current analog**: None. No `context_neighbors` tool exists anywhere in `tools.rs` or the tool list. `context_search` is semantic. `context_lookup` filters by category/tags. Neither traverses graph edges.

**Storage query — single hop, outgoing:**
```sql
SELECT ge.*, e.* FROM graph_edges ge
JOIN entries e ON e.id = ge.target_id
WHERE ge.source_id = ?1 AND ge.relation_type IN (?, ?, ...)
```
Incoming: same with `ge.target_id = ?1` and join on `ge.source_id`. Both directions: UNION of the two.

**Multi-hop (depth > 1)**: Cannot be a single SQL query. Requires BFS iteration. The in-memory `TypedRelationGraph` (an `Arc<RwLock<_>>` rebuilt by tick) is the right substrate. `graph_expand.rs` already implements BFS over it, but is hardcoded to 4 positive edge types (CoAccess, Supports, Informs, Prerequisite) in the outgoing direction only (`graph_expand.rs:115–136`). A `context_neighbors` with arbitrary edge types needs a new BFS variant that:
1. Accepts a caller-supplied `&[RelationType]`
2. Supports inbound traversal (current graph_expand is outgoing-only via `Direction::Outgoing`)
3. Returns edge records in addition to node IDs (graph_expand returns only `HashSet<u64>`)

The in-memory graph avoids SQL round-trips per hop. Known limitation: the in-memory graph is tick-rebuilt, so edges written within the last tick interval may not appear (see OQ-B-4).

**Classification**: New MCP tool. No storage change. Requires new BFS traversal function in `graph_expand.rs` or a sibling module.

**resolve_supersessions**: At each hop, after resolving target_id (or source_id for incoming), check `entries.superseded_by`. If non-null and resolve=true, follow the chain to the terminal. O(chain_length) entries reads per deprecated hop. Logic belongs at the Store layer as a helper function. Low complexity.

---

### context_subgraph(seed_ids, edge_types, direction, max_depth, max_nodes, resolve_supersessions)

**Current analog**: `graph_expand` does multi-hop BFS from a seed set but returns only `HashSet<u64>` (node IDs), no edge records, and is hardcoded to 4 positive edge types outgoing. Cannot be reused directly.

**In-memory graph carries edge records**: `TypedRelationGraph.inner` is `StableGraph<u64, RelationEdge>`. `RelationEdge` (`graph.rs:131–144`) carries `relation_type`, `weight`, `created_at`, `created_by`, `source`, `bootstrap_only`. Edge records ARE available from the in-memory graph via `EdgeReference`. BFS can collect both node IDs and edge records without GRAPH_EDGES reads.

**Edge metadata gap**: `RelationEdge` does NOT carry the `metadata` column from GRAPH_EDGES. The research domain requires edge properties (`contribution_kind`, `strength`, `salience`, `human_confirmed`) stored in `metadata` JSON. For a result that includes edge properties, either: (a) post-processing — after BFS collects edge (source_id, target_id, relation_type) triples, fetch `metadata` from GRAPH_EDGES for each, or (b) add `metadata: Option<String>` to `RelationEdge` and load it in `query_graph_edges()` — a one-line change in both `schema.rs` and `read.rs`. See OQ-B-1.

**Result size estimate for max_nodes=200**: ~200 entries × 1KB + ~600 edges × 150 bytes ≈ 290KB as JSON. Within LLM context window capacity. The 200-node cap is appropriate.

**Classification**: New MCP tool. No storage schema change required for topology-only results. If edge properties are required, add `metadata` to `RelationEdge` (trivial). Requires new BFS function returning `(Vec<EntryRecord>, Vec<EdgeRecord>)`.

**resolve_supersessions**: Same per-hop resolution as context_neighbors. For BFS with max_nodes cap, supersession resolution must happen before enqueuing a node (to avoid expanding deprecated intermediate nodes).

---

### context_inverse(entity_type, missing_edge_types, limit)

**Current analog**: None. `context_lookup` returns entries by category but cannot express antijoin semantics.

**SQL antijoin**:
```sql
SELECT e.id, e.title, e.topic, e.confidence
FROM entries e
LEFT JOIN graph_edges g
    ON e.id = g.target_id
    AND g.relation_type = ?1
WHERE e.category = ?2
  AND e.status = 0
  AND g.target_id IS NULL
LIMIT ?3
```

For multiple `missing_edge_types`: multiple LEFT JOINs or NOT EXISTS subqueries.

**Index analysis**: Three GRAPH_EDGES indexes exist (confirmed `migration.rs:361–385` and `db.rs:956–963`):
- `idx_graph_edges_source_id ON graph_edges(source_id)` — single column
- `idx_graph_edges_target_id ON graph_edges(target_id)` — single column
- `idx_graph_edges_relation_type ON graph_edges(relation_type)` — single column

There is NO composite index on `(target_id, relation_type)`. The LEFT JOIN uses `idx_graph_edges_target_id` to find rows by `target_id = e.id`, then applies `relation_type = ?1` as a secondary in-memory filter on the retrieved rows.

**Query plan at 3k entries**: Outer scan bounded by `idx_entries_category`. For 300 source entries: 300 × (O(log 9000) + avg_incoming_edges=3) ≈ ~4,800 operations. Estimated latency: 1–3ms. This is a millisecond query, not a tens-of-milliseconds scan.

**Application-side alternative infeasible**: `context_lookup` + application-side filtering requires N+1 MCP calls. SQL antijoin is the only correct implementation.

**Classification**: New MCP tool. No storage schema change required at stated scale. Composite index `ON graph_edges(target_id, relation_type)` recommended as optimization for scale (see OQ-B-3).

---

### context_supersession_chain(id, direction)

**Current analog**: `find_terminal_active` in `graph.rs:462–505` finds the terminal active node via DFS, but: returns only the terminal (not the full chain), is not MCP-exposed, operates on the in-memory graph snapshot.

**Critical finding — supersession model**: Supersession is stored on the `entries` table as two fields confirmed in `db.rs:542–543` and `schema.rs:67–69`:
- `entries.supersedes: Option<u64>` — "I supersede entry X" (set on the new correction)
- `entries.superseded_by: Option<u64>` — "I was replaced by entry Y" (set on the original)

`context_correct` does NOT write a GRAPH_EDGES Supersedes row. Confirmed by reading `store_correct.rs` exhaustively (zero GRAPH_EDGES INSERTs) and `write_ext.rs:correct_entry()` (steps 1–8: no GRAPH_EDGES write). `graph.rs:228–235` explicitly states: "Supersedes topology is derived from the canonical entries field, not from GRAPH_EDGES rows." SCOPE.md prior art confirms: "GRAPH_EDGES Supersedes rows skipped in Pass 2a."

**Recursive CTE — forward chain (newest entry first)**:
```sql
WITH RECURSIVE forward_chain(id, depth) AS (
    SELECT id, 0 FROM entries WHERE id = ?1
    UNION ALL
    SELECT e.id, fc.depth + 1
    FROM entries e
    JOIN forward_chain fc ON e.supersedes = fc.id
    WHERE fc.depth < 50
)
SELECT e.* FROM entries e
JOIN forward_chain fc ON e.id = fc.id
ORDER BY fc.depth;
```

**Backward chain (oldest entry first)**:
```sql
WITH RECURSIVE backward_chain(id, depth) AS (
    SELECT id, 0 FROM entries WHERE id = ?1
    UNION ALL
    SELECT e.supersedes, bc.depth + 1
    FROM entries e
    JOIN backward_chain bc ON e.id = bc.id
    WHERE e.supersedes IS NOT NULL AND bc.depth < 50
)
SELECT e.* FROM entries e
JOIN backward_chain bc ON e.id = bc.id
ORDER BY bc.depth;
```

**Performance**: Chains are short (< 10 in practice, 50 as safety cap). No indexes on `entries.supersedes` or `entries.superseded_by` (confirmed: `db.rs:572–583` lists only topic, category, status, created_at indexes). At 3k entries, each CTE step is an O(N) scan per link. For chain depth 10: ~30,000 comparisons = fast (~1–2ms). At 30k+ entries, indexes on these columns become necessary (see OQ-B-2).

**Classification**: New MCP tool. No storage schema change required at stated scale. Recommend adding `idx_entries_supersedes ON entries(supersedes)` and `idx_entries_superseded_by ON entries(superseded_by)` in the same migration as the traversal tools.

---

### context_current(id)

**Current analog**: `find_terminal_active` in `graph.rs:462–505` — finds the terminal active node via DFS on the in-memory graph. Not MCP-exposed.

**Single recursive CTE**:
```sql
WITH RECURSIVE chain(id) AS (
    SELECT id FROM entries WHERE id = ?1
    UNION ALL
    SELECT e.id
    FROM entries e
    JOIN chain c ON e.supersedes = c.id
)
SELECT e.*
FROM entries e
JOIN chain c ON e.id = c.id
WHERE e.superseded_by IS NULL
  AND e.status = 0
LIMIT 1;
```

**Can this piggyback on context_get with `follow_supersessions=true`?** Technically yes. A parameter extension to context_get that triggers this CTE would avoid a new tool. However, the semantics differ: context_get returns the requested entry; context_current returns a potentially different entry. A separate tool is semantically cleaner, but the implementation is thin.

**Classification**: New MCP tool (preferred) or context_get parameter extension (acceptable). No storage change. Implementation is ~30 lines of Rust wrapping the CTE above.

---

### context_path(from_id, to_id, edge_types, max_depth)

**Current analog**: None. `graph_ppr.rs` computes PageRank mass flow, not explicit paths. `graph_expand.rs` does BFS for candidate expansion without tracking paths or targeting a specific node.

**In-memory petgraph approach**: The `TypedRelationGraph.inner` is `StableGraph<u64, RelationEdge>`. petgraph's `algo` module is already imported (`graph.rs:21`: `use petgraph::algo::is_cyclic_directed`). petgraph's `algo::astar` and `algo::dijkstra` are available in the same `petgraph::algo` namespace and work directly on `StableGraph`.

`node_index: HashMap<u64, NodeIndex>` in `TypedRelationGraph` (`graph.rs:186`) provides O(1) lookup from entry ID to petgraph NodeIndex. A BFS from `from_id` following the specified edge types (using `edges_of_type()`) until `to_id` is found or depth exceeds `max_depth` is directly implementable. For unweighted shortest path: edge cost = 1, heuristic = 0. For edge-type filtering: return cost = infinity for excluded edge types.

**Performance at depth=5, 3k nodes, 10k edges**: BFS with visited-set pruning visits at most (avg_degree ^ max_depth) nodes before finding the target or exhausting the graph. For avg_degree=5, max_depth=5: worst case 5^5=3,125 node visits — comparable to the total graph size. In practice, BFS terminates far earlier. Sub-millisecond for in-memory traversal.

**SQL recursive CTE alternative**: Feasible but requires multiple round-trips or a complex CTE with path tracking. At this graph size, in-memory BFS is strictly better — no I/O, no SQL parsing, petgraph algorithms already linked.

**Classification**: New MCP tool. No storage change. In-memory petgraph BFS is the implementation path. No SQL changes needed.

**resolve_supersessions**: At each hop, substitute deprecated intermediate nodes with their superseded_by terminal before enqueuing for further expansion.

---

### context_batch_write — see Section 2 below.

---

### context_filter(entity_type, where, edge_filters, limit)

**Current analog**: `context_lookup` handles category + topic + tag + status filters but cannot express edge count filters. Application-side filtering requires O(N × round_trips) MCP calls, which is impractical.

**SQL (correlated subquery for edge count filter)**:
```sql
SELECT e.*
FROM entries e
WHERE e.category = ?1
  AND e.status = 0
  AND <property_where_clauses>
  AND (
      SELECT COUNT(*)
      FROM graph_edges g
      WHERE g.source_id = e.id
        AND g.relation_type = ?2
  ) >= ?3
LIMIT ?4
```

**Query plan at 3k entries**: Outer scan bounded by `idx_entries_category`. For 300 entries in the category: 300 correlated subquery evaluations, each using `idx_graph_edges_source_id`. Each index lookup: O(log N_edges + fanout_avg). Estimated: 300 × ~15 operations ≈ 4,500 operations. Well under 10ms.

A composite index `ON graph_edges(source_id, relation_type)` would collapse steps 2 and 3 of the subquery into a single index range scan. Does not exist currently; `idx_graph_edges_source_id` is single-column (confirmed `db.rs:956`).

**Classification**: New MCP tool. No storage schema change required at stated scale. Composite index `ON graph_edges(source_id, relation_type)` recommended for production scale.

---

## 2. context_batch_write Deep-Dive

### Current write model — two channels

**Channel 1 — Direct write_pool (integrity writes)**: `store.insert()` (`write.rs:18–94`) and `store.correct_entry()` (`write_ext.rs:400–602`). Each acquires a write pool connection, runs a full transaction (entries + entry_tags + counters + vector_map), commits, and returns the result to the caller. The caller `await`s completion. Write pool is `write_max_connections=1` (default, `pool_config.rs:73`) or max 2 (hard cap).

**Channel 2 — Analytics queue (fire-and-forget)**: `enqueue_analytics()` (`db.rs:236–245`) uses `try_send()` on a bounded `tokio::sync::mpsc::channel` with capacity `ANALYTICS_QUEUE_CAPACITY=1000` (confirmed `pool_config.rs:24` and test at `analytics.rs:981`). Non-async. Caller does NOT await completion. The drain task (`analytics.rs:253–312`) collects events in batches: up to `DRAIN_BATCH_SIZE=50` events (confirmed `pool_config.rs:32`), waits up to `DRAIN_FLUSH_INTERVAL=500ms` (confirmed `pool_config.rs:35`) for a partial batch to fill, then commits all events in one transaction. On failure, the batch is discarded silently.

**No "write and confirm" API exists**: The analytics queue is fully fire-and-forget. There is no mechanism to await confirmation that a specific event was committed.

**Can N events be grouped into one transaction?** The drain task collects events within the same `DRAIN_BATCH_SIZE` window and commits together. But (a) the caller has no control over batch boundaries — events from different callers intermix in the queue, and (b) a caller's 50 events might span two drain flush cycles. There is no atomicity guarantee across caller boundaries.

### Compatibility verdict: "Requires synchronous write bypass"

`context_batch_write` requires a single logical transaction boundary across N entries + M edges + K supersession operations. The analytics queue model cannot provide this guarantee. The only compatible implementation is a new store method using Channel 1 (direct write_pool):

```rust
pub async fn batch_write(
    &self,
    entries: Vec<NewEntry>,
    edges: Vec<NewEdgeRow>,
    supersessions: Vec<(u64, NewEntry)>,   // (original_id, correction)
) -> Result<BatchWriteResult>
```

This method opens one transaction, inserts all entries, inserts all GRAPH_EDGES rows, processes each supersession (deprecate original + insert correction), updates all counters, and commits. One `BEGIN`...`COMMIT` for the entire batch.

### Content hash computation

`compute_content_hash(title, content)` in `hash.rs:7–16` is a pure function (SHA-256 of `"{title}: {content}"`). In the batch write path, each entry's hash is computed independently before the transaction begins. The `previous_hash` field in `EntryRecord` is set to empty string `""` for new entries and corrections (confirmed `write.rs:66` and `write_ext.rs:555`). It is NOT a chained hash in the current implementation — the field exists in schema but is unused as a chain. Batch write can compute all content hashes independently without ordering constraints.

### Latency estimate

Current single entry write: ~3–5ms (connection acquisition + transaction + fsync in WAL NORMAL mode). For 50 entries + 200 edges in one transaction:
- SQL execution within a single transaction: ~2–5ms total for 250 insert statements (SQLite defers fsync until COMMIT)
- Single fsync on commit: ~1–3ms
- **Total estimate: 5–15ms for 50 entries + 200 edges.** Well within "sub-second is sufficient."

### Security capability

`context_store` is gated on `Capability::Write` (confirmed `tools.rs:607`). `context_correct` is also gated on `Capability::Write` (`tools.rs:845`). `context_batch_write` should use the same `Capability::Write` gate. No new `BatchWrite` capability needed. Recommend adding a max-batch-size limit (e.g., 100 entries, 500 edges) enforced in the MCP tool handler to bound blast radius.

### HNSW atomicity concern (OQ-B-5)

The current single-entry write path inserts the embedding into HNSW AFTER the DB transaction commits (`store_correct.rs:104–109`). For a 50-entry batch, if the DB commits and then HNSW insertion 23 of 50 fails, the DB and vector index are desynchronized. Design decision required before implementing batch_write: either accept partial HNSW state with reconciliation on next tick-rebuild, or implement a rollback mechanism for HNSW (not trivial). This is a blocker for production implementation.

### Existing bulk insert precedent

The migration v12 bootstrap demonstrates bulk INSERT via `SELECT ... FROM entries WHERE supersedes IS NOT NULL` — a single statement inserting N rows. This is the model for batch edge insertion (one `INSERT ... SELECT` from a values table). For entries, individual INSERTs within a transaction are the established pattern.

### New typed edges must use write_pool directly

NLI edges bypass the analytics queue and use `write_pool_server()` directly (`nli_detection.rs:34–62`) because they must not be shed. The analytics queue `AnalyticsWrite::GraphEdge` variant is shed-safe. New research domain edges (cites, supports, refutes, etc.) that must not be lost should follow the NLI pattern and use `write_graph_edge()` directly, not `enqueue_analytics()`.

---

## 3. Supersession Semantics Gap

### Current state

Supersession is stored on `entries` table, not as GRAPH_EDGES rows. Fields confirmed at `db.rs:542–543` and `schema.rs:67–69`:
- `entries.supersedes: Option<u64>`
- `entries.superseded_by: Option<u64>`

`context_correct` does NOT write a GRAPH_EDGES Supersedes row. Confirmed exhaustively: `store_correct.rs` — zero GRAPH_EDGES INSERTs; `write_ext.rs:400–602` — steps 1–8 include entries UPDATE, entries INSERT, tags INSERT, vector_map INSERT, counters UPDATE, no GRAPH_EDGES write.

The in-memory `TypedRelationGraph` derives Supersedes edges from `entries.supersedes` in Pass 2a (`graph.rs:258–284`), explicitly skipping GRAPH_EDGES Supersedes rows in Pass 2b (`graph.rs:294–296`).

### Current status filtering behavior

`context_lookup` default: `WHERE status = 0` (Active only) — confirmed `read.rs:305` and `read.rs:435`. `context_search` filters deprecated entries via confidence penalty (CLEAN_REPLACEMENT_PENALTY=0.40 at depth 1). `context_get` fetches by ID regardless of status. No traversal tool exists with a `resolve_supersessions` parameter.

### What resolve_supersessions=true requires

At each traversal hop, after resolving a neighbor entry ID: if `entry.status == Deprecated` and `resolve_supersessions == true`, follow `entry.superseded_by` until a non-deprecated terminal is found, and substitute the terminal for the hop destination.

Store-layer helper function:
```rust
async fn follow_to_current(store: &Store, id: u64) -> Option<u64> {
    let mut current = id;
    for _ in 0..50 {  // safety cap
        let entry = store.get_entry(current).await?;
        match entry.superseded_by {
            None => return Some(current),
            Some(next_id) => current = next_id,
        }
    }
    None  // chain too long or cycle
}
```

### Concrete cost estimate

**Storage**: No schema change needed at stated scale. `entries.superseded_by` already exists. No index on it (confirmed `db.rs:572–583`). Short chains (< 10 links) acceptable without an index at 3k entries. Add `idx_entries_superseded_by` in the traversal feature migration.

**Code**: Thread `resolve_supersessions: bool` into the new traversal tools. Apply the helper function at each hop in the BFS frontier expansion. The helper is ~20 lines of Rust. Threading into 3 tools is ~30 lines per tool. **Total effort: 1–2 engineering days** once the traversal tools exist. Build in from day one rather than retrofitting.

**resolve_supersessions=false (audit mode)**: Return edges as stored, including deprecated endpoints. This is the default behavior without the parameter — no extra work.

---

## 4. Antijoin Query Plan — Q9: Sources with No Incoming Cites Edge

### Exact SQL

```sql
SELECT e.id, e.title, e.topic, e.confidence, e.created_at
FROM entries e
LEFT JOIN graph_edges g
    ON e.id = g.target_id
    AND g.relation_type = 'cites'
WHERE e.category = 'source'
  AND e.status = 0
  AND g.target_id IS NULL
LIMIT 100;
```

### Index analysis

GRAPH_EDGES indexes (confirmed `migration.rs:361–385` and `db.rs:956–963`):
- `idx_graph_edges_source_id ON graph_edges(source_id)` — single-column
- `idx_graph_edges_target_id ON graph_edges(target_id)` — single-column
- `idx_graph_edges_relation_type ON graph_edges(relation_type)` — single-column

No composite index `ON graph_edges(target_id, relation_type)` exists.

### Query plan without composite index

1. Outer scan: `idx_entries_category` lookup on `category='source'` → ~300 rows (estimated)
2. For each row: index lookup `idx_graph_edges_target_id WHERE target_id = e.id` → returns all incoming edges for this entry (any relation_type)
3. Secondary filter: `relation_type = 'cites'` applied in-memory on retrieved edge rows (not indexed)
4. LEFT JOIN null check: if no qualifying row, include entry in result

At 3k entries, ~10k GRAPH_EDGES rows, avg 3 incoming edges per node: 300 outer rows × ~17 operations = ~5,100 total operations. **Estimated latency: 1–3ms.** Millisecond query, not tens of milliseconds.

### With composite index `ON graph_edges(target_id, relation_type)`

Steps 2 and 3 collapse into a single index range scan. Estimated latency: sub-millisecond. Highly recommended for production scale.

### Not expressible via existing tools

`context_lookup(category='source')` returns N entries. Application-side filter requires N additional graph queries (one per entry to check incoming edges). At N=300: 301 MCP calls vs. 1 SQL query. The SQL antijoin is required.

---

## 5. Open Questions and Blockers

**OQ-B-1 — Edge metadata in in-memory graph**: `RelationEdge` does not carry the `metadata` column. For `context_subgraph` and `context_neighbors` returning edge properties, post-fetch from GRAPH_EDGES is needed or `metadata: Option<String>` must be added to `RelationEdge`. Either option is low-effort. Decision needed before API design is finalized.

**OQ-B-2 — Missing supersedes/superseded_by indexes**: No indexes on `entries.supersedes` or `entries.superseded_by` (confirmed `db.rs:572–583`). Supersession chain queries are O(N) per CTE step without indexes. Acceptable at 3k entries. Must add `idx_entries_supersedes` and `idx_entries_superseded_by` in the traversal feature migration for production viability.

**OQ-B-3 — Missing composite GRAPH_EDGES indexes**: No composite indexes on `(target_id, relation_type)` or `(source_id, relation_type)`. Both context_inverse (antijoin) and context_filter (correlated subquery) perform secondary relation_type filtering after single-column index lookups. Sufficient at 3k entries; bottleneck at 10k+. Add `idx_graph_edges_target_type ON graph_edges(target_id, relation_type)` and `idx_graph_edges_source_type ON graph_edges(source_id, relation_type)` in the same migration.

**OQ-B-4 — In-memory graph staleness**: The `TypedRelationGraph` is tick-rebuilt from GRAPH_EDGES. Edges written within the last tick interval may not appear in the in-memory graph. For the research domain where writes and reads are tightly interleaved in a single session, this staleness window is unexpected. Options: (a) accept and document, (b) add direct GRAPH_EDGES SQL reads as fallback for fresh queries, (c) trigger partial graph refresh on write. This is an architectural decision affecting all traversal APIs.

**OQ-B-5 — HNSW batch atomicity for context_batch_write**: Current single-entry write path inserts HNSW embedding AFTER DB commit. For 50-entry batch, partial HNSW failure after DB commit desynchronizes vector index and DB. Design decision required: accept partial HNSW state + reconcile on tick, or implement batch rollback mechanism (complex). **This is a blocker for production batch_write implementation.**

---

## Recommendations Summary

- **context_neighbors**: New MCP tool. New BFS function in graph_expand.rs accepting caller-supplied edge types + bidirectional traversal. In-memory TypedRelationGraph for multi-hop. No storage change.
- **context_subgraph**: New MCP tool. New BFS function returning (nodes, edges). In-memory graph for topology; GRAPH_EDGES post-fetch or RelationEdge metadata field for edge properties. 200-node cap appropriate.
- **context_inverse**: New MCP tool. SQL LEFT JOIN antijoin. Existing single-column indexes sufficient at 3k. Composite `(target_id, relation_type)` index strongly recommended.
- **context_supersession_chain**: New MCP tool. SQLite recursive CTE on entries.supersedes/superseded_by. Add index on these fields in traversal feature migration.
- **context_current**: New MCP tool or context_get parameter. Single recursive CTE. No storage change.
- **context_path**: New MCP tool. petgraph algo (astar or BFS) over in-memory TypedRelationGraph. petgraph algo module already linked. No storage change. Sub-millisecond for 3k nodes.
- **context_batch_write**: Verdict: "Requires synchronous write bypass." New store method using direct write_pool transaction. Same Capability::Write gate. Estimated latency 5–15ms for 50 entries + 200 edges. HNSW batch atomicity requires design decision before implementation (OQ-B-5 — blocker).
- **context_filter**: New MCP tool. SQL correlated subquery. Composite `(source_id, relation_type)` index recommended.
- **Q4 — Supersession semantics**: Low-cost addition (~1–2 engineering days) once traversal tools exist. Implement from day one as a Store-layer helper function.
- **Q6 — Antijoin feasibility (Q9)**: Confirmed. 1–3ms at 3k without composite index; sub-ms with it. Bounded index scan + secondary filter, not a full table scan.
- **Q7 — Batch write atomicity**: Analytics queue incompatible with batch_write atomicity requirements. New synchronous write_pool path required. HNSW atomicity is the open blocker.
