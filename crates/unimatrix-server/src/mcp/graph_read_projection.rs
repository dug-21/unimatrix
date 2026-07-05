//! Graph-local lean node/edge projection for the `context_graph` `detail=summary` axis
//! (vnc-044, ADR-002 §3/§4).
//!
//! Defines `NodeSummary` — the `context_graph` instance of ADR-001 §5's 8-field set — the
//! record → summary builders, and the `GraphSummaryProjection` trait implemented for the five
//! node-bearing envelopes. Keeping lean serialization here (rather than adding
//! `skip_serializing_if` to shared `EntryRecord`/`EdgeRecord` — C-2/C-3/SR-07) means the shared
//! store types serialize identically for every other context tool, and the projection stays out
//! of the already-over-limit `graph_read_subgraph.rs` (C-7/SR-08).
//!
//! This is a `#[path]` child module of `graph_read` (declared there), so it reaches the envelope
//! types via `super::…`.

use serde::Serialize;
use serde_json::{Value, json};
use unimatrix_core::EntryRecord;

use super::{
    ChainResult, CurrentResponse, EdgeRecord, FilterResponse, InverseResponse, SubgraphResponse,
};
use crate::mcp::response::status_str;
use crate::mcp::response::verbosity::content_preview;

/// The lean node shape (ADR-001 §5 field set, `context_graph` instance).
///
/// `derive(Serialize)` ⇒ JSON key order follows declaration order — this type IS the field-set
/// contract (AC-03). No `serde(skip)`, no extra fields. Distinct from `EntryRecord`, which gains
/// no `skip_serializing_if` (C-3/SR-07).
///
/// `status` is the **lifecycle** status (`active|deprecated|proposed|quarantined`) via
/// `status_str(entry.status)` — **NOT** capability delivery status (`missing/partial/proven/
/// claimed`, which lives in the entry `content` blob). A capability subgraph returns
/// `status:"active"` for every node (SR-09 / R-11). This is the honestly-carried #913 gap.
#[derive(Debug, Serialize)]
struct NodeSummary {
    id: u64,
    title: String,
    category: String,
    tags: Vec<String>,
    status: &'static str,
    confidence: f64,
    content_preview: String,
    content_truncated: bool,
}

/// Build the lean summary of a single node. Pure, total (`content_preview` never panics).
fn node_summary(entry: &EntryRecord) -> NodeSummary {
    let (preview, truncated) = content_preview(&entry.content);
    NodeSummary {
        id: entry.id,
        title: entry.title.clone(),
        category: entry.category.clone(),
        // tags already hydrated by fetch_nodes_batch (R-10).
        tags: entry.tags.clone(),
        status: status_str(entry.status),
        confidence: entry.confidence,
        content_preview: preview,
        content_truncated: truncated,
    }
}

/// Build the lean summary of a single edge, projecting exactly
/// `{source_id, target_id, relation_type, depth}`.
///
/// `direction` and `metadata` are DROPPED at projection time (FR-5/AC-03/R-07). `EdgeRecord`
/// itself is NOT mutated and keeps no `skip_serializing_if` (C-2/SR-07) — this is a distinct
/// `serde_json::Value`, not a filtered `EdgeRecord`.
fn edge_summary(edge: &EdgeRecord) -> Value {
    json!({
        "source_id": edge.source_id,
        "target_id": edge.target_id,
        "relation_type": edge.relation_type,
        "depth": edge.depth,
    })
}

/// Serialize an envelope into its lean summary `Value`, mapping node bodies through
/// `node_summary` / edges through `edge_summary` while **preserving every envelope metadata
/// field** (R-03 — silent metadata loss is the primary bug surface).
///
/// Implemented only for the five node-bearing envelopes. `NeighborsResponse` / `PathResponse`
/// carry no node bodies and do NOT implement it (detail accept-and-ignore — the always-full arm
/// in graph_read.rs handles them).
pub(super) trait GraphSummaryProjection {
    fn to_summary_json(&self) -> Value;
}

impl GraphSummaryProjection for SubgraphResponse {
    fn to_summary_json(&self) -> Value {
        json!({
            "nodes": self.nodes.iter().map(node_summary).collect::<Vec<_>>(),
            "edges": self.edges.iter().map(edge_summary).collect::<Vec<_>>(),
            "truncated": self.truncated,
            "seed_ids": self.seed_ids,
            "depth_reached": self.depth_reached,
        })
    }
}

impl GraphSummaryProjection for ChainResult {
    fn to_summary_json(&self) -> Value {
        json!({
            "entries": self.entries.iter().map(node_summary).collect::<Vec<_>>(),
            // Preserve the whole Truncated {forward, backward} struct.
            "truncated": self.truncated,
        })
    }
}

impl GraphSummaryProjection for CurrentResponse {
    fn to_summary_json(&self) -> Value {
        json!({
            // SINGLE node object, NOT an array (R-03 shape trap).
            "entry": node_summary(&self.entry),
        })
    }
}

impl GraphSummaryProjection for InverseResponse {
    fn to_summary_json(&self) -> Value {
        json!({
            "entries": self.entries.iter().map(node_summary).collect::<Vec<_>>(),
            "total_returned": self.total_returned,
        })
    }
}

impl GraphSummaryProjection for FilterResponse {
    fn to_summary_json(&self) -> Value {
        json!({
            "entries": self.entries.iter().map(node_summary).collect::<Vec<_>>(),
            "total_returned": self.total_returned,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::Truncated;
    use super::*;
    use std::collections::BTreeSet;
    use unimatrix_core::Status;

    fn make_entry(id: u64) -> EntryRecord {
        EntryRecord {
            id,
            title: format!("Entry {id}"),
            content: String::new(),
            topic: "topic".to_string(),
            category: "pattern".to_string(),
            tags: vec![],
            source: String::new(),
            status: Status::Active,
            confidence: 0.5,
            created_at: 0,
            updated_at: 0,
            last_accessed_at: 0,
            access_count: 0,
            supersedes: None,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 0,
            created_by: String::new(),
            modified_by: String::new(),
            content_hash: String::new(),
            previous_hash: String::new(),
            version: 1,
            feature_cycle: String::new(),
            trust_source: "agent".to_string(),
            helpful_count: 0,
            unhelpful_count: 0,
            pre_quarantine_status: None,
        }
    }

    fn make_edge(source_id: u64, target_id: u64) -> EdgeRecord {
        EdgeRecord {
            source_id,
            target_id,
            relation_type: "Supports".to_string(),
            direction: "outgoing".to_string(),
            depth: 1,
            metadata: None,
        }
    }

    fn keys(value: &Value) -> BTreeSet<String> {
        value
            .as_object()
            .expect("expected a JSON object")
            .keys()
            .cloned()
            .collect()
    }

    // --- R-07: exact node field set (present AND absent) ---

    #[test]
    fn test_node_summary_exact_key_set() {
        let obj = serde_json::to_value(node_summary(&make_entry(1))).expect("serialize");
        let expected: BTreeSet<String> = [
            "id",
            "title",
            "category",
            "tags",
            "status",
            "confidence",
            "content_preview",
            "content_truncated",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(keys(&obj), expected);
    }

    #[test]
    fn test_node_summary_omits_content_and_hashes() {
        let obj = serde_json::to_value(node_summary(&make_entry(1))).expect("serialize");
        // Highest-value leaks: the content blob and integrity hashes.
        for absent in [
            "content",
            "content_hash",
            "previous_hash",
            "embedding_dim",
            "created_at",
            "updated_at",
            "last_accessed_at",
            "access_count",
            "created_by",
            "modified_by",
            "correction_count",
            "version",
            "supersedes",
            "superseded_by",
            "topic",
            "source",
            "trust_source",
            "helpful_count",
            "unhelpful_count",
            "feature_cycle",
            "pre_quarantine_status",
        ] {
            assert!(
                !obj.as_object().unwrap().contains_key(absent),
                "leaked key: {absent}"
            );
        }
    }

    // --- R-07: field value correctness ---

    #[test]
    fn test_node_summary_status_is_lifecycle_string() {
        for (status, expected) in [
            (Status::Active, "active"),
            (Status::Deprecated, "deprecated"),
            (Status::Proposed, "proposed"),
            (Status::Quarantined, "quarantined"),
        ] {
            let mut entry = make_entry(1);
            entry.status = status;
            let summary = node_summary(&entry);
            assert_eq!(summary.status, expected);
        }
    }

    #[test]
    fn test_node_summary_copies_id_title_category() {
        let mut entry = make_entry(7);
        entry.title = "My Title".to_string();
        entry.category = "decision".to_string();
        let obj = serde_json::to_value(node_summary(&entry)).expect("serialize");
        assert_eq!(obj["id"], json!(7));
        assert_eq!(obj["title"], json!("My Title"));
        assert_eq!(obj["category"], json!("decision"));
    }

    #[test]
    fn test_node_summary_preview_wiring_over_cap() {
        use crate::mcp::response::verbosity::CONTENT_PREVIEW_BYTES;
        let mut entry = make_entry(1);
        entry.content = "a".repeat(CONTENT_PREVIEW_BYTES + 44);
        let summary = node_summary(&entry);
        assert_eq!(summary.content_preview.len(), CONTENT_PREVIEW_BYTES);
        assert!(summary.content_truncated);
    }

    // --- R-10: empty/boundary fidelity ---

    #[test]
    fn test_node_summary_empty_content() {
        let summary = node_summary(&make_entry(1));
        assert_eq!(summary.content_preview, "");
        assert!(!summary.content_truncated);
    }

    #[test]
    fn test_node_summary_preserves_all_tags() {
        let mut entry = make_entry(1);
        entry.tags = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let obj = serde_json::to_value(node_summary(&entry)).expect("serialize");
        assert_eq!(obj["tags"], json!(["a", "b", "c"]));
    }

    #[test]
    fn test_node_summary_zero_tags_empty_array() {
        let obj = serde_json::to_value(node_summary(&make_entry(1))).expect("serialize");
        assert_eq!(obj["tags"], json!([]));
    }

    #[test]
    fn test_node_summary_confidence_is_number() {
        let mut entry = make_entry(1);
        entry.confidence = 0.75;
        let obj = serde_json::to_value(node_summary(&entry)).expect("serialize");
        assert!(obj["confidence"].is_number());
        assert_eq!(obj["confidence"], json!(0.75));
    }

    // --- R-07: exact edge field set (present AND absent) ---

    #[test]
    fn test_edge_summary_exact_key_set() {
        let obj = edge_summary(&make_edge(1, 2));
        let expected: BTreeSet<String> = ["source_id", "target_id", "relation_type", "depth"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(keys(&obj), expected);
    }

    #[test]
    fn test_edge_summary_omits_direction_and_metadata() {
        let obj = edge_summary(&make_edge(1, 2));
        let map = obj.as_object().unwrap();
        assert!(!map.contains_key("direction"), "direction leaked");
        assert!(!map.contains_key("metadata"), "metadata leaked");
    }

    #[test]
    fn test_edge_summary_values() {
        let obj = edge_summary(&make_edge(10, 20));
        assert_eq!(obj["source_id"], json!(10));
        assert_eq!(obj["target_id"], json!(20));
        assert_eq!(obj["relation_type"], json!("Supports"));
        assert_eq!(obj["depth"], json!(1));
    }

    // --- R-03: per-envelope metadata preservation ---

    #[test]
    fn test_subgraph_summary_projects_nodes_preserves_metadata() {
        let resp = SubgraphResponse {
            nodes: vec![make_entry(1), make_entry(2)],
            edges: vec![make_edge(1, 2)],
            truncated: true,
            seed_ids: vec![1],
            depth_reached: 3,
        };
        let obj = resp.to_summary_json();
        // Nodes are 8-field summaries.
        assert_eq!(obj["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(keys(&obj["nodes"][0]).len(), 8);
        // Edges are 4-field summaries.
        assert_eq!(keys(&obj["edges"][0]).len(), 4);
        // Metadata preserved.
        assert_eq!(obj["truncated"], json!(true));
        assert_eq!(obj["seed_ids"], json!([1]));
        assert_eq!(obj["depth_reached"], json!(3));
    }

    #[test]
    fn test_chain_summary_projects_nodes_preserves_metadata() {
        let resp = ChainResult {
            entries: vec![make_entry(1)],
            truncated: Truncated {
                forward: true,
                backward: false,
            },
        };
        let obj = resp.to_summary_json();
        assert_eq!(obj["entries"].as_array().unwrap().len(), 1);
        assert_eq!(keys(&obj["entries"][0]).len(), 8);
        assert_eq!(
            obj["truncated"],
            json!({"forward": true, "backward": false})
        );
    }

    #[test]
    fn test_current_summary_is_single_node_not_array() {
        let resp = CurrentResponse {
            entry: make_entry(42),
        };
        let obj = resp.to_summary_json();
        assert!(obj["entry"].is_object(), "entry must be a single object");
        assert!(!obj["entry"].is_array(), "entry must NOT be an array");
        assert_eq!(obj["entry"]["id"], json!(42));
        assert_eq!(keys(&obj["entry"]).len(), 8);
    }

    #[test]
    fn test_inverse_summary_projects_nodes_preserves_metadata() {
        let resp = InverseResponse {
            entries: vec![make_entry(1), make_entry(2)],
            total_returned: 2,
        };
        let obj = resp.to_summary_json();
        assert_eq!(obj["entries"].as_array().unwrap().len(), 2);
        assert_eq!(keys(&obj["entries"][0]).len(), 8);
        assert_eq!(obj["total_returned"], json!(2));
    }

    #[test]
    fn test_filter_summary_projects_nodes_preserves_metadata() {
        let resp = FilterResponse {
            entries: vec![make_entry(1)],
            total_returned: 1,
        };
        let obj = resp.to_summary_json();
        assert_eq!(obj["entries"].as_array().unwrap().len(), 1);
        assert_eq!(keys(&obj["entries"][0]).len(), 8);
        assert_eq!(obj["total_returned"], json!(1));
    }
}
