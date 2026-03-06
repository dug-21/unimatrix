# C2: Supersession Candidate Injection — Pseudocode

## Location
`crates/unimatrix-server/src/services/search.rs` (integrated into SearchService::search, between Step 6a and Step 7)

## Pseudocode

```
// Step 6b: Supersession candidate injection
//
// Skip entirely if:
// 1. explicit_status_filter is Some(Deprecated) — agent wants deprecated content (FR-6.2)
// 2. No results have superseded_by set

let should_inject = explicit_status_filter != Some(Status::Deprecated)

if should_inject:
    // Collect successor IDs from results that have superseded_by set
    let successor_ids: Vec<u64> = results_with_scores
        .iter()
        .filter_map(|(entry, _)| entry.superseded_by)
        .collect()

    if !successor_ids.is_empty():
        // Deduplicate successor IDs
        let unique_successor_ids: HashSet<u64> = successor_ids.iter().copied().collect()

        // Remove IDs already in result set
        let existing_ids: HashSet<u64> = results_with_scores
            .iter()
            .map(|(e, _)| e.id)
            .collect()

        let to_fetch: Vec<u64> = unique_successor_ids
            .into_iter()
            .filter(|id| !existing_ids.contains(id))
            .collect()

        // Batch-fetch successors (FR-2.2: single batch, not N fetches)
        for successor_id in to_fetch:
            match entry_store.get(successor_id).await:
                Ok(successor):
                    // FR-2.3: Only inject if:
                    // (a) Active
                    // (b) not already in results (checked above)
                    // (c) not itself superseded
                    if successor.status != Status::Active:
                        continue
                    if successor.superseded_by.is_some():
                        continue  // Single-hop only (ADR-003, AC-06)

                    // Compute cosine similarity between query embedding and successor's stored embedding
                    let successor_embedding = vector_store.get_embedding(successor_id).await

                    match successor_embedding:
                        Some(emb):
                            let sim = cosine_similarity(&embedding, &emb) as f64
                            results_with_scores.push((successor, sim))
                        None:
                            // No embedding stored — skip injection (R-01 fallback)
                            tracing::debug!("successor {successor_id} has no stored embedding, skipping injection")

                Err(_):
                    // Dangling reference — skip silently (FR-2.7, AC-07)
                    continue
```

## Key Design Points

- Single-hop only: if successor B has `superseded_by` set, B is skipped even if B is Active (ADR-003)
- Dangling references (non-existent successor) silently skipped (FR-2.7)
- Successor similarity is from cosine of stored embedding, not inherited (ADR-002, AC-05)
- Injected successors enter the penalty_map check: they are Active with no superseded_by, so penalty = 1.0 (no penalty)
- Injected successors go through normal re-rank pipeline (FR-2.5)
- Multiple deprecated entries superseded by the same Active entry: dedup via HashSet ensures single injection
- Self-referential supersession (entry.superseded_by == entry.id): handled by existing_ids check
- `get_embedding` returns `Option<Vec<f32>>` — None means no vector mapping exists

## Error Handling

| Error | Handling |
|-------|----------|
| entry_store.get(successor_id) returns Err | Skip, continue |
| vector_store.get_embedding returns None | Skip injection for this successor |
| Successor is Deprecated/Quarantined | Skip injection |
| Successor has superseded_by set | Skip injection (single-hop) |

## Dependencies

- `VectorIndex::get_embedding()` (new method on unimatrix-vector)
- `AsyncVectorStore::get_embedding()` (new async wrapper on unimatrix-core)
- `cosine_similarity()` from `unimatrix-engine/src/confidence.rs` (C7)
- `query_embedding` from Step 4 (already available as `embedding: Vec<f32>`)
