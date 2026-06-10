//! Unit tests for `GraphPenaltyParams` + `graph_penalty_with` (nan-018, engine-penalty).
//!
//! Split into a separate file to keep `graph.rs` and `graph_tests.rs` within the
//! 500-line module limit. All tests use `super::*` to reach the graph module items.
//!
//! Coverage (test-plan/engine-penalty.md):
//! - R-01 / NFR-01: bit-for-bit default equivalence — for EVERY status-shape branch
//!   AND the clamp, `graph_penalty(..) == graph_penalty_with(.., &Default::default())
//!   == the named const`.
//! - R-02: `GraphPenaltyParams::default()` fields equal the named consts.
//! - Clamp coupling (ADR-001, R-13): the hop-decay ceiling tracks the swept
//!   `params.clean_replacement`, not the const; depth-2 <= depth-1 monotonicity;
//!   the `0.10` floor stays a literal.
//! - R-13: severity scaling changes output; shape params (`hop_decay`,
//!   `max_traversal_depth`) behave as shape; `max_traversal_depth` below the deepest
//!   chain truncates without panic.
//! - Wrapper integrity + guard preservation.

use unimatrix_core::{EntryRecord, Status};

use super::*;

/// Minimal `EntryRecord` with the topology fields that drive `graph_penalty`.
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

// -- Fixtures: one per status-shape branch (chain direction pred_id -> entry.id) --

/// Orphan: Deprecated, no outgoing Supersedes edges. Query node 1.
fn fixture_orphan() -> (TypedRelationGraph, Vec<EntryRecord>) {
    let entries = vec![make_entry(1, Status::Deprecated, None, None)];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    (g, entries)
}

/// Dead-end: node 1 has a successor but it is Deprecated (no active reachable). Query 1.
fn fixture_dead_end() -> (TypedRelationGraph, Vec<EntryRecord>) {
    let entries = vec![
        make_entry(1, Status::Active, None, Some(2)),
        make_entry(2, Status::Deprecated, Some(1), None),
    ];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    (g, entries)
}

/// Partial supersession: node 1 has two active successors. Query 1.
fn fixture_partial() -> (TypedRelationGraph, Vec<EntryRecord>) {
    let entries = vec![
        make_entry(1, Status::Active, None, None),
        make_entry(2, Status::Active, Some(1), None),
        make_entry(3, Status::Active, Some(1), None),
    ];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    (g, entries)
}

/// Clean replacement at depth 1: 1 -> 2 (active head). Query 1.
fn fixture_clean_depth1() -> (TypedRelationGraph, Vec<EntryRecord>) {
    let entries = vec![
        make_entry(1, Status::Active, None, Some(2)),
        make_entry(2, Status::Active, Some(1), None),
    ];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    (g, entries)
}

/// Hop-decay depth >= 2: 1 -> 2 -> 3 (active head). Query 1 (depth 2).
fn fixture_chain_depth2() -> (TypedRelationGraph, Vec<EntryRecord>) {
    let entries = vec![
        make_entry(1, Status::Active, None, Some(2)),
        make_entry(2, Status::Active, Some(1), Some(3)),
        make_entry(3, Status::Active, Some(2), None),
    ];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    (g, entries)
}

/// Deep chain (depth 5) — the const path clamps to the 0.10 floor. Query 1.
fn fixture_chain_depth5() -> (TypedRelationGraph, Vec<EntryRecord>) {
    let entries = vec![
        make_entry(1, Status::Active, None, Some(2)),
        make_entry(2, Status::Active, Some(1), Some(3)),
        make_entry(3, Status::Active, Some(2), Some(4)),
        make_entry(4, Status::Active, Some(3), Some(5)),
        make_entry(5, Status::Active, Some(4), Some(6)),
        make_entry(6, Status::Active, Some(5), None),
    ];
    let g = build_typed_relation_graph(&entries, &[]).unwrap();
    (g, entries)
}

// =====================================================================================
// R-01 / NFR-01 — default-equivalence: graph_penalty == graph_penalty_with(Default)
//                  == named const, enumerated per status-shape branch.
// =====================================================================================

#[test]
fn test_graph_penalty_with_default_equals_graph_penalty_orphan() {
    let (g, entries) = fixture_orphan();
    let dflt = GraphPenaltyParams::default();
    assert_eq!(
        graph_penalty_with(1, &g, &entries, &dflt),
        graph_penalty(1, &g, &entries)
    );
}

#[test]
fn test_graph_penalty_with_default_equals_named_const_orphan() {
    let (g, entries) = fixture_orphan();
    let dflt = GraphPenaltyParams::default();
    assert_eq!(graph_penalty_with(1, &g, &entries, &dflt), ORPHAN_PENALTY);
    assert_eq!(graph_penalty(1, &g, &entries), 0.75);
}

#[test]
fn test_graph_penalty_with_default_equals_graph_penalty_dead_end() {
    let (g, entries) = fixture_dead_end();
    let dflt = GraphPenaltyParams::default();
    assert_eq!(
        graph_penalty_with(1, &g, &entries, &dflt),
        graph_penalty(1, &g, &entries)
    );
}

#[test]
fn test_graph_penalty_with_default_equals_named_const_dead_end() {
    let (g, entries) = fixture_dead_end();
    let dflt = GraphPenaltyParams::default();
    assert_eq!(graph_penalty_with(1, &g, &entries, &dflt), DEAD_END_PENALTY);
    assert_eq!(graph_penalty(1, &g, &entries), 0.65);
}

#[test]
fn test_graph_penalty_with_default_equals_graph_penalty_partial() {
    let (g, entries) = fixture_partial();
    let dflt = GraphPenaltyParams::default();
    assert_eq!(
        graph_penalty_with(1, &g, &entries, &dflt),
        graph_penalty(1, &g, &entries)
    );
}

#[test]
fn test_graph_penalty_with_default_equals_named_const_partial() {
    let (g, entries) = fixture_partial();
    let dflt = GraphPenaltyParams::default();
    assert_eq!(
        graph_penalty_with(1, &g, &entries, &dflt),
        PARTIAL_SUPERSESSION_PENALTY
    );
    assert_eq!(graph_penalty(1, &g, &entries), 0.60);
}

#[test]
fn test_graph_penalty_with_default_equals_graph_penalty_clean_depth1() {
    let (g, entries) = fixture_clean_depth1();
    let dflt = GraphPenaltyParams::default();
    assert_eq!(
        graph_penalty_with(1, &g, &entries, &dflt),
        graph_penalty(1, &g, &entries)
    );
}

#[test]
fn test_graph_penalty_with_default_equals_named_const_clean_depth1() {
    let (g, entries) = fixture_clean_depth1();
    let dflt = GraphPenaltyParams::default();
    assert_eq!(
        graph_penalty_with(1, &g, &entries, &dflt),
        CLEAN_REPLACEMENT_PENALTY
    );
    assert_eq!(graph_penalty(1, &g, &entries), 0.40);
}

#[test]
fn test_graph_penalty_with_default_equals_graph_penalty_hop_decay_depth2() {
    let (g, entries) = fixture_chain_depth2();
    let dflt = GraphPenaltyParams::default();
    assert_eq!(
        graph_penalty_with(1, &g, &entries, &dflt),
        graph_penalty(1, &g, &entries)
    );
}

#[test]
fn test_graph_penalty_with_default_equals_formula_hop_decay_depth2() {
    let (g, entries) = fixture_chain_depth2();
    let dflt = GraphPenaltyParams::default();
    // depth-2 = 0.40 * 0.60 = 0.24, inside [0.10, 0.40] so unclamped.
    let expected = CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR;
    assert!((graph_penalty_with(1, &g, &entries, &dflt) - expected).abs() < 1e-12);
    assert!((graph_penalty(1, &g, &entries) - 0.24).abs() < 1e-12);
}

#[test]
fn test_graph_penalty_with_default_equals_graph_penalty_clamp_floor() {
    // Deep chain whose raw decay drops below the 0.10 floor; clamp engages on BOTH paths.
    let (g, entries) = fixture_chain_depth5();
    let dflt = GraphPenaltyParams::default();
    assert_eq!(
        graph_penalty_with(1, &g, &entries, &dflt),
        graph_penalty(1, &g, &entries)
    );
    assert!((graph_penalty_with(1, &g, &entries, &dflt) - 0.10).abs() < 1e-12);
}

// -- Guard preservation: node-not-in-graph and entry-not-found still return 1.0 --

#[test]
fn test_graph_penalty_with_default_guard_node_absent_returns_one() {
    let (g, entries) = fixture_clean_depth1();
    let dflt = GraphPenaltyParams::default();
    assert_eq!(graph_penalty_with(9999, &g, &entries, &dflt), 1.0);
    assert_eq!(graph_penalty(9999, &g, &entries), 1.0);
}

#[test]
fn test_graph_penalty_with_default_guard_entry_absent_returns_one() {
    let (g, _entries) = fixture_clean_depth1();
    let dflt = GraphPenaltyParams::default();
    // Graph has node 1, but the entries slice is empty → entry_by_id miss → 1.0.
    assert_eq!(graph_penalty_with(1, &g, &[], &dflt), 1.0);
    assert_eq!(graph_penalty(1, &g, &[]), 1.0);
}

// =====================================================================================
// R-02 — GraphPenaltyParams::default() triangulates to the named consts (literal values).
// =====================================================================================

#[test]
fn test_graph_penalty_params_default_references_consts() {
    let d = GraphPenaltyParams::default();
    assert_eq!(d.orphan, ORPHAN_PENALTY);
    assert_eq!(d.clean_replacement, CLEAN_REPLACEMENT_PENALTY);
    assert_eq!(d.hop_decay, HOP_DECAY_FACTOR);
    assert_eq!(d.partial_supersession, PARTIAL_SUPERSESSION_PENALTY);
    assert_eq!(d.dead_end, DEAD_END_PENALTY);
    assert_eq!(d.fallback, FALLBACK_PENALTY);
    assert_eq!(d.max_traversal_depth, MAX_TRAVERSAL_DEPTH);
}

#[test]
fn test_graph_penalty_params_default_literal_values() {
    // Assert literal values (#3548) — not "some default".
    let d = GraphPenaltyParams::default();
    assert_eq!(d.orphan, 0.75);
    assert_eq!(d.clean_replacement, 0.40);
    assert_eq!(d.hop_decay, 0.60);
    assert_eq!(d.partial_supersession, 0.60);
    assert_eq!(d.dead_end, 0.65);
    assert_eq!(d.fallback, 0.70);
    assert_eq!(d.max_traversal_depth, 10);
}

// =====================================================================================
// Clamp coupling (ADR-001, R-13) — ceiling tracks params.clean_replacement, not const.
// =====================================================================================

#[test]
fn test_graph_penalty_with_clamp_ceiling_tracks_swept_clean_replacement() {
    // Swept clean_replacement = 0.25. A depth-2 raw = 0.25 * 0.60 = 0.15, inside
    // [0.10, 0.25] → unclamped 0.15, and crucially <= the swept ceiling 0.25 (NOT 0.40).
    let (g, entries) = fixture_chain_depth2();
    let params = GraphPenaltyParams {
        clean_replacement: 0.25,
        ..GraphPenaltyParams::default()
    };
    let p = graph_penalty_with(1, &g, &entries, &params);
    assert!(
        p <= 0.25,
        "depth-2 must clamp to the swept ceiling 0.25, got {p}"
    );
    assert!(
        (p - 0.15).abs() < 1e-12,
        "depth-2 = 0.25*0.60 = 0.15, got {p}"
    );
}

#[test]
fn test_graph_penalty_with_clamp_ceiling_engages_when_decay_above_ceiling() {
    // If a hypothetical raw exceeded the swept ceiling it must be clamped DOWN to it.
    // Force raw > ceiling by hop_decay = 1.0 (no decay): raw = clean_replacement * 1 =
    // clean_replacement, exactly the ceiling — the equality boundary of the clamp.
    let (g, entries) = fixture_chain_depth2();
    let params = GraphPenaltyParams {
        clean_replacement: 0.25,
        hop_decay: 1.0,
        ..GraphPenaltyParams::default()
    };
    let p = graph_penalty_with(1, &g, &entries, &params);
    assert!(
        (p - 0.25).abs() < 1e-12,
        "raw==ceiling clamps to 0.25, got {p}"
    );
}

#[test]
fn test_graph_penalty_with_depth2_le_depth1_monotonicity() {
    // For any clean_replacement, a depth-2 penalty must be <= the depth-1 penalty.
    let (g2, e2) = fixture_chain_depth2();
    let (g1, e1) = fixture_clean_depth1();
    for cr in [0.10, 0.25, 0.40, 0.60, 0.90] {
        let params = GraphPenaltyParams {
            clean_replacement: cr,
            ..GraphPenaltyParams::default()
        };
        let depth1 = graph_penalty_with(1, &g1, &e1, &params);
        let depth2 = graph_penalty_with(1, &g2, &e2, &params);
        assert!(
            depth2 <= depth1 + 1e-12,
            "clean_replacement={cr}: depth2 ({depth2}) must be <= depth1 ({depth1})"
        );
    }
}

#[test]
fn test_graph_penalty_with_clamp_lower_bound_literal() {
    // The lower bound is a literal 0.10, NOT coupled to clean_replacement. With
    // clean_replacement = 0.15 (above the floor), the depth-2 raw = 0.15*0.60 = 0.09 is
    // BELOW the floor and must be clamped UP to exactly 0.10 — proving the floor stays a
    // fixed literal and does not scale down with the swept base.
    let (g, entries) = fixture_chain_depth2();
    let params = GraphPenaltyParams {
        clean_replacement: 0.15,
        ..GraphPenaltyParams::default()
    };
    let p = graph_penalty_with(1, &g, &entries, &params);
    assert!(
        (p - 0.10).abs() < 1e-12,
        "literal 0.10 floor must hold, got {p}"
    );
}

#[test]
fn test_graph_penalty_with_clean_replacement_below_floor_no_panic() {
    // Extreme: clean_replacement swept BELOW the 0.10 literal floor would make
    // clamp(0.10, clean_replacement) violate min<=max. The fn must NOT panic; the
    // sub-floor ceiling dominates (the base penalty is already below the floor).
    let (g, entries) = fixture_chain_depth2();
    let params = GraphPenaltyParams {
        clean_replacement: 0.05,
        ..GraphPenaltyParams::default()
    };
    let p = graph_penalty_with(1, &g, &entries, &params);
    assert!(
        (p - 0.05).abs() < 1e-12,
        "sub-floor ceiling dominates, got {p}"
    );
}

// =====================================================================================
// R-13 — severity scaling vs shape params.
// =====================================================================================

#[test]
fn test_graph_penalty_with_scaled_severities_change_output() {
    // Scaling the severities (as the multiplier would) moves orphan/clean/partial/dead_end.
    let m = 0.5;
    let scaled = GraphPenaltyParams {
        orphan: ORPHAN_PENALTY * m,
        clean_replacement: CLEAN_REPLACEMENT_PENALTY * m,
        partial_supersession: PARTIAL_SUPERSESSION_PENALTY * m,
        dead_end: DEAD_END_PENALTY * m,
        fallback: FALLBACK_PENALTY * m,
        // shape params unchanged
        hop_decay: HOP_DECAY_FACTOR,
        max_traversal_depth: MAX_TRAVERSAL_DEPTH,
    };

    let (go, eo) = fixture_orphan();
    assert!((graph_penalty_with(1, &go, &eo, &scaled) - 0.375).abs() < 1e-12);

    let (gc, ec) = fixture_clean_depth1();
    assert!((graph_penalty_with(1, &gc, &ec, &scaled) - 0.20).abs() < 1e-12);

    let (gp, ep) = fixture_partial();
    assert!((graph_penalty_with(1, &gp, &ep, &scaled) - 0.30).abs() < 1e-12);

    let (gd, ed) = fixture_dead_end();
    assert!((graph_penalty_with(1, &gd, &ed, &scaled) - 0.325).abs() < 1e-12);
}

#[test]
fn test_graph_penalty_with_max_depth_truncates_not_panics() {
    // Deepest chain is depth 5 (node 1 -> ... -> active head at hop 5). Set
    // max_traversal_depth = 1 so the active head is unreachable within the cap.
    // Result must be a DEFINED penalty (dead-end), never a panic.
    let (g, entries) = fixture_chain_depth5();
    let params = GraphPenaltyParams {
        max_traversal_depth: 1,
        ..GraphPenaltyParams::default()
    };
    let p = graph_penalty_with(1, &g, &entries, &params);
    assert_eq!(
        p, DEAD_END_PENALTY,
        "truncated traversal → dead-end, got {p}"
    );
}

#[test]
fn test_graph_penalty_with_max_depth_zero_does_not_panic() {
    // Degenerate cap (0) — must still return a defined value, no panic / no overflow.
    let (g, entries) = fixture_chain_depth2();
    let params = GraphPenaltyParams {
        max_traversal_depth: 0,
        ..GraphPenaltyParams::default()
    };
    let p = graph_penalty_with(1, &g, &entries, &params);
    assert!(
        (0.0..=1.0).contains(&p),
        "defined penalty in (0,1], got {p}"
    );
}

// =====================================================================================
// Wrapper integrity + Copy semantics.
// =====================================================================================

#[test]
fn test_graph_penalty_is_thin_wrapper() {
    // Across every shape, graph_penalty(..) is observably identical to
    // graph_penalty_with(.., &Default::default()).
    let cases: Vec<(TypedRelationGraph, Vec<EntryRecord>)> = vec![
        fixture_orphan(),
        fixture_dead_end(),
        fixture_partial(),
        fixture_clean_depth1(),
        fixture_chain_depth2(),
        fixture_chain_depth5(),
    ];
    let dflt = GraphPenaltyParams::default();
    for (g, entries) in &cases {
        assert_eq!(
            graph_penalty(1, g, entries),
            graph_penalty_with(1, g, entries, &dflt)
        );
    }
}

#[test]
fn test_graph_penalty_params_is_copy() {
    // Copy + PartialEq: using the value after a copy must compile and compare equal.
    let a = GraphPenaltyParams::default();
    let b = a; // Copy, not move
    assert_eq!(a, b);
    assert_eq!(a.orphan, b.orphan);
}
