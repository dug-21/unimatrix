//! vnc-044 tests: `resolve_graph_output` decision table, the detail serialization seam
//! (`:251` parse-and-drop fix), and the additive `detail` GraphParams field.
//!
//! Declared as a child of `graph_read_tests` to keep the parent test file under 500 lines.
//! Reaches graph_read's private items (`resolve_graph_output`, `GraphSerialization`,
//! `serialize_detail`, the envelopes) via `super::super::…`.

use super::super::{
    ChainResult, CurrentResponse, GraphParams, GraphSerialization, SubgraphResponse, Truncated,
    resolve_graph_output, serialize_detail,
};
use crate::mcp::response::verbosity::Detail;
use unimatrix_core::{EntryRecord, Status};

fn params(format: Option<&str>, detail: Option<&str>) -> GraphParams {
    GraphParams {
        mode: "subgraph".to_string(),
        format: format.map(|s| s.to_string()),
        detail: detail.map(|s| s.to_string()),
        ..Default::default()
    }
}

fn make_entry(id: u64) -> EntryRecord {
    EntryRecord {
        id,
        title: format!("Entry {id}"),
        content: "hello world".to_string(),
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
        content_hash: "abc123".to_string(),
        previous_hash: String::new(),
        version: 1,
        feature_cycle: String::new(),
        trust_source: "agent".to_string(),
        helpful_count: 0,
        unhelpful_count: 0,
        pre_quarantine_status: None,
    }
}

fn subgraph_fixture() -> SubgraphResponse {
    SubgraphResponse {
        nodes: vec![make_entry(1)],
        edges: vec![],
        truncated: false,
        seed_ids: vec![1],
        depth_reached: 0,
    }
}

// ---------------------------------------------------------------------------
// resolve_graph_output decision table (R-03, R-05, R-08)
// ---------------------------------------------------------------------------

#[test]
fn test_resolve_default_summary_json() {
    let (detail, ser) = resolve_graph_output(&params(None, None)).expect("ok");
    assert_eq!(detail, Detail::Summary);
    assert!(matches!(ser, GraphSerialization::Json));
}

#[test]
fn test_resolve_none_full() {
    let (detail, _) = resolve_graph_output(&params(None, Some("full"))).expect("ok");
    assert_eq!(detail, Detail::Full);
}

#[test]
fn test_resolve_json_summary() {
    let (detail, _) = resolve_graph_output(&params(Some("json"), Some("summary"))).expect("ok");
    assert_eq!(detail, Detail::Summary);
}

#[test]
fn test_resolve_json_none_defaults_summary() {
    let (detail, _) = resolve_graph_output(&params(Some("json"), None)).expect("ok");
    assert_eq!(detail, Detail::Summary);
}

#[test]
fn test_resolve_markdown_rejected_substring() {
    for detail in [None, Some("summary"), Some("full")] {
        let err = resolve_graph_output(&params(Some("markdown"), detail)).unwrap_err();
        let msg = err.message.to_string();
        assert!(msg.contains("markdown"), "got: {msg}");
        assert!(msg.contains("format=json"), "got: {msg}");
    }
}

#[test]
fn test_resolve_legacy_summary_alias() {
    // format=summary, no detail → (Summary, Json) alias.
    let (detail, ser) = resolve_graph_output(&params(Some("summary"), None)).expect("ok");
    assert_eq!(detail, Detail::Summary);
    assert!(matches!(ser, GraphSerialization::Json));
}

#[test]
fn test_resolve_legacy_summary_alias_case_insensitive() {
    let (detail, _) = resolve_graph_output(&params(Some("SUMMARY"), None)).expect("ok");
    assert_eq!(detail, Detail::Summary);
}

#[test]
fn test_resolve_summary_plus_explicit_full_conflict() {
    let err = resolve_graph_output(&params(Some("summary"), Some("full"))).unwrap_err();
    assert!(err.message.to_string().contains("deprecated alias"));
}

#[test]
fn test_resolve_summary_plus_explicit_summary_conflict() {
    // R-08 order pin: the legacy-alias-conflict branch fires BEFORE the verbosity parse, so
    // format=summary + detail=summary is a CONFLICT, not a silent agreement (ADR-002 §2).
    let err = resolve_graph_output(&params(Some("summary"), Some("summary"))).unwrap_err();
    assert!(err.message.to_string().contains("deprecated alias"));
}

#[test]
fn test_resolve_bogus_format_rejected() {
    let err = resolve_graph_output(&params(Some("xml"), None)).unwrap_err();
    assert!(err.message.to_string().contains("must be json"));
}

#[test]
fn test_resolve_bogus_detail_rejected() {
    // From parse_detail — universal, runs for every mode.
    let err = resolve_graph_output(&params(Some("json"), Some("brief"))).unwrap_err();
    assert!(err.message.to_string().contains("detail"));
}

// ---------------------------------------------------------------------------
// Serialization seam (R-04 full-not-projected guard, summary-projected)
// ---------------------------------------------------------------------------

#[test]
fn test_full_arm_serializes_raw_result() {
    // Detail::Full serializes the ORIGINAL envelope: full-only fields (content, content_hash)
    // are present — it does NOT route through the lean projection.
    let json = serialize_detail(Detail::Full, &subgraph_fixture()).expect("serialize");
    assert!(json.contains("content_hash"), "full must keep content_hash");
    assert!(json.contains("hello world"), "full must keep content body");
}

#[test]
fn test_full_arm_byte_identical_to_direct_to_string() {
    // The full arm must be byte-for-byte identical to today's serde_json::to_string(&result).
    let fixture = subgraph_fixture();
    let via_seam = serialize_detail(Detail::Full, &fixture).expect("serialize");
    let direct = serde_json::to_string(&fixture).expect("serialize");
    assert_eq!(via_seam, direct);
}

#[test]
fn test_summary_arm_uses_projection() {
    // Detail::Summary routes through to_summary_json: content/content_hash are dropped.
    let json = serialize_detail(Detail::Summary, &subgraph_fixture()).expect("serialize");
    assert!(
        !json.contains("content_hash"),
        "summary must drop content_hash"
    );
    assert!(!json.contains("\"content\":"), "summary must drop content");
    // But the preview and metadata survive.
    assert!(json.contains("content_preview"));
    assert!(json.contains("seed_ids"));
}

#[test]
fn test_summary_differs_from_full() {
    let fixture = subgraph_fixture();
    let full = serialize_detail(Detail::Full, &fixture).expect("serialize");
    let summary = serialize_detail(Detail::Summary, &fixture).expect("serialize");
    assert_ne!(full, summary, "detail axis must reach serialization");
}

#[test]
fn test_seam_chain_summary_projects() {
    let chain = ChainResult {
        entries: vec![make_entry(1)],
        truncated: Truncated {
            forward: false,
            backward: false,
        },
    };
    let summary = serialize_detail(Detail::Summary, &chain).expect("serialize");
    assert!(!summary.contains("content_hash"));
    assert!(summary.contains("content_preview"));
}

#[test]
fn test_seam_current_summary_single_node() {
    let current = CurrentResponse {
        entry: make_entry(9),
    };
    let summary = serialize_detail(Detail::Summary, &current).expect("serialize");
    let value: serde_json::Value = serde_json::from_str(&summary).expect("parse");
    assert!(
        value["entry"].is_object(),
        "current entry must be an object"
    );
}

// ---------------------------------------------------------------------------
// Additive GraphParams.detail (R-06 / AC-09)
// ---------------------------------------------------------------------------

#[test]
fn test_graph_params_detail_additive_defaults_none() {
    // Deserializing a payload with no `detail` leaves it None (additive, ADR-003).
    let p: GraphParams = serde_json::from_str(r#"{"mode":"subgraph"}"#).expect("deserialize");
    assert!(p.detail.is_none());
    assert_eq!(p.mode, "subgraph");
}

#[test]
fn test_graph_params_detail_roundtrips() {
    let p: GraphParams =
        serde_json::from_str(r#"{"mode":"subgraph","detail":"full"}"#).expect("deserialize");
    assert_eq!(p.detail.as_deref(), Some("full"));
}

// ---------------------------------------------------------------------------
// detail universal — not rejected by validate on neighbors/path (R-09)
// ---------------------------------------------------------------------------

#[test]
fn test_detail_not_rejected_on_neighbors() {
    let p = GraphParams {
        mode: "neighbors".to_string(),
        detail: Some("summary".to_string()),
        ..Default::default()
    };
    assert!(super::super::validate_no_unsupported_params(&p).is_ok());
}

#[test]
fn test_detail_not_rejected_on_path() {
    let p = GraphParams {
        mode: "path".to_string(),
        detail: Some("full".to_string()),
        ..Default::default()
    };
    assert!(super::super::validate_no_unsupported_params(&p).is_ok());
}
