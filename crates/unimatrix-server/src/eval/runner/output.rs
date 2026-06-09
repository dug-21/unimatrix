//! Result types for eval runner output (nan-007).
//!
//! All types derive `Serialize` + `Deserialize` and are written as pretty-printed
//! JSON per scenario to the output directory.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::trust::TrustOutcome;

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Per-entry result produced by one profile's search replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredEntry {
    pub id: u64,
    pub title: String,
    /// Snippet/content text the agent reads (nan-018 ADR-003 carry-flag).
    /// Populated from `se.entry.content` in replay.rs. Part of the cost-metric
    /// payload (`title + content`); `#[serde(default)]` for backward-compat with
    /// pre-nan-018 result JSON that carried only `title`.
    #[serde(default)]
    pub content: String,
    /// Knowledge category of this entry — populated from `se.entry.category` in replay.rs.
    pub category: String,
    pub final_score: f64,
    pub similarity: f64,
    pub confidence: f64,
    pub status: String,
    /// Always `None` in nan-007 — NLI re-ranking is W1-4.
    pub nli_rerank_delta: Option<f64>,
}

/// Metric result for one profile against one scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileResult {
    pub entries: Vec<ScoredEntry>,
    pub latency_ms: u64,
    pub p_at_k: f64,
    pub mrr: f64,
    /// Fraction of configured categories represented in the result — range [0.0, 1.0].
    pub cc_at_k: f64,
    /// Raw Shannon entropy (natural log) over result category distribution — range [0.0, ln(n)].
    pub icd: f64,
    /// Token-weighted cost of the result set (nan-018 ADR-003): `Σ token_proxy(entry)`.
    /// Primary cost axis; `k` is secondary (= `entries.len()`). `#[serde(default)]`
    /// for backward-compat with pre-nan-018 result JSON.
    #[serde(default)]
    pub cost_tokens: f64,
    /// Property-based trust verdict for this profile/scenario (nan-018 ADR-004).
    /// Trivially-passing when the scenario carries no assertions (log-sourced runs).
    /// `#[serde(default)]` for backward-compat (the #3548 named-backward-compat boundary).
    #[serde(default)]
    pub trust: TrustOutcome,
}

/// Rank change record for a single entry between baseline and candidate profiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankChange {
    pub entry_id: u64,
    /// 1-indexed position in baseline result list (or `baseline_len + 1` if absent).
    pub from_rank: usize,
    /// 1-indexed position in candidate result list (or `candidate_len + 1` if absent).
    pub to_rank: usize,
}

/// Comparison metrics across profiles for one scenario.
///
/// First profile is baseline by convention. Comparison is baseline vs. first
/// non-baseline profile. All profiles stored in `ScenarioResult.profiles`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonMetrics {
    pub kendall_tau: f64,
    pub rank_changes: Vec<RankChange>,
    /// `candidate.mrr - baseline.mrr`
    pub mrr_delta: f64,
    /// `candidate.p_at_k - baseline.p_at_k`
    pub p_at_k_delta: f64,
    /// `candidate.latency_ms as i64 - baseline.latency_ms as i64`
    pub latency_overhead_ms: i64,
    /// `candidate.cc_at_k - baseline.cc_at_k`; positive means candidate improved.
    pub cc_at_k_delta: f64,
    /// `candidate.icd - baseline.icd`; positive means candidate improved.
    pub icd_delta: f64,
}

/// Complete result for one scenario across all profiles.
///
/// Written as a pretty-printed JSON file per scenario to the output directory.
/// `phase` carries the `query_log.phase` value through from scenario context.
/// `#[serde(default)]` only — no `skip_serializing_if`; the runner always emits
/// `"phase":null` or `"phase":"delivery"` so downstream tooling can rely on key
/// presence (ADR-001: consistent key presence on the writer side).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    pub scenario_id: String,
    pub query: String,
    pub profiles: HashMap<String, ProfileResult>,
    pub comparison: ComparisonMetrics,
    #[serde(default)]
    pub phase: Option<String>,
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// Write one JSON result file for a scenario to the output directory.
///
/// Filename is derived from `scenario_id` with path separators replaced by `_`.
pub(super) fn write_scenario_result(
    result: ScenarioResult,
    out_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let filename = result.scenario_id.replace('/', "_").replace('\\', "_") + ".json";
    let out_path = out_dir.join(&filename);
    let json = serde_json::to_string_pretty(&result)?;
    std::fs::write(&out_path, json.as_bytes())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scored_entry(category: &str) -> ScoredEntry {
        ScoredEntry {
            id: 1,
            title: "Entry 1".to_string(),
            content: "entry one body".to_string(),
            category: category.to_string(),
            final_score: 0.9,
            similarity: 0.85,
            confidence: 0.7,
            status: "Active".to_string(),
            nli_rerank_delta: None,
        }
    }

    fn make_profile_result(cc_at_k: f64, icd: f64) -> ProfileResult {
        ProfileResult {
            entries: vec![make_scored_entry("decision")],
            latency_ms: 10,
            p_at_k: 1.0,
            mrr: 1.0,
            cc_at_k,
            icd,
            cost_tokens: 0.0,
            trust: TrustOutcome::trivial_pass(),
        }
    }

    fn make_comparison_metrics(cc_at_k_delta: f64, icd_delta: f64) -> ComparisonMetrics {
        ComparisonMetrics {
            kendall_tau: 1.0,
            rank_changes: vec![],
            mrr_delta: 0.0,
            p_at_k_delta: 0.0,
            latency_overhead_ms: 0,
            cc_at_k_delta,
            icd_delta,
        }
    }

    #[test]
    fn test_scored_entry_category_serializes() {
        let entry = make_scored_entry("lesson-learned");
        let json = serde_json::to_string(&entry).expect("serialization failed");
        assert!(json.contains("\"category\""), "JSON missing 'category' key");
        assert!(
            json.contains("\"lesson-learned\""),
            "JSON missing 'lesson-learned' value"
        );
    }

    #[test]
    fn test_profile_result_cc_at_k_icd_serialize() {
        let result = make_profile_result(0.75, 1.1);
        let json = serde_json::to_string(&result).expect("serialization failed");
        assert!(json.contains("\"cc_at_k\""), "JSON missing 'cc_at_k' key");
        assert!(json.contains("\"icd\""), "JSON missing 'icd' key");
        assert!(json.contains("0.75"), "JSON missing cc_at_k value");
        assert!(json.contains("1.1"), "JSON missing icd value");
    }

    #[test]
    fn test_comparison_metrics_delta_fields_serialize() {
        let cm = make_comparison_metrics(0.15, -0.05);
        let json = serde_json::to_string(&cm).expect("serialization failed");
        assert!(
            json.contains("\"cc_at_k_delta\""),
            "JSON missing 'cc_at_k_delta' key"
        );
        assert!(
            json.contains("\"icd_delta\""),
            "JSON missing 'icd_delta' key"
        );
    }

    #[test]
    fn test_scored_entry_round_trip() {
        let entry = make_scored_entry("decision");
        let json = serde_json::to_string(&entry).expect("serialization failed");
        let decoded: ScoredEntry = serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(decoded.id, entry.id);
        assert_eq!(decoded.category, "decision");
        assert_eq!(decoded.title, entry.title);
        assert_eq!(decoded.final_score, entry.final_score);
    }

    #[test]
    fn test_profile_result_round_trip() {
        let result = make_profile_result(0.857, 1.234);
        let json = serde_json::to_string(&result).expect("serialization failed");
        let decoded: ProfileResult = serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(decoded.cc_at_k, 0.857);
        assert_eq!(decoded.icd, 1.234);
    }

    /// Producer side (#3557): `cost_tokens` + `trust` round-trip with NON-TRIVIAL
    /// values (cost != 0, a populated `TrustOutcome` with a violation).
    #[test]
    fn test_profile_result_cost_trust_roundtrip_nontrivial() {
        let mut result = make_profile_result(0.5, 0.9);
        result.cost_tokens = 42.5;
        result.trust = TrustOutcome {
            absence_pass: false,
            rank_pass: true,
            violations: vec!["absence: forbidden 'F' present at rank 0".to_string()],
        };

        let json = serde_json::to_string(&result).expect("serialize");
        assert!(json.contains("\"cost_tokens\""), "missing cost_tokens key");
        assert!(json.contains("\"trust\""), "missing trust key");
        assert!(json.contains("42.5"), "missing non-trivial cost value");

        let decoded: ProfileResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.cost_tokens, 42.5);
        assert!(!decoded.trust.absence_pass);
        assert!(decoded.trust.rank_pass);
        assert_eq!(decoded.trust.violations.len(), 1);
    }

    /// Backward-compat (#3548 named test): a pre-nan-018 `ProfileResult` JSON missing
    /// `cost_tokens` and `trust` (and the new `ScoredEntry.content`) deserializes via
    /// `#[serde(default)]` — cost defaults 0.0, trust defaults to the trivial pass.
    #[test]
    fn test_profile_result_backward_compat_pre_nan018_json() {
        // Note: no `cost_tokens`, no `trust`, and an entry without `content`.
        let legacy = r#"{
            "entries": [
                {"id": 7, "title": "legacy", "category": "decision",
                 "final_score": 0.5, "similarity": 0.4, "confidence": 0.3,
                 "status": "Active", "nli_rerank_delta": null}
            ],
            "latency_ms": 5, "p_at_k": 0.5, "mrr": 0.5, "cc_at_k": 0.0, "icd": 0.0
        }"#;
        let decoded: ProfileResult =
            serde_json::from_str(legacy).expect("legacy ProfileResult must deserialize");
        assert_eq!(
            decoded.cost_tokens, 0.0,
            "missing cost_tokens defaults to 0.0"
        );
        assert_eq!(
            decoded.trust,
            TrustOutcome::trivial_pass(),
            "missing trust defaults to trivial pass"
        );
        assert_eq!(
            decoded.entries[0].content, "",
            "missing ScoredEntry.content defaults to empty string"
        );
    }

    #[test]
    fn test_comparison_metrics_round_trip() {
        let cm = make_comparison_metrics(0.143, 0.211);
        let json = serde_json::to_string(&cm).expect("serialization failed");
        let decoded: ComparisonMetrics =
            serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(decoded.cc_at_k_delta, 0.143);
        assert_eq!(decoded.icd_delta, 0.211);
    }

    fn make_scenario_result(phase: Option<String>) -> ScenarioResult {
        ScenarioResult {
            scenario_id: "test-id".to_string(),
            query: "test query".to_string(),
            profiles: HashMap::new(),
            comparison: make_comparison_metrics(0.0, 0.0),
            phase,
        }
    }

    /// Runner copy carries `#[serde(default)]` only — no `skip_serializing_if`.
    /// Confirms the key IS present as `"phase":null` when phase is None (R-05, AC-03).
    /// If a delivery agent mistakenly adds `skip_serializing_if`, this test fails.
    #[test]
    fn test_scenario_result_phase_null_serialized_as_null() {
        let result = make_scenario_result(None);
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(
            json.contains("\"phase\":null"),
            "runner copy must emit explicit \"phase\":null for None — got: {json}"
        );
        assert!(
            !json.contains("\"phase\":\""),
            "phase must not be a non-null string value when None — got: {json}"
        );
    }

    /// Confirms non-null phase serializes as the string value (AC-03).
    #[test]
    fn test_scenario_result_phase_non_null_serialized() {
        let result = make_scenario_result(Some("delivery".to_string()));
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(
            json.contains("\"phase\":\"delivery\""),
            "runner copy must emit \"phase\":\"delivery\" — got: {json}"
        );
    }
}
