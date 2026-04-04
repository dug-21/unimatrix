# crt-046 — Component: briefing-blending

## Purpose

Goal-conditioned blending path in the `context_briefing` handler
(`crates/unimatrix-server/src/mcp/tools.rs`, inside the `#[cfg(feature = "mcp-briefing")]` block).

Adds two guards and a score-based interleaving step. Degrades silently to pure-semantic
retrieval when no valid goal embedding exists (cold-start guarantee, NFR-02).

Wave: 4 (depends on store-v22 for `GoalClusterRow`, `query_goal_clusters_by_embedding`,
`get_cycle_start_goal_embedding`, `InferenceConfig` new fields;
depends on behavioral_signals for `blend_cluster_entries`).

---

## NAMING COLLISION — Critical (ADR-005)

These two fields share the name `confidence` in different structs. Both compile.
The wrong one silently produces incorrect cluster_score weights.

| Field | Type | Struct | Value | Source |
|-------|------|--------|-------|--------|
| `IndexEntry.confidence` | f64 | `mcp::response::briefing::IndexEntry` | Raw HNSW cosine similarity [0,1] | `briefing.index()` return |
| `EntryRecord.confidence` | f64 | `unimatrix_store::EntryRecord` | Wilson-score composite [0,1] | `store.get(id)` return |

The `cluster_score` formula uses `EntryRecord.confidence` (Wilson-score).
In the code, the `EntryRecord` objects are fetched individually via `store.get(id)`.
They must be stored in a separate variable from `IndexEntry` objects.
The implementation agent must not use `index_entry.confidence` in the cluster_score formula.

---

## Two-Level Guard (ADR-004, Resolution 3)

Both guards must fire BEFORE any DB call for cluster data.

### Level 1 Guard — before any DB call

```
// Level 1 guard (ADR-004, Resolution 3):
// If feature is absent OR current_goal is empty → cold-start immediately.
let feature = match session_state.as_ref().and_then(|ss| ss.feature.as_deref()) {
    Some(f) if !f.is_empty() => f,
    _ => {
        // Cold-start: no feature attribution or empty feature string.
        // Fall through to pure-semantic briefing.index() call (step 8 below).
        proceed_to_pure_semantic = true;
    }
};

let current_goal = session_state.as_ref()
    .and_then(|ss| Some(ss.current_goal.as_str()))
    .unwrap_or("");

if current_goal.is_empty() {
    // Resolution 3: empty current_goal is identical to absent.
    // Cold-start: skip ALL cluster logic including DB call.
    proceed_to_pure_semantic = true;
}
```

When `proceed_to_pure_semantic` is true: call `briefing.index(briefing_params, ...)` with
no blending and return the result unchanged.

AC-16 / R-08: this guard ensures `get_cycle_start_goal_embedding` is NEVER called when
`session_state.feature` is absent. Zero DB queries for cluster path. Zero overhead.
I-04: `feature.is_some() && current_goal.is_empty()` → cold-start before DB call.

### Level 2 Guard — after get_cycle_start_goal_embedding

```
// Level 2 guard (ADR-004):
// If no stored goal embedding → cold-start.
let goal_embedding: Vec<f32> = match store.get_cycle_start_goal_embedding(feature).await {
    Ok(Some(emb)) => emb,
    Ok(None) => {
        // Cold-start: no stored embedding for this cycle's start event.
        // E-08: feature has no cycle_start event → Ok(None) → cold-start.
        proceed_to_pure_semantic = true;
    }
    Err(e) => {
        // F-02: DB error → cold-start, no error propagated to caller.
        warn!("context_briefing: get_cycle_start_goal_embedding failed for {feature}: {e}");
        proceed_to_pure_semantic = true;
    }
};
```

When `proceed_to_pure_semantic` is true from Level 2: call `briefing.index()` unchanged.

E-03: if `decode_goal_embedding` fails inside `get_cycle_start_goal_embedding`, that
method returns `Ok(None)` (as specified in store-v22 pseudocode). Cold-start activates.

---

## Full Blending Sequence (when both guards pass)

### Step 1: Cluster query

```
let config = Arc::clone(&self.inference_config);
// Note: RECENCY_CAP is a constant from behavioral_signals module.
let matching_clusters: Vec<GoalClusterRow> = match store.query_goal_clusters_by_embedding(
    &goal_embedding,
    config.goal_cluster_similarity_threshold,  // f32, default 0.80; from InferenceConfig
    behavioral_signals::RECENCY_CAP,           // u64 = 100
).await {
    Ok(clusters) => clusters,
    Err(e) => {
        warn!("context_briefing: query_goal_clusters_by_embedding failed: {e}");
        vec![]  // treat as empty → cold-start
    }
};

if matching_clusters.is_empty() {
    // AC-09: empty table or no match above threshold → cold-start.
    proceed_to_pure_semantic = true;
}
```

When `proceed_to_pure_semantic` from empty clusters: call `briefing.index()` unchanged.

### Step 2: Collect cluster entry IDs (union, top-5 matching clusters)

```
// Use at most 5 matching clusters (best cosine similarity — already sorted desc by query).
let top_clusters = &matching_clusters[..matching_clusters.len().min(5)];

let mut cluster_entry_ids_raw: Vec<u64> = Vec::new();
for cluster_row in top_clusters {
    // Parse entry_ids_json as Vec<u64>.
    match serde_json::from_str::<Vec<u64>>(&cluster_row.entry_ids_json) {
        Ok(ids) => cluster_entry_ids_raw.extend(ids),
        Err(e) => {
            warn!(
                "context_briefing: failed to parse entry_ids_json for {}: {e}",
                cluster_row.feature_cycle
            );
            // Skip this cluster row; continue with others.
        }
    }
}

// Deduplicate entry IDs across clusters.
cluster_entry_ids_raw.sort_unstable();
cluster_entry_ids_raw.dedup();
```

E-05 note: if `entry_ids_json = "[]"`, `serde_json::from_str` returns `Ok(vec![])`.
No entries added. No error.

### Step 3: Fetch Active EntryRecord objects

```
// store.get() fetches a single EntryRecord by ID.
// There is no get_by_ids bulk method yet; fetch individually.
// FR-20: Active filter — skip entries that are not Status::Active.

let mut entry_records: Vec<(u64, EntryRecord)> = Vec::new();
for &id in &cluster_entry_ids_raw {
    match store.get(id).await {
        Ok(record) if record.status == Status::Active => {
            entry_records.push((id, record));
        }
        Ok(_) => {
            // Inactive, deprecated, or quarantined — excluded (AC-10, R-12).
            debug!("context_briefing: cluster entry {id} is not Active — excluded");
        }
        Err(StoreError::EntryNotFound(_)) => {
            // Entry was deleted after cluster was written — skip silently.
            debug!("context_briefing: cluster entry {id} not found — skip");
        }
        Err(e) => {
            warn!("context_briefing: store.get({id}) failed: {e} — skip");
        }
    }
}
```

OQ-1 note: the brief references `store.get_by_ids()` but that method does not currently
exist. Implementation should use `store.get(id)` individually (see open question in this
file). If a bulk method is added in Wave 1, update this step.

### Step 4: Compute cluster_score for each entry

```
// NAMING COLLISION WARNING: record.confidence below is EntryRecord.confidence (Wilson-score).
// Do NOT use IndexEntry.confidence here — those objects are not in scope yet.

let cluster_entries_with_scores: Vec<(IndexEntry, f32)> = entry_records
    .iter()
    .map(|(entry_id, record)| {
        // Find the matching cluster row to get goal_cosine (row.similarity).
        // A cluster entry may appear in multiple cluster rows; use the highest similarity.
        let goal_cosine: f32 = top_clusters
            .iter()
            .filter(|row| {
                // Check if this entry_id appears in this row's entry_ids_json.
                // For efficiency, pre-parse all row entry IDs above (step 2) and keep
                // a HashMap<feature_cycle, Vec<u64>> for O(1) lookup here.
                // Pseudocode simplifies to: does this cluster row contain entry_id?
                row_contains_entry_id(row, *entry_id)
            })
            .map(|row| row.similarity)
            .fold(0.0_f32, f32::max);

        // cluster_score uses EntryRecord.confidence (Wilson-score), NOT IndexEntry.confidence.
        let cluster_score: f32 =
            (record.confidence as f32 * config.w_goal_cluster_conf)
            + (goal_cosine * config.w_goal_boost);

        // Build IndexEntry from EntryRecord for blend_cluster_entries.
        let index_entry = IndexEntry {
            id: record.id,
            topic: record.topic.clone(),
            category: record.category.clone(),
            confidence: record.confidence,  // f64 Wilson-score — passed through for display
            snippet: record.content.chars().take(SNIPPET_CHARS).collect(),
        };

        (index_entry, cluster_score)
    })
    .collect();
```

Implementation notes:
- `SNIPPET_CHARS` is imported from `crate::mcp::response::briefing`.
- `config.w_goal_cluster_conf` and `config.w_goal_boost` are read from `Arc<InferenceConfig>`
  at call time. They are NOT constants in behavioral_signals.
- `record.confidence` is f64; cast to f32 for the cluster_score formula (acceptable precision
  loss at this dimensionality — the formula operates in f32).
- For finding `goal_cosine`, a pre-built HashMap from entry_id → max_similarity across
  matching cluster rows is more efficient than the nested filter above. Implementation agent
  should build this map before the `.map()` call.

### Step 5: Semantic search (existing path)

```
// Unchanged from current implementation — call briefing.index() with briefing_params.
let semantic_results: Vec<IndexEntry> = self
    .services
    .briefing
    .index(briefing_params, &ctx.audit_ctx, Some(&ctx.caller_id))
    .await
    .map_err(rmcp::ErrorData::from)?;
```

### Step 6: Score-based interleaving (Option A, ADR-005)

```
let final_entries: Vec<IndexEntry> = if cluster_entries_with_scores.is_empty() {
    // No cluster candidates survived the Active filter → pure semantic result.
    semantic_results
} else {
    behavioral_signals::blend_cluster_entries(
        semantic_results,
        cluster_entries_with_scores,
        20,  // k=20 — hardcoded per IndexBriefingService contract
    )
};
```

### Step 7: Continue with existing handler steps

The `final_entries` vector replaces what was previously `entries` from `briefing.index()`.
All downstream steps (entry_ids collection, format_index_table, audit, usage recording)
operate on `final_entries` unchanged.

---

## Complete context_briefing Blending Integration

The following describes the new structure of the `context_briefing` handler within the
`#[cfg(feature = "mcp-briefing")]` block. Existing steps retain their numbers.

```
// [existing step 1] identity + capability check
// [existing step 2] validation
// [existing step 3] max_tokens
// [existing step 4] session_state resolution

// [NEW: extract blending inputs]
let current_goal = session_state.as_ref()
    .map(|ss| ss.current_goal.as_str())
    .unwrap_or("");
let feature_for_blending = session_state.as_ref()
    .and_then(|ss| ss.feature.as_deref());

// [existing step 5] category histogram
// [existing step 6] query derivation
// [existing step 7] build IndexBriefingParams (unchanged)

// [NEW: Level 1 guard — before any DB call]
let should_blend = feature_for_blending.is_some()
    && !feature_for_blending.unwrap().is_empty()
    && !current_goal.is_empty();

// [NEW: Level 2 guard + blending]
let entries: Vec<IndexEntry> = if should_blend {
    let feature = feature_for_blending.unwrap();

    // Level 2 guard: get_cycle_start_goal_embedding
    let goal_embedding_opt: Option<Vec<f32>> = match store
        .get_cycle_start_goal_embedding(feature).await
    {
        Ok(opt) => opt,
        Err(e) => {
            warn!("context_briefing: get_cycle_start_goal_embedding error: {e}");
            None
        }
    };

    match goal_embedding_opt {
        None => {
            // Level 2 cold-start
            self.services.briefing.index(briefing_params, &ctx.audit_ctx, Some(&ctx.caller_id))
                .await.map_err(rmcp::ErrorData::from)?
        }
        Some(goal_embedding) => {
            // Cluster query
            let matching_clusters = match store.query_goal_clusters_by_embedding(
                &goal_embedding,
                config.goal_cluster_similarity_threshold,
                behavioral_signals::RECENCY_CAP,
            ).await {
                Ok(c) => c,
                Err(e) => { warn!(...); vec![] }
            };

            if matching_clusters.is_empty() {
                // Cold-start: no matching clusters
                self.services.briefing.index(briefing_params, ...).await...
            } else {
                // [Steps 2-6 from blending sequence above]
                // collect IDs → fetch Active EntryRecords → compute cluster_scores
                // → semantic search → blend_cluster_entries → final_entries

                // See "Full Blending Sequence" above for details.
                final_entries  // Vec<IndexEntry>
            }
        }
    }
} else {
    // Level 1 cold-start — no DB calls
    self.services.briefing.index(briefing_params, &ctx.audit_ctx, Some(&ctx.caller_id))
        .await.map_err(rmcp::ErrorData::from)?
};

// [existing step 9] entry_ids: entries.iter().map(|e| e.id).collect()
// [existing step 10] format_index_table(&entries)
// [existing step 11] audit
// [existing step 12] usage recording
```

---

## InferenceConfig Access

The handler needs `Arc<InferenceConfig>` to read the three new fields. The handler
currently accesses `self.inference_config` or a field on `ServiceLayer`. The implementation
agent should:
1. Verify the current path to `InferenceConfig` from within the `context_briefing` handler.
2. Add `config.goal_cluster_similarity_threshold`, `config.w_goal_cluster_conf`,
   `config.w_goal_boost` reads.

These fields must NOT be hardcoded in the handler or in behavioral_signals.rs.

---

## store Reference in Briefing Handler

The briefing handler needs to call `store.get_cycle_start_goal_embedding()` and
`store.query_goal_clusters_by_embedding()` and `store.get(id)`. These are all methods
on `SqlxStore`.

The implementation agent must verify how `store` (Arc<SqlxStore>) is accessible from
within the `context_briefing` handler. In the existing handler, `store` is accessible
via `Arc::clone(&self.store)` (same pattern as used in `context_cycle_review`).

---

## Key Test Scenarios (briefing-blending)

| Test | Risk | Assertion |
|------|------|-----------|
| AC-07 | R-13 | Cluster entry with high cluster_score displaces weakest semantic result in top-20 |
| AC-08 | R-11 | NULL goal embedding → result identical to pure-semantic baseline |
| AC-09 | R-11 | Empty goal_clusters table → result identical to pure-semantic baseline |
| AC-10 | R-12 | Deprecated entry in cluster IDs excluded from briefing result |
| AC-11 | R-07 | 101 rows in goal_clusters; oldest excluded by recency cap |
| AC-16 | R-08 | session_state.feature = None → get_cycle_start_goal_embedding NOT called |
| I-04 | I-04 | feature.is_some(), current_goal = "" → cold-start before DB call |
| E-03 | E-03 | Malformed BLOB in cycle_events → Ok(None) → cold-start, no panic |
| E-05 | E-05 | entry_ids_json = "[]" → no cluster entries injected, no crash |
| E-07 | E-07 | Cosine exactly at threshold (0.80) → row included |
| E-08 | E-08 | feature present but no cycle_start event → Ok(None) → cold-start |
| R-11 cold paths | R-11 | All four cold-start paths produce bit-for-bit identical output to pure-semantic |
| Naming collision | ADR-005 | cluster_score uses EntryRecord.confidence (Wilson), not IndexEntry.confidence (cosine) |
| F-02 | F-02 | get_cycle_start_goal_embedding Err → briefing returns success, pure-semantic result |

### R-11 Cold-Start Paths (all four must have explicit tests)

1. `session_state.feature = None` → Level 1 guard fires, zero DB calls.
2. `get_cycle_start_goal_embedding` returns `Ok(None)` → Level 2 guard fires.
3. `goal_clusters` table empty → `matching_clusters.is_empty()` → cold-start.
4. `cosine_similarity < threshold` for all rows → `matching_clusters.is_empty()` → cold-start.

Each test must assert that the result IDs and order match the pure-semantic baseline exactly
(not just same count).

### Drain Flush (I-02)

The briefing blending path reads `goal_clusters` via `read_pool()`. Writes to `goal_clusters`
go through `write_pool_server()` (direct, not analytics drain). Integration tests that:
1. Call `context_cycle_review` to populate `goal_clusters`.
2. Then call `context_briefing` to test blending.

Do NOT need an analytics drain flush between steps 1 and 2 for the `goal_clusters` row.
However, if the test also asserts `graph_edges` from step 1, it does NOT need a flush
either (since `emit_behavioral_edges` uses `write_graph_edge` directly, per behavioral-signals.md).
