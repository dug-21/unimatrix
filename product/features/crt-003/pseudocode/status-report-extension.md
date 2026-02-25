# Pseudocode: C5 Status Report Extension

## File: crates/unimatrix-server/src/tools.rs

### StatusParams extension

```
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatusParams {
    // ... existing fields (agent_id, format, topic, category) ...

    /// Opt-in embedding consistency check (default: false).
    pub check_embeddings: Option<bool>,    // NEW
}
```

### context_status handler modifications

```
async fn context_status(params):
    // Steps 1-4 unchanged (identity, capability, validation, format)

    // Step 5: Build report in spawn_blocking (existing)
    //   5a-5d: counters, distributions, metrics (existing)
    //   5a (modified): also read total_quarantined counter

    let total_quarantined = counters.get("total_quarantined")
        .map(|g| g.value()).unwrap_or(0)

    //   5d: entries scan (existing -- corrections, trust sources, attribution)
    //   5e: build StatusReport (modified -- add total_quarantined)

    let report = StatusReport {
        // ... existing fields ...
        total_quarantined,                          // NEW
        contradictions: Vec::new(),                 // NEW (populated below)
        contradiction_count: 0,                     // NEW
        embedding_inconsistencies: Vec::new(),      // NEW
        contradiction_scan_performed: false,         // NEW
        embedding_check_performed: false,            // NEW
    }

    // Step 6 (NEW): Contradiction scanning + embedding consistency
    //   Run outside of the read transaction (scanning needs embed_service)

    let check_embeddings = params.check_embeddings.unwrap_or(false)

    // Check if embed service is ready
    if let Ok(adapter) = self.embed_service.get_adapter().await:
        // 6a. Contradiction scan (default ON)
        let config = ContradictionConfig::default()

        let contradictions = tokio::task::spawn_blocking({
            let store = Arc::clone(&self.store)
            let vector_store = Arc::clone(&self.vector_store)
            move || scan_contradictions(&store, &*vector_store, &*adapter, &config)
        }).await??

        report.contradiction_count = contradictions.len()
        report.contradictions = contradictions
        report.contradiction_scan_performed = true

        // 6b. Embedding consistency check (opt-in)
        if check_embeddings:
            let adapter = self.embed_service.get_adapter().await?
            let inconsistencies = tokio::task::spawn_blocking({
                let store = Arc::clone(&self.store)
                let vector_store = Arc::clone(&self.vector_store)
                move || check_embedding_consistency(&store, &*vector_store, &*adapter, &config)
            }).await??

            report.embedding_inconsistencies = inconsistencies
            report.embedding_check_performed = true

    // Step 7: Format response (existing, extended)
    let result = format_status_report(&report, format)

    // Step 8: Audit (existing)
```

## File: crates/unimatrix-server/src/response.rs

### StatusReport struct extension

```
pub struct StatusReport {
    // Existing fields:
    pub total_active: u64,
    pub total_deprecated: u64,
    pub total_proposed: u64,
    pub category_distribution: Vec<(String, u64)>,
    pub topic_distribution: Vec<(String, u64)>,
    pub entries_with_supersedes: u64,
    pub entries_with_superseded_by: u64,
    pub total_correction_count: u64,
    pub trust_source_distribution: Vec<(String, u64)>,
    pub entries_without_attribution: u64,

    // NEW fields:
    pub total_quarantined: u64,
    pub contradictions: Vec<ContradictionPair>,
    pub contradiction_count: usize,
    pub embedding_inconsistencies: Vec<EmbeddingInconsistency>,
    pub contradiction_scan_performed: bool,
    pub embedding_check_performed: bool,
}
```

### format_status_report modifications

```
fn format_status_report(report, format):
    match format:
        Summary =>
            // Existing: "Active: {a} | Deprecated: {d} | Proposed: {p} | ..."
            // Modified: append quarantine and contradiction counts
            text = existing_summary
            text += " | Quarantined: {report.total_quarantined}"
            if report.contradiction_scan_performed:
                text += " | Contradictions: {report.contradiction_count}"

        Markdown =>
            // Existing sections: ## Entries, ## Category Distribution, etc.
            // Add new sections:

            // After existing status section, add quarantined count
            // Modify the existing "## Entries" section to include quarantined:
            //   | Status | Count |
            //   | Active | ... |
            //   | Deprecated | ... |
            //   | Proposed | ... |
            //   | Quarantined | ... |     <-- NEW row

            // NEW section: ## Contradictions
            if report.contradiction_scan_performed:
                text += "## Contradictions\n\n"
                if report.contradictions.is_empty():
                    text += "No contradictions detected.\n\n"
                else:
                    text += format!("{} contradiction(s) found:\n\n", report.contradiction_count)
                    text += "| Entry A | Entry B | Similarity | Conflict Score | Explanation |\n"
                    text += "|---------|---------|-----------|---------------|-------------|\n"
                    for pair in &report.contradictions:
                        text += format!(
                            "| #{} {} | #{} {} | {:.2} | {:.2} | {} |\n",
                            pair.entry_id_a, pair.title_a,
                            pair.entry_id_b, pair.title_b,
                            pair.similarity, pair.conflict_score,
                            pair.explanation,
                        )

            // NEW section: ## Embedding Integrity
            if report.embedding_check_performed:
                text += "## Embedding Integrity\n\n"
                if report.embedding_inconsistencies.is_empty():
                    text += "All embeddings consistent.\n\n"
                else:
                    text += format!(
                        "{} inconsistency(ies) found:\n\n",
                        report.embedding_inconsistencies.len()
                    )
                    text += "| Entry | Title | Self-Match Similarity |\n"
                    text += "|-------|-------|----------------------|\n"
                    for inc in &report.embedding_inconsistencies:
                        text += format!(
                            "| #{} | {} | {:.4} |\n",
                            inc.entry_id, inc.title, inc.expected_similarity,
                        )

        Json =>
            // Existing JSON object with entry_counts, distributions, etc.
            // Add new fields:
            json["quarantined"] = report.total_quarantined

            if report.contradiction_scan_performed:
                json["contradictions"] = report.contradictions.iter().map(|p| {
                    "entry_id_a": p.entry_id_a,
                    "entry_id_b": p.entry_id_b,
                    "title_a": p.title_a,
                    "title_b": p.title_b,
                    "similarity": p.similarity,
                    "conflict_score": p.conflict_score,
                    "explanation": p.explanation,
                }).collect()
                json["contradiction_count"] = report.contradiction_count

            if report.embedding_check_performed:
                json["embedding_inconsistencies"] = report.embedding_inconsistencies.iter().map(|i| {
                    "entry_id": i.entry_id,
                    "title": i.title,
                    "self_match_similarity": i.expected_similarity,
                }).collect()
```

## File: crates/unimatrix-server/src/validation.rs

### validate_status_params extension

```
fn validate_status_params(params):
    // Existing validation for topic, category, format
    // No additional validation needed for check_embeddings (it's a bool)
    // The existing function is sufficient
```
