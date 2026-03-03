# Component 3: Lesson-Learned Auto-Persistence — Pseudocode

## Files Modified

- `crates/unimatrix-server/src/tools.rs` — Fire-and-forget lesson-learned write in context_retrospective
- `crates/unimatrix-server/src/server.rs` — Fix embedding_dim in insert_with_audit and correct_with_audit

## CRITICAL: ADR-002 Architecture

The lesson-learned write MUST use `self.clone()` + `insert_with_audit()`.
- NO free function reimplementing the store pipeline
- The `tokio::spawn` closure captures `server.clone()` (cheap: Arc clones only)
- The spawned task calls `server_clone.insert_with_audit(entry, embedding, audit_event)`
- This guarantees: atomic ENTRIES + VECTOR_MAP, HNSW insertion, audit trail

## CRITICAL: embedding_dim Fix (server.rs)

Both `insert_with_audit` and `correct_with_audit` currently hardcode `embedding_dim: 0`.

Fix in BOTH functions:
```pseudo
// In the EntryRecord construction inside insert_with_audit:
let record = EntryRecord {
    // ... existing fields ...
    embedding_dim: embedding.len() as u16,  // FIX: was hardcoded to 0
    // ... rest of fields ...
}
```

The `embedding` parameter is already available in both functions' scope
(it's passed to `insert_hnsw_only` later). The fix is to use its length
when building the `EntryRecord`.

Note: The `embedding` Vec<f32> is consumed by `insert_hnsw_only` AFTER the
spawn_blocking closure. Since `embedding.len()` is a `usize` we can capture
it before the closure:
```pseudo
let embedding_dim = embedding.len() as u16;
// ... then inside spawn_blocking:
let record = EntryRecord {
    embedding_dim,
    // ...
}
```

For `correct_with_audit`, the same pattern applies: capture `embedding.len()`
before the `spawn_blocking` closure that builds the correction `EntryRecord`.

## 1. Trigger Logic (tools.rs)

In `context_retrospective`, after populating narratives and recommendations,
before clone-and-truncate:

```pseudo
// [Component 3] Fire-and-forget lesson-learned write
if !report.hotspots.is_empty() || !report.recommendations.is_empty():
    let server = self.clone()  // ADR-002: cheap Arc clones
    let report_clone = report.clone()
    let fc = feature_cycle.clone()
    tokio::spawn(async move {
        if let Err(e) = write_lesson_learned(&server, &report_clone, &fc).await:
            tracing::warn!("lesson-learned write failed for {}: {}", fc, e)
    })
```

## 2. write_lesson_learned Method (tools.rs, private async fn)

```pseudo
async fn write_lesson_learned(
    server: &UnimatrixServer,
    report: &RetrospectiveReport,
    feature_cycle: &str,
) -> Result<(), ServerError>:

    // 2a. CategoryAllowlist check
    if !server.categories.contains("lesson-learned"):
        tracing::error!("lesson-learned category not in allowlist, skipping write for {}", feature_cycle)
        return Ok(())

    // 2b. Build content from report
    let content = build_lesson_learned_content(report)
    let title = format!("Retrospective findings: {}", feature_cycle)
    let topic = format!("retrospective/{}", feature_cycle)

    // 2c. Supersede check: find existing active lesson-learned with same topic
    let existing = {
        let store = Arc::clone(&server.store)
        let topic_clone = topic.clone()
        tokio::task::spawn_blocking(move || {
            // Scan TOPIC_INDEX for entries with this topic, then filter by
            // category == "lesson-learned" and status == Active
            let filter = QueryFilter {
                topic: Some(topic_clone),
                category: Some("lesson-learned".to_string()),
                ..Default::default()
            }
            store.query(&filter)
        }).await??
    }

    let supersedes = if !existing.is_empty():
        // Take the most recent active entry
        let old_id = existing.iter().max_by_key(|e| e.created_at).map(|e| e.id)
        old_id
    else:
        None

    // 2d. Embed content (fire-and-forget from caller's perspective)
    let embedding = {
        let embed = Arc::clone(&server.embed_service)
        let title_clone = title.clone()
        let content_clone = content.clone()
        match tokio::task::spawn_blocking(move || {
            embed.embed(&title_clone, &content_clone)
        }).await {
            Ok(Ok(emb)) => emb,
            Ok(Err(e)) => {
                tracing::warn!("lesson-learned embedding failed for {}: {}", feature_cycle, e)
                vec![]  // empty embedding -> embedding_dim = 0
            }
            Err(e) => {
                tracing::warn!("lesson-learned embedding task panicked for {}: {}", feature_cycle, e)
                vec![]
            }
        }
    }

    // 2e. Build NewEntry
    let new_entry = NewEntry {
        title,
        content,
        topic,
        category: "lesson-learned".to_string(),
        tags: vec![
            format!("feature_cycle:{}", feature_cycle),
            format!("hotspot_count:{}", report.hotspots.len()),
            "source:retrospective".to_string(),
        ],
        source: String::new(),
        status: Status::Active,
        created_by: "cortical-implant".to_string(),
        feature_cycle: feature_cycle.to_string(),
        trust_source: "system".to_string(),
    }

    // 2f. Insert via insert_with_audit (ADR-002)
    let audit_event = AuditEvent {
        event_id: 0,
        timestamp: 0,
        session_id: String::new(),
        agent_id: "cortical-implant".to_string(),
        operation: "context_retrospective/lesson-learned".to_string(),
        target_ids: vec![],  // filled by insert_with_audit
        outcome: Outcome::Success,
        detail: format!("auto-persist lesson-learned for {}", feature_cycle),
    }

    let (new_id, _record) = server.insert_with_audit(new_entry, embedding, audit_event).await?

    // 2g. Supersede chain: deprecate old entry if found
    if let Some(old_id) = supersedes:
        let store = Arc::clone(&server.store)
        tokio::task::spawn_blocking(move || {
            // Read old entry, set superseded_by = new_id, status = Deprecated
            let txn = store.begin_write()?
            let old_bytes = {
                let table = txn.open_table(ENTRIES)?
                let guard = table.get(old_id)?.ok_or(StoreError::EntryNotFound(old_id))?
                guard.value().to_vec()
            }
            let mut old_entry = deserialize_entry(&old_bytes)?
            let old_status = old_entry.status
            old_entry.status = Status::Deprecated
            old_entry.superseded_by = Some(new_id)
            old_entry.updated_at = now()

            // Write updated old entry
            let bytes = serialize_entry(&old_entry)?
            { let mut t = txn.open_table(ENTRIES)?; t.insert(old_id, bytes.as_slice())?; }

            // Update STATUS_INDEX
            { let mut t = txn.open_table(STATUS_INDEX)?;
              t.remove((old_status as u8, old_id))?;
              t.insert((Status::Deprecated as u8, old_id), ())?; }

            // Update counters
            decrement_counter(&txn, status_counter_key(old_status), 1)?
            increment_counter(&txn, status_counter_key(Status::Deprecated), 1)?

            txn.commit()?
            Ok(())
        }).await??

    // 2h. Seed confidence on new entry
    let store = Arc::clone(&server.store)
    let registry = Arc::clone(&server.registry)
    tokio::task::spawn_blocking(move || {
        // Compute initial confidence for the new entry
        if let Ok(bytes) = {
            let rtxn = store.begin_read().ok()?;
            let table = rtxn.open_table(ENTRIES).ok()?;
            table.get(new_id).ok()?.map(|g| g.value().to_vec())
        } {
            if let Ok(mut entry) = deserialize_entry(&bytes) {
                let conf = unimatrix_engine::confidence::compute_confidence(&entry, &registry);
                entry.confidence = conf;
                entry.updated_at = now();
                // Write back
                let txn = store.begin_write()?;
                let bytes = serialize_entry(&entry)?;
                { let mut t = txn.open_table(ENTRIES)?; t.insert(new_id, bytes.as_slice())?; }
                txn.commit()?;
            }
        }
    }).await.ok();

    Ok(())
```

## 3. Content Generation (tools.rs, private fn)

```pseudo
function build_lesson_learned_content(report: &RetrospectiveReport) -> String:
    let mut content = String::new()

    // Use narratives if available (structured path), else hotspot claims (JSONL path)
    if let Some(narratives) = &report.narratives:
        for n in narratives:
            content.push_str(&format!("- {}: {}\n", n.hotspot_type, n.summary))
    else:
        for h in &report.hotspots:
            content.push_str(&format!("- {}: {}\n", h.rule_name, h.claim))

    // Include recommendations
    for r in &report.recommendations:
        content.push_str(&format!("Recommendation ({}): {}\n", r.hotspot_type, r.action))

    // Guard against empty content (R-09)
    if content.is_empty():
        content = "Retrospective analysis completed with no specific findings.".to_string()

    return content
```

## 4. Ordering in context_retrospective

```
1. [existing] Parse, attribute, detect, compute, build report
2. [Component 2] Synthesize narratives + recommendations, populate report
3. [Component 3] Spawn lesson-learned write (on full report, BEFORE truncation)
4. [Component 1] Clone-and-truncate for serialization
5. [existing] Return formatted report
```

The lesson-learned spawn happens BEFORE truncation because it needs the full
evidence arrays for content generation. The spawn is fire-and-forget so it
does not block the response.
