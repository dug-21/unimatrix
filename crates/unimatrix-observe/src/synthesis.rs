//! Narrative synthesis for hotspot findings (col-010b).
//!
//! Deterministic heuristics that transform raw hotspot evidence into
//! structured narratives with timestamp clustering, file ranking, and
//! sequence pattern detection.

use std::collections::HashMap;

use crate::types::{EvidenceCluster, EvidenceRecord, HotspotFinding, HotspotNarrative};

/// Time window for clustering evidence events (seconds).
pub const CLUSTER_WINDOW_SECS: u64 = 30;

/// Synthesize one `HotspotNarrative` per `HotspotFinding`.
pub fn synthesize_narratives(hotspots: &[HotspotFinding]) -> Vec<HotspotNarrative> {
    hotspots.iter().map(synthesize_one).collect()
}

fn synthesize_one(hotspot: &HotspotFinding) -> HotspotNarrative {
    let clusters = cluster_evidence(&hotspot.evidence);
    let top_files = extract_top_files(&hotspot.evidence, 5);
    let sequence_pattern = extract_sequence_pattern(hotspot);
    let summary = build_summary(hotspot, &clusters, &top_files);
    HotspotNarrative {
        hotspot_type: hotspot.rule_name.clone(),
        summary,
        clusters,
        top_files,
        sequence_pattern,
    }
}

/// Group evidence events into clusters where consecutive events are within
/// `CLUSTER_WINDOW_SECS * 1000` ms of each other.
fn cluster_evidence(evidence: &[EvidenceRecord]) -> Vec<EvidenceCluster> {
    if evidence.is_empty() {
        return vec![];
    }

    let mut sorted: Vec<&EvidenceRecord> = evidence.iter().collect();
    sorted.sort_by_key(|e| e.ts);

    let window_ms = CLUSTER_WINDOW_SECS * 1000;
    let mut clusters = Vec::new();
    let mut current_start = sorted[0].ts;
    let mut current_count: u32 = 1;
    let mut descriptions: Vec<&str> = vec![&sorted[0].description];

    for event in &sorted[1..] {
        if event.ts.saturating_sub(current_start) <= window_ms {
            current_count += 1;
            descriptions.push(&event.description);
        } else {
            // Finalize current cluster
            let desc = format_cluster_description(current_count, &descriptions);
            clusters.push(EvidenceCluster {
                window_start: current_start,
                event_count: current_count,
                description: desc,
            });
            // Start new cluster
            current_start = event.ts;
            current_count = 1;
            descriptions = vec![&event.description];
        }
    }

    // Finalize last cluster
    let desc = format_cluster_description(current_count, &descriptions);
    clusters.push(EvidenceCluster {
        window_start: current_start,
        event_count: current_count,
        description: desc,
    });

    clusters
}

fn format_cluster_description(count: u32, descriptions: &[&str]) -> String {
    let joined = descriptions.join("; ");
    let truncated = if joined.len() > 200 {
        format!("{}...", &joined[..197])
    } else {
        joined
    };
    format!("{} event(s): {}", count, truncated)
}

/// Extract monotone-increasing numeric sequence from sleep_workarounds evidence.
///
/// Only applies to hotspots with `rule_name == "sleep_workarounds"`.
/// Scans evidence descriptions for numeric values (digits followed by optional 's').
/// Returns formatted pattern if strictly monotonically increasing with >= 2 values.
fn extract_sequence_pattern(hotspot: &HotspotFinding) -> Option<String> {
    if hotspot.rule_name != "sleep_workarounds" {
        return None;
    }

    let mut values: Vec<u64> = Vec::new();
    for ev in &hotspot.evidence {
        for num in extract_numbers(&ev.description) {
            values.push(num);
        }
    }

    if values.len() < 2 {
        return None;
    }

    // Check strictly monotonically increasing
    for i in 1..values.len() {
        if values[i] <= values[i - 1] {
            return None;
        }
    }

    Some(
        values
            .iter()
            .map(|v| format!("{}s", v))
            .collect::<Vec<_>>()
            .join("->"),
    )
}

/// Extract numeric values from a description string.
/// Looks for patterns like "30s", "60s", "sleep 30", etc.
fn extract_numbers(text: &str) -> Vec<u64> {
    let mut result = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_digit() {
            let mut num_str = String::new();
            num_str.push(ch);
            while let Some(&next) = chars.peek() {
                if next.is_ascii_digit() {
                    num_str.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            // Skip optional 's' suffix
            if let Some(&'s') = chars.peek() {
                chars.next();
            }
            if let Ok(n) = num_str.parse::<u64>() {
                result.push(n);
            }
        }
    }
    result
}

/// Extract top N files from evidence descriptions and detail fields.
///
/// Parses file paths (anything containing '/' or ending in known extensions),
/// counts occurrences, returns sorted by count descending.
fn extract_top_files(evidence: &[EvidenceRecord], limit: usize) -> Vec<(String, u32)> {
    let mut file_counts: HashMap<String, u32> = HashMap::new();

    for ev in evidence {
        for path in extract_file_paths(&ev.description) {
            *file_counts.entry(path).or_default() += 1;
        }
        for path in extract_file_paths(&ev.detail) {
            *file_counts.entry(path).or_default() += 1;
        }
    }

    let mut sorted: Vec<(String, u32)> = file_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    sorted.truncate(limit);
    sorted
}

/// Extract file paths from text. Looks for tokens containing '/' or common extensions.
fn extract_file_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for token in text.split_whitespace() {
        // Remove trailing punctuation
        let cleaned = token.trim_end_matches(|c: char| c == ',' || c == '.' || c == ';' || c == ':' || c == ')');
        if cleaned.contains('/') || cleaned.ends_with(".rs") || cleaned.ends_with(".toml") || cleaned.ends_with(".md") {
            paths.push(cleaned.to_string());
        }
    }
    paths
}

/// Build a human-readable summary from hotspot data, clusters, and top files.
fn build_summary(
    hotspot: &HotspotFinding,
    clusters: &[EvidenceCluster],
    top_files: &[(String, u32)],
) -> String {
    let mut summary = format!("{}: {}", hotspot.rule_name, hotspot.claim);

    if !clusters.is_empty() {
        summary.push_str(&format!(". {} event cluster(s) detected", clusters.len()));
    }

    if !top_files.is_empty() {
        let file_names: Vec<&str> = top_files.iter().map(|(f, _)| f.as_str()).collect();
        summary.push_str(&format!(". Top files: {}", file_names.join(", ")));
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{HotspotCategory, Severity};

    fn make_evidence(ts: u64, desc: &str) -> EvidenceRecord {
        EvidenceRecord {
            description: desc.to_string(),
            ts,
            tool: None,
            detail: String::new(),
        }
    }

    fn make_hotspot(rule_name: &str, evidence: Vec<EvidenceRecord>) -> HotspotFinding {
        HotspotFinding {
            category: HotspotCategory::Friction,
            severity: Severity::Warning,
            rule_name: rule_name.to_string(),
            claim: format!("{} detected", rule_name),
            measured: evidence.len() as f64,
            threshold: 1.0,
            evidence,
        }
    }

    // -- T-ES-01: synthesize_narratives produces one per hotspot --
    #[test]
    fn test_synthesize_narratives_one_per_hotspot() {
        let hotspots = vec![
            make_hotspot("permission_retries", vec![make_evidence(1000, "retry 1")]),
            make_hotspot("sleep_workarounds", vec![make_evidence(2000, "sleep 30s")]),
            make_hotspot("compile_cycles", vec![make_evidence(3000, "compile")]),
        ];
        let narratives = synthesize_narratives(&hotspots);
        assert_eq!(narratives.len(), 3);
        assert_eq!(narratives[0].hotspot_type, "permission_retries");
        assert_eq!(narratives[1].hotspot_type, "sleep_workarounds");
        assert_eq!(narratives[2].hotspot_type, "compile_cycles");
    }

    // -- T-ES-02: cluster_evidence groups by timestamp window --
    #[test]
    fn test_cluster_evidence_groups_by_window() {
        let evidence = vec![
            make_evidence(1000, "event a"),
            make_evidence(10_000, "event b"),      // within 30s of 1000
            make_evidence(20_000, "event c"),       // within 30s of 1000
            make_evidence(60_000_000, "event d"),   // far away
            make_evidence(60_010_000, "event e"),   // within 30s of d
        ];
        let clusters = cluster_evidence(&evidence);
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].window_start, 1000);
        assert_eq!(clusters[0].event_count, 3);
        assert_eq!(clusters[1].window_start, 60_000_000);
        assert_eq!(clusters[1].event_count, 2);
    }

    // -- T-ES-03: cluster_evidence with empty evidence --
    #[test]
    fn test_cluster_evidence_empty() {
        let clusters = cluster_evidence(&[]);
        assert!(clusters.is_empty());
    }

    // -- T-ES-04: monotone increasing sequence pattern (AC-04) --
    #[test]
    fn test_sequence_pattern_monotone() {
        let evidence = vec![
            make_evidence(1000, "sleep 30s"),
            make_evidence(2000, "sleep 60s"),
            make_evidence(3000, "sleep 90s"),
            make_evidence(4000, "sleep 120s"),
        ];
        let hotspot = make_hotspot("sleep_workarounds", evidence);
        let pattern = extract_sequence_pattern(&hotspot);
        assert_eq!(pattern, Some("30s->60s->90s->120s".to_string()));
    }

    // -- T-ES-05: non-monotone returns None (AC-04) --
    #[test]
    fn test_sequence_pattern_non_monotone() {
        let evidence = vec![
            make_evidence(1000, "sleep 30s"),
            make_evidence(2000, "sleep 60s"),
            make_evidence(3000, "sleep 30s"),
            make_evidence(4000, "sleep 120s"),
        ];
        let hotspot = make_hotspot("sleep_workarounds", evidence);
        let pattern = extract_sequence_pattern(&hotspot);
        assert_eq!(pattern, None);
    }

    // -- T-ES-06: non-sleep rule returns None --
    #[test]
    fn test_sequence_pattern_non_sleep_rule() {
        let evidence = vec![
            make_evidence(1000, "retry 30s"),
            make_evidence(2000, "retry 60s"),
        ];
        let hotspot = make_hotspot("permission_retries", evidence);
        let pattern = extract_sequence_pattern(&hotspot);
        assert_eq!(pattern, None);
    }

    // -- T-ES-07: extract_top_files with > 5 distinct files --
    #[test]
    fn test_extract_top_files_limit() {
        let evidence = vec![
            make_evidence(1000, "file src/a.rs src/b.rs src/c.rs"),
            make_evidence(2000, "file src/d.rs src/e.rs src/f.rs"),
            make_evidence(3000, "file src/g.rs src/h.rs src/a.rs"),
        ];
        let top = extract_top_files(&evidence, 5);
        assert!(top.len() <= 5);
        // src/a.rs should appear twice, so it should be first
        assert_eq!(top[0].0, "src/a.rs");
        assert_eq!(top[0].1, 2);
    }

    // -- T-ES-08: build_summary is non-empty --
    #[test]
    fn test_build_summary_non_empty() {
        // Empty evidence
        let hotspot = make_hotspot("test_rule", vec![]);
        let summary = build_summary(&hotspot, &[], &[]);
        assert!(!summary.is_empty());

        // Single event
        let hotspot = make_hotspot("test_rule", vec![make_evidence(1000, "event")]);
        let clusters = cluster_evidence(&hotspot.evidence);
        let summary = build_summary(&hotspot, &clusters, &[]);
        assert!(!summary.is_empty());
    }

    // -- extract_numbers tests --
    #[test]
    fn test_extract_numbers() {
        assert_eq!(extract_numbers("sleep 30s"), vec![30]);
        assert_eq!(extract_numbers("sleep 60s then 90s"), vec![60, 90]);
        assert_eq!(extract_numbers("no numbers here"), Vec::<u64>::new());
        assert_eq!(extract_numbers("retry after 120 seconds"), vec![120]);
    }

    // -- single-event cluster --
    #[test]
    fn test_cluster_single_event() {
        let evidence = vec![make_evidence(5000, "single event")];
        let clusters = cluster_evidence(&evidence);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].event_count, 1);
        assert_eq!(clusters[0].window_start, 5000);
    }

    // -- extract_file_paths --
    #[test]
    fn test_extract_file_paths() {
        let paths = extract_file_paths("modified src/lib.rs and Cargo.toml");
        assert!(paths.contains(&"src/lib.rs".to_string()));
        assert!(paths.contains(&"Cargo.toml".to_string()));
    }
}
