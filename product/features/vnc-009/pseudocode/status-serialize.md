# Pseudocode: status-serialize

## File: `crates/unimatrix-server/src/mcp/response/status.rs` (modifications)

### Add derive(Serialize) to StatusReport

```
use serde::Serialize;

#[derive(Serialize)]     // NEW
pub struct StatusReport {
    // ... all existing fields unchanged ...
}

#[derive(Serialize)]     // NEW
pub struct CoAccessClusterEntry {
    // ... all existing fields unchanged ...
}
```

### StatusReportJson intermediate struct (ADR-001)

```
/// Intermediate serialization struct that maps StatusReport flat fields
/// into the nested JSON structure used by the existing output.
/// Preserves backward compatibility without coupling domain struct to formatting.
#[derive(Serialize)]
struct StatusReportJson {
    total_active: u64,
    total_deprecated: u64,
    total_proposed: u64,
    total_quarantined: u64,
    category_distribution: serde_json::Map<String, serde_json::Value>,
    topic_distribution: serde_json::Map<String, serde_json::Value>,
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
    contradictions: Option<Vec<ContradictionJson>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    contradiction_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    embedding_inconsistencies: Option<Vec<EmbeddingInconsistencyJson>>,
    co_access: CoAccessJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    outcomes: Option<OutcomesJson>,
    observation: ObservationJson,
}

#[derive(Serialize)]
struct CorrectionChainsJson {
    entries_with_supersedes: u64,
    entries_with_superseded_by: u64,
    total_correction_count: u64,
}

#[derive(Serialize)]
struct SecurityJson {
    trust_source_distribution: serde_json::Map<String, serde_json::Value>,
    entries_without_attribution: u64,
}

#[derive(Serialize)]
struct ContradictionJson {
    entry_id_a: u64,
    entry_id_b: u64,
    title_a: String,
    title_b: String,
    similarity: f32,
    conflict_score: f32,
    explanation: String,
}

#[derive(Serialize)]
struct EmbeddingInconsistencyJson {
    entry_id: u64,
    title: String,
    self_match_similarity: f32,   // NOTE: field name is self_match_similarity, value from expected_similarity
}

#[derive(Serialize)]
struct CoAccessClusterJson {
    entry_a: CoAccessEntryJson,
    entry_b: CoAccessEntryJson,
    count: u32,
    last_updated: u64,
}

#[derive(Serialize)]
struct CoAccessEntryJson {
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
    by_type: serde_json::Map<String, serde_json::Value>,
    by_result: serde_json::Map<String, serde_json::Value>,
    top_feature_cycles: Vec<FeatureCycleJson>,
}

#[derive(Serialize)]
struct FeatureCycleJson {
    feature_cycle: String,
    count: u64,
}

#[derive(Serialize)]
struct ObservationJson {
    file_count: u64,
    total_size_bytes: u64,
    oldest_file_days: u64,
    approaching_cleanup: Vec<String>,
    retrospected_feature_count: u64,
}
```

### Helper: Vec<(String, u64)> to serde_json::Map

```
fn dist_to_map(dist: &[(String, u64)]) -> serde_json::Map<String, serde_json::Value> {
    dist.iter()
        .map(|(k, v)| (k.clone(), serde_json::json!(v)))
        .collect()
}
```

### impl From<&StatusReport> for StatusReportJson

```
impl From<&StatusReport> for StatusReportJson {
    fn from(r: &StatusReport) -> Self {
        // Conditional sections
        LET contradictions = IF r.contradiction_scan_performed THEN
            Some(r.contradictions.iter().map(|p| ContradictionJson {
                entry_id_a: p.entry_id_a,
                entry_id_b: p.entry_id_b,
                title_a: p.title_a.clone(),
                title_b: p.title_b.clone(),
                similarity: p.similarity,
                conflict_score: p.conflict_score,
                explanation: p.explanation.clone(),
            }).collect())
        ELSE
            None
        END IF

        LET contradiction_count = IF r.contradiction_scan_performed THEN
            Some(r.contradiction_count)
        ELSE
            None
        END IF

        LET embedding_inconsistencies = IF r.embedding_check_performed THEN
            Some(r.embedding_inconsistencies.iter().map(|i| EmbeddingInconsistencyJson {
                entry_id: i.entry_id,
                title: i.title.clone(),
                self_match_similarity: i.expected_similarity,
            }).collect())
        ELSE
            None
        END IF

        LET outcomes = IF r.total_outcomes > 0 || !r.outcomes_by_type.is_empty() THEN
            Some(OutcomesJson {
                total: r.total_outcomes,
                by_type: dist_to_map(&r.outcomes_by_type),
                by_result: dist_to_map(&r.outcomes_by_result),
                top_feature_cycles: r.outcomes_by_feature_cycle.iter().map(|(fc, count)| {
                    FeatureCycleJson { feature_cycle: fc.clone(), count: *count }
                }).collect(),
            })
        ELSE
            None
        END IF

        StatusReportJson {
            total_active: r.total_active,
            total_deprecated: r.total_deprecated,
            total_proposed: r.total_proposed,
            total_quarantined: r.total_quarantined,
            category_distribution: dist_to_map(&r.category_distribution),
            topic_distribution: dist_to_map(&r.topic_distribution),
            correction_chains: CorrectionChainsJson {
                entries_with_supersedes: r.entries_with_supersedes,
                entries_with_superseded_by: r.entries_with_superseded_by,
                total_correction_count: r.total_correction_count,
            },
            security: SecurityJson {
                trust_source_distribution: dist_to_map(&r.trust_source_distribution),
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
                top_clusters: r.top_co_access_pairs.iter().map(|c| CoAccessClusterJson {
                    entry_a: CoAccessEntryJson { id: c.entry_id_a, title: c.title_a.clone() },
                    entry_b: CoAccessEntryJson { id: c.entry_id_b, title: c.title_b.clone() },
                    count: c.count,
                    last_updated: c.last_updated,
                }).collect(),
            },
            outcomes,
            observation: ObservationJson {
                file_count: r.observation_file_count,
                total_size_bytes: r.observation_total_size_bytes,
                oldest_file_days: r.observation_oldest_file_days,
                approaching_cleanup: r.observation_approaching_cleanup.clone(),
                retrospected_feature_count: r.retrospected_feature_count,
            },
        }
    }
}
```

### Replace JSON branch in format_status_report

```
ResponseFormat::Json => {
    LET json_report = StatusReportJson::from(report);
    LET json_string = serde_json::to_string_pretty(&json_report).unwrap_or_default();
    CallToolResult::success(vec![Content::text(json_string)])
}
```

This replaces the ~130 lines of manual json! assembly (lines 381-512 of status.rs).

## File: `crates/unimatrix-server/src/infra/contradiction.rs` (modifications)

### Add derive(Serialize) to types

```
#[derive(Debug, Clone, serde::Serialize)]    // ADD Serialize
pub struct ContradictionPair {
    // ... existing fields unchanged ...
}

// Find EmbeddingInconsistency struct, add Serialize:
#[derive(Debug, Clone, serde::Serialize)]    // ADD Serialize
pub struct EmbeddingInconsistency {
    // ... existing fields unchanged ...
}
```

## Open Questions

None. ADR-001 resolves the serialization strategy. The intermediate struct approach
avoids polluting the domain model while preserving exact JSON output compatibility.
