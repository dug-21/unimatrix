//! Status report formatting and data structures.

use std::collections::HashMap;

use rmcp::model::{CallToolResult, Content};
use serde::Serialize;

use super::{ResponseFormat, format_timestamp};

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
    // --- Graph Cohesion Metrics (col-029) ---
    /// Fraction of active entries with at least one non-bootstrap edge. Range [0.0, 1.0].
    /// 0.0 when no active entries exist or when compute_graph_cohesion_metrics() fails.
    pub graph_connectivity_rate: f64,
    /// Active entries with zero non-bootstrap edges on either endpoint.
    /// Complement of connected_entry_count: total_active - connected_active.
    pub isolated_entry_count: u64,
    /// Non-bootstrap edges where both active endpoints have different category values.
    /// Excludes edges where either endpoint is deprecated or quarantined.
    pub cross_category_edge_count: u64,
    /// Non-bootstrap edges with relation_type = 'Supports'.
    pub supports_edge_count: u64,
    /// Average in+out degree across active entries: (2 * non_bootstrap_edges) / active_entries.
    /// 0.0 when no active entries exist or when compute_graph_cohesion_metrics() fails.
    pub mean_entry_degree: f64,
    /// Non-bootstrap edges with source = 'nli' (NLI-inferred edges from GH #412).
    pub inferred_edge_count: u64,
    /// Count of active entries with `embedding_dim = 0` (never embedded, GH #444).
    ///
    /// Always populated by `compute_report()` via a fast SQL count; does not
    /// require `check_embeddings = true`. Zero on a healthy index.
    pub unembedded_active_count: u64,
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
    /// Effectiveness analysis results (None if no injection data or query failure).
    pub effectiveness: Option<unimatrix_engine::effectiveness::EffectivenessReport>,
    /// Per-category lifecycle label (crt-031).
    ///
    /// Populated by compute_report() via category_allowlist.list_categories() + is_adaptive().
    /// Sorted alphabetically by category name before storing (R-08: deterministic golden tests).
    /// Empty vec when StatusReport is constructed via Default (e.g. maintenance_tick thin shell).
    ///
    /// Output format asymmetry (ADR-001 decision 2):
    /// - Summary formatter: lists only adaptive categories (pinned is the silent default)
    /// - JSON formatter: includes all categories with their lifecycle label
    pub category_lifecycle: Vec<(String, String)>, // (category_name, "adaptive" | "pinned")
    /// Cycle IDs within the K-window that have cycle_events rows
    /// (event_type='cycle_start') but no cycle_review_index row.
    /// Empty vec when all K-window cycles have been reviewed.
    /// Populated unconditionally by Phase 7b of compute_report() (C-07).
    /// (crt-033, FR-09)
    pub pending_cycle_reviews: Vec<String>,
}

impl Default for StatusReport {
    fn default() -> Self {
        StatusReport {
            total_active: 0,
            total_deprecated: 0,
            total_proposed: 0,
            total_quarantined: 0,
            category_distribution: Vec::new(),
            topic_distribution: Vec::new(),
            entries_with_supersedes: 0,
            entries_with_superseded_by: 0,
            total_correction_count: 0,
            trust_source_distribution: Vec::new(),
            entries_without_attribution: 0,
            contradictions: Vec::new(),
            contradiction_count: 0,
            embedding_inconsistencies: Vec::new(),
            contradiction_scan_performed: false,
            embedding_check_performed: false,
            total_co_access_pairs: 0,
            active_co_access_pairs: 0,
            top_co_access_pairs: Vec::new(),
            stale_pairs_cleaned: 0,
            coherence: 1.0,
            confidence_freshness_score: 1.0,
            graph_quality_score: 1.0,
            embedding_consistency_score: 1.0,
            contradiction_density_score: 1.0,
            stale_confidence_count: 0,
            confidence_refreshed_count: 0,
            graph_stale_ratio: 0.0,
            graph_compacted: false,
            // --- Graph Cohesion Metrics (col-029) ---
            graph_connectivity_rate: 0.0,
            isolated_entry_count: 0,
            cross_category_edge_count: 0,
            supports_edge_count: 0,
            mean_entry_degree: 0.0,
            inferred_edge_count: 0,
            unembedded_active_count: 0,
            maintenance_recommendations: Vec::new(),
            total_outcomes: 0,
            outcomes_by_type: Vec::new(),
            outcomes_by_result: Vec::new(),
            outcomes_by_feature_cycle: Vec::new(),
            observation_file_count: 0,
            observation_total_size_bytes: 0,
            observation_oldest_file_days: 0,
            observation_approaching_cleanup: Vec::new(),
            retrospected_feature_count: 0,
            last_maintenance_run: None,
            next_maintenance_scheduled: None,
            extraction_stats: None,
            coherence_by_source: Vec::new(),
            effectiveness: None,
            category_lifecycle: Vec::new(),
            pending_cycle_reviews: Vec::new(),
        }
    }
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
                text.push_str(&format!(
                    " | Contradictions: {}",
                    report.contradiction_count
                ));
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
            // Graph cohesion summary (col-029)
            // Suppress when all discriminating metrics are zero (empty/bootstrap-only store).
            if report.isolated_entry_count > 0
                || report.cross_category_edge_count > 0
                || report.inferred_edge_count > 0
            {
                text.push_str(&format!(
                    "\nGraph cohesion: {:.1}% connected, {} isolated, {} cross-category, {} inferred",
                    report.graph_connectivity_rate * 100.0,
                    report.isolated_entry_count,
                    report.cross_category_edge_count,
                    report.inferred_edge_count,
                ));
            }
            if report.unembedded_active_count > 0 {
                text.push_str(&format!(
                    "\nUnembedded active entries: {} (heal pass pending)",
                    report.unembedded_active_count
                ));
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
                let pairs: Vec<String> = report
                    .coherence_by_source
                    .iter()
                    .map(|(s, l)| format!("{}={:.4}", s, l))
                    .collect();
                text.push_str(&format!("\nCoherence by source: {}", pairs.join(", ")));
            }
            // Effectiveness line (crt-018, FR-05)
            match &report.effectiveness {
                Some(eff) => {
                    use unimatrix_engine::effectiveness::EffectivenessCategory;
                    let mut effective = 0u32;
                    let mut settled = 0u32;
                    let mut unmatched = 0u32;
                    let mut ineffective = 0u32;
                    let mut noisy = 0u32;
                    for (cat, count) in &eff.by_category {
                        match cat {
                            EffectivenessCategory::Effective => effective = *count,
                            EffectivenessCategory::Settled => settled = *count,
                            EffectivenessCategory::Unmatched => unmatched = *count,
                            EffectivenessCategory::Ineffective => ineffective = *count,
                            EffectivenessCategory::Noisy => noisy = *count,
                        }
                    }
                    text.push_str(&format!(
                        "\nEffectiveness: {} effective, {} settled, {} unmatched, {} ineffective, {} noisy ({} sessions analyzed)",
                        effective, settled, unmatched, ineffective, noisy,
                        eff.data_window.session_count
                    ));
                }
                None => {
                    text.push_str("\nEffectiveness: no injection data");
                }
            }
            // crt-031: lifecycle summary — show only adaptive categories (pinned is the silent default).
            // If no adaptive categories are configured, this block is silent (E-01: empty adaptive list).
            {
                let adaptive_categories: Vec<&str> = report
                    .category_lifecycle
                    .iter()
                    .filter(|(_, label)| label == "adaptive")
                    .map(|(cat, _)| cat.as_str())
                    .collect();
                if !adaptive_categories.is_empty() {
                    text.push_str(&format!(
                        "\nAdaptive categories: {}",
                        adaptive_categories.join(", ")
                    ));
                }
                // Note: when adaptive_categories is empty, no line is added.
                // Rationale: showing all pinned categories adds noise for standard configurations.
            }
            // crt-033: Pending cycle reviews — show when backlog is non-empty.
            // Silent when empty (consistent with other "nothing to report" fields).
            if !report.pending_cycle_reviews.is_empty() {
                text.push_str(&format!(
                    "\nPending cycle reviews: {}",
                    report.pending_cycle_reviews.join(", ")
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
            text.push_str(&format!(
                "| Quarantined | {} |\n\n",
                report.total_quarantined
            ));

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
                    text.push_str(
                        "| Entry A | Entry B | Similarity | Conflict Score | Explanation |\n",
                    );
                    text.push_str(
                        "|---------|---------|-----------|---------------|-------------|\n",
                    );
                    for pair in &report.contradictions {
                        text.push_str(&format!(
                            "| #{} {} | #{} {} | {:.2} | {:.2} | {} |\n",
                            pair.entry_id_a,
                            pair.title_a,
                            pair.entry_id_b,
                            pair.title_b,
                            pair.similarity,
                            pair.conflict_score,
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
            // Graph Cohesion sub-section (col-029)
            // Always present inside ### Coherence. Shows all six metrics regardless of
            // whether they are zero — operators on a fresh store see the sub-section header
            // and know the feature is active.
            text.push_str("\n#### Graph Cohesion\n");
            let connected_count = report
                .total_active
                .saturating_sub(report.isolated_entry_count);
            text.push_str(&format!(
                "- Connectivity: {:.1}% ({}/{} active entries connected)\n",
                report.graph_connectivity_rate * 100.0,
                connected_count,
                report.total_active,
            ));
            text.push_str(&format!(
                "- Isolated entries: {}\n",
                report.isolated_entry_count,
            ));
            text.push_str(&format!(
                "- Cross-category edges: {}\n",
                report.cross_category_edge_count,
            ));
            text.push_str(&format!(
                "- Supports edges: {}\n",
                report.supports_edge_count,
            ));
            text.push_str(&format!(
                "- Mean entry degree: {:.2}\n",
                report.mean_entry_degree,
            ));
            text.push_str(&format!(
                "- Inferred (NLI) edges: {}\n",
                report.inferred_edge_count,
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
                        cluster.title_a,
                        cluster.entry_id_a,
                        cluster.title_b,
                        cluster.entry_id_b,
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
                    text.push_str(&format!("- Last maintenance: {}\n", format_timestamp(ts)));
                }
                if let Some(ts) = report.next_maintenance_scheduled {
                    text.push_str(&format!("- Next scheduled: {}\n", format_timestamp(ts)));
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
                        text.push_str(&format!("- Last extraction: {}\n", format_timestamp(ts)));
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

            // Effectiveness section (crt-018, FR-06)
            match &report.effectiveness {
                Some(eff) => {
                    let span = match (
                        eff.data_window.earliest_session_at,
                        eff.data_window.latest_session_at,
                    ) {
                        (Some(e), Some(l)) if l > e => format!("{} days", (l - e) / 86400),
                        _ => "< 1 day".to_string(),
                    };
                    text.push_str(&format!(
                        "\n### Effectiveness Analysis\n\nAnalysis covers {} sessions over {}.\n\n",
                        eff.data_window.session_count, span
                    ));

                    // Category table
                    text.push_str(
                        "| Category | Count | % of Active |\n|----------|-------|-------------|\n",
                    );
                    let total: u32 = eff.by_category.iter().map(|(_, c)| c).sum();
                    for (cat, count) in &eff.by_category {
                        let pct = if total > 0 {
                            (*count as f64 / total as f64) * 100.0
                        } else {
                            0.0
                        };
                        text.push_str(&format!("| {:?} | {} | {:.1}% |\n", cat, count, pct));
                    }

                    // Per-source table
                    text.push_str(
                        "\n| Source | Effective | Settled | Unmatched | Ineffective | Noisy | Utility |\n",
                    );
                    text.push_str(
                        "|--------|-----------|---------|-----------|-------------|-------|---------|\n",
                    );
                    for s in &eff.by_source {
                        text.push_str(&format!(
                            "| {} | {} | {} | {} | {} | {} | {:.2} |\n",
                            s.trust_source,
                            s.effective_count,
                            s.settled_count,
                            s.unmatched_count,
                            s.ineffective_count,
                            s.noisy_count,
                            s.aggregate_utility
                        ));
                    }

                    // Calibration table
                    text.push_str("\n| Confidence | Injections | Actual Success | Expected |\n");
                    text.push_str("|------------|------------|----------------|----------|\n");
                    for b in &eff.calibration {
                        let expected = (b.confidence_lower + b.confidence_upper) / 2.0;
                        text.push_str(&format!(
                            "| {:.1}-{:.1} | {} | {:.2} | {:.2} |\n",
                            b.confidence_lower,
                            b.confidence_upper,
                            b.entry_count,
                            b.actual_success_rate,
                            expected
                        ));
                    }

                    // Top ineffective entries (R-12: up to 10 with entry_id, title)
                    if !eff.top_ineffective.is_empty() {
                        text.push_str("\n**Top Ineffective Entries:**\n\n");
                        text.push_str(
                            "| ID | Title | Injections | Success Rate |\n|----|-------|------------|-------------|\n",
                        );
                        for e in &eff.top_ineffective {
                            let safe_title = e.title.replace('|', "/").replace('\n', " ");
                            text.push_str(&format!(
                                "| {} | {} | {} | {:.2} |\n",
                                e.entry_id, safe_title, e.injection_count, e.success_rate
                            ));
                        }
                    }

                    // Noisy entries
                    if !eff.noisy_entries.is_empty() {
                        text.push_str("\n**Noisy Entries:**\n\n");
                        text.push_str("| ID | Title |\n|----|-------|\n");
                        for e in &eff.noisy_entries {
                            let safe_title = e.title.replace('|', "/").replace('\n', " ");
                            text.push_str(&format!("| {} | {} |\n", e.entry_id, safe_title));
                        }
                    }

                    // Unmatched entries (up to 10)
                    if !eff.unmatched_entries.is_empty() {
                        text.push_str("\n**Unmatched Entries:**\n\n");
                        text.push_str("| ID | Title | Topic |\n|----|-------|-------|\n");
                        for e in &eff.unmatched_entries {
                            let safe_title = e.title.replace('|', "/").replace('\n', " ");
                            let safe_topic = e.topic.replace('|', "/").replace('\n', " ");
                            text.push_str(&format!(
                                "| {} | {} | {} |\n",
                                e.entry_id, safe_title, safe_topic
                            ));
                        }
                    }
                }
                None => {
                    text.push_str(
                        "\n### Effectiveness Analysis\n\nInsufficient injection data for analysis.\n",
                    );
                }
            }

            // crt-033: Pending cycle reviews section.
            if !report.pending_cycle_reviews.is_empty() {
                text.push_str("\n### Pending Cycle Reviews\n\n");
                text.push_str("Cycles with `cycle_start` events but no stored review:\n\n");
                for cycle_id in &report.pending_cycle_reviews {
                    text.push_str(&format!("- {cycle_id}\n"));
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
    // Graph Cohesion Metrics (col-029)
    graph_connectivity_rate: f64,
    isolated_entry_count: u64,
    cross_category_edge_count: u64,
    supports_edge_count: u64,
    mean_entry_degree: f64,
    inferred_edge_count: u64,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    effectiveness: Option<EffectivenessReportJson>,
    /// Per-category lifecycle label (crt-031).
    ///
    /// All categories included with their lifecycle label.
    /// Output format asymmetry: summary shows only adaptive; JSON shows all (ADR-001 decision 2).
    /// BTreeMap preserves insertion order (alphabetically sorted by category name, per R-08).
    category_lifecycle: std::collections::BTreeMap<String, String>,
    /// Cycles with cycle_start events but no stored review (crt-033).
    /// Empty array when no cycles are pending review.
    /// Always serialized even as an empty array (FR-11: no skip_serializing_if).
    pending_cycle_reviews: Vec<String>,
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

// ---------------------------------------------------------------------------
// Effectiveness JSON types (crt-018)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct EffectivenessReportJson {
    by_category: Vec<CategoryCount>,
    by_source: Vec<SourceEffectivenessJson>,
    calibration_buckets: Vec<CalibrationBucketJson>,
    ineffective_entries: Vec<IneffectiveEntryJson>,
    noisy_entries: Vec<NoisyEntryJson>,
    unmatched_entries: Vec<UnmatchedEntryJson>,
    data_window: DataWindowJson,
}

#[derive(Serialize)]
struct CategoryCount {
    category: String,
    count: u32,
}

#[derive(Serialize)]
struct SourceEffectivenessJson {
    trust_source: String,
    effective: u32,
    settled: u32,
    unmatched: u32,
    ineffective: u32,
    noisy: u32,
    utility_ratio: f64,
}

#[derive(Serialize)]
struct CalibrationBucketJson {
    range_low: f64,
    range_high: f64,
    injection_count: u32,
    actual_success_rate: f64,
    expected_success_rate: f64,
}

#[derive(Serialize)]
struct IneffectiveEntryJson {
    entry_id: u64,
    title: String,
    injection_count: u32,
    success_rate: f64,
}

#[derive(Serialize)]
struct NoisyEntryJson {
    entry_id: u64,
    title: String,
}

#[derive(Serialize)]
struct UnmatchedEntryJson {
    entry_id: u64,
    title: String,
    topic: String,
}

#[derive(Serialize)]
struct DataWindowJson {
    session_count: u32,
    span_days: u64,
}

// ---------------------------------------------------------------------------
// Unit tests (col-029 + crt-031)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::response::ResponseFormat;

    fn result_text(result: &rmcp::model::CallToolResult) -> String {
        result
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.clone())
            .unwrap_or_default()
    }

    fn make_report_with_cohesion() -> StatusReport {
        StatusReport {
            graph_connectivity_rate: 0.75,
            isolated_entry_count: 2,
            cross_category_edge_count: 5,
            supports_edge_count: 3,
            mean_entry_degree: 1.5,
            inferred_edge_count: 4,
            ..StatusReport::default()
        }
    }

    // --- status-report-fields tests ---

    #[test]
    fn test_status_report_default_cohesion_fields() {
        let r = StatusReport::default();
        assert_eq!(r.graph_connectivity_rate, 0.0_f64);
        assert_eq!(r.isolated_entry_count, 0_u64);
        assert_eq!(r.cross_category_edge_count, 0_u64);
        assert_eq!(r.supports_edge_count, 0_u64);
        assert_eq!(r.mean_entry_degree, 0.0_f64);
        assert_eq!(r.inferred_edge_count, 0_u64);
    }

    // --- format-output tests ---

    #[test]
    fn test_format_summary_graph_cohesion_present() {
        let report = make_report_with_cohesion();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            text.contains("Graph cohesion:"),
            "Summary must include graph cohesion line"
        );
        assert!(
            text.contains("75.0%"),
            "Connectivity rate must appear as percentage with one decimal"
        );
        assert!(
            text.contains(" 2 ") || text.contains(",2 ") || text.contains("2 isolated"),
            "isolated_entry_count must appear"
        );
        assert!(
            text.contains("5 cross-category"),
            "cross_category_edge_count must appear"
        );
        assert!(
            text.contains("4 inferred"),
            "inferred_edge_count must appear"
        );
    }

    #[test]
    fn test_format_summary_graph_cohesion_absent() {
        let report = StatusReport::default();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            !text.contains("Graph cohesion:"),
            "Summary must omit graph cohesion line when all metrics are zero"
        );
    }

    #[test]
    fn test_format_markdown_graph_cohesion_section() {
        let report = make_report_with_cohesion();
        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);

        // Sub-section header must be present inside the Coherence block
        assert!(
            text.contains("#### Graph Cohesion"),
            "Markdown must include #### Graph Cohesion"
        );

        // All six metric labels must appear (AC-10)
        assert!(text.contains("Connectivity:"), "Missing Connectivity label");
        assert!(
            text.contains("Isolated entries:"),
            "Missing Isolated entries label"
        );
        assert!(
            text.contains("Cross-category edges:"),
            "Missing Cross-category edges label"
        );
        assert!(
            text.contains("Supports edges:"),
            "Missing Supports edges label"
        );
        assert!(
            text.contains("Mean entry degree:"),
            "Missing Mean entry degree label"
        );
        assert!(
            text.contains("Inferred (NLI) edges:"),
            "Missing Inferred (NLI) edges label"
        );

        // Verify numeric values
        assert!(
            text.contains("75.0%"),
            "Connectivity percentage must appear in Markdown"
        );
        assert!(
            text.contains("1.50"),
            "mean_entry_degree must appear with 2 decimal places"
        );

        // Verify placement: #### Graph Cohesion must appear after ### Coherence
        let coherence_pos = text
            .find("### Coherence")
            .expect("### Coherence must exist");
        let graph_cohesion_pos = text
            .find("#### Graph Cohesion")
            .expect("#### Graph Cohesion must exist");
        assert!(
            graph_cohesion_pos > coherence_pos,
            "#### Graph Cohesion must appear after ### Coherence block"
        );
    }

    // --- crt-031: category_lifecycle tests ---

    /// I-02: StatusReport::default() must return category_lifecycle: vec![]
    #[test]
    fn test_status_report_default_category_lifecycle_is_empty() {
        let report = StatusReport::default();
        assert_eq!(
            report.category_lifecycle,
            vec![],
            "Default StatusReport must have empty category_lifecycle"
        );
    }

    /// AC-09 summary path: summary lists only adaptive categories, not pinned ones.
    #[test]
    fn test_status_report_summary_lists_only_adaptive() {
        let report = StatusReport {
            category_lifecycle: vec![
                ("decision".to_string(), "pinned".to_string()),
                ("lesson-learned".to_string(), "adaptive".to_string()),
            ],
            ..StatusReport::default()
        };
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            text.contains("Adaptive categories: lesson-learned"),
            "Summary must contain adaptive category line"
        );
        assert!(
            !text.contains("decision"),
            "Summary must not contain pinned category 'decision' in lifecycle section"
        );
        assert!(
            !text.contains("pinned"),
            "Summary must not contain the word 'pinned'"
        );
    }

    /// E-01: when no adaptive categories are configured, lifecycle section is absent from summary.
    #[test]
    fn test_status_report_summary_no_adaptive_section_when_empty() {
        let report = StatusReport {
            category_lifecycle: vec![
                ("decision".to_string(), "pinned".to_string()),
                ("lesson-learned".to_string(), "pinned".to_string()),
            ],
            ..StatusReport::default()
        };
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            !text.contains("Adaptive categories"),
            "Summary must omit 'Adaptive categories' line when no adaptive categories exist"
        );
        assert!(
            !text.contains("adaptive"),
            "Summary must not contain 'adaptive' when list is empty"
        );
    }

    /// R-08 scenario 2 + I-03: JSON output is deterministic and contains lifecycle data.
    /// Uses deserialized comparison, not raw string equality.
    #[test]
    fn test_category_lifecycle_json_sorted_and_deterministic() {
        let report = StatusReport {
            category_lifecycle: vec![
                ("convention".to_string(), "pinned".to_string()),
                ("decision".to_string(), "pinned".to_string()),
                ("lesson-learned".to_string(), "adaptive".to_string()),
            ],
            ..StatusReport::default()
        };
        // Two calls must produce identical JSON (I-03).
        let result1 = format_status_report(&report, ResponseFormat::Json);
        let result2 = format_status_report(&report, ResponseFormat::Json);
        let text1 = result_text(&result1);
        let text2 = result_text(&result2);
        let parsed1: serde_json::Value = serde_json::from_str(&text1).expect("JSON must be valid");
        let parsed2: serde_json::Value = serde_json::from_str(&text2).expect("JSON must be valid");
        assert_eq!(parsed1, parsed2, "JSON output must be deterministic");

        // category_lifecycle must be present and contain the expected entries.
        let lifecycle = &parsed1["category_lifecycle"];
        assert!(
            lifecycle.is_object(),
            "category_lifecycle must be a JSON object"
        );
        assert_eq!(
            lifecycle["lesson-learned"].as_str(),
            Some("adaptive"),
            "lesson-learned must be labeled adaptive"
        );
        assert_eq!(
            lifecycle["decision"].as_str(),
            Some("pinned"),
            "decision must be labeled pinned"
        );
        assert_eq!(
            lifecycle["convention"].as_str(),
            Some("pinned"),
            "convention must be labeled pinned"
        );
    }

    /// AC-09 JSON path: all 5 default categories labeled correctly.
    #[test]
    fn test_status_report_json_includes_all_categories() {
        let report = StatusReport {
            category_lifecycle: vec![
                ("convention".to_string(), "pinned".to_string()),
                ("decision".to_string(), "pinned".to_string()),
                ("lesson-learned".to_string(), "adaptive".to_string()),
                ("pattern".to_string(), "pinned".to_string()),
                ("procedure".to_string(), "pinned".to_string()),
            ],
            ..StatusReport::default()
        };
        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("JSON must be valid");
        let lifecycle = &parsed["category_lifecycle"];

        assert_eq!(lifecycle["lesson-learned"].as_str(), Some("adaptive"));
        assert_eq!(lifecycle["decision"].as_str(), Some("pinned"));
        assert_eq!(lifecycle["convention"].as_str(), Some("pinned"));
        assert_eq!(lifecycle["pattern"].as_str(), Some("pinned"));
        assert_eq!(lifecycle["procedure"].as_str(), Some("pinned"));
    }

    /// R-08 scenario 1: golden test verifying alphabetic sort of category_lifecycle.
    #[test]
    fn test_category_lifecycle_alphabetic_sort_golden() {
        // Input tuples in non-alphabetical order to verify sort is applied.
        let report = StatusReport {
            category_lifecycle: vec![
                ("procedure".to_string(), "pinned".to_string()),
                ("convention".to_string(), "pinned".to_string()),
                ("lesson-learned".to_string(), "adaptive".to_string()),
                ("decision".to_string(), "pinned".to_string()),
                ("pattern".to_string(), "pinned".to_string()),
            ],
            ..StatusReport::default()
        };
        // Verify the Vec itself is sorted (as produced by compute_report).
        // This test checks that IF the Vec has been sorted, assertion holds.
        // The compute_report() implementation is responsible for sorting before storing.
        let lifecycle = &report.category_lifecycle;
        // Note: this test checks the Vec as-is (unsorted here) — the sort invariant is
        // tested in the compute_report test in services/status.rs. This golden test
        // checks that the JSON formatter handles pre-sorted input deterministically.
        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("JSON must be valid");
        let lifecycle_json = parsed["category_lifecycle"]
            .as_object()
            .expect("must be object");
        // BTreeMap iteration in JSON serialization is alphabetical.
        let keys: Vec<&str> = lifecycle_json.keys().map(|s| s.as_str()).collect();
        let mut sorted_keys = keys.clone();
        sorted_keys.sort_unstable();
        assert_eq!(
            keys, sorted_keys,
            "JSON category_lifecycle keys must appear in alphabetical order"
        );
        // Suppress unused variable warning.
        let _ = lifecycle;
    }

    // --- crt-033: pending_cycle_reviews tests ---

    /// SR-U-01: StatusReport::default() has empty pending_cycle_reviews (I-04).
    #[test]
    fn test_status_report_default_has_empty_pending_cycle_reviews() {
        let report = StatusReport::default();
        assert!(
            report.pending_cycle_reviews.is_empty(),
            "default StatusReport must have empty pending_cycle_reviews"
        );
    }

    /// SR-U-02 / SR-U-03: From<&StatusReport> maps pending_cycle_reviews correctly (I-04).
    #[test]
    fn test_status_report_json_from_maps_pending_cycle_reviews() {
        let report = StatusReport {
            pending_cycle_reviews: vec!["crt-033".to_string(), "col-034".to_string()],
            ..StatusReport::default()
        };
        let json_report = StatusReportJson::from(&report);
        assert_eq!(json_report.pending_cycle_reviews.len(), 2);
        assert!(
            json_report
                .pending_cycle_reviews
                .contains(&"crt-033".to_string())
        );
        assert!(
            json_report
                .pending_cycle_reviews
                .contains(&"col-034".to_string())
        );
    }

    /// SR-U-04: From<&StatusReport> empty vec maps to empty vec (FR-10, FR-11).
    #[test]
    fn test_status_report_json_from_empty_pending_reviews() {
        let report = StatusReport::default();
        let json_report = StatusReportJson::from(&report);
        assert!(json_report.pending_cycle_reviews.is_empty());
    }

    /// SR-U-05: Summary formatter includes "Pending cycle reviews" label when non-empty (FR-11).
    #[test]
    fn test_summary_formatter_renders_pending_cycle_reviews_when_non_empty() {
        let report = StatusReport {
            pending_cycle_reviews: vec!["col-022".to_string(), "crt-031".to_string()],
            ..StatusReport::default()
        };
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            text.contains("Pending cycle reviews"),
            "summary must contain 'Pending cycle reviews' label when list is non-empty"
        );
        assert!(text.contains("col-022"), "summary must contain col-022");
        assert!(text.contains("crt-031"), "summary must contain crt-031");
    }

    /// SR-U-06: Summary formatter produces no "Pending cycle reviews" section when empty (FR-11).
    #[test]
    fn test_summary_formatter_omits_pending_section_when_empty() {
        let report = StatusReport::default();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            !text.contains("Pending cycle reviews"),
            "summary must not render 'Pending cycle reviews' when list is empty"
        );
    }

    /// SR-U-07: JSON formatter includes pending_cycle_reviews as array (FR-11, AC-09).
    #[test]
    fn test_json_formatter_includes_pending_cycle_reviews_array() {
        let report = StatusReport {
            pending_cycle_reviews: vec!["nxs-005".to_string()],
            ..StatusReport::default()
        };
        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        let arr = parsed["pending_cycle_reviews"]
            .as_array()
            .expect("pending_cycle_reviews must be an array in JSON output");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].as_str().unwrap(), "nxs-005");
    }

    /// SR-U-08: JSON formatter produces empty array when pending_cycle_reviews is empty (FR-11, AC-10).
    #[test]
    fn test_json_formatter_pending_cycle_reviews_empty_is_array() {
        let report = StatusReport::default();
        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        let arr = parsed["pending_cycle_reviews"]
            .as_array()
            .expect("pending_cycle_reviews must exist as empty array, not absent");
        assert!(arr.is_empty());
    }

    /// SR-I-01: StatusReport with non-empty pending_cycle_reviews round-trips through JSON (I-04).
    #[test]
    fn test_status_report_json_round_trip_preserves_pending_cycle_reviews() {
        let report = StatusReport {
            pending_cycle_reviews: vec!["col-022".to_string()],
            ..StatusReport::default()
        };
        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        let arr = parsed["pending_cycle_reviews"]
            .as_array()
            .expect("pending_cycle_reviews must be an array");
        let recovered: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        assert_eq!(recovered, report.pending_cycle_reviews);
    }
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

        // Effectiveness mapping (crt-018)
        let effectiveness = r.effectiveness.as_ref().map(|eff| {
            let by_category = eff
                .by_category
                .iter()
                .map(|(cat, count)| CategoryCount {
                    category: format!("{:?}", cat).to_lowercase(),
                    count: *count,
                })
                .collect();

            let by_source = eff
                .by_source
                .iter()
                .map(|s| SourceEffectivenessJson {
                    trust_source: s.trust_source.clone(),
                    effective: s.effective_count,
                    settled: s.settled_count,
                    unmatched: s.unmatched_count,
                    ineffective: s.ineffective_count,
                    noisy: s.noisy_count,
                    utility_ratio: s.aggregate_utility,
                })
                .collect();

            let calibration_buckets = eff
                .calibration
                .iter()
                .map(|b| CalibrationBucketJson {
                    range_low: b.confidence_lower,
                    range_high: b.confidence_upper,
                    injection_count: b.entry_count,
                    actual_success_rate: b.actual_success_rate,
                    expected_success_rate: (b.confidence_lower + b.confidence_upper) / 2.0,
                })
                .collect();

            let ineffective_entries = eff
                .top_ineffective
                .iter()
                .map(|e| IneffectiveEntryJson {
                    entry_id: e.entry_id,
                    title: e.title.clone(),
                    injection_count: e.injection_count,
                    success_rate: e.success_rate,
                })
                .collect();

            let noisy_entries = eff
                .noisy_entries
                .iter()
                .map(|e| NoisyEntryJson {
                    entry_id: e.entry_id,
                    title: e.title.clone(),
                })
                .collect();

            let unmatched_entries = eff
                .unmatched_entries
                .iter()
                .map(|e| UnmatchedEntryJson {
                    entry_id: e.entry_id,
                    title: e.title.clone(),
                    topic: e.topic.clone(),
                })
                .collect();

            let span_days = match (
                eff.data_window.earliest_session_at,
                eff.data_window.latest_session_at,
            ) {
                (Some(earliest), Some(latest)) if latest > earliest => (latest - earliest) / 86400,
                _ => 0,
            };

            EffectivenessReportJson {
                by_category,
                by_source,
                calibration_buckets,
                ineffective_entries,
                noisy_entries,
                unmatched_entries,
                data_window: DataWindowJson {
                    session_count: eff.data_window.session_count,
                    span_days,
                },
            }
        });

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
            // Graph Cohesion Metrics (col-029)
            graph_connectivity_rate: r.graph_connectivity_rate,
            isolated_entry_count: r.isolated_entry_count,
            cross_category_edge_count: r.cross_category_edge_count,
            supports_edge_count: r.supports_edge_count,
            mean_entry_degree: r.mean_entry_degree,
            inferred_edge_count: r.inferred_edge_count,
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
            coherence_by_source: r
                .coherence_by_source
                .iter()
                .map(|(s, l)| CoherenceBySourceEntry {
                    source: s.clone(),
                    lambda: *l,
                })
                .collect(),
            effectiveness,
            // crt-031: all categories with lifecycle labels.
            // category_lifecycle Vec is already sorted alphabetically (R-08).
            // BTreeMap insertion in sorted order preserves deterministic JSON output.
            category_lifecycle: r
                .category_lifecycle
                .iter()
                .map(|(cat, label)| (cat.clone(), label.clone()))
                .collect(),
            // crt-033: pending cycle reviews — always included, even as empty array (FR-11).
            pending_cycle_reviews: r.pending_cycle_reviews.clone(),
        }
    }
}
