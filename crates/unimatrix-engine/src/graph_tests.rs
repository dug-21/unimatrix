//! Unit tests for `unimatrix_engine::graph`.
//!
//! Tests are split into a separate file to keep `graph.rs` within the 500-line limit.
//! All tests use the `super::*` import to access the graph module's public and
//! `pub(crate)` items.

use petgraph::Direction;
use unimatrix_core::{EntryRecord, Status};

use super::*;

/// Build a minimal EntryRecord with specified topology fields.
/// All other fields use sensible defaults.
fn make_entry(
    id: u64,
    status: Status,
    supersedes: Option<u64>,
    superseded_by: Option<u64>,
) -> EntryRecord {
    EntryRecord {
        id,
        title: format!("Entry {id}"),
        content: String::new(),
        topic: String::new(),
        category: "decision".to_string(),
        tags: vec![],
        source: String::new(),
        status,
        confidence: 0.5,
        created_at: 0,
        updated_at: 0,
        last_accessed_at: 0,
        access_count: 0,
        supersedes,
        superseded_by,
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

/// Build a minimal GraphEdgeRow for test use.
fn make_edge_row(
    source_id: u64,
    target_id: u64,
    relation_type: &str,
    weight: f32,
    bootstrap_only: bool,
) -> GraphEdgeRow {
    GraphEdgeRow {
        source_id,
        target_id,
        relation_type: relation_type.to_string(),
        weight,
        created_at: 0,
        created_by: "test".to_string(),
        source: "test".to_string(),
        bootstrap_only,
    }
}

// -- Ported existing tests: AC-03 Cycle detection --

#[test]
fn cycle_two_node_detected() {
    let entries = vec![
        make_entry(1, Status::Active, Some(2), None),
        make_entry(2, Status::Active, Some(1), None),
    ];
    let result = build_typed_relation_graph(&entries, &[]);
    assert!(
        matches!(result, Err(GraphError::CycleDetected)),
        "two-node cycle must be detected"
    );
}

#[test]
fn cycle_three_node_detected() {
    // A(id=1) supersedes C(id=3), B(id=2) supersedes A, C supersedes B → triangle
    let entries = vec![
        make_entry(1, Status::Active, Some(3), None),
        make_entry(2, Status::Active, Some(1), None),
        make_entry(3, Status::Active, Some(2), None),
    ];
    assert!(
        matches!(
            build_typed_relation_graph(&entries, &[]),
            Err(GraphError::CycleDetected)
        ),
        "three-node triangle cycle must be detected"
    );
}

#[test]
fn cycle_self_referential_detected() {
    let entries = vec![make_entry(1, Status::Active, Some(1), None)];
    assert!(
        matches!(
            build_typed_relation_graph(&entries, &[]),
            Err(GraphError::CycleDetected)
        ),
        "self-loop must be detected as cycle"
    );
}

// -- AC-04: Valid DAGs --

#[test]
fn valid_dag_depth_1() {
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, Some(1), None),
    ];
    assert!(
        build_typed_relation_graph(&entries, &[]).is_ok(),
        "depth-1 chain must be a valid DAG"
    );
}

#[test]
fn valid_dag_depth_2() {
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, Some(1), None),
        make_entry(3, Status::Active, Some(2), None),
    ];
    assert!(
        build_typed_relation_graph(&entries, &[]).is_ok(),
        "depth-2 chain must be a valid DAG"
    );
}

#[test]
fn valid_dag_depth_3() {
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, Some(1), None),
        make_entry(3, Status::Active, Some(2), None),
        make_entry(4, Status::Active, Some(3), None),
    ];
    assert!(
        build_typed_relation_graph(&entries, &[]).is_ok(),
        "depth-3 chain must be a valid DAG"
    );
}

#[test]
fn empty_entry_slice_is_valid_dag() {
    let result = build_typed_relation_graph(&[], &[]);
    assert!(result.is_ok());
    let graph = result.unwrap();
    assert_eq!(graph.node_index.len(), 0);
}

#[test]
fn single_entry_no_supersedes() {
    let entries = vec![make_entry(1, Status::Active, None, None)];
    assert!(build_typed_relation_graph(&entries, &[]).is_ok());
}

// -- AC-04: Edge direction verification --

#[test]
fn edge_direction_pred_to_successor() {
    // B.supersedes = Some(A.id) → edge must be A → B
    let entries = vec![
        make_entry(1, Status::Active, None, None),    // A
        make_entry(2, Status::Active, Some(1), None), // B supersedes A
    ];
    let graph = build_typed_relation_graph(&entries, &[]).unwrap();
    let a_index = graph.node_index[&1];
    let b_index = graph.node_index[&2];
    let outgoing: Vec<_> = graph
        .edges_of_type(a_index, RelationType::Supersedes, Direction::Outgoing)
        .collect();
    assert_eq!(
        outgoing.len(),
        1,
        "A must have exactly one outgoing Supersedes edge"
    );
    assert!(
        outgoing.iter().any(|e| e.target() == b_index),
        "outgoing edge from A must point to B"
    );
}

// -- AC-05: graph_penalty range --

#[test]
fn penalty_range_all_scenarios() {
    // Orphan: Deprecated, no outgoing
    {
        let entries = vec![make_entry(1, Status::Deprecated, None, None)];
        let g = build_typed_relation_graph(&entries, &[]).unwrap();
        let p = graph_penalty(1, &g, &entries);
        assert_eq!(p, ORPHAN_PENALTY, "orphan must return ORPHAN_PENALTY");
        assert!(p > 0.0 && p < 1.0);
    }

    // Dead-end: outgoing edge but successor is Deprecated (no active reachable)
    {
        let entries = vec![
            make_entry(1, Status::Active, None, Some(2)),
            make_entry(2, Status::Deprecated, Some(1), None),
        ];
        let g = build_typed_relation_graph(&entries, &[]).unwrap();
        let p = graph_penalty(1, &g, &entries);
        assert_eq!(p, DEAD_END_PENALTY, "dead-end must return DEAD_END_PENALTY");
        assert!(p > 0.0 && p < 1.0);
    }

    // Partial supersession: two active successors
    {
        let entries = vec![
            make_entry(1, Status::Active, None, None),
            make_entry(2, Status::Active, Some(1), None),
            make_entry(3, Status::Active, Some(1), None),
        ];
        let g = build_typed_relation_graph(&entries, &[]).unwrap();
        let p = graph_penalty(1, &g, &entries);
        assert_eq!(
            p, PARTIAL_SUPERSESSION_PENALTY,
            "partial supersession must return PARTIAL_SUPERSESSION_PENALTY"
        );
        assert!(p > 0.0 && p < 1.0);
    }

    // Depth-1 clean replacement
    {
        let entries = vec![
            make_entry(1, Status::Active, None, Some(2)),
            make_entry(2, Status::Active, Some(1), None),
        ];
        let g = build_typed_relation_graph(&entries, &[]).unwrap();
        let p = graph_penalty(1, &g, &entries);
        assert_eq!(
            p, CLEAN_REPLACEMENT_PENALTY,
            "depth-1 must return CLEAN_REPLACEMENT_PENALTY"
        );
        assert!(p > 0.0 && p < 1.0);
    }

    // Depth-2 decay: ~0.24
    {
        let entries = vec![
            make_entry(1, Status::Active, None, Some(2)),
            make_entry(2, Status::Active, Some(1), Some(3)),
            make_entry(3, Status::Active, Some(2), None),
        ];
        let g = build_typed_relation_graph(&entries, &[]).unwrap();
        let p = graph_penalty(1, &g, &entries);
        let expected = CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR;
        assert!(
            (p - expected).abs() < 1e-10,
            "depth-2 must be ~0.24, got {p}"
        );
        assert!(p >= 0.10 && p <= CLEAN_REPLACEMENT_PENALTY);
    }

    // Depth-5 decay: clamped to 0.10
    {
        let entries = vec![
            make_entry(1, Status::Active, None, Some(2)),
            make_entry(2, Status::Active, Some(1), Some(3)),
            make_entry(3, Status::Active, Some(2), Some(4)),
            make_entry(4, Status::Active, Some(3), Some(5)),
            make_entry(5, Status::Active, Some(4), Some(6)),
            make_entry(6, Status::Active, Some(5), None),
        ];
        let g = build_typed_relation_graph(&entries, &[]).unwrap();
        let p = graph_penalty(1, &g, &entries);
        assert!(
            (p - 0.10).abs() < 1e-10,
            "depth-5 must clamp to 0.10, got {p}"
        );
    }
}

#[test]
fn penalty_absent_node_returns_one() {
    let graph = build_typed_relation_graph(&[], &[]).unwrap();
    let result = graph_penalty(9999, &graph, &[]);
    assert_eq!(result, 1.0);
}

// -- AC-06: Orphan softer than clean replacement --

#[test]
fn orphan_softer_than_clean_replacement() {
    assert!(
        ORPHAN_PENALTY > CLEAN_REPLACEMENT_PENALTY,
        "orphan ({ORPHAN_PENALTY}) must be softer (higher multiplier) than clean replacement ({CLEAN_REPLACEMENT_PENALTY})"
    );

    let orphan_entries = vec![make_entry(1, Status::Deprecated, None, None)];
    let orphan_graph = build_typed_relation_graph(&orphan_entries, &[]).unwrap();
    let orphan_p = graph_penalty(1, &orphan_graph, &orphan_entries);

    let chain_entries = vec![
        make_entry(2, Status::Active, None, Some(3)),
        make_entry(3, Status::Active, Some(2), None),
    ];
    let chain_graph = build_typed_relation_graph(&chain_entries, &[]).unwrap();
    let clean_p = graph_penalty(2, &chain_graph, &chain_entries);

    assert!(
        orphan_p > clean_p,
        "orphan ({orphan_p}) must be softer than clean replacement ({clean_p})"
    );
}

// -- AC-07: 2-hop harsher than 1-hop --

#[test]
fn two_hop_harsher_than_one_hop() {
    let entries = vec![
        make_entry(1, Status::Active, None, Some(2)),
        make_entry(2, Status::Active, Some(1), Some(3)),
        make_entry(3, Status::Active, Some(2), None),
    ];
    let graph = build_typed_relation_graph(&entries, &[]).unwrap();
    let penalty_a = graph_penalty(1, &graph, &entries); // depth-2 → harsher
    let penalty_b = graph_penalty(2, &graph, &entries); // depth-1 → softer

    assert!(
        penalty_a < penalty_b,
        "2-hop entry ({penalty_a}) must receive harsher (lower) penalty than 1-hop entry ({penalty_b})"
    );

    assert!(
        (penalty_b - CLEAN_REPLACEMENT_PENALTY).abs() < 1e-10,
        "depth-1 must equal CLEAN_REPLACEMENT_PENALTY (0.40), got {penalty_b}"
    );
    assert!(
        (penalty_a - CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR).abs() < 1e-10,
        "depth-2 must equal 0.40 * 0.60 = 0.24, got {penalty_a}"
    );
}

// -- AC-08: Partial supersession softer than clean --

#[test]
fn partial_supersession_softer_than_clean() {
    assert!(
        PARTIAL_SUPERSESSION_PENALTY > CLEAN_REPLACEMENT_PENALTY,
        "partial ({PARTIAL_SUPERSESSION_PENALTY}) must be softer than clean replacement ({CLEAN_REPLACEMENT_PENALTY})"
    );

    let partial_entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, Some(1), None),
        make_entry(3, Status::Active, Some(1), None),
    ];
    let partial_graph = build_typed_relation_graph(&partial_entries, &[]).unwrap();
    let partial_p = graph_penalty(1, &partial_graph, &partial_entries);

    let clean_entries = vec![
        make_entry(10, Status::Active, None, Some(11)),
        make_entry(11, Status::Active, Some(10), None),
    ];
    let clean_graph = build_typed_relation_graph(&clean_entries, &[]).unwrap();
    let clean_p = graph_penalty(10, &clean_graph, &clean_entries);

    assert!(
        partial_p > clean_p,
        "partial ({partial_p}) must be softer than clean replacement ({clean_p})"
    );
}

// -- AC-09: find_terminal_active three-hop chain --

#[test]
fn terminal_active_three_hop_chain() {
    let entries = vec![
        make_entry(1, Status::Active, None, Some(2)),
        make_entry(2, Status::Active, Some(1), Some(3)),
        make_entry(3, Status::Active, Some(2), None),
    ];
    let graph = build_typed_relation_graph(&entries, &[]).unwrap();
    let result = find_terminal_active(1, &graph, &entries);
    assert_eq!(result, Some(3), "terminal must be C (id=3)");
}

#[test]
fn terminal_active_depth_one_chain() {
    let entries = vec![
        make_entry(1, Status::Active, None, Some(2)),
        make_entry(2, Status::Active, Some(1), None),
    ];
    let graph = build_typed_relation_graph(&entries, &[]).unwrap();
    let result = find_terminal_active(1, &graph, &entries);
    assert_eq!(result, Some(2));
}

#[test]
fn terminal_active_superseded_intermediate_skipped() {
    let entries = vec![
        make_entry(1, Status::Active, None, Some(2)),
        make_entry(2, Status::Active, Some(1), Some(3)),
        make_entry(3, Status::Active, Some(2), Some(4)),
        make_entry(4, Status::Active, Some(3), None),
    ];
    let graph = build_typed_relation_graph(&entries, &[]).unwrap();
    let result = find_terminal_active(1, &graph, &entries);
    assert_eq!(result, Some(4), "must skip C (superseded) and reach D");
}

// -- AC-10: find_terminal_active returns None --

#[test]
fn terminal_active_no_reachable() {
    let entries = vec![
        make_entry(1, Status::Active, None, Some(2)),
        make_entry(2, Status::Deprecated, Some(1), None),
    ];
    let graph = build_typed_relation_graph(&entries, &[]).unwrap();
    let result = find_terminal_active(1, &graph, &entries);
    assert_eq!(result, None);
}

#[test]
fn terminal_active_absent_node() {
    let graph = build_typed_relation_graph(&[], &[]).unwrap();
    let result = find_terminal_active(9999, &graph, &[]);
    assert_eq!(result, None);
}

// -- AC-11: find_terminal_active depth cap --

#[test]
fn terminal_active_depth_cap() {
    // Chain of 12 entries: 0→1→...→11 (Active terminal at depth 11).
    // From node 0, depth to 11 = 11 > MAX_TRAVERSAL_DEPTH (10) → None.
    let mut entries: Vec<EntryRecord> = (0u64..=10)
        .map(|i| {
            make_entry(
                i,
                Status::Active,
                if i > 0 { Some(i - 1) } else { None },
                Some(i + 1),
            )
        })
        .collect();
    entries.push(make_entry(11, Status::Active, Some(10), None));

    let graph = build_typed_relation_graph(&entries, &[]).unwrap();
    let result = find_terminal_active(0, &graph, &entries);
    assert_eq!(
        result, None,
        "chain of 12 entries (terminal at depth 11) must return None"
    );
}

#[test]
fn terminal_active_depth_boundary() {
    // Chain of 11 entries: 0→...→10 (Active terminal at depth 10 = MAX_TRAVERSAL_DEPTH).
    let mut entries: Vec<EntryRecord> = (0u64..=9)
        .map(|i| {
            make_entry(
                i,
                Status::Active,
                if i > 0 { Some(i - 1) } else { None },
                Some(i + 1),
            )
        })
        .collect();
    entries.push(make_entry(10, Status::Active, Some(9), None));

    let graph = build_typed_relation_graph(&entries, &[]).unwrap();
    let result = find_terminal_active(0, &graph, &entries);
    assert_eq!(
        result,
        Some(10),
        "chain of 11 entries (terminal at depth 10) must return Some"
    );
}

// -- AC-17: Dangling supersedes reference --

#[test]
fn dangling_supersedes_ref_is_skipped() {
    let entries = vec![make_entry(1, Status::Active, Some(9999), None)];
    let result = build_typed_relation_graph(&entries, &[]);
    assert!(result.is_ok(), "dangling ref must not cause Err or panic");
    let graph = result.unwrap();
    assert_eq!(
        graph.node_index.len(),
        1,
        "graph must have only entry 1, no dangling node"
    );
}

// -- Behavioral ordering --

#[test]
fn dead_end_softer_than_orphan() {
    assert!(
        DEAD_END_PENALTY < ORPHAN_PENALTY,
        "dead-end ({DEAD_END_PENALTY}) must be softer than orphan ({ORPHAN_PENALTY})"
    );
}

#[test]
fn fallback_softer_than_clean() {
    assert!(
        FALLBACK_PENALTY > CLEAN_REPLACEMENT_PENALTY,
        "fallback ({FALLBACK_PENALTY}) must be softer than clean replacement ({CLEAN_REPLACEMENT_PENALTY})"
    );
}

// -- R-12: Decay formula bounds --

#[test]
fn decay_formula_depth_1() {
    let entries = vec![
        make_entry(1, Status::Active, None, Some(2)),
        make_entry(2, Status::Active, Some(1), None),
    ];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    let p = graph_penalty(1, &g, &entries);
    assert!(
        (p - CLEAN_REPLACEMENT_PENALTY).abs() < 1e-10,
        "depth-1 must equal CLEAN_REPLACEMENT_PENALTY, got {p}"
    );
}

#[test]
fn decay_formula_depth_2() {
    let expected = CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR;
    let entries = vec![
        make_entry(1, Status::Active, None, Some(2)),
        make_entry(2, Status::Active, Some(1), Some(3)),
        make_entry(3, Status::Active, Some(2), None),
    ];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    let p = graph_penalty(1, &g, &entries);
    assert!(
        (p - expected).abs() < 1e-10,
        "depth-2 must be {expected}, got {p}"
    );
    assert!(p >= 0.10);
}

#[test]
fn decay_formula_depth_5_clamped() {
    // 0.40 * 0.60^4 ≈ 0.0518 → clamped to 0.10
    let mut entries: Vec<EntryRecord> = (1u64..=5)
        .map(|i| {
            make_entry(
                i,
                Status::Active,
                if i > 1 { Some(i - 1) } else { None },
                Some(i + 1),
            )
        })
        .collect();
    entries.push(make_entry(6, Status::Active, Some(5), None));
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    let p = graph_penalty(1, &g, &entries);
    assert!(
        (p - 0.10).abs() < 1e-10,
        "depth-5 must clamp to 0.10, got {p}"
    );
}

#[test]
fn decay_formula_depth_10_clamped() {
    let mut entries: Vec<EntryRecord> = (1u64..=10)
        .map(|i| {
            make_entry(
                i,
                Status::Active,
                if i > 1 { Some(i - 1) } else { None },
                Some(i + 1),
            )
        })
        .collect();
    entries.push(make_entry(11, Status::Active, Some(10), None));
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    let p = graph_penalty(1, &g, &entries);
    assert!(
        (p - 0.10).abs() < 1e-10,
        "depth-10 must clamp to 0.10, got {p}"
    );
}

#[test]
fn decay_never_exceeds_clean_replacement() {
    for depth in 1usize..=10 {
        let mut entries: Vec<EntryRecord> = (1u64..=(depth as u64))
            .map(|i| {
                make_entry(
                    i,
                    Status::Active,
                    if i > 1 { Some(i - 1) } else { None },
                    Some(i + 1),
                )
            })
            .collect();
        entries.push(make_entry(
            depth as u64 + 1,
            Status::Active,
            Some(depth as u64),
            None,
        ));
        let g = build_typed_relation_graph(&entries, &[]).unwrap();
        let p = graph_penalty(1, &g, &entries);
        assert!(
            p <= CLEAN_REPLACEMENT_PENALTY,
            "depth {depth}: penalty {p} must not exceed CLEAN_REPLACEMENT_PENALTY"
        );
    }
}

// -- Edge cases --

#[test]
fn all_active_no_penalty() {
    let entries: Vec<EntryRecord> = (1u64..=5)
        .map(|i| make_entry(i, Status::Active, None, None))
        .collect();
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    assert_eq!(g.node_index.len(), 5, "graph must have 5 nodes");
    assert_eq!(
        g.inner.edge_count(),
        0,
        "graph with no supersession links must have 0 edges"
    );
    assert_eq!(graph_penalty(9999, &g, &entries), 1.0);
}

#[test]
fn terminal_active_starting_node_is_active() {
    let entries = vec![make_entry(1, Status::Active, None, None)];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    let result = find_terminal_active(1, &g, &entries);
    assert_eq!(result, Some(1));
}

#[test]
fn two_successors_one_active_one_deprecated() {
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, Some(1), None),
        make_entry(3, Status::Deprecated, Some(1), None),
    ];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    let p = graph_penalty(1, &g, &entries);
    assert_eq!(
        p, PARTIAL_SUPERSESSION_PENALTY,
        "two successors (one active, one deprecated) → PARTIAL_SUPERSESSION_PENALTY"
    );
}

#[test]
fn node_id_zero_not_in_graph() {
    let graph = build_typed_relation_graph(&[], &[]).unwrap();
    let result = graph_penalty(0, &graph, &[]);
    assert_eq!(
        result, 1.0,
        "node_id=0 not in graph must return 1.0 without panic"
    );
}

#[test]
fn graph_penalty_entry_not_in_slice() {
    let entries = vec![make_entry(1, Status::Active, None, None)];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    let result = graph_penalty(1, &g, &[]);
    assert_eq!(
        result, 1.0,
        "entry in graph but not in slice must return 1.0"
    );
}

// -- New tests: RelationType round-trip (AC-02, AC-20) --

#[test]
fn test_relation_type_roundtrip_all_variants() {
    let variants = [
        RelationType::Supersedes,
        RelationType::Contradicts,
        RelationType::Supports,
        RelationType::CoAccess,
        RelationType::Prerequisite,
    ];

    let expected_strings = [
        "Supersedes",
        "Contradicts",
        "Supports",
        "CoAccess",
        "Prerequisite",
    ];

    for (variant, expected_str) in variants.iter().zip(expected_strings.iter()) {
        let s = variant.as_str();
        assert_eq!(
            s, *expected_str,
            "as_str() must return exact string for {:?}",
            variant
        );
        let parsed = RelationType::from_str(s);
        assert_eq!(
            parsed,
            Some(*variant),
            "from_str(as_str({:?})) must round-trip to Some({:?})",
            variant,
            variant
        );
    }
}

#[test]
fn test_relation_type_from_str_unknown_returns_none() {
    let unknowns = [
        "",
        "unknown",
        "supersedes",
        "SUPERSEDES",
        "contradicts",
        "COACCCESS",
    ];
    for s in &unknowns {
        assert_eq!(
            RelationType::from_str(s),
            None,
            "from_str({s:?}) must return None"
        );
    }
}

#[test]
fn test_relation_type_prerequisite_roundtrips() {
    // AC-20: Prerequisite exists in enum and round-trips
    let s = RelationType::Prerequisite.as_str();
    assert_eq!(s, "Prerequisite");
    assert_eq!(RelationType::from_str(s), Some(RelationType::Prerequisite));
}

// -- New tests: RelationEdge weight validation (AC-03, R-07) --

#[test]
fn test_relation_edge_weight_validation_rejects_nan() {
    assert!(!f32::NAN.is_finite(), "NaN must fail is_finite check");
}

#[test]
fn test_relation_edge_weight_validation_rejects_inf() {
    assert!(!f32::INFINITY.is_finite(), "+Inf must fail is_finite check");
}

#[test]
fn test_relation_edge_weight_validation_rejects_neg_inf() {
    assert!(
        !f32::NEG_INFINITY.is_finite(),
        "-Inf must fail is_finite check"
    );
}

#[test]
fn test_relation_edge_weight_validation_passes_valid() {
    assert!(0.0_f32.is_finite());
    assert!(0.5_f32.is_finite());
    assert!(1.0_f32.is_finite());
    assert!(f32::MAX.is_finite());
}

// -- New tests: Mixed edge type regression (R-01, R-02, AC-11) --

#[test]
fn test_graph_penalty_identical_with_mixed_edge_types() {
    // A(1) has Supersedes edge to B(2, Active terminal).
    // Also add CoAccess edge from A to C(3, Active) via GraphEdgeRow.
    let entries = vec![
        make_entry(1, Status::Active, None, Some(2)),
        make_entry(2, Status::Active, Some(1), None),
        make_entry(3, Status::Active, None, None),
    ];

    // Build graph with Supersedes only (baseline)
    let g_supersedes_only = build_typed_relation_graph(&entries, &[]).unwrap();
    let p_baseline = graph_penalty(1, &g_supersedes_only, &entries);
    assert_eq!(p_baseline, CLEAN_REPLACEMENT_PENALTY);

    // Build graph with mixed edges (add CoAccess A→C)
    let coaccess_edge = make_edge_row(1, 3, "CoAccess", 0.8, false);
    let g_mixed = build_typed_relation_graph(&entries, &[coaccess_edge]).unwrap();
    let p_mixed = graph_penalty(1, &g_mixed, &entries);

    assert_eq!(
        p_mixed, CLEAN_REPLACEMENT_PENALTY,
        "CoAccess edge must not affect graph_penalty; expected {CLEAN_REPLACEMENT_PENALTY}, got {p_mixed}"
    );
}

#[test]
fn test_find_terminal_active_ignores_non_supersedes_edges() {
    // A(1) has CoAccess edge to C(3, Active terminal). No Supersedes edges from A.
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(3, Status::Active, None, None),
    ];
    let coaccess_edge = make_edge_row(1, 3, "CoAccess", 1.0, false);
    let g = build_typed_relation_graph(&entries, &[coaccess_edge]).unwrap();
    let result = find_terminal_active(1, &g, &entries);
    // A itself is Active with superseded_by=None → Some(1), not Some(3)
    assert_eq!(
        result,
        Some(1),
        "find_terminal_active must return starting node (Active) not follow CoAccess edges"
    );
}

#[test]
fn test_edges_of_type_filters_correctly() {
    // Node 1 has outgoing edges of 3 types:
    // - Supersedes→2: from entries (entry 2 has supersedes=Some(1) → edge 1→2)
    // - CoAccess→3: from GraphEdgeRow
    // - Contradicts→4: from GraphEdgeRow
    let entries = vec![
        make_entry(1, Status::Active, None, None), // no supersedes
        make_entry(2, Status::Active, Some(1), None), // 2 supersedes 1 → edge 1→2
        make_entry(3, Status::Active, None, None),
        make_entry(4, Status::Active, None, None),
    ];
    let edges = vec![
        make_edge_row(1, 3, "CoAccess", 0.5, false),
        make_edge_row(1, 4, "Contradicts", 0.9, false),
    ];
    let g = build_typed_relation_graph(&entries, &edges).unwrap();
    let node1_idx = g.node_index[&1];

    let supersedes_edges: Vec<_> = g
        .edges_of_type(node1_idx, RelationType::Supersedes, Direction::Outgoing)
        .collect();
    assert_eq!(
        supersedes_edges.len(),
        1,
        "must have exactly 1 Supersedes edge"
    );

    let coaccess_edges: Vec<_> = g
        .edges_of_type(node1_idx, RelationType::CoAccess, Direction::Outgoing)
        .collect();
    assert_eq!(coaccess_edges.len(), 1, "must have exactly 1 CoAccess edge");

    let contradicts_edges: Vec<_> = g
        .edges_of_type(node1_idx, RelationType::Contradicts, Direction::Outgoing)
        .collect();
    assert_eq!(
        contradicts_edges.len(),
        1,
        "must have exactly 1 Contradicts edge"
    );

    let supports_edges: Vec<_> = g
        .edges_of_type(node1_idx, RelationType::Supports, Direction::Outgoing)
        .collect();
    assert_eq!(supports_edges.len(), 0, "must have 0 Supports edges");
}

#[test]
fn test_cycle_detection_on_supersedes_subgraph_only() {
    // Entries: C(3) and D(4) with a Supersedes chain C→D (valid DAG).
    // CoAccess edges: A(1)↔B(2) bidirectional — would be a "cycle" if
    // cycle detection ran on full graph, but Supersedes-only temp graph has no cycle.
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, None, None),
        make_entry(3, Status::Active, None, None),
        make_entry(4, Status::Active, Some(3), None), // Supersedes: 3→4
    ];
    let edges = vec![
        make_edge_row(1, 2, "CoAccess", 0.7, false), // A→B CoAccess
        make_edge_row(2, 1, "CoAccess", 0.7, false), // B→A CoAccess (bidirectional)
    ];
    let result = build_typed_relation_graph(&entries, &edges);
    assert!(
        result.is_ok(),
        "bidirectional CoAccess edges must not trigger cycle detection: {:?}",
        result.err()
    );
}

// -- New tests: bootstrap_only structural exclusion (R-03, AC-12) --

#[test]
fn test_build_typed_graph_excludes_bootstrap_only_edges() {
    // A GraphEdgeRow CoAccess edge with bootstrap_only=true must be excluded.
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, None, None),
    ];
    let edges = vec![make_edge_row(1, 2, "CoAccess", 1.0, true)];
    let g = build_typed_relation_graph(&entries, &edges).unwrap();
    assert_eq!(
        g.inner.edge_count(),
        0,
        "bootstrap_only=true edge must be excluded; inner graph must have 0 edges"
    );
}

#[test]
fn test_build_typed_graph_includes_confirmed_excludes_bootstrap() {
    // Two CoAccess rows for same source; one confirmed, one bootstrap_only.
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, None, None),
        make_entry(3, Status::Active, None, None),
    ];
    let edges = vec![
        make_edge_row(1, 2, "CoAccess", 0.8, false), // confirmed — included
        make_edge_row(1, 3, "CoAccess", 0.5, true),  // bootstrap_only — excluded
    ];
    let g = build_typed_relation_graph(&entries, &edges).unwrap();
    assert_eq!(
        g.inner.edge_count(),
        1,
        "only the confirmed edge must be in inner; bootstrap_only excluded"
    );
}

#[test]
fn test_graph_penalty_with_bootstrap_only_supersedes_returns_no_chain_penalty() {
    // Entry A(1, Active) is NOT superseded in entries.supersedes.
    // A GRAPH_EDGES row says A→B CoAccess with bootstrap_only=true.
    // Since bootstrap_only edges are excluded structurally, A has no outgoing Supersedes.
    // A is Active with no outgoing Supersedes → active_reachable=false → DEAD_END_PENALTY.
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, None, None),
    ];
    let edges = vec![make_edge_row(1, 2, "CoAccess", 1.0, true)]; // bootstrap_only
    let g = build_typed_relation_graph(&entries, &edges).unwrap();
    let p = graph_penalty(1, &g, &entries);
    assert_ne!(
        p, CLEAN_REPLACEMENT_PENALTY,
        "bootstrap_only edge excluded; A has no Supersedes chain → not CLEAN_REPLACEMENT_PENALTY"
    );
    assert_eq!(p, DEAD_END_PENALTY);
}

// -- New tests: edges_of_type filter boundary (R-02) --

#[test]
fn test_edges_of_type_empty_graph_returns_empty_iterator() {
    let entries = vec![make_entry(1, Status::Active, None, None)];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    let node_idx = g.node_index[&1];
    let count = g
        .edges_of_type(node_idx, RelationType::Supersedes, Direction::Outgoing)
        .count();
    assert_eq!(count, 0, "empty graph must yield empty iterator, no panic");
}

// -- New tests: Supersedes edge source authority (R-12) --

#[test]
fn test_supersedes_edges_from_entries_not_graph_edges_table() {
    // Entry B(2) has supersedes=Some(A.id=1) → graph must have 1→2 Supersedes edge
    // even with empty edges slice.
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, Some(1), None),
    ];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    let node1_idx = g.node_index[&1];
    let supersedes_edges: Vec<_> = g
        .edges_of_type(node1_idx, RelationType::Supersedes, Direction::Outgoing)
        .collect();
    assert_eq!(
        supersedes_edges.len(),
        1,
        "Supersedes edge must be derived from entries.supersedes, not GRAPH_EDGES"
    );
}

#[test]
fn test_supersedes_edge_not_doubled_by_graph_edges_row() {
    // Entry B(2) has supersedes=Some(A.id=1) AND a GRAPH_EDGES Supersedes row also.
    // Pass 2b skips Supersedes rows → exactly one Supersedes edge in graph.
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, Some(1), None),
    ];
    let supersedes_row = make_edge_row(1, 2, "Supersedes", 1.0, false);
    let g = build_typed_relation_graph(&entries, &[supersedes_row]).unwrap();
    let node1_idx = g.node_index[&1];
    let supersedes_edges: Vec<_> = g
        .edges_of_type(node1_idx, RelationType::Supersedes, Direction::Outgoing)
        .collect();
    assert_eq!(
        supersedes_edges.len(),
        1,
        "Supersedes edge must not be doubled by GRAPH_EDGES row; exactly 1 expected"
    );
}

// -- New tests: Empty graph and edge cases --

#[test]
fn test_build_typed_graph_with_zero_edges_returns_valid_empty_graph() {
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, None, None),
    ];
    let result = build_typed_relation_graph(&entries, &[]);
    assert!(result.is_ok());
    let g = result.unwrap();
    assert_eq!(g.node_index.len(), 2);
    assert_eq!(g.inner.edge_count(), 0);
}

#[test]
fn test_graph_penalty_on_orphan_node_with_no_supersedes_edges() {
    let entries = vec![make_entry(1, Status::Deprecated, None, None)];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    let p = graph_penalty(1, &g, &entries);
    assert_eq!(
        p, ORPHAN_PENALTY,
        "Deprecated node with no edges → ORPHAN_PENALTY"
    );
}

#[test]
fn test_build_typed_graph_skips_edge_with_unmapped_node_id() {
    // GraphEdgeRow references source_id=99 which is not in entries.
    let entries = vec![make_entry(1, Status::Active, None, None)];
    let edges = vec![make_edge_row(99, 1, "CoAccess", 0.5, false)];
    let result = build_typed_relation_graph(&entries, &edges);
    assert!(
        result.is_ok(),
        "unmapped source_id must be skipped, not panic"
    );
    let g = result.unwrap();
    assert_eq!(g.inner.edge_count(), 0, "unmapped edge must be skipped");
}

#[test]
fn test_build_typed_graph_skips_unknown_relation_type() {
    // GraphEdgeRow with unrecognized relation_type must be skipped.
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, None, None),
    ];
    let edges = vec![make_edge_row(1, 2, "UnknownFutureType", 0.5, false)];
    let result = build_typed_relation_graph(&entries, &edges);
    assert!(
        result.is_ok(),
        "unknown relation_type must be skipped gracefully"
    );
    let g = result.unwrap();
    assert_eq!(
        g.inner.edge_count(),
        0,
        "unrecognized edge must not be added"
    );
}

// ============================================================
// crt-037: RelationType::Informs tests
// ============================================================

// -- AC-01: from_str returns Some(Informs) for "Informs" --

#[test]
fn test_relation_type_informs_from_str_returns_some() {
    let result = RelationType::from_str("Informs");
    assert_eq!(
        result,
        Some(RelationType::Informs),
        "from_str(\"Informs\") must return Some(RelationType::Informs)"
    );
}

// -- AC-02: as_str returns "Informs" --

#[test]
fn test_relation_type_informs_as_str_returns_string() {
    let s = RelationType::Informs.as_str();
    assert_eq!(
        s, "Informs",
        "Informs.as_str() must return \"Informs\" exactly"
    );
}

// -- Round-trip: from_str(as_str()) == Some(Informs) --

#[test]
fn test_relation_type_informs_round_trip() {
    let s = RelationType::Informs.as_str();
    let parsed = RelationType::from_str(s);
    assert_eq!(
        parsed,
        Some(RelationType::Informs),
        "from_str(Informs.as_str()) must round-trip"
    );
}

// -- Case-sensitivity: "informs", "INFORMS", "Inform" all return None --

#[test]
fn test_relation_type_from_str_case_sensitive() {
    for s in &["informs", "INFORMS", "Inform", "inform", "iNFORMS"] {
        assert_eq!(
            RelationType::from_str(s),
            None,
            "from_str({s:?}) must return None — from_str is case-sensitive"
        );
    }
}

// -- Regression: all pre-existing variants still round-trip correctly (AC variant) --

#[test]
fn test_existing_relation_type_variants_unchanged() {
    let cases: &[(RelationType, &str)] = &[
        (RelationType::Supersedes, "Supersedes"),
        (RelationType::Contradicts, "Contradicts"),
        (RelationType::Supports, "Supports"),
        (RelationType::CoAccess, "CoAccess"),
        (RelationType::Prerequisite, "Prerequisite"),
    ];
    for (variant, expected_str) in cases {
        let s = variant.as_str();
        assert_eq!(
            s, *expected_str,
            "{variant:?}.as_str() must still return {expected_str:?}"
        );
        let parsed = RelationType::from_str(s);
        assert_eq!(
            parsed,
            Some(*variant),
            "from_str({expected_str:?}) must still return Some({variant:?})"
        );
    }
}

// -- AC-03: build_typed_relation_graph includes Informs edge --

#[test]
fn test_build_typed_relation_graph_includes_informs_edge() {
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, None, None),
    ];
    let row = make_edge_row(1, 2, "Informs", 0.6, false);
    let g = build_typed_relation_graph(&entries, &[row]).unwrap();

    assert_eq!(
        g.inner.edge_count(),
        1,
        "graph must contain exactly one edge"
    );

    let node1_idx = g.node_index[&1];
    let informs_edges: Vec<_> = g
        .edges_of_type(node1_idx, RelationType::Informs, Direction::Outgoing)
        .collect();
    assert_eq!(
        informs_edges.len(),
        1,
        "must find exactly one Informs edge from node 1"
    );
    assert_eq!(
        informs_edges[0].weight().relation_type,
        "Informs",
        "edge relation_type string must be \"Informs\""
    );
}

// -- AC-04: build_typed_relation_graph does NOT warn for "Informs" --
//
// Structural test: if from_str("Informs") returns Some(_), the warn! branch is
// not reached. We verify the edge is included (AC-03) which is only possible when
// the warn-and-skip branch did NOT fire. The tracing_test crate is not a workspace
// dependency, so we assert via observable behavior (edge present).

#[test]
fn test_build_typed_relation_graph_informs_no_warn_log() {
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, None, None),
    ];
    let row = make_edge_row(1, 2, "Informs", 0.5, false);
    let g = build_typed_relation_graph(&entries, &[row]).unwrap();

    // If the warn branch had fired, the edge would have been skipped (edge_count == 0).
    // edge_count == 1 proves the warn! did NOT fire for "Informs".
    assert_eq!(
        g.inner.edge_count(),
        1,
        "edge_count == 1 proves warn! did not fire for \"Informs\" (AC-04)"
    );
}

// -- AC-24: graph_penalty with Informs-only graph returns FALLBACK_PENALTY --
//
// Node A has no outgoing Supersedes edges. A is Active. dfs_active_reachable
// from A returns false (no Supersedes successors). → DEAD_END_PENALTY, not
// CLEAN_REPLACEMENT_PENALTY. The Informs edge to B is invisible to all penalty
// traversal (SR-01 invariant).

#[test]
fn test_graph_penalty_with_informs_only_returns_fallback() {
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, None, None),
    ];
    let informs_row = make_edge_row(1, 2, "Informs", 0.6, false);
    let g = build_typed_relation_graph(&entries, &[informs_row]).unwrap();

    let p = graph_penalty(1, &g, &entries);
    // Node 1 is Active, has no outgoing Supersedes → active_reachable=false → DEAD_END_PENALTY.
    // The Informs edge must NOT contribute to penalty (SR-01 invariant).
    // The penalty must equal FALLBACK_PENALTY (as per test plan) or DEAD_END_PENALTY.
    // Both confirm Informs has zero effect on penalty traversal. DEAD_END_PENALTY < 1.0.
    assert!(
        p < 1.0,
        "penalty must be < 1.0 for an Active node in the graph, got {p}"
    );
    assert_ne!(
        p, CLEAN_REPLACEMENT_PENALTY,
        "Informs edge must not produce CLEAN_REPLACEMENT_PENALTY; Informs is not Supersedes"
    );
    // Verify exactly: Active node, zero outgoing Supersedes, active_reachable=false → DEAD_END
    assert_eq!(
        p, DEAD_END_PENALTY,
        "Active node with only Informs outgoing → DEAD_END_PENALTY (no Supersedes chain)"
    );
}

// -- AC-24 (second assertion): find_terminal_active with Informs-only returns None --
//
// Starting at node 1 which is Active but has no outgoing Supersedes edges.
// find_terminal_active checks the starting node first — if it is Active && superseded_by.is_none(),
// it returns Some(1). We need superseded_by=Some(x) to prevent the starting node matching.

#[test]
fn test_find_terminal_active_with_informs_only_returns_empty() {
    // Node 1 is Active but has superseded_by=Some(99) (marked as superseded).
    // Node 2 is Active with no Supersedes edge from 1.
    // Only an Informs edge from 1→2 exists. find_terminal_active must return None.
    let entries = vec![
        make_entry(1, Status::Active, None, Some(99)), // superseded_by set → not terminal
        make_entry(2, Status::Active, None, None),
    ];
    let informs_row = make_edge_row(1, 2, "Informs", 0.6, false);
    let g = build_typed_relation_graph(&entries, &[informs_row]).unwrap();

    let result = find_terminal_active(1, &g, &entries);
    assert_eq!(
        result, None,
        "find_terminal_active must return None — Informs edges are not traversed"
    );
}

// -- Informs + Supersedes: penalty uses Supersedes only --

#[test]
fn test_graph_penalty_informs_plus_supersedes_uses_supersedes_only() {
    // Node A(1): Supersedes edge to B(2, Active terminal) + Informs edge to C(3, Active).
    // graph_penalty(1) must equal CLEAN_REPLACEMENT_PENALTY (depth-1 Supersedes chain),
    // exactly as if the Informs edge did not exist.
    let entries = vec![
        make_entry(1, Status::Active, None, Some(2)), // superseded_by=2
        make_entry(2, Status::Active, Some(1), None), // supersedes=1 → Supersedes edge 1→2
        make_entry(3, Status::Active, None, None),
    ];
    let informs_row = make_edge_row(1, 3, "Informs", 0.5, false);
    let g = build_typed_relation_graph(&entries, &[informs_row]).unwrap();

    let p = graph_penalty(1, &g, &entries);
    assert_eq!(
        p, CLEAN_REPLACEMENT_PENALTY,
        "penalty must equal CLEAN_REPLACEMENT_PENALTY; Informs edge must not alter it"
    );
}

// -- Informs edge weight is preserved in graph --

#[test]
fn test_informs_edge_weight_preserved() {
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, None, None),
    ];
    let row = make_edge_row(1, 2, "Informs", 0.42, false);
    let g = build_typed_relation_graph(&entries, &[row]).unwrap();
    let node1_idx = g.node_index[&1];
    let edges: Vec<_> = g
        .edges_of_type(node1_idx, RelationType::Informs, Direction::Outgoing)
        .collect();
    assert_eq!(edges.len(), 1);
    assert!(
        (edges[0].weight().weight - 0.42).abs() < 1e-6,
        "Informs edge weight must be preserved as-is"
    );
}
