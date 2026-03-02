# Pseudocode: lesson-learned

Component: Lesson-Learned Auto-Persistence + Provenance Boost (P1)
Files:
  - `crates/unimatrix-engine/src/confidence.rs` (PROVENANCE_BOOST constant)
  - `crates/unimatrix-server/src/tools.rs` (fire-and-forget persist + boost application)
  - `crates/unimatrix-server/src/uds_listener.rs` (provenance boost in search re-ranking)

---

## Purpose

After `context_retrospective` computes a report with hotspots or recommendations, fire-and-forget persist a `lesson-learned` entry to Unimatrix with full embedding. Add `PROVENANCE_BOOST = 0.02` at query time for `lesson-learned` entries in both search paths.

---

## 1. confidence.rs — PROVENANCE_BOOST Constant

Add after the existing `SEARCH_SIMILARITY_WEIGHT` constant:

```
/// Query-time boost applied to `lesson-learned` category entries during search re-ranking.
/// Applied in both uds_listener.rs (ContextSearch hook) and tools.rs (context_search tool).
/// Does NOT modify stored confidence or the 0.92 stored-weight invariant.
/// Defined here alongside co-access boost constants for consistency.
pub const PROVENANCE_BOOST: f64 = 0.02;
```

---

## 2. tools.rs — Lesson-Learned Fire-and-Forget Persist

In `handle_context_retrospective`, after building the report:

```
// NEW (col-010): persist lesson-learned if report has findings
if !report.hotspots.is_empty() || !report.recommendations.is_empty():
    let feature_cycle_clone = feature_cycle.clone()
    let report_clone = report.clone()
    let store_clone = Arc::clone(&store)
    let embed_adapter_clone = Arc::clone(&embed_adapter)  // share ONNX adapter

    tokio::spawn(async move {
        let topic = format!("retrospective/{}", feature_cycle_clone)
        let content = build_lesson_learned_content(&report_clone)
        let title = format!("Retrospective findings: {}", feature_cycle_clone)

        // Check for existing active entry with same topic
        let existing = store_clone.lookup_by_topic(&topic)  // returns Option<EntryRecord>
        let supersedes_id = existing.and_then(|e| {
            if e.status == Status::Active { Some(e.id) } else { None }
        })

        // Embed the content (blocking ONNX call)
        let embed_result = tokio::task::spawn_blocking({
            let adapter = Arc::clone(&embed_adapter_clone)
            let title_c = title.clone()
            let content_c = content.clone()
            move || adapter.embed(&title_c, &content_c)
        }).await

        match embed_result:
            Ok(Ok(embedding)) =>
                // Write entry with embedding
                let entry_id = store_clone.insert_lesson_learned_entry(
                    &topic, &title, &content, &feature_cycle_clone,
                    embedding, supersedes_id
                )
                if let Ok(id) = entry_id:
                    tracing::debug!(entry_id = %id, topic = %topic, "lesson-learned entry persisted")
                    // Deprecate superseded entry if any
                    if let Some(old_id) = supersedes_id:
                        let _ = store_clone.deprecate_entry(old_id, id)
            Ok(Err(embed_err)) =>
                // Write entry WITHOUT embedding (embedding_dim = 0)
                tracing::warn!(error = %embed_err, topic = %topic, "lesson-learned embed failed; writing without vector")
                let _ = store_clone.insert_lesson_learned_entry(
                    &topic, &title, &content, &feature_cycle_clone,
                    vec![], supersedes_id  // empty = no embedding
                )
            Err(join_err) =>
                tracing::warn!(error = %join_err, "lesson-learned embed task panicked")
    })
    // context_retrospective returns here, before spawn completes
```

### build_lesson_learned_content

```
fn build_lesson_learned_content(report: &RetrospectiveReport) -> String:
    let mut parts = Vec::new()

    // Hotspot summary
    if !report.hotspots.is_empty():
        parts.push(format!("Hotspots ({}): ", report.hotspots.len()))
        for h in &report.hotspots:
            parts.push(format!("  - {} ({}): {}", h.rule_name, h.severity_str(), h.claim))

    // Recommendations
    if !report.recommendations.is_empty():
        parts.push("Recommendations:".to_string())
        for r in &report.recommendations:
            parts.push(format!("  - [{}] {}: {}", r.hotspot_type, r.action, r.rationale))

    // Layer 2 narratives (if available)
    if let Some(narratives) = &report.narratives:
        for n in narratives:
            if !n.summary.is_empty():
                parts.push(format!("Narrative: {}", n.summary))

    parts.join("\n")
```

### insert_lesson_learned_entry (store helper)

```
// Thin wrapper in write.rs or db.rs
pub fn insert_lesson_learned_entry(
    &self,
    topic: &str,
    title: &str,
    content: &str,
    feature_cycle: &str,
    embedding: Vec<f32>,   // empty = no embedding
    _supersedes: Option<u64>,   // handled by caller via deprecate_entry
) -> Result<u64>:
    let new_entry = NewEntry {
        title: title.to_string(),
        content: content.to_string(),
        topic: topic.to_string(),
        category: "lesson-learned".to_string(),
        tags: vec![format!("feature:{}", feature_cycle), "type:retrospective".to_string()],
        source: "retrospective".to_string(),
        status: Status::Active,
        created_by: "cortical-implant".to_string(),
        feature_cycle: feature_cycle.to_string(),
        trust_source: "system".to_string(),
    }
    // Use existing insert_entry_with_embedding or adapt write path
    // embedding_dim = embedding.len() as u16 (0 if empty)
    self.insert_entry_with_optional_embedding(new_entry, embedding)
```

---

## 3. Provenance Boost Application

### In uds_listener.rs (ContextSearch hook path)

In the search result re-ranking logic (where co-access boost is applied):

```
// EXISTING re-ranking formula:
// final_score = SEARCH_SIMILARITY_WEIGHT * sim + (1 - SEARCH_SIMILARITY_WEIGHT) * confidence + co_access_boost

// NEW (col-010): add provenance boost for lesson-learned entries
let provenance = if entry.category == "lesson-learned":
    PROVENANCE_BOOST
else:
    0.0

let final_score = SEARCH_SIMILARITY_WEIGHT * sim
                + (1.0 - SEARCH_SIMILARITY_WEIGHT) * confidence
                + co_access_boost
                + provenance
```

Import `PROVENANCE_BOOST` from `unimatrix_engine::confidence`.

### In tools.rs (context_search tool path)

Same formula applied in `handle_context_search`. Apply provenance boost in the same re-ranking step as co-access boost. Both call sites must be updated (ADR-005: R-07 risk mitigation — two callsite divergence risk).

---

## 4. Supersede De-duplication (SR-09 tolerated race)

The check-then-supersede sequence is NOT atomic. Concurrent `context_retrospective` calls for the same `feature_cycle` may both read `None` from `lookup_by_topic` and both create a new entry. This is tolerated:
- Rare in practice (concurrent calls for same feature_cycle).
- crt-003 contradiction detection will surface duplicates.
- Next retrospective call will supersede back to one active entry.
- Document as known limitation in tests.

---

## Error Handling

| Error | Handling |
|-------|---------|
| ONNX embed failure | Write entry with `embedding_dim = 0`; log warn |
| `insert_lesson_learned_entry` fails | Log warn; context_retrospective response unaffected |
| `deprecate_entry` fails | Log warn; two active entries may exist briefly (SR-09) |
| `tokio::spawn` join error | Log warn (shouldn't happen with well-bounded tasks) |

---

## Key Test Scenarios

1. `PROVENANCE_BOOST` constant = 0.02.
2. `rerank_score(sim=0.8, conf=0.6, coac=0.01, category="lesson-learned")` - `rerank_score(sim=0.8, conf=0.6, coac=0.01, category="convention")` = 0.02 (exactly).
3. After `context_retrospective` with >= 1 hotspot: wait for fire-and-forget embed task; `context_lookup(category:"lesson-learned", topic:"retrospective/{fc}")` returns entry with `trust_source="system"`, `embedding_dim > 0`, non-empty content.
4. Second `context_retrospective` for same feature_cycle: prior entry becomes Deprecated; superseded_by set to new entry id; exactly 1 Active entry remains.
5. `context_search("permission retry patterns")` returns lesson-learned entry in top 5.
6. ONNX failure: entry still written with `embedding_dim = 0`; not in vector search results.
7. lesson-learned entry ranks above convention entry with identical sim/conf by exactly 0.02.
8. Both `uds_listener.rs` and `tools.rs` search paths apply provenance boost (integration test verifies both).
