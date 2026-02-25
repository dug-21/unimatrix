# Pseudocode: C4 Contradiction Detection

## File: crates/unimatrix-server/src/contradiction.rs (NEW MODULE)

### Constants

```
const SIMILARITY_THRESHOLD: f32 = 0.85
const DEFAULT_CONFLICT_SENSITIVITY: f32 = 0.5
const NEIGHBORS_PER_ENTRY: usize = 10
const EMBEDDING_CONSISTENCY_THRESHOLD: f32 = 0.99

const NEGATION_WEIGHT: f32 = 0.6
const DIRECTIVE_WEIGHT: f32 = 0.3
const SENTIMENT_WEIGHT: f32 = 0.1
```

### Types

```
pub struct ContradictionPair {
    pub entry_id_a: u64,
    pub entry_id_b: u64,
    pub title_a: String,
    pub title_b: String,
    pub similarity: f32,
    pub conflict_score: f32,
    pub explanation: String,
}

pub struct EmbeddingInconsistency {
    pub entry_id: u64,
    pub title: String,
    pub expected_similarity: f32,  // actual observed self-match similarity
}

pub struct ContradictionConfig {
    pub similarity_threshold: f32,       // default: 0.85
    pub conflict_sensitivity: f32,       // default: 0.5
    pub neighbors_per_entry: usize,      // default: 10
    pub embedding_consistency_threshold: f32,  // default: 0.99
}

impl Default for ContradictionConfig:
    similarity_threshold: SIMILARITY_THRESHOLD,
    conflict_sensitivity: DEFAULT_CONFLICT_SENSITIVITY,
    neighbors_per_entry: NEIGHBORS_PER_ENTRY,
    embedding_consistency_threshold: EMBEDDING_CONSISTENCY_THRESHOLD,
```

### scan_contradictions

```
pub fn scan_contradictions(
    store: &Store,
    vector_store: &dyn VectorStore,
    embed_adapter: &dyn EmbedAdapter,
    config: &ContradictionConfig,
) -> Result<Vec<ContradictionPair>, ServerError>:

    // 1. Read all active entries
    active_entries = read_active_entries(store)  // Vec<EntryRecord>

    // 2. Dedup tracker
    seen_pairs: HashSet<(u64, u64)> = HashSet::new()
    results: Vec<ContradictionPair> = Vec::new()

    // 3. For each active entry, re-embed and search for neighbors
    for entry in active_entries:
        // Re-embed from title + content (ADR-002)
        embedding = embed_adapter.embed_entry(&entry.title, &entry.content)
        if embedding is Err:
            // Log and skip this entry (graceful degradation)
            continue

        // Search HNSW for neighbors
        neighbors = vector_store.search(
            embedding,
            config.neighbors_per_entry,
            EF_SEARCH,  // reuse constant from tools.rs
        )
        if neighbors is Err:
            continue

        // 4. Check each neighbor for conflict
        for neighbor in neighbors:
            // Skip self-match
            if neighbor.entry_id == entry.id:
                continue

            // Skip below similarity threshold
            if neighbor.similarity < config.similarity_threshold:
                continue

            // Canonical pair key for dedup
            pair_key = (min(entry.id, neighbor.entry_id), max(entry.id, neighbor.entry_id))
            if seen_pairs.contains(pair_key):
                continue
            seen_pairs.insert(pair_key)

            // Fetch neighbor entry
            neighbor_entry = store.get(neighbor.entry_id)
            if neighbor_entry is Err:
                continue

            // Skip non-active neighbors
            if neighbor_entry.status != Status::Active:
                continue

            // Run conflict heuristic
            (conflict_score, explanation) = conflict_heuristic(
                &entry.content,
                &neighbor_entry.content,
                config.conflict_sensitivity,
            )

            if conflict_score > 0.0:
                results.push(ContradictionPair {
                    entry_id_a: pair_key.0,
                    entry_id_b: pair_key.1,
                    title_a: if entry.id == pair_key.0 { entry.title } else { neighbor_entry.title },
                    title_b: if entry.id == pair_key.1 { entry.title } else { neighbor_entry.title },
                    similarity: neighbor.similarity,
                    conflict_score,
                    explanation,
                })

    // 5. Sort by conflict_score descending
    results.sort_by(|a, b| b.conflict_score.partial_cmp(&a.conflict_score).unwrap_or(Equal))

    return Ok(results)
```

### read_active_entries

```
fn read_active_entries(store: &Store) -> Result<Vec<EntryRecord>, ServerError>:
    let txn = store.begin_read()
    let entries_table = txn.open_table(ENTRIES)
    let status_table = txn.open_table(STATUS_INDEX)

    // Use STATUS_INDEX to find active entry IDs
    let active_range = status_table.range(
        (Status::Active as u8, 0u64)..=(Status::Active as u8, u64::MAX)
    )

    let mut entries = Vec::new()
    for item in active_range:
        let ((_, entry_id), _) = item
        let bytes = entries_table.get(entry_id)
        if bytes is Some:
            let record = deserialize_entry(bytes)
            entries.push(record)

    return Ok(entries)
```

### check_embedding_consistency

```
pub fn check_embedding_consistency(
    store: &Store,
    vector_store: &dyn VectorStore,
    embed_adapter: &dyn EmbedAdapter,
    config: &ContradictionConfig,
) -> Result<Vec<EmbeddingInconsistency>, ServerError>:

    // 1. Read all active entries
    active_entries = read_active_entries(store)

    let mut inconsistencies = Vec::new()

    // 2. For each entry, re-embed and check self-match
    for entry in active_entries:
        // Re-embed from title + content
        embedding = embed_adapter.embed_entry(&entry.title, &entry.content)
        if embedding is Err:
            continue  // skip entries that fail to embed

        // Search for top-1 (self-match expected)
        results = vector_store.search(embedding, 1, EF_SEARCH)
        if results is Err or results is empty:
            // No match at all -- flag as inconsistent
            inconsistencies.push(EmbeddingInconsistency {
                entry_id: entry.id,
                title: entry.title.clone(),
                expected_similarity: 0.0,
            })
            continue

        let top_result = results[0]

        // Check: is the entry its own top-1 match?
        if top_result.entry_id != entry.id:
            // Another entry is more similar than self -- suspicious
            inconsistencies.push(EmbeddingInconsistency {
                entry_id: entry.id,
                title: entry.title.clone(),
                expected_similarity: top_result.similarity,
            })
        else if top_result.similarity < config.embedding_consistency_threshold:
            // Self-match but similarity too low
            inconsistencies.push(EmbeddingInconsistency {
                entry_id: entry.id,
                title: entry.title.clone(),
                expected_similarity: top_result.similarity,
            })

    return Ok(inconsistencies)
```

### conflict_heuristic

```
pub fn conflict_heuristic(
    content_a: &str,
    content_b: &str,
    sensitivity: f32,
) -> (f32, String):

    let mut signals: Vec<(&str, f32)> = Vec::new()
    let mut explanations: Vec<String> = Vec::new()

    // Signal 1: Negation opposition (weight: 0.6)
    let neg_score = check_negation_opposition(content_a, content_b)
    if neg_score > 0.0:
        let weighted = neg_score * NEGATION_WEIGHT
        signals.push(("negation", weighted))
        explanations.push(format!("negation opposition ({neg_score:.2})"))

    // Signal 2: Incompatible directives (weight: 0.3)
    let dir_score = check_incompatible_directives(content_a, content_b)
    if dir_score > 0.0:
        let weighted = dir_score * DIRECTIVE_WEIGHT
        signals.push(("directive", weighted))
        explanations.push(format!("incompatible directives ({dir_score:.2})"))

    // Signal 3: Opposing sentiment (weight: 0.1)
    let sent_score = check_opposing_sentiment(content_a, content_b)
    if sent_score > 0.0:
        let weighted = sent_score * SENTIMENT_WEIGHT
        signals.push(("sentiment", weighted))
        explanations.push(format!("opposing sentiment ({sent_score:.2})"))

    // Composite score
    let total: f32 = signals.iter().map(|(_, w)| w).sum()
    let total = total.clamp(0.0, 1.0)

    // Apply sensitivity threshold: flag if score >= (1.0 - sensitivity)
    let threshold = 1.0 - sensitivity
    if total < threshold:
        return (0.0, String::new())

    let explanation = explanations.join("; ")
    return (total, explanation)
```

### check_negation_opposition

```
fn check_negation_opposition(content_a: &str, content_b: &str) -> f32:
    // Extract directive phrases from each content
    let directives_a = extract_directives(content_a)
    let directives_b = extract_directives(content_b)

    let mut max_score: f32 = 0.0

    // Check for opposing directive pairs
    for (verb_a, subject_a) in &directives_a:
        for (verb_b, subject_b) in &directives_b:
            // Check if one is affirmative and other is negative
            let a_affirm = is_affirmative(verb_a)
            let b_affirm = is_affirmative(verb_b)

            if a_affirm == b_affirm:
                continue  // same polarity, no opposition

            // Compare subjects
            let subject_match = compare_subjects(subject_a, subject_b)
            if subject_match > 0.0:
                max_score = max_score.max(subject_match)

    return max_score
```

### extract_directives

```
fn extract_directives(content: &str) -> Vec<(String, String)>:
    // Patterns for directive verbs (case-insensitive)
    // Affirmative: "use", "always", "prefer", "should", "must", "enable"
    // Negative: "avoid", "never", "do not", "don't", "should not", "must not", "disable"

    let directive_regex = Regex::new(
        r"(?i)\b(use|always|prefer|should|must|enable|avoid|never|do\s+not|don't|should\s+not|must\s+not|disable)\s+(\w[\w\s\-]*)"
    )

    let mut directives = Vec::new()
    for cap in directive_regex.captures_iter(content):
        let verb = cap[1].to_lowercase()
        let subject = cap[2].trim().to_lowercase()
        // Truncate subject to first few words (avoid matching entire sentences)
        let subject = first_n_words(subject, 4)
        directives.push((verb, subject))

    return directives
```

### is_affirmative

```
fn is_affirmative(verb: &str) -> bool:
    match verb:
        "use" | "always" | "prefer" | "should" | "must" | "enable" => true
        "avoid" | "never" | "do not" | "don't" | "should not" | "must not" | "disable" => false
        _ => true  // default to affirmative
```

### compare_subjects

```
fn compare_subjects(subject_a: &str, subject_b: &str) -> f32:
    if subject_a == subject_b:
        return 1.0  // exact match
    if subject_a.contains(subject_b) or subject_b.contains(subject_a):
        return 0.5  // partial (substring) match
    return 0.0  // no match
```

### check_incompatible_directives

```
fn check_incompatible_directives(content_a: &str, content_b: &str) -> f32:
    let directives_a = extract_directives(content_a)
    let directives_b = extract_directives(content_b)

    // Look for "use X" in A and "use Y" in B where X != Y
    // Both must be affirmative directives with different subjects
    let affirm_a: Vec<String> = directives_a.iter()
        .filter(|(v, _)| is_affirmative(v))
        .map(|(_, s)| s.clone())
        .collect()

    let affirm_b: Vec<String> = directives_b.iter()
        .filter(|(v, _)| is_affirmative(v))
        .map(|(_, s)| s.clone())
        .collect()

    // If both have affirmative directives with different subjects -> incompatible
    for sub_a in &affirm_a:
        for sub_b in &affirm_b:
            if sub_a != sub_b and !sub_a.contains(sub_b) and !sub_b.contains(sub_a):
                return 1.0

    return 0.0
```

### check_opposing_sentiment

```
fn check_opposing_sentiment(content_a: &str, content_b: &str) -> f32:
    let positive_markers = ["recommended", "best practice", "preferred", "ideal", "excellent"]
    let negative_markers = ["anti-pattern", "discouraged", "problematic", "risky", "avoid", "bad practice"]

    let a_lower = content_a.to_lowercase()
    let b_lower = content_b.to_lowercase()

    let a_positive = positive_markers.iter().any(|m| a_lower.contains(m))
    let a_negative = negative_markers.iter().any(|m| a_lower.contains(m))
    let b_positive = positive_markers.iter().any(|m| b_lower.contains(m))
    let b_negative = negative_markers.iter().any(|m| b_lower.contains(m))

    // Opposing: one positive + other negative
    if (a_positive and b_negative) or (a_negative and b_positive):
        return 1.0

    return 0.0
```

### first_n_words helper

```
fn first_n_words(s: &str, n: usize) -> String:
    s.split_whitespace().take(n).collect::<Vec<_>>().join(" ")
```

## File: crates/unimatrix-server/src/lib.rs

### Module declaration

```
// Add to existing module declarations
pub mod contradiction;
```
