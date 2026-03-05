//! Status report formatting and data structures.

use std::collections::HashMap;

use rmcp::model::{CallToolResult, Content};
use serde::Serialize;

use super::{format_timestamp, ResponseFormat};

/// Aggregated health metrics for format_status_report.
pub struct StatusReport {
    /// Count of active entries.
    pub total_active: u64,
    /// Count of deprecated entries.
    pub total_deprecated: u64,
    /// Count of proposed entries.
    pub total_proposed: u64,
    /// Count of quarantined entries.
    pub total_quarantined: u64,
    /// Category name to entry count.
    pub category_distribution: Vec<(String, u64)>,
    /// Topic name to entry count.
    pub topic_distribution: Vec<(String, u64)>,
    /// Entries that supersede another entry.
    pub entries_with_supersedes: u64,
    /// Entries that are superseded by another entry.
    pub entries_with_superseded_by: u64,
    /// Sum of correction_count across all entries.
    pub total_correction_count: u64,
    /// Trust source to entry count.
    pub trust_source_distribution: Vec<(String, u64)>,
    /// Entries with empty created_by field.
    pub entries_without_attribution: u64,
    /// Detected contradictions between entries.
    pub contradictions: Vec<crate::infra::contradiction::ContradictionPair>,
    /// Number of contradictions detected.
    pub contradiction_count: usize,
    /// Entries with inconsistent embeddings.
    pub embedding_inconsistencies: Vec<crate::infra::contradiction::EmbeddingInconsistency>,
    /// Whether contradiction scanning was performed.
    pub contradiction_scan_performed: bool,
    /// Whether embedding consistency check was performed.
    pub embedding_check_performed: bool,
    /// Total co-access pairs in CO_ACCESS table.
    pub total_co_access_pairs: u64,
    /// Active (non-stale) co-access pairs.
    pub active_co_access_pairs: u64,
    /// Top co-access pairs by count.
    pub top_co_access_pairs: Vec<CoAccessClusterEntry>,
    /// Number of stale pairs cleaned during this status call.
    pub stale_pairs_cleaned: u64,
    /// Composite lambda coherence score [0.0, 1.0].
    pub coherence: f64,
    /// Confidence freshness dimension score.
    pub confidence_freshness_score: f64,
    /// Graph quality dimension score.
    pub graph_quality_score: f64,
    /// Embedding consistency dimension score (1.0 if not checked).
    pub embedding_consistency_score: f64,
    /// Contradiction density dimension score.
    pub contradiction_density_score: f64,
    /// Number of entries with stale confidence.
    pub stale_confidence_count: u64,
    /// Number of entries whose confidence was refreshed this call.
    pub confidence_refreshed_count: u64,
    /// Stale node ratio in HNSW graph.
    pub graph_stale_ratio: f64,
    /// Whether graph compaction was performed this call.
    pub graph_compacted: bool,
    /// Actionable maintenance recommendations.
    pub maintenance_recommendations: Vec<String>,
    /// Total outcome entries.
    pub total_outcomes: u64,
    /// Outcome count by workflow type (from type: tag).
    pub outcomes_by_type: Vec<(String, u64)>,
    /// Outcome count by result (from result: tag).
    pub outcomes_by_result: Vec<(String, u64)>,
    /// Top feature cycles by outcome count.
    pub outcomes_by_feature_cycle: Vec<(String, u64)>,
    /// Number of observation JSONL files.
    pub observation_file_count: u64,
    /// Total size of observation files in bytes.
    pub observation_total_size_bytes: u64,
    /// Age of oldest observation file in days.
    pub observation_oldest_file_days: u64,
    /// Session IDs approaching 60-day cleanup.
    pub observation_approaching_cleanup: Vec<String>,
    /// Number of feature cycles with stored metrics.
    pub retrospected_feature_count: u64,
    /// Unix timestamp of last background maintenance tick.
    pub last_maintenance_run: Option<u64>,
    /// Unix timestamp of next scheduled background tick.
    pub next_maintenance_scheduled: Option<u64>,
    /// Extraction pipeline statistics from background tick.
    pub extraction_stats: Option<ExtractionStatsResponse>,
    /// Per-trust-source coherence lambda scores.
    pub coherence_by_source: Vec<(String, f64)>,
}

/// Extraction pipeline statistics for status reporting (col-013).
#[derive(Debug, Clone, Serialize)]
pub struct ExtractionStatsResponse {
    /// Total entries accepted and stored by the extraction pipeline.
    pub entries_extracted_total: u64,
    /// Total entries rejected by the quality gate.
    pub entries_rejected_total: u64,
    /// Unix timestamp of last extraction run.
    pub last_extraction_run: Option<u64>,
    /// Per-rule counts of fired extractions.
    pub rules_fired: Vec<(String, u64)>,
}

/// A co-access cluster entry for status reporting.
#[derive(Serialize)]
pub struct CoAccessClusterEntry {
    /// First entry ID in the pair.
    pub entry_id_a: u64,
    /// Second entry ID in the pair.
    pub entry_id_b: u64,
    /// Title of the first entry.
    pub title_a: String,
    /// Title of the second entry.
    pub title_b: String,
    /// Number of times the pair was co-retrieved.
    pub count: u32,
    /// Unix timestamp of most recent co-retrieval.
    pub last_updated: u64,
}

/// Format a status report response with health metrics.
pub fn format_status_report(report: &StatusReport, format: ResponseFormat) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let mut text = format!(
                "Active: {} | Deprecated: {} | Proposed: {} | Quarantined: {} | Corrections: {}",
                report.total_active,
                report.total_deprecated,
                report.total_proposed,
                report.total_quarantined,
                report.total_correction_count
            );
            if report.contradiction_scan_performed {
                text.push_str(&format!(" | Contradictions: {}", report.contradiction_count));
            }
            text.push_str(&format!(
                "\nCoherence: {:.4} (confidence_freshness: {:.4}, graph_quality: {:.4}, embedding_consistency: {:.4}, contradiction_density: {:.4})",
                report.coherence,
                report.confidence_freshness_score,
                report.graph_quality_score,
                report.embedding_consistency_score,
                report.contradiction_density_score,
            ));
            if report.stale_confidence_count > 0 {
                text.push_str(&format!(
                    "\nStale confidence: {} entries",
                    report.stale_confidence_count
                ));
            }
            if report.confidence_refreshed_count > 0 {
                text.push_str(&format!(
                    "\nConfidence refreshed: {} entries",
                    report.confidence_refreshed_count
                ));
            }
            if report.graph_stale_ratio > 0.0 {
                text.push_str(&format!(
                    "\nGraph stale ratio: {:.2}%",
                    report.graph_stale_ratio * 100.0
                ));
            }
            if report.graph_compacted {
                text.push_str("\nGraph compacted: yes");
            }
            for rec in &report.maintenance_recommendations {
                text.push_str(&format!("\nRecommendation: {rec}"));
            }
            text.push_str(&format!(
                "\nCo-access: {} active pairs ({} total), {} stale pairs cleaned",
                report.active_co_access_pairs,
                report.total_co_access_pairs,
                report.stale_pairs_cleaned,
            ));
            if report.total_outcomes > 0 {
                text.push_str(&format!("\nOutcomes: {} total", report.total_outcomes));
            }
            text.push_str(&format!(
                "\nObservation: {} records ({} sessions), oldest {} days, {} retrospected",
                report.observation_file_count,
                report.observation_total_size_bytes,
                report.observation_oldest_file_days,
                report.retrospected_feature_count,
            ));
            if !report.observation_approaching_cleanup.is_empty() {
                text.push_str(&format!(
                    "\nApproaching cleanup (>45 days): {}",
                    report.observation_approaching_cleanup.join(", ")
                ));
            }
            if let Some(ts) = report.last_maintenance_run {
                text.push_str(&format!("\nLast maintenance: {}", format_timestamp(ts)));
            }
            if let Some(ts) = report.next_maintenance_scheduled {
                text.push_str(&format!("\nNext maintenance: {}", format_timestamp(ts)));
            }
            if let Some(ref stats) = report.extraction_stats {
                text.push_str(&format!(
                    "\nExtraction: {} extracted, {} rejected",
                    stats.entries_extracted_total, stats.entries_rejected_total
                ));
            }
            if !report.coherence_by_source.is_empty() {
                let pairs: Vec<String> = report.coherence_by_source.iter()
                    .map(|(s, l)| format!("{}={:.4}", s, l))
                    .collect();
                text.push_str(&format!("\nCoherence by source: {}", pairs.join(", ")));
            }
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Markdown => {
            let mut text = String::from("## Knowledge Base Status\n\n");
            text.push_str("### Entry Counts\n");
            text.push_str("| Status | Count |\n|--------|-------|\n");
            text.push_str(&format!("| Active | {} |\n", report.total_active));
            text.push_str(&format!("| Deprecated | {} |\n", report.total_deprecated));
            text.push_str(&format!("| Proposed | {} |\n", report.total_proposed));
            text.push_str(&format!("| Quarantined | {} |\n\n", report.total_quarantined));

            text.push_str("### Category Distribution\n");
            text.push_str("| Category | Count |\n|----------|-------|\n");
            for (cat, count) in &report.category_distribution {
                text.push_str(&format!("| {cat} | {count} |\n"));
            }

            text.push_str("\n### Topic Distribution\n");
            text.push_str("| Topic | Count |\n|-------|-------|\n");
            for (topic, count) in &report.topic_distribution {
                text.push_str(&format!("| {topic} | {count} |\n"));
            }

            text.push_str("\n### Correction Chains\n");
            text.push_str(&format!(
                "- Entries with supersedes: {}\n",
                report.entries_with_supersedes
            ));
            text.push_str(&format!(
                "- Entries with superseded_by: {}\n",
                report.entries_with_superseded_by
            ));
            text.push_str(&format!(
                "- Total correction count: {}\n",
                report.total_correction_count
            ));

            text.push_str("\n### Security Metrics\n");
            text.push_str("| Trust Source | Count |\n|-------------|-------|\n");
            for (source, count) in &report.trust_source_distribution {
                text.push_str(&format!("| {source} | {count} |\n"));
            }
            text.push_str(&format!(
                "\n- Entries without attribution: {}\n",
                report.entries_without_attribution
            ));

            if report.contradiction_scan_performed {
                text.push_str("\n### Contradictions\n\n");
                if report.contradictions.is_empty() {
                    text.push_str("No contradictions detected.\n");
                } else {
                    text.push_str(&format!(
                        "{} contradiction(s) found:\n\n",
                        report.contradiction_count
                    ));
                    text.push_str("| Entry A | Entry B | Similarity | Conflict Score | Explanation |\n");
                    text.push_str("|---------|---------|-----------|---------------|-------------|\n");
                    for pair in &report.contradictions {
                        text.push_str(&format!(
                            "| #{} {} | #{} {} | {:.2} | {:.2} | {} |\n",
                            pair.entry_id_a, pair.title_a,
                            pair.entry_id_b, pair.title_b,
                            pair.similarity, pair.conflict_score,
                            pair.explanation,
                        ));
                    }
                }
            }

            if report.embedding_check_performed {
                text.push_str("\n### Embedding Integrity\n\n");
                if report.embedding_inconsistencies.is_empty() {
                    text.push_str("All embeddings consistent.\n");
                } else {
                    text.push_str(&format!(
                        "{} inconsistency(ies) found:\n\n",
                        report.embedding_inconsistencies.len()
                    ));
                    text.push_str("| Entry | Title | Self-Match Similarity |\n");
                    text.push_str("|-------|-------|----------------------|\n");
                    for inc in &report.embedding_inconsistencies {
                        text.push_str(&format!(
                            "| #{} | {} | {:.4} |\n",
                            inc.entry_id, inc.title, inc.expected_similarity,
                        ));
                    }
                }
            }

            text.push_str("\n### Coherence\n\n");
            text.push_str(&format!("- **Lambda**: {:.4}\n", report.coherence));
            text.push_str(&format!(
                "- **Confidence Freshness**: {:.4}\n",
                report.confidence_freshness_score
            ));
            text.push_str(&format!(
                "- **Graph Quality**: {:.4}\n",
                report.graph_quality_score
            ));
            text.push_str(&format!(
                "- **Embedding Consistency**: {:.4}\n",
                report.embedding_consistency_score
            ));
            text.push_str(&format!(
                "- **Contradiction Density**: {:.4}\n\n",
                report.contradiction_density_score
            ));
            text.push_str(&format!(
                "Stale confidence entries: {}\n",
                report.stale_confidence_count
            ));
            text.push_str(&format!(
                "Confidence refreshed: {}\n",
                report.confidence_refreshed_count
            ));
            text.push_str(&format!(
                "Graph stale ratio: {:.2}%\n",
                report.graph_stale_ratio * 100.0
            ));
            text.push_str(&format!(
                "Graph compacted: {}\n",
                if report.graph_compacted { "yes" } else { "no" }
            ));
            if !report.maintenance_recommendations.is_empty() {
                text.push_str("\n#### Maintenance Recommendations\n\n");
                for rec in &report.maintenance_recommendations {
                    text.push_str(&format!("- {rec}\n"));
                }
            }

            text.push_str("\n### Co-Access Patterns\n\n");
            text.push_str(&format!(
                "- Active pairs: {} of {} total\n",
                report.active_co_access_pairs, report.total_co_access_pairs
            ));
            text.push_str(&format!(
                "- Stale pairs cleaned: {}\n",
                report.stale_pairs_cleaned
            ));
            if !report.top_co_access_pairs.is_empty() {
                text.push_str("\n#### Top Co-Access Clusters\n");
                text.push_str("| Entry A | Entry B | Count | Last Updated |\n");
                text.push_str("|---------|---------|-------|-------------|\n");
                for cluster in &report.top_co_access_pairs {
                    text.push_str(&format!(
                        "| {} (#{}) | {} (#{}) | {} | {} |\n",
                        cluster.title_a, cluster.entry_id_a,
                        cluster.title_b, cluster.entry_id_b,
                        cluster.count,
                        format_timestamp(cluster.last_updated),
                    ));
                }
            }

            if report.total_outcomes > 0 || !report.outcomes_by_type.is_empty() {
                text.push_str("\n### Outcome Statistics\n\n");
                text.push_str(&format!("- Total outcomes: {}\n", report.total_outcomes));

                if !report.outcomes_by_type.is_empty() {
                    text.push_str("\n#### By Workflow Type\n");
                    text.push_str("| Type | Count |\n|------|-------|\n");
                    for (type_name, count) in &report.outcomes_by_type {
                        text.push_str(&format!("| {} | {} |\n", type_name, count));
                    }
                }

                if !report.outcomes_by_result.is_empty() {
                    text.push_str("\n#### By Result\n");
                    text.push_str("| Result | Count |\n|--------|-------|\n");
                    for (result_name, count) in &report.outcomes_by_result {
                        text.push_str(&format!("| {} | {} |\n", result_name, count));
                    }
                }

                if !report.outcomes_by_feature_cycle.is_empty() {
                    text.push_str("\n#### Top Feature Cycles\n");
                    text.push_str("| Feature Cycle | Outcomes |\n|--------------|----------|\n");
                    for (fc, count) in &report.outcomes_by_feature_cycle {
                        text.push_str(&format!("| {} | {} |\n", fc, count));
                    }
                }
            }

            text.push_str("\n### Observation Pipeline\n\n");
            text.push_str(&format!("- Records: {}\n", report.observation_file_count));
            text.push_str(&format!(
                "- Sessions: {}\n",
                report.observation_total_size_bytes
            ));
            text.push_str(&format!(
                "- Oldest record: {} days\n",
                report.observation_oldest_file_days
            ));
            text.push_str(&format!(
                "- Retrospected features: {}\n",
                report.retrospected_feature_count
            ));
            if !report.observation_approaching_cleanup.is_empty() {
                text.push_str(&format!(
                    "- **Approaching cleanup**: {}\n",
                    report.observation_approaching_cleanup.join(", ")
                ));
            }

            // Background tick metadata (col-013)
            if report.last_maintenance_run.is_some()
                || report.next_maintenance_scheduled.is_some()
                || report.extraction_stats.is_some()
            {
                text.push_str("\n### Background Tick\n\n");
                if let Some(ts) = report.last_maintenance_run {
                    text.push_str(&format!(
                        "- Last maintenance: {}\n",
                        format_timestamp(ts)
                    ));
                }
                if let Some(ts) = report.next_maintenance_scheduled {
                    text.push_str(&format!(
                        "- Next scheduled: {}\n",
                        format_timestamp(ts)
                    ));
                }
                if let Some(ref stats) = report.extraction_stats {
                    text.push_str(&format!(
                        "- Entries extracted: {}\n",
                        stats.entries_extracted_total
                    ));
                    text.push_str(&format!(
                        "- Entries rejected: {}\n",
                        stats.entries_rejected_total
                    ));
                    if let Some(ts) = stats.last_extraction_run {
                        text.push_str(&format!(
                            "- Last extraction: {}\n",
                            format_timestamp(ts)
                        ));
                    }
                    if !stats.rules_fired.is_empty() {
                        text.push_str("\n#### Rules Fired\n");
                        text.push_str("| Rule | Count |\n|------|-------|\n");
                        for (rule, count) in &stats.rules_fired {
                            text.push_str(&format!("| {} | {} |\n", rule, count));
                        }
                    }
                }
            }

            // Coherence by source (col-013)
            if !report.coherence_by_source.is_empty() {
                text.push_str("\n### Coherence by Source\n\n");
                text.push_str("| Source | Lambda |\n|--------|--------|\n");
                for (source, lambda) in &report.coherence_by_source {
                    text.push_str(&format!("| {} | {:.4} |\n", source, lambda));
                }
            }

            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let json_report = StatusReportJson::from(report);
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&json_report).unwrap_or_default(),
            )])
        }
    }
}

// ---------------------------------------------------------------------------
// Serializable JSON representation (ADR-001 vnc-009)
// ---------------------------------------------------------------------------

/// Intermediate serializable representation of `StatusReport` for JSON output.
///
/// Replaces ~130 lines of manual `serde_json::json!()` construction (ADR-001).
#[derive(Serialize)]
struct StatusReportJson {
    total_active: u64,
    total_deprecated: u64,
    total_proposed: u64,
    total_quarantined: u64,
    category_distribution: HashMap<String, u64>,
    topic_distribution: HashMap<String, u64>,
    correction_chains: CorrectionChainsJson,
    security: SecurityJson,
    coherence: f64,
    confidence_freshness_score: f64,
    graph_quality_score: f64,
    embedding_consistency_score: f64,
    contradiction_density_score: f64,
    stale_confidence_count: u64,
    confidence_refreshed_count: u64,
    graph_stale_ratio: f64,
    graph_compacted: bool,
    maintenance_recommendations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    contradictions: Option<Vec<crate::infra::contradiction::ContradictionPair>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    contradiction_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    embedding_inconsistencies: Option<Vec<crate::infra::contradiction::EmbeddingInconsistency>>,
    co_access: CoAccessJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    outcomes: Option<OutcomesJson>,
    observation: ObservationJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_maintenance_run: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_maintenance_scheduled: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extraction_stats: Option<ExtractionStatsResponse>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    coherence_by_source: Vec<CoherenceBySourceEntry>,
}

#[derive(Serialize)]
struct CoherenceBySourceEntry {
    source: String,
    lambda: f64,
}

#[derive(Serialize)]
struct CorrectionChainsJson {
    entries_with_supersedes: u64,
    entries_with_superseded_by: u64,
    total_correction_count: u64,
}

#[derive(Serialize)]
struct SecurityJson {
    trust_source_distribution: HashMap<String, u64>,
    entries_without_attribution: u64,
}

#[derive(Serialize)]
struct CoAccessClusterJson {
    entry_a: CoAccessEntryRef,
    entry_b: CoAccessEntryRef,
    count: u32,
    last_updated: u64,
}

#[derive(Serialize)]
struct CoAccessEntryRef {
    id: u64,
    title: String,
}

#[derive(Serialize)]
struct CoAccessJson {
    total_pairs: u64,
    active_pairs: u64,
    stale_pairs_cleaned: u64,
    top_clusters: Vec<CoAccessClusterJson>,
}

#[derive(Serialize)]
struct OutcomesJson {
    total: u64,
    by_type: HashMap<String, u64>,
    by_result: HashMap<String, u64>,
    top_feature_cycles: Vec<FeatureCycleCount>,
}

#[derive(Serialize)]
struct FeatureCycleCount {
    feature_cycle: String,
    count: u64,
}

#[derive(Serialize)]
struct ObservationJson {
    record_count: u64,
    session_count: u64,
    oldest_record_days: u64,
    approaching_cleanup: Vec<String>,
    retrospected_feature_count: u64,
}

impl From<&StatusReport> for StatusReportJson {
    fn from(r: &StatusReport) -> Self {
        let contradictions = if r.contradiction_scan_performed {
            Some(r.contradictions.clone())
        } else {
            None
        };
        let contradiction_count = if r.contradiction_scan_performed {
            Some(r.contradiction_count)
        } else {
            None
        };
        let embedding_inconsistencies = if r.embedding_check_performed {
            Some(r.embedding_inconsistencies.clone())
        } else {
            None
        };

        let top_clusters: Vec<CoAccessClusterJson> = r
            .top_co_access_pairs
            .iter()
            .map(|c| CoAccessClusterJson {
                entry_a: CoAccessEntryRef {
                    id: c.entry_id_a,
                    title: c.title_a.clone(),
                },
                entry_b: CoAccessEntryRef {
                    id: c.entry_id_b,
                    title: c.title_b.clone(),
                },
                count: c.count,
                last_updated: c.last_updated,
            })
            .collect();

        let outcomes = if r.total_outcomes > 0 || !r.outcomes_by_type.is_empty() {
            Some(OutcomesJson {
                total: r.total_outcomes,
                by_type: r.outcomes_by_type.iter().cloned().collect(),
                by_result: r.outcomes_by_result.iter().cloned().collect(),
                top_feature_cycles: r
                    .outcomes_by_feature_cycle
                    .iter()
                    .map(|(fc, count)| FeatureCycleCount {
                        feature_cycle: fc.clone(),
                        count: *count,
                    })
                    .collect(),
            })
        } else {
            None
        };

        StatusReportJson {
            total_active: r.total_active,
            total_deprecated: r.total_deprecated,
            total_proposed: r.total_proposed,
            total_quarantined: r.total_quarantined,
            category_distribution: r.category_distribution.iter().cloned().collect(),
            topic_distribution: r.topic_distribution.iter().cloned().collect(),
            correction_chains: CorrectionChainsJson {
                entries_with_supersedes: r.entries_with_supersedes,
                entries_with_superseded_by: r.entries_with_superseded_by,
                total_correction_count: r.total_correction_count,
            },
            security: SecurityJson {
                trust_source_distribution: r.trust_source_distribution.iter().cloned().collect(),
                entries_without_attribution: r.entries_without_attribution,
            },
            coherence: r.coherence,
            confidence_freshness_score: r.confidence_freshness_score,
            graph_quality_score: r.graph_quality_score,
            embedding_consistency_score: r.embedding_consistency_score,
            contradiction_density_score: r.contradiction_density_score,
            stale_confidence_count: r.stale_confidence_count,
            confidence_refreshed_count: r.confidence_refreshed_count,
            graph_stale_ratio: r.graph_stale_ratio,
            graph_compacted: r.graph_compacted,
            maintenance_recommendations: r.maintenance_recommendations.clone(),
            contradictions,
            contradiction_count,
            embedding_inconsistencies,
            co_access: CoAccessJson {
                total_pairs: r.total_co_access_pairs,
                active_pairs: r.active_co_access_pairs,
                stale_pairs_cleaned: r.stale_pairs_cleaned,
                top_clusters,
            },
            outcomes,
            observation: ObservationJson {
                record_count: r.observation_file_count,
                session_count: r.observation_total_size_bytes,
                oldest_record_days: r.observation_oldest_file_days,
                approaching_cleanup: r.observation_approaching_cleanup.clone(),
                retrospected_feature_count: r.retrospected_feature_count,
            },
            last_maintenance_run: r.last_maintenance_run,
            next_maintenance_scheduled: r.next_maintenance_scheduled,
            extraction_stats: r.extraction_stats.clone(),
            coherence_by_source: r.coherence_by_source.iter()
                .map(|(s, l)| CoherenceBySourceEntry {
                    source: s.clone(),
                    lambda: *l,
                })
                .collect(),
        }
    }
}
