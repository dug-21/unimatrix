# Pseudocode: C6 -- Tool Integration

## Crate: unimatrix-server

### response.rs changes

**New types**:

```rust
/// A co-access cluster entry for status reporting.
pub struct CoAccessClusterEntry {
    pub entry_id_a: u64,
    pub entry_id_b: u64,
    pub title_a: String,
    pub title_b: String,
    pub count: u32,
    pub last_updated: u64,
}
```

**StatusReport extension** (4 new fields):

```rust
pub struct StatusReport {
    // ... existing fields unchanged ...
    pub total_co_access_pairs: u64,        // NEW
    pub active_co_access_pairs: u64,       // NEW
    pub top_co_access_pairs: Vec<CoAccessClusterEntry>,  // NEW
    pub stale_pairs_cleaned: u64,          // NEW
}
```

Default values for new fields: 0, 0, vec![], 0.

**format_status_report changes**:

Summary format -- append after existing content:
```
Co-access: {active} active pairs ({total} total), {cleaned} stale pairs cleaned
```

Markdown format -- add new section before existing closing:
```markdown
## Co-Access Patterns

- Active pairs: {active} of {total} total
- Stale pairs cleaned: {cleaned}

### Top Co-Access Clusters
| Entry A | Entry B | Count | Last Updated |
|---------|---------|-------|-------------|
| {title_a} (#{id_a}) | {title_b} (#{id_b}) | {count} | {timestamp} |
```

JSON format -- add `co_access` object:
```json
{
  "co_access": {
    "total_pairs": N,
    "active_pairs": N,
    "stale_pairs_cleaned": N,
    "top_clusters": [
      {
        "entry_a": { "id": N, "title": "..." },
        "entry_b": { "id": N, "title": "..." },
        "count": N,
        "last_updated": N
      }
    ]
  }
}
```

### tools.rs changes

**context_search: add step 9c after step 9b**

```rust
// 9b. Re-rank by blended score: similarity * 0.85 + confidence * 0.15 (crt-002)
// ... existing code unchanged ...

// 9c. Co-access boost (crt-004)
{
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let staleness_cutoff = now.saturating_sub(crate::coaccess::CO_ACCESS_STALENESS_SECONDS);

    // Anchor IDs: top min(3, result_count) entries
    let anchor_count = results_with_scores.len().min(3);
    let anchor_ids: Vec<u64> = results_with_scores.iter()
        .take(anchor_count)
        .map(|(e, _)| e.id)
        .collect();

    let result_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();

    if !anchor_ids.is_empty() && results_with_scores.len() > 1 {
        let store = Arc::clone(&self.store);
        let boost_map = tokio::task::spawn_blocking(move || {
            crate::coaccess::compute_search_boost(&anchor_ids, &result_ids, &store, staleness_cutoff)
        }).await
        .unwrap_or_else(|e| {
            tracing::warn!("co-access boost task failed: {e}");
            std::collections::HashMap::new()
        });

        if !boost_map.is_empty() {
            // Re-sort with boost applied
            results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
                let base_a = crate::confidence::rerank_score(*sim_a, entry_a.confidence);
                let base_b = crate::confidence::rerank_score(*sim_b, entry_b.confidence);
                let boost_a = boost_map.get(&entry_a.id).copied().unwrap_or(0.0);
                let boost_b = boost_map.get(&entry_b.id).copied().unwrap_or(0.0);
                let final_a = base_a + boost_a;
                let final_b = base_b + boost_b;
                final_b.partial_cmp(&final_a).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }
}

// 10. Trim to k results (boost may have changed order)
results_with_scores.truncate(k);
```

**context_briefing: add co-access boost after step 8 search results**

```rust
// After fetching search results and feature boost (existing step 8):

// 8b. Co-access boost for briefing (crt-004)
if relevant_context.len() > 1 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let staleness_cutoff = now.saturating_sub(crate::coaccess::CO_ACCESS_STALENESS_SECONDS);

    let anchor_count = relevant_context.len().min(3);
    let anchor_ids: Vec<u64> = relevant_context.iter()
        .take(anchor_count)
        .map(|(e, _)| e.id)
        .collect();
    let result_ids: Vec<u64> = relevant_context.iter().map(|(e, _)| e.id).collect();

    let store = Arc::clone(&self.store);
    let boost_map = tokio::task::spawn_blocking(move || {
        crate::coaccess::compute_briefing_boost(&anchor_ids, &result_ids, &store, staleness_cutoff)
    }).await
    .unwrap_or_else(|e| {
        tracing::warn!("co-access briefing boost task failed: {e}");
        std::collections::HashMap::new()
    });

    if !boost_map.is_empty() {
        relevant_context.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
            let boost_a = boost_map.get(&entry_a.id).copied().unwrap_or(0.0);
            let boost_b = boost_map.get(&entry_b.id).copied().unwrap_or(0.0);
            let score_a = *sim_a + boost_a;
            let score_b = *sim_b + boost_b;
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}
```

**context_status: add co-access stats and cleanup**

After the existing status report build (step 5e) and before contradiction scanning:

```rust
// 5f. Co-access stats and cleanup (crt-004)
{
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let staleness_cutoff = now.saturating_sub(crate::coaccess::CO_ACCESS_STALENESS_SECONDS);

    let store_for_coaccess = Arc::clone(&self.store);
    let co_access_result = tokio::task::spawn_blocking(move || {
        // Stats
        let (total, active) = store_for_coaccess.co_access_stats(staleness_cutoff)?;

        // Top clusters
        let top_pairs = store_for_coaccess.top_co_access_pairs(5, staleness_cutoff)?;

        // Resolve titles for top pairs
        let mut clusters = Vec::new();
        for ((id_a, id_b), record) in &top_pairs {
            let title_a = store_for_coaccess.get(*id_a)
                .map(|e| e.title.clone())
                .unwrap_or_else(|_| format!("#{id_a}"));
            let title_b = store_for_coaccess.get(*id_b)
                .map(|e| e.title.clone())
                .unwrap_or_else(|_| format!("#{id_b}"));
            clusters.push(CoAccessClusterEntry {
                entry_id_a: *id_a,
                entry_id_b: *id_b,
                title_a,
                title_b,
                count: record.count,
                last_updated: record.last_updated,
            });
        }

        // Cleanup stale pairs (piggybacked maintenance)
        let cleaned = store_for_coaccess.cleanup_stale_co_access(staleness_cutoff)?;

        Ok::<_, unimatrix_store::StoreError>((total, active, clusters, cleaned))
    }).await;

    match co_access_result {
        Ok(Ok((total, active, clusters, cleaned))) => {
            report.total_co_access_pairs = total;
            report.active_co_access_pairs = active;
            report.top_co_access_pairs = clusters;
            report.stale_pairs_cleaned = cleaned;
        }
        Ok(Err(e)) => {
            tracing::warn!("co-access stats failed: {e}");
            // Fields remain at default (0, 0, vec![], 0)
        }
        Err(e) => {
            tracing::warn!("co-access stats task failed: {e}");
        }
    }
}
```

### server.rs changes

**record_usage_for_entries: add Step 5**

(See pseudocode/co-access-recording.md for the complete Step 5 code.)

### lib.rs changes

```rust
pub mod coaccess;  // NEW: add to module declarations
```

Key design notes:
- context_search step 9c runs co-access boost computation via spawn_blocking (same pattern as usage recording)
- Boost computation failure is graceful: search returns results without boost
- context_briefing applies a smaller boost (MAX_BRIEFING_CO_ACCESS_BOOST = 0.01)
- context_status piggybacks stale pair cleanup on the status call (same pattern as contradiction scanning)
- StatusReport new fields default to zero/empty if co-access stats fail
- Title resolution for top clusters uses fallback to "#{id}" if entry lookup fails
- The truncate(k) in context_search is moved AFTER co-access boost to ensure boost doesn't push entries below the cut
