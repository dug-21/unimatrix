# Pseudocode: extraction-rules (Wave 2)

## Module Structure

```
crates/unimatrix-observe/src/extraction/
  mod.rs                    -- ExtractionRule trait, ProposedEntry, QualityGateResult,
                               ExtractionContext, ExtractionStats, run_extraction_pipeline(),
                               quality_gate(), default_extraction_rules()
  knowledge_gap.rs          -- KnowledgeGapRule
  implicit_convention.rs    -- ImplicitConventionRule
  dead_knowledge.rs         -- DeadKnowledgeRule
  recurring_friction.rs     -- RecurringFrictionRule
  file_dependency.rs        -- FileDependencyRule
```

## 1. Core Types (mod.rs)

```rust
use unimatrix_store::Store;
use crate::types::ObservationRecord;

pub trait ExtractionRule: Send {
    fn name(&self) -> &str;
    fn evaluate(&self, observations: &[ObservationRecord], store: &Store) -> Vec<ProposedEntry>;
}

pub struct ProposedEntry {
    pub title: String,
    pub content: String,
    pub category: String,
    pub topic: String,
    pub tags: Vec<String>,
    pub source_rule: String,
    pub source_features: Vec<String>,
    pub extraction_confidence: f64,
}

pub enum QualityGateResult {
    Accept,
    Reject { reason: String, check_name: String },
}

pub struct ExtractionContext {
    pub last_watermark: u64,        // last processed observation id
    pub rate_count: u64,            // extractions this hour
    pub rate_hour: u64,             // current hour (epoch / 3600)
    pub stats: ExtractionStats,
}

pub struct ExtractionStats {
    pub entries_extracted_total: u64,
    pub entries_rejected_total: u64,
    pub last_extraction_run: Option<u64>,
    pub rules_fired: std::collections::HashMap<String, u64>,
}

impl ExtractionContext {
    pub fn new() -> Self { ... }  // all zeros, rate_hour = current hour

    pub fn check_and_increment_rate(&mut self) -> bool {
        let current_hour = now_secs() / 3600;
        if current_hour != self.rate_hour {
            self.rate_count = 0;
            self.rate_hour = current_hour;
        }
        if self.rate_count >= 10 {
            return false;  // rate limited
        }
        self.rate_count += 1;
        true
    }
}
```

## 2. Quality Gate Pipeline (mod.rs)

```rust
// Category allowlist for auto-extracted entries
const ALLOWED_CATEGORIES: &[&str] = &[
    "convention", "pattern", "lesson-learned", "gap", "decision",
];

// Minimum source features per rule
fn min_features_for_rule(rule_name: &str) -> usize {
    match rule_name {
        "knowledge-gap" => 2,
        "implicit-convention" | "recurring-friction" | "file-dependency" => 3,
        "dead-knowledge" => 5,
        _ => 3,
    }
}

pub fn quality_gate(entry: &ProposedEntry, ctx: &mut ExtractionContext) -> QualityGateResult {
    // Check 1: Rate limit (O(1))
    if !ctx.check_and_increment_rate() {
        return Reject { reason: "Rate limit exceeded (10/hour)", check_name: "rate_limit" };
    }

    // Check 2: Content validation (O(1))
    if entry.title.len() < 10 {
        return Reject { reason: "Title too short (<10 chars)", check_name: "content_validation" };
    }
    if entry.content.len() < 20 {
        return Reject { reason: "Content too short (<20 chars)", check_name: "content_validation" };
    }
    if !ALLOWED_CATEGORIES.contains(&entry.category.as_str()) {
        return Reject { reason: format!("Category '{}' not in allowlist", entry.category), check_name: "content_validation" };
    }

    // Check 3: Cross-feature validation (O(1))
    let min_features = min_features_for_rule(&entry.source_rule);
    if entry.source_features.len() < min_features {
        return Reject {
            reason: format!("Need {} features, got {}", min_features, entry.source_features.len()),
            check_name: "cross_feature",
        };
    }

    // Check 4: Confidence floor (O(1))
    if entry.extraction_confidence < 0.2 {
        return Reject { reason: "Confidence below 0.2 floor", check_name: "confidence_floor" };
    }

    // Checks 5 and 6 (near-duplicate, contradiction) require embedding + store access.
    // These are handled in the server-side pipeline (run_extraction_pipeline)
    // because they need EmbedService and VectorStore which are server-level deps.

    Accept
}

pub fn default_extraction_rules() -> Vec<Box<dyn ExtractionRule>> {
    vec![
        Box::new(knowledge_gap::KnowledgeGapRule),
        Box::new(implicit_convention::ImplicitConventionRule),
        Box::new(dead_knowledge::DeadKnowledgeRule),
        Box::new(recurring_friction::RecurringFrictionRule),
        Box::new(file_dependency::FileDependencyRule),
    ]
}
```

## 3. KnowledgeGapRule (knowledge_gap.rs)

```rust
pub struct KnowledgeGapRule;

impl ExtractionRule for KnowledgeGapRule {
    fn name(&self) -> &str { "knowledge-gap" }

    fn evaluate(&self, observations: &[ObservationRecord], _store: &Store) -> Vec<ProposedEntry> {
        // 1. Filter observations for context_search calls (PreToolUse, tool="mcp__unimatrix__context_search")
        //    that have zero results (response_size == Some(0) or response_snippet contains "No results")
        // 2. Extract query from input JSON: input["query"] or input as string
        // 3. Normalize query: lowercase, trim
        // 4. Group by (normalized_query, feature_cycle_from_session)
        //    Feature cycle comes from observation's session_id -> need to map session to feature
        //    For simplicity: group by session_id as proxy for feature
        // 5. For each query that appears in 2+ distinct sessions:
        //    - Collect unique session_ids as source_features
        //    - Create ProposedEntry with category="gap"
        //    - confidence = min(0.8, 0.4 + 0.1 * feature_count)
        //    - tags = ["auto-extracted", "knowledge-gap"]

        let mut query_sessions: HashMap<String, HashSet<String>> = HashMap::new();

        for obs in observations {
            if obs.hook != HookType::PreToolUse { continue; }
            let tool = match &obs.tool {
                Some(t) if t.contains("context_search") => t,
                _ => continue,
            };
            // Check for zero results in a paired PostToolUse
            // Actually: PreToolUse has the query, PostToolUse has the result.
            // We need to correlate. Simpler approach: look at PostToolUse with same tool
            // that has response_size == 0 or snippet containing "No results"
            // For extraction, we can look at PostToolUse observations directly.
            ...
        }

        // Actually, scan PostToolUse for context_search with zero results:
        for obs in observations {
            if obs.hook != HookType::PostToolUse { continue; }
            let tool = match &obs.tool {
                Some(t) if t.contains("context_search") => t,
                _ => continue,
            };
            let is_zero = obs.response_size == Some(0)
                || obs.response_snippet.as_ref().map_or(false, |s| s.contains("No results"));
            if !is_zero { continue; }

            // Extract query from the input field
            let query = extract_search_query(&obs.input);
            if query.is_empty() { continue; }

            let normalized = query.trim().to_lowercase();
            query_sessions.entry(normalized)
                .or_default()
                .insert(obs.session_id.clone());
        }

        // Build proposals for queries appearing in 2+ sessions
        let mut proposals = Vec::new();
        for (query, sessions) in &query_sessions {
            if sessions.len() < 2 { continue; }
            let features: Vec<String> = sessions.iter().cloned().collect();
            let confidence = (0.4 + 0.1 * features.len() as f64).min(0.8);
            proposals.push(ProposedEntry {
                title: format!("Knowledge gap: {}", query),
                content: format!(
                    "Agents searched for '{}' across {} sessions with no results. This topic may need explicit documentation.",
                    query, features.len()
                ),
                category: "gap".to_string(),
                topic: "knowledge-management".to_string(),
                tags: vec!["auto-extracted".into(), "knowledge-gap".into()],
                source_rule: "knowledge-gap".to_string(),
                source_features: features,
                extraction_confidence: confidence,
            });
        }
        proposals
    }
}

fn extract_search_query(input: &Option<serde_json::Value>) -> String {
    match input {
        Some(serde_json::Value::Object(map)) => {
            map.get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        }
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}
```

## 4. ImplicitConventionRule (implicit_convention.rs)

```rust
pub struct ImplicitConventionRule;

impl ExtractionRule for ImplicitConventionRule {
    fn name(&self) -> &str { "implicit-convention" }

    fn evaluate(&self, observations: &[ObservationRecord], _store: &Store) -> Vec<ProposedEntry> {
        // 1. Filter for file access operations (Read, Write, Edit tool calls)
        // 2. Extract file paths from input JSON
        // 3. Group by session_id (proxy for feature)
        // 4. Find path patterns that appear in 100% of sessions
        // 5. Require minimum 3 sessions
        // 6. Produce convention entries for universal patterns

        let mut session_files: HashMap<String, HashSet<String>> = HashMap::new();
        let all_sessions: HashSet<String> = observations.iter()
            .map(|o| o.session_id.clone())
            .collect();

        if all_sessions.len() < 3 { return vec![]; }

        for obs in observations {
            let tool = match &obs.tool {
                Some(t) if is_file_tool(t) => t,
                _ => continue,
            };
            let path = extract_file_path(&obs.input);
            if path.is_empty() { continue; }
            // Normalize to directory pattern (strip filename, keep dir)
            let pattern = normalize_path_pattern(&path);
            session_files.entry(obs.session_id.clone())
                .or_default()
                .insert(pattern);
        }

        // Find patterns present in ALL sessions
        let total_sessions = session_files.len();
        if total_sessions < 3 { return vec![]; }

        let mut pattern_counts: HashMap<String, usize> = HashMap::new();
        for (_, patterns) in &session_files {
            for p in patterns {
                *pattern_counts.entry(p.clone()).or_insert(0) += 1;
            }
        }

        let mut proposals = Vec::new();
        for (pattern, count) in &pattern_counts {
            if *count == total_sessions {
                let features: Vec<String> = session_files.keys().cloned().collect();
                let confidence = (0.5 + 0.05 * total_sessions as f64).min(0.9);
                proposals.push(ProposedEntry {
                    title: format!("Convention: agents access {}", pattern),
                    content: format!(
                        "All {} observed sessions access '{}'. This is a consistent workflow pattern.",
                        total_sessions, pattern
                    ),
                    category: "convention".to_string(),
                    topic: "workflow".to_string(),
                    tags: vec!["auto-extracted".into(), "implicit-convention".into()],
                    source_rule: "implicit-convention".to_string(),
                    source_features: features,
                    extraction_confidence: confidence,
                });
            }
        }
        proposals
    }
}

fn is_file_tool(tool: &str) -> bool {
    tool == "Read" || tool == "Write" || tool == "Edit"
        || tool.ends_with("__Read") || tool.ends_with("__Write") || tool.ends_with("__Edit")
}

fn extract_file_path(input: &Option<serde_json::Value>) -> String {
    match input {
        Some(serde_json::Value::Object(map)) => {
            map.get("file_path")
                .or_else(|| map.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        }
        _ => String::new(),
    }
}

fn normalize_path_pattern(path: &str) -> String {
    // Strip to directory level for pattern matching
    // e.g., "/workspaces/unimatrix/crates/foo/src/bar.rs" -> "crates/foo/src/"
    if let Some(pos) = path.rfind('/') {
        let dir = &path[..=pos];
        // Strip common prefix
        dir.trim_start_matches("/workspaces/unimatrix/").to_string()
    } else {
        path.to_string()
    }
}
```

## 5. DeadKnowledgeRule (dead_knowledge.rs)

```rust
pub struct DeadKnowledgeRule;

impl ExtractionRule for DeadKnowledgeRule {
    fn name(&self) -> &str { "dead-knowledge" }

    fn evaluate(&self, observations: &[ObservationRecord], store: &Store) -> Vec<ProposedEntry> {
        // 1. Get recent 5 sessions (distinct session_ids sorted by max ts)
        // 2. Get all active entries from store with access_count > 0
        // 3. For each entry, check if it was accessed (via context_get, context_lookup)
        //    in any of the recent 5 sessions
        // 4. If NOT accessed in recent 5 but was accessed earlier -> deprecation signal

        let mut session_times: HashMap<String, u64> = HashMap::new();
        for obs in observations {
            let ts = session_times.entry(obs.session_id.clone()).or_insert(0);
            if obs.ts > *ts { *ts = obs.ts; }
        }
        let mut sessions_sorted: Vec<(String, u64)> = session_times.into_iter().collect();
        sessions_sorted.sort_by(|a, b| b.1.cmp(&a.1));  // newest first

        if sessions_sorted.len() < 5 { return vec![]; }

        let recent_5: HashSet<&str> = sessions_sorted[..5].iter()
            .map(|(s, _)| s.as_str())
            .collect();

        // Collect entry IDs accessed in recent sessions
        let mut recent_entry_ids: HashSet<u64> = HashSet::new();
        for obs in observations {
            if !recent_5.contains(obs.session_id.as_str()) { continue; }
            let tool = match &obs.tool {
                Some(t) if t.contains("context_get") || t.contains("context_lookup") || t.contains("context_search") => t,
                _ => continue,
            };
            // Extract entry IDs from response
            if let Some(snippet) = &obs.response_snippet {
                // Parse entry IDs from response (look for "id": NNN patterns)
                for id in extract_entry_ids(snippet) {
                    recent_entry_ids.insert(id);
                }
            }
        }

        // Get all active entries from store
        let conn = store.lock_conn();
        // Query active entries with access_count > 0
        // Check which ones are NOT in recent_entry_ids
        let active_entries = query_active_entries_with_access(&conn);

        let mut proposals = Vec::new();
        let all_features: Vec<String> = sessions_sorted.iter().map(|(s, _)| s.clone()).collect();

        for entry in &active_entries {
            if recent_entry_ids.contains(&entry.id) { continue; }
            if entry.access_count == 0 { continue; }

            // This entry was accessed before but not in recent 5 sessions
            let recent_names: Vec<String> = sessions_sorted[..5].iter()
                .map(|(s, _)| s.clone()).collect();

            proposals.push(ProposedEntry {
                title: format!("Possible dead knowledge: {}", entry.title),
                content: format!(
                    "Entry '{}' (ID: {}) has {} accesses but was not used in the last 5 sessions. Consider deprecating.",
                    entry.title, entry.id, entry.access_count
                ),
                category: "lesson-learned".to_string(),
                topic: "knowledge-management".to_string(),
                tags: vec!["auto-extracted".into(), "dead-knowledge".into(), "deprecation-signal".into()],
                source_rule: "dead-knowledge".to_string(),
                source_features: all_features.clone(),
                extraction_confidence: 0.5,
            });
        }
        proposals
    }
}
```

## 6. RecurringFrictionRule (recurring_friction.rs)

```rust
pub struct RecurringFrictionRule;

impl ExtractionRule for RecurringFrictionRule {
    fn name(&self) -> &str { "recurring-friction" }

    fn evaluate(&self, observations: &[ObservationRecord], _store: &Store) -> Vec<ProposedEntry> {
        // 1. Group observations by session_id
        // 2. For each session, run detection rules
        // 3. Collect rule_name -> set of sessions that fired
        // 4. For rules firing in 3+ sessions, produce entry

        let mut session_records: HashMap<String, Vec<ObservationRecord>> = HashMap::new();
        for obs in observations {
            session_records.entry(obs.session_id.clone())
                .or_default()
                .push(obs.clone());
        }

        let detection_rules = crate::detection::default_rules(None);
        let mut rule_sessions: HashMap<String, HashSet<String>> = HashMap::new();

        for (session_id, records) in &session_records {
            let findings = crate::detection::detect_hotspots(records, &detection_rules);
            for finding in &findings {
                rule_sessions.entry(finding.rule_name.clone())
                    .or_default()
                    .insert(session_id.clone());
            }
        }

        let mut proposals = Vec::new();
        for (rule_name, sessions) in &rule_sessions {
            if sessions.len() < 3 { continue; }
            let features: Vec<String> = sessions.iter().cloned().collect();
            let confidence = (0.5 + 0.1 * features.len() as f64).min(0.85);
            proposals.push(ProposedEntry {
                title: format!("Recurring friction: {}", rule_name),
                content: format!(
                    "Detection rule '{}' fired in {} sessions: [{}]. This recurring pattern indicates a systemic issue.",
                    rule_name, features.len(), features.join(", ")
                ),
                category: "lesson-learned".to_string(),
                topic: "process-improvement".to_string(),
                tags: vec!["auto-extracted".into(), "recurring-friction".into()],
                source_rule: "recurring-friction".to_string(),
                source_features: features,
                extraction_confidence: confidence,
            });
        }
        proposals
    }
}
```

## 7. FileDependencyRule (file_dependency.rs)

```rust
pub struct FileDependencyRule;

const DEPENDENCY_WINDOW_SECS: u64 = 60;

impl ExtractionRule for FileDependencyRule {
    fn name(&self) -> &str { "file-dependency" }

    fn evaluate(&self, observations: &[ObservationRecord], _store: &Store) -> Vec<ProposedEntry> {
        // 1. Group observations by session_id, sorted by timestamp
        // 2. For each session, find Read(A) -> Write/Edit(B) chains within 60s window
        // 3. Collect (A, B) pairs per session
        // 4. Find pairs appearing in 3+ sessions

        let mut session_records: HashMap<String, Vec<&ObservationRecord>> = HashMap::new();
        for obs in observations {
            session_records.entry(obs.session_id.clone())
                .or_default()
                .push(obs);
        }

        let mut pair_sessions: HashMap<(String, String), HashSet<String>> = HashMap::new();

        for (session_id, records) in &session_records {
            let mut sorted = records.clone();
            sorted.sort_by_key(|r| r.ts);

            // Find Read -> Write/Edit pairs within window
            for (i, read_obs) in sorted.iter().enumerate() {
                let tool = match &read_obs.tool {
                    Some(t) if is_read_tool(t) => t,
                    _ => continue,
                };
                let read_path = extract_file_path(&read_obs.input);
                if read_path.is_empty() { continue; }

                for write_obs in &sorted[i+1..] {
                    // Check time window
                    if write_obs.ts > read_obs.ts + DEPENDENCY_WINDOW_SECS * 1000 {
                        break;  // beyond window
                    }
                    let write_tool = match &write_obs.tool {
                        Some(t) if is_write_tool(t) => t,
                        _ => continue,
                    };
                    let write_path = extract_file_path(&write_obs.input);
                    if write_path.is_empty() || write_path == read_path { continue; }

                    pair_sessions
                        .entry((read_path.clone(), write_path.clone()))
                        .or_default()
                        .insert(session_id.clone());
                }
            }
        }

        let mut proposals = Vec::new();
        for ((file_a, file_b), sessions) in &pair_sessions {
            if sessions.len() < 3 { continue; }
            let features: Vec<String> = sessions.iter().cloned().collect();
            let confidence = (0.4 + 0.1 * features.len() as f64).min(0.8);
            proposals.push(ProposedEntry {
                title: format!("File dependency: {} -> {}", file_a, file_b),
                content: format!(
                    "Read of '{}' consistently followed by write to '{}' within 60s, observed in {} sessions.",
                    file_a, file_b, features.len()
                ),
                category: "pattern".to_string(),
                topic: "workflow".to_string(),
                tags: vec!["auto-extracted".into(), "file-dependency".into()],
                source_rule: "file-dependency".to_string(),
                source_features: features,
                extraction_confidence: confidence,
            });
        }
        proposals
    }
}

fn is_read_tool(tool: &str) -> bool {
    tool == "Read" || tool.ends_with("__Read")
}

fn is_write_tool(tool: &str) -> bool {
    tool == "Write" || tool == "Edit" || tool.ends_with("__Write") || tool.ends_with("__Edit")
}
```

## Error Handling

- ExtractionRule::evaluate never panics; returns empty vec on errors
- Quality gate returns structured rejection reasons
- Store access errors in DeadKnowledgeRule are logged and skipped
- All rules use defensive parsing of observation input fields
