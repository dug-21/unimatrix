//! Unit tests for the `[graph_penalty]` config section (nan-018, Wave 1).
//!
//! Test plan: `product/features/nan-018/test-plan/penalty-config.md`.
//! Risks covered: R-02 (dual-default divergence), R-13 (multiplier overlay),
//! security (range validation), NFR-02 (empty-section default equivalence).

use std::path::Path;

use unimatrix_engine::graph::{
    CLEAN_REPLACEMENT_PENALTY, DEAD_END_PENALTY, FALLBACK_PENALTY, GraphPenaltyParams,
    HOP_DECAY_FACTOR, MAX_TRAVERSAL_DEPTH, ORPHAN_PENALTY, PARTIAL_SUPERSESSION_PENALTY,
};

use super::{
    ConfigError, GraphPenaltyConfig, UnimatrixConfig, default_clean_replacement, default_dead_end,
    default_fallback, default_hop_decay, default_max_traversal_depth, default_orphan,
    default_partial_supersession, validate_graph_penalty,
};

// ---------------------------------------------------------------------------
// R-02 — dual-default triangulation (AC-01)
// ---------------------------------------------------------------------------

#[test]
fn test_graph_penalty_config_dual_default_orphan_triangulates() {
    let cfg = GraphPenaltyConfig::default();
    let params = GraphPenaltyParams::default();
    assert_eq!(default_orphan(), ORPHAN_PENALTY);
    assert_eq!(cfg.orphan, ORPHAN_PENALTY);
    assert_eq!(params.orphan, ORPHAN_PENALTY);
    assert_eq!(ORPHAN_PENALTY, 0.75);
}

#[test]
fn test_graph_penalty_config_dual_default_clean_replacement_triangulates() {
    let cfg = GraphPenaltyConfig::default();
    let params = GraphPenaltyParams::default();
    assert_eq!(default_clean_replacement(), CLEAN_REPLACEMENT_PENALTY);
    assert_eq!(cfg.clean_replacement, CLEAN_REPLACEMENT_PENALTY);
    assert_eq!(params.clean_replacement, CLEAN_REPLACEMENT_PENALTY);
    assert_eq!(CLEAN_REPLACEMENT_PENALTY, 0.40);
}

#[test]
fn test_graph_penalty_config_dual_default_hop_decay_triangulates() {
    let cfg = GraphPenaltyConfig::default();
    let params = GraphPenaltyParams::default();
    assert_eq!(default_hop_decay(), HOP_DECAY_FACTOR);
    assert_eq!(cfg.hop_decay, HOP_DECAY_FACTOR);
    assert_eq!(params.hop_decay, HOP_DECAY_FACTOR);
    assert_eq!(HOP_DECAY_FACTOR, 0.60);
}

#[test]
fn test_graph_penalty_config_dual_default_partial_supersession_triangulates() {
    let cfg = GraphPenaltyConfig::default();
    let params = GraphPenaltyParams::default();
    assert_eq!(default_partial_supersession(), PARTIAL_SUPERSESSION_PENALTY);
    assert_eq!(cfg.partial_supersession, PARTIAL_SUPERSESSION_PENALTY);
    assert_eq!(params.partial_supersession, PARTIAL_SUPERSESSION_PENALTY);
    assert_eq!(PARTIAL_SUPERSESSION_PENALTY, 0.60);
}

#[test]
fn test_graph_penalty_config_dual_default_dead_end_triangulates() {
    let cfg = GraphPenaltyConfig::default();
    let params = GraphPenaltyParams::default();
    assert_eq!(default_dead_end(), DEAD_END_PENALTY);
    assert_eq!(cfg.dead_end, DEAD_END_PENALTY);
    assert_eq!(params.dead_end, DEAD_END_PENALTY);
    assert_eq!(DEAD_END_PENALTY, 0.65);
}

#[test]
fn test_graph_penalty_config_dual_default_fallback_triangulates() {
    let cfg = GraphPenaltyConfig::default();
    let params = GraphPenaltyParams::default();
    assert_eq!(default_fallback(), FALLBACK_PENALTY);
    assert_eq!(cfg.fallback, FALLBACK_PENALTY);
    assert_eq!(params.fallback, FALLBACK_PENALTY);
    assert_eq!(FALLBACK_PENALTY, 0.70);
}

#[test]
fn test_graph_penalty_config_dual_default_max_traversal_depth_triangulates() {
    let cfg = GraphPenaltyConfig::default();
    let params = GraphPenaltyParams::default();
    assert_eq!(default_max_traversal_depth(), MAX_TRAVERSAL_DEPTH);
    assert_eq!(cfg.max_traversal_depth, MAX_TRAVERSAL_DEPTH);
    assert_eq!(params.max_traversal_depth, MAX_TRAVERSAL_DEPTH);
    assert_eq!(MAX_TRAVERSAL_DEPTH, 10);
}

// ---------------------------------------------------------------------------
// Empty / omitted-section deserialization (NFR-02) — AC-01(a)
// ---------------------------------------------------------------------------

#[test]
fn test_config_omits_graph_penalty_section_deserializes_to_defaults() {
    let toml_str = "";
    let config: UnimatrixConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.graph_penalty, GraphPenaltyConfig::default());
    assert_eq!(
        config.graph_penalty.resolve_params(),
        GraphPenaltyParams::default()
    );
}

#[test]
fn test_config_empty_graph_penalty_table_deserializes_to_defaults() {
    let toml_str = "[graph_penalty]\n";
    let config: UnimatrixConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.graph_penalty, GraphPenaltyConfig::default());
    assert_eq!(
        config.graph_penalty.resolve_params(),
        GraphPenaltyParams::default()
    );
}

#[test]
fn test_config_partial_graph_penalty_section_fills_rest_from_default() {
    let toml_str = "[graph_penalty]\nclean_replacement = 0.30\n";
    let config: UnimatrixConfig = toml::from_str(toml_str).unwrap();
    let gp = &config.graph_penalty;
    assert_eq!(gp.clean_replacement, 0.30);
    assert_eq!(gp.orphan, ORPHAN_PENALTY);
    assert_eq!(gp.hop_decay, HOP_DECAY_FACTOR);
    assert_eq!(gp.partial_supersession, PARTIAL_SUPERSESSION_PENALTY);
    assert_eq!(gp.dead_end, DEAD_END_PENALTY);
    assert_eq!(gp.fallback, FALLBACK_PENALTY);
    assert_eq!(gp.max_traversal_depth, MAX_TRAVERSAL_DEPTH);
    assert_eq!(gp.multiplier, None);
}

// ---------------------------------------------------------------------------
// Multiplier overlay field (R-13) — AC-01
// ---------------------------------------------------------------------------

#[test]
fn test_graph_penalty_config_multiplier_defaults_none() {
    assert_eq!(GraphPenaltyConfig::default().multiplier, None);
}

#[test]
fn test_config_multiplier_some_parsed() {
    let toml_str = "[graph_penalty]\nmultiplier = 0.5\n";
    let config: UnimatrixConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.graph_penalty.multiplier, Some(0.5));
}

// ---------------------------------------------------------------------------
// resolve_params() — multiplier semantics (R-13)
// ---------------------------------------------------------------------------

#[test]
fn test_resolve_params_multiplier_none_no_scaling() {
    let cfg = GraphPenaltyConfig::default();
    assert_eq!(cfg.resolve_params(), GraphPenaltyParams::default());
}

#[test]
fn test_resolve_params_multiplier_scales_severities_not_shape() {
    let cfg = GraphPenaltyConfig {
        multiplier: Some(0.5),
        ..GraphPenaltyConfig::default()
    };
    let p = cfg.resolve_params();
    assert_eq!(p.orphan, ORPHAN_PENALTY * 0.5);
    assert_eq!(p.clean_replacement, CLEAN_REPLACEMENT_PENALTY * 0.5);
    assert_eq!(p.partial_supersession, PARTIAL_SUPERSESSION_PENALTY * 0.5);
    assert_eq!(p.dead_end, DEAD_END_PENALTY * 0.5);
    assert_eq!(p.fallback, FALLBACK_PENALTY * 0.5);
    assert_eq!(p.hop_decay, HOP_DECAY_FACTOR);
    assert_eq!(p.max_traversal_depth, MAX_TRAVERSAL_DEPTH);
}

#[test]
fn test_resolve_params_per_field_override_wins_over_multiplier() {
    let cfg = GraphPenaltyConfig {
        orphan: 0.20,
        multiplier: Some(0.5),
        ..GraphPenaltyConfig::default()
    };
    let p = cfg.resolve_params();
    assert_eq!(p.orphan, 0.20);
    assert_eq!(p.dead_end, DEAD_END_PENALTY * 0.5);
}

// ---------------------------------------------------------------------------
// Range validation (security) — reuse validate_graph_penalty
// ---------------------------------------------------------------------------

fn path() -> &'static Path {
    Path::new("/tmp/test-config.toml")
}

#[test]
fn test_config_penalty_out_of_range_rejected() {
    let cfg = GraphPenaltyConfig {
        orphan: 99.0,
        ..GraphPenaltyConfig::default()
    };
    let err = validate_graph_penalty(&cfg, path()).unwrap_err();
    assert!(matches!(
        err,
        ConfigError::GraphPenaltyFieldOutOfRange {
            field: "orphan",
            ..
        }
    ));
}

#[test]
fn test_config_penalty_negative_rejected() {
    let cfg = GraphPenaltyConfig {
        clean_replacement: -0.1,
        ..GraphPenaltyConfig::default()
    };
    let err = validate_graph_penalty(&cfg, path()).unwrap_err();
    assert!(matches!(
        err,
        ConfigError::GraphPenaltyFieldOutOfRange {
            field: "clean_replacement",
            ..
        }
    ));
}

#[test]
fn test_config_penalty_nan_rejected() {
    let cfg = GraphPenaltyConfig {
        dead_end: f64::NAN,
        ..GraphPenaltyConfig::default()
    };
    let err = validate_graph_penalty(&cfg, path()).unwrap_err();
    assert!(matches!(
        err,
        ConfigError::GraphPenaltyFieldOutOfRange {
            field: "dead_end",
            ..
        }
    ));
}

#[test]
fn test_config_max_traversal_depth_zero_rejected() {
    let cfg = GraphPenaltyConfig {
        max_traversal_depth: 0,
        ..GraphPenaltyConfig::default()
    };
    let err = validate_graph_penalty(&cfg, path()).unwrap_err();
    assert!(matches!(
        err,
        ConfigError::GraphPenaltyFieldOutOfRange {
            field: "max_traversal_depth",
            ..
        }
    ));
}

#[test]
fn test_config_multiplier_out_of_range_rejected() {
    let cfg = GraphPenaltyConfig {
        multiplier: Some(2.0),
        ..GraphPenaltyConfig::default()
    };
    let err = validate_graph_penalty(&cfg, path()).unwrap_err();
    assert!(matches!(
        err,
        ConfigError::GraphPenaltyFieldOutOfRange {
            field: "multiplier",
            ..
        }
    ));
}

#[test]
fn test_config_multiplier_zero_rejected() {
    let cfg = GraphPenaltyConfig {
        multiplier: Some(0.0),
        ..GraphPenaltyConfig::default()
    };
    let err = validate_graph_penalty(&cfg, path()).unwrap_err();
    assert!(matches!(
        err,
        ConfigError::GraphPenaltyFieldOutOfRange {
            field: "multiplier",
            ..
        }
    ));
}

#[test]
fn test_config_default_passes_validation() {
    assert!(validate_graph_penalty(&GraphPenaltyConfig::default(), path()).is_ok());
}

#[test]
fn test_config_multiplier_one_accepted() {
    let cfg = GraphPenaltyConfig {
        multiplier: Some(1.0),
        ..GraphPenaltyConfig::default()
    };
    assert!(validate_graph_penalty(&cfg, path()).is_ok());
}

// ---------------------------------------------------------------------------
// Serde round-trip with non-trivial values (#3557)
// ---------------------------------------------------------------------------

#[test]
fn test_graph_penalty_config_serde_roundtrip_nontrivial() {
    let original = GraphPenaltyConfig {
        orphan: 0.81,
        clean_replacement: 0.33,
        hop_decay: 0.55,
        partial_supersession: 0.62,
        dead_end: 0.49,
        fallback: 0.71,
        max_traversal_depth: 7,
        multiplier: Some(0.42),
    };
    let serialized = toml::to_string(&original).unwrap();
    let parsed: GraphPenaltyConfig = toml::from_str(&serialized).unwrap();
    assert_eq!(parsed, original);
}
