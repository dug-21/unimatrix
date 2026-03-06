//! Knowledge extraction pipeline: trait, rules, and quality gate (col-013).
//!
//! Mirrors the detection module pattern: trait + rule implementations + pipeline runner.
//! Extraction rules analyze observation data to propose new knowledge entries.
//! The quality gate pipeline filters proposals before storage (ADR-005).

pub mod dead_knowledge;
pub mod file_dependency;
pub mod implicit_convention;
pub mod knowledge_gap;
pub mod neural;
pub mod recurring_friction;
pub mod shadow;

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use unimatrix_store::Store;

use crate::types::ObservationRecord;

/// Trait for knowledge extraction rules.
///
/// Rules inspect observation records and the store to produce proposed entries.
/// Mirrors `DetectionRule` from the detection module.
pub trait ExtractionRule: Send {
    /// Unique rule name.
    fn name(&self) -> &str;
    /// Analyze observations and produce proposed entries.
    fn evaluate(&self, observations: &[ObservationRecord], store: &Store) -> Vec<ProposedEntry>;
}

/// A proposed knowledge entry produced by an extraction rule.
#[derive(Debug, Clone)]
pub struct ProposedEntry {
    pub title: String,
    pub content: String,
    pub category: String,
    pub topic: String,
    pub tags: Vec<String>,
    /// Name of the rule that produced this entry.
    pub source_rule: String,
    /// Feature cycles (or session IDs) that contributed evidence.
    pub source_features: Vec<String>,
    /// Rule's confidence in this extraction, in [0.0, 1.0].
    pub extraction_confidence: f64,
}

/// Result of the quality gate pipeline for a proposed entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QualityGateResult {
    Accept,
    Reject {
        reason: String,
        check_name: String,
    },
}

/// Mutable context shared across the extraction pipeline within a tick.
#[derive(Debug, Clone)]
pub struct ExtractionContext {
    /// Last processed observation ID (watermark for incremental processing).
    pub last_watermark: u64,
    /// Number of entries accepted this clock hour.
    pub rate_count: u64,
    /// Current clock hour (epoch_secs / 3600).
    pub rate_hour: u64,
    /// Cumulative statistics.
    pub stats: ExtractionStats,
}

/// Aggregate extraction statistics for status reporting (FR-10.3).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ExtractionStats {
    pub entries_extracted_total: u64,
    pub entries_rejected_total: u64,
    pub last_extraction_run: Option<u64>,
    pub rules_fired: HashMap<String, u64>,
}

impl Default for ExtractionContext {
    fn default() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        ExtractionContext {
            last_watermark: 0,
            rate_count: 0,
            rate_hour: now / 3600,
            stats: ExtractionStats::default(),
        }
    }
}

impl ExtractionContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if rate limit allows another extraction. If yes, increments counter.
    /// Returns false if the hourly limit (10) has been reached.
    pub fn check_and_increment_rate(&mut self) -> bool {
        let current_hour = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / 3600;
        if current_hour != self.rate_hour {
            self.rate_count = 0;
            self.rate_hour = current_hour;
        }
        if self.rate_count >= 10 {
            return false;
        }
        self.rate_count += 1;
        true
    }
}

/// Categories allowed for auto-extracted entries.
const ALLOWED_CATEGORIES: &[&str] = &[
    "convention",
    "pattern",
    "lesson-learned",
    "gap",
    "decision",
];

/// Minimum source features required per rule for cross-feature validation.
fn min_features_for_rule(rule_name: &str) -> usize {
    match rule_name {
        "knowledge-gap" => 2,
        "implicit-convention" | "recurring-friction" | "file-dependency" => 3,
        "dead-knowledge" => 5,
        _ => 3,
    }
}

/// Run the quality gate pipeline on a proposed entry (ADR-005: cheapest first).
///
/// Checks 1-4 are in-memory and O(1). Checks 5-6 (near-duplicate, contradiction)
/// require embedding and are handled at the server level after this function.
pub fn quality_gate(entry: &ProposedEntry, ctx: &mut ExtractionContext) -> QualityGateResult {
    // Check 1: Rate limit (O(1))
    if !ctx.check_and_increment_rate() {
        return QualityGateResult::Reject {
            reason: "Rate limit exceeded (10/hour)".to_string(),
            check_name: "rate_limit".to_string(),
        };
    }

    // Check 2: Content validation (O(1))
    if entry.title.len() < 10 {
        return QualityGateResult::Reject {
            reason: format!("Title too short ({} chars, min 10)", entry.title.len()),
            check_name: "content_validation".to_string(),
        };
    }
    if entry.content.len() < 20 {
        return QualityGateResult::Reject {
            reason: format!("Content too short ({} chars, min 20)", entry.content.len()),
            check_name: "content_validation".to_string(),
        };
    }
    if !ALLOWED_CATEGORIES.contains(&entry.category.as_str()) {
        return QualityGateResult::Reject {
            reason: format!("Category '{}' not in allowlist", entry.category),
            check_name: "content_validation".to_string(),
        };
    }

    // Check 3: Cross-feature validation (O(1))
    let min_features = min_features_for_rule(&entry.source_rule);
    if entry.source_features.len() < min_features {
        return QualityGateResult::Reject {
            reason: format!(
                "Insufficient features: need {}, got {} for rule '{}'",
                min_features,
                entry.source_features.len(),
                entry.source_rule,
            ),
            check_name: "cross_feature".to_string(),
        };
    }

    // Check 4: Confidence floor (O(1))
    if entry.extraction_confidence < 0.2 {
        return QualityGateResult::Reject {
            reason: format!(
                "Confidence {:.2} below 0.2 floor",
                entry.extraction_confidence
            ),
            check_name: "confidence_floor".to_string(),
        };
    }

    QualityGateResult::Accept
}

/// Return the default set of extraction rules (5 total).
pub fn default_extraction_rules() -> Vec<Box<dyn ExtractionRule>> {
    vec![
        Box::new(knowledge_gap::KnowledgeGapRule),
        Box::new(implicit_convention::ImplicitConventionRule),
        Box::new(dead_knowledge::DeadKnowledgeRule),
        Box::new(recurring_friction::RecurringFrictionRule),
        Box::new(file_dependency::FileDependencyRule),
    ]
}

/// Run all extraction rules against observations and return proposals.
pub fn run_extraction_rules(
    observations: &[ObservationRecord],
    store: &Store,
    rules: &[Box<dyn ExtractionRule>],
) -> Vec<ProposedEntry> {
    let mut proposals = Vec::new();
    for rule in rules {
        proposals.extend(rule.evaluate(observations, store));
    }
    proposals
}

// -- Shared helpers used by multiple rules --

/// Extract a file path from an observation's input JSON.
pub(crate) fn extract_file_path(input: &Option<serde_json::Value>) -> String {
    match input {
        Some(serde_json::Value::Object(map)) => map
            .get("file_path")
            .or_else(|| map.get("path"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

/// Check if a tool name is a file-reading tool.
pub(crate) fn is_read_tool(tool: &str) -> bool {
    tool == "Read" || tool.ends_with("__Read")
}

/// Check if a tool name is a file-writing/editing tool.
pub(crate) fn is_write_tool(tool: &str) -> bool {
    tool == "Write"
        || tool == "Edit"
        || tool.ends_with("__Write")
        || tool.ends_with("__Edit")
}

/// Check if a tool name is any file access tool.
pub(crate) fn is_file_tool(tool: &str) -> bool {
    is_read_tool(tool) || is_write_tool(tool)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_entry() -> ProposedEntry {
        ProposedEntry {
            title: "Valid title that is long enough".to_string(),
            content: "This is valid content with enough length for the quality gate".to_string(),
            category: "convention".to_string(),
            topic: "test".to_string(),
            tags: vec!["auto-extracted".to_string()],
            source_rule: "knowledge-gap".to_string(),
            source_features: vec!["session-1".to_string(), "session-2".to_string()],
            extraction_confidence: 0.5,
        }
    }

    #[test]
    fn default_extraction_rules_returns_five() {
        let rules = default_extraction_rules();
        assert_eq!(rules.len(), 5);
    }

    #[test]
    fn default_extraction_rules_names() {
        let rules = default_extraction_rules();
        let names: Vec<&str> = rules.iter().map(|r| r.name()).collect();
        assert!(names.contains(&"knowledge-gap"));
        assert!(names.contains(&"implicit-convention"));
        assert!(names.contains(&"dead-knowledge"));
        assert!(names.contains(&"recurring-friction"));
        assert!(names.contains(&"file-dependency"));
    }

    #[test]
    fn quality_gate_accepts_valid_entry() {
        let entry = make_valid_entry();
        let mut ctx = ExtractionContext::new();
        assert_eq!(quality_gate(&entry, &mut ctx), QualityGateResult::Accept);
    }

    #[test]
    fn quality_gate_rate_limit_rejects_after_ten() {
        let entry = make_valid_entry();
        let mut ctx = ExtractionContext::new();
        // Accept 10 entries
        for _ in 0..10 {
            assert_eq!(quality_gate(&entry, &mut ctx), QualityGateResult::Accept);
        }
        // 11th should be rejected
        let result = quality_gate(&entry, &mut ctx);
        match result {
            QualityGateResult::Reject { check_name, .. } => {
                assert_eq!(check_name, "rate_limit");
            }
            _ => panic!("expected rate limit rejection"),
        }
    }

    #[test]
    fn quality_gate_rejects_short_title() {
        let mut entry = make_valid_entry();
        entry.title = "Short".to_string();
        let mut ctx = ExtractionContext::new();
        match quality_gate(&entry, &mut ctx) {
            QualityGateResult::Reject { check_name, .. } => {
                assert_eq!(check_name, "content_validation");
            }
            _ => panic!("expected content validation rejection"),
        }
    }

    #[test]
    fn quality_gate_rejects_short_content() {
        let mut entry = make_valid_entry();
        entry.content = "Too short".to_string();
        let mut ctx = ExtractionContext::new();
        match quality_gate(&entry, &mut ctx) {
            QualityGateResult::Reject { check_name, .. } => {
                assert_eq!(check_name, "content_validation");
            }
            _ => panic!("expected content validation rejection"),
        }
    }

    #[test]
    fn quality_gate_rejects_invalid_category() {
        let mut entry = make_valid_entry();
        entry.category = "invalid-category".to_string();
        let mut ctx = ExtractionContext::new();
        match quality_gate(&entry, &mut ctx) {
            QualityGateResult::Reject { check_name, .. } => {
                assert_eq!(check_name, "content_validation");
            }
            _ => panic!("expected content validation rejection"),
        }
    }

    #[test]
    fn quality_gate_rejects_insufficient_features() {
        let mut entry = make_valid_entry();
        entry.source_rule = "implicit-convention".to_string();
        entry.source_features = vec!["s1".to_string(), "s2".to_string()]; // needs 3
        let mut ctx = ExtractionContext::new();
        match quality_gate(&entry, &mut ctx) {
            QualityGateResult::Reject { check_name, .. } => {
                assert_eq!(check_name, "cross_feature");
            }
            _ => panic!("expected cross-feature rejection"),
        }
    }

    #[test]
    fn quality_gate_accepts_at_minimum_features() {
        let mut entry = make_valid_entry();
        entry.source_rule = "implicit-convention".to_string();
        entry.source_features = vec!["s1".to_string(), "s2".to_string(), "s3".to_string()]; // exactly 3
        let mut ctx = ExtractionContext::new();
        assert_eq!(quality_gate(&entry, &mut ctx), QualityGateResult::Accept);
    }

    #[test]
    fn quality_gate_rejects_low_confidence() {
        let mut entry = make_valid_entry();
        entry.extraction_confidence = 0.15;
        let mut ctx = ExtractionContext::new();
        match quality_gate(&entry, &mut ctx) {
            QualityGateResult::Reject { check_name, .. } => {
                assert_eq!(check_name, "confidence_floor");
            }
            _ => panic!("expected confidence floor rejection"),
        }
    }

    #[test]
    fn min_features_for_each_rule() {
        assert_eq!(min_features_for_rule("knowledge-gap"), 2);
        assert_eq!(min_features_for_rule("implicit-convention"), 3);
        assert_eq!(min_features_for_rule("recurring-friction"), 3);
        assert_eq!(min_features_for_rule("file-dependency"), 3);
        assert_eq!(min_features_for_rule("dead-knowledge"), 5);
        assert_eq!(min_features_for_rule("unknown-rule"), 3);
    }

    #[test]
    fn extraction_context_rate_resets_on_new_hour() {
        let mut ctx = ExtractionContext::new();
        // Fill up the rate
        for _ in 0..10 {
            assert!(ctx.check_and_increment_rate());
        }
        assert!(!ctx.check_and_increment_rate());
        // Simulate hour change
        ctx.rate_hour -= 1;
        assert!(ctx.check_and_increment_rate());
    }

    #[test]
    fn extract_file_path_from_object() {
        let input = Some(serde_json::json!({"file_path": "/tmp/test.rs"}));
        assert_eq!(extract_file_path(&input), "/tmp/test.rs");
    }

    #[test]
    fn extract_file_path_from_path_key() {
        let input = Some(serde_json::json!({"path": "/tmp/other.rs"}));
        assert_eq!(extract_file_path(&input), "/tmp/other.rs");
    }

    #[test]
    fn extract_file_path_none() {
        assert_eq!(extract_file_path(&None), "");
    }

    #[test]
    fn is_file_tool_variants() {
        assert!(is_read_tool("Read"));
        assert!(is_read_tool("mcp__fs__Read"));
        assert!(!is_read_tool("Write"));
        assert!(is_write_tool("Write"));
        assert!(is_write_tool("Edit"));
        assert!(is_write_tool("mcp__fs__Edit"));
        assert!(!is_write_tool("Read"));
        assert!(is_file_tool("Read"));
        assert!(is_file_tool("Write"));
        assert!(!is_file_tool("Bash"));
    }
}
