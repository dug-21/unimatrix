//! Status report formatting and data structures.

use rmcp::model::{CallToolResult, Content};

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
}

/// A co-access cluster entry for status reporting.
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
                "\nObservation: {} files ({} bytes), oldest {} days, {} retrospected",
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
            text.push_str(&format!("- Files: {}\n", report.observation_file_count));
            text.push_str(&format!(
                "- Total size: {} bytes\n",
                report.observation_total_size_bytes
            ));
            text.push_str(&format!(
                "- Oldest file: {} days\n",
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

            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let cat_dist: serde_json::Value = report
                .category_distribution
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                .collect::<serde_json::Map<String, serde_json::Value>>()
                .into();
            let topic_dist: serde_json::Value = report
                .topic_distribution
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                .collect::<serde_json::Map<String, serde_json::Value>>()
                .into();
            let trust_dist: serde_json::Value = report
                .trust_source_distribution
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                .collect::<serde_json::Map<String, serde_json::Value>>()
                .into();

            let mut obj = serde_json::json!({
                "total_active": report.total_active,
                "total_deprecated": report.total_deprecated,
                "total_proposed": report.total_proposed,
                "total_quarantined": report.total_quarantined,
                "category_distribution": cat_dist,
                "topic_distribution": topic_dist,
                "correction_chains": {
                    "entries_with_supersedes": report.entries_with_supersedes,
                    "entries_with_superseded_by": report.entries_with_superseded_by,
                    "total_correction_count": report.total_correction_count,
                },
                "security": {
                    "trust_source_distribution": trust_dist,
                    "entries_without_attribution": report.entries_without_attribution,
                },
            });

            obj["coherence"] = serde_json::json!(report.coherence);
            obj["confidence_freshness_score"] = serde_json::json!(report.confidence_freshness_score);
            obj["graph_quality_score"] = serde_json::json!(report.graph_quality_score);
            obj["embedding_consistency_score"] = serde_json::json!(report.embedding_consistency_score);
            obj["contradiction_density_score"] = serde_json::json!(report.contradiction_density_score);
            obj["stale_confidence_count"] = serde_json::json!(report.stale_confidence_count);
            obj["confidence_refreshed_count"] = serde_json::json!(report.confidence_refreshed_count);
            obj["graph_stale_ratio"] = serde_json::json!(report.graph_stale_ratio);
            obj["graph_compacted"] = serde_json::json!(report.graph_compacted);
            obj["maintenance_recommendations"] = serde_json::json!(report.maintenance_recommendations);

            if report.contradiction_scan_performed {
                let contradictions: Vec<serde_json::Value> = report.contradictions.iter().map(|p| {
                    serde_json::json!({
                        "entry_id_a": p.entry_id_a,
                        "entry_id_b": p.entry_id_b,
                        "title_a": p.title_a,
                        "title_b": p.title_b,
                        "similarity": p.similarity,
                        "conflict_score": p.conflict_score,
                        "explanation": p.explanation,
                    })
                }).collect();
                obj["contradictions"] = serde_json::json!(contradictions);
                obj["contradiction_count"] = serde_json::json!(report.contradiction_count);
            }

            if report.embedding_check_performed {
                let inconsistencies: Vec<serde_json::Value> = report.embedding_inconsistencies.iter().map(|i| {
                    serde_json::json!({
                        "entry_id": i.entry_id,
                        "title": i.title,
                        "self_match_similarity": i.expected_similarity,
                    })
                }).collect();
                obj["embedding_inconsistencies"] = serde_json::json!(inconsistencies);
            }

            let top_clusters: Vec<serde_json::Value> = report.top_co_access_pairs.iter().map(|c| {
                serde_json::json!({
                    "entry_a": { "id": c.entry_id_a, "title": c.title_a },
                    "entry_b": { "id": c.entry_id_b, "title": c.title_b },
                    "count": c.count,
                    "last_updated": c.last_updated,
                })
            }).collect();
            obj["co_access"] = serde_json::json!({
                "total_pairs": report.total_co_access_pairs,
                "active_pairs": report.active_co_access_pairs,
                "stale_pairs_cleaned": report.stale_pairs_cleaned,
                "top_clusters": top_clusters,
            });

            if report.total_outcomes > 0 || !report.outcomes_by_type.is_empty() {
                let type_dist: serde_json::Value = report
                    .outcomes_by_type
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                    .collect::<serde_json::Map<String, serde_json::Value>>()
                    .into();
                let result_dist: serde_json::Value = report
                    .outcomes_by_result
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                    .collect::<serde_json::Map<String, serde_json::Value>>()
                    .into();
                let fc_list: Vec<serde_json::Value> = report
                    .outcomes_by_feature_cycle
                    .iter()
                    .map(|(fc, count)| {
                        serde_json::json!({"feature_cycle": fc, "count": count})
                    })
                    .collect();

                obj["outcomes"] = serde_json::json!({
                    "total": report.total_outcomes,
                    "by_type": type_dist,
                    "by_result": result_dist,
                    "top_feature_cycles": fc_list,
                });
            }

            obj["observation"] = serde_json::json!({
                "file_count": report.observation_file_count,
                "total_size_bytes": report.observation_total_size_bytes,
                "oldest_file_days": report.observation_oldest_file_days,
                "approaching_cleanup": report.observation_approaching_cleanup,
                "retrospected_feature_count": report.retrospected_feature_count,
            });

            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&obj).unwrap_or_default(),
            )])
        }
    }
}
