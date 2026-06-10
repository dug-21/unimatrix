//! Retrieval-shape manifest — ordered, versioned, ENUMERATED inputs (nan-018, ADR-002 #4895).
//!
//! The manifest is the single declared description of "retrieval shape" that the
//! drift guard hashes. Determinism is **structural**: every collection is a
//! `Vec` sorted at build time, never a `HashMap` (the #2610 non-determinism
//! lineage). Embedding identity (`model_id` + `dimension`) is a first-class input
//! (OQ-3 branch (b)) so embed-model drift trips the guard without a frozen vector
//! sidecar.
//!
//! ## R-04 — DECLARED set is a NAMED HUMAN delivery gate (LOCKED §7.3)
//!
//! [`RETRIEVAL_RELEVANT_COLUMNS`], [`RETRIEVAL_EDGE_TYPES`] and
//! [`confidence_dimension_names`] are the *declared* retrieval-shape inputs. A
//! test can only prove the hash is **sensitive to the declared set**; it CANNOT
//! prove the declared set is **complete**. Before nan-018 delivery is accepted, a
//! NAMED human reviewer MUST certify that these lists cover every column / edge /
//! confidence dimension the live retrieval/ranking path reads — that no
//! retrieval-relevant input was mis-classified as display-only and silently
//! omitted (the R-04 silent-staleness path). This module flags it; it cannot
//! close it.

use unimatrix_embed::EmbeddingModel;
use unimatrix_engine::graph::RelationType;

use crate::infra::config::InferenceConfig;

/// Manifest schema version. Bumped ONLY when the input *set* changes (a new
/// declared column / edge type / confidence dim / embedding field), NOT per
/// corpus. Distinct from the per-corpus `migration_number` (legibility only).
pub const MANIFEST_VERSION: u32 = 2;

/// DECLARED retrieval-relevant `entries` columns — `(column_name, sql_type)`.
///
/// These are the columns the live retrieval/penalty/ranking path reads. Display-only
/// columns (e.g. `content`, `summary`, `created_at` for prose rendering) are
/// deliberately EXCLUDED and are NOT in this list — so "does a display-only column
/// count?" is answered by this manifest, not by reviewer judgment.
///
/// # R-04 HUMAN-REVIEW SURFACE (see module docs)
/// This is the exact list a named human reviewer must certify as complete against
/// the live retrieval/ranking path at delivery. It is NOT machine-verifiable.
///
/// Authored unsorted for human legibility; [`build_running_manifest`] sorts before
/// hashing so source order never affects the hash.
pub const RETRIEVAL_RELEVANT_COLUMNS: &[(&str, &str)] = &[
    ("status", "TEXT"), // Active/Deprecated/Superseded — penalty + redirect path
    ("supersedes", "INTEGER"), // correction-chain traversal (find_terminal_active)
    ("superseded_by", "INTEGER"), // correction-chain traversal + penalty trigger
    ("category", "TEXT"), // category-aware ranking / PPR seeding
    ("trust_source", "TEXT"), // confidence: base/trust component input
    ("access_count", "INTEGER"), // confidence: usage component input
    ("last_accessed_at", "INTEGER"), // confidence: freshness component input
    ("created_at", "INTEGER"), // confidence: freshness fallback reference
    ("helpful_count", "INTEGER"), // confidence: helpfulness (Bayesian) input
    ("unhelpful_count", "INTEGER"), // confidence: helpfulness (Bayesian) input
    ("correction_count", "INTEGER"), // confidence: correction (corr) component input
];

/// DECLARED retrieval-participating `RelationType` variants.
///
/// The Supersedes-penalty + PPR-positive set per `graph.rs`. Authored unsorted;
/// sorted by `as_str()` before hashing.
///
/// # R-04 HUMAN-REVIEW SURFACE (see module docs)
pub const RETRIEVAL_EDGE_TYPES: &[RelationType] = &[
    RelationType::Supersedes,
    RelationType::Contradicts,
    RelationType::Supports,
    RelationType::CoAccess,
    RelationType::Prerequisite,
    RelationType::Informs,
    RelationType::RelatedTo,
];

/// DECLARED confidence-dimension names, read from the LIVE
/// [`crate::infra::config::ConfidenceWeights`] struct field set (6 dims) — the
/// per-domain weight vector that feeds scoring. These mirror the engine's
/// `ConfidenceParams` `w_*` levers (base/usage/fresh/help/corr/trust).
///
/// Enumerated against the live struct at delivery (the brief's DELIVERY READ).
/// Authored unsorted; [`build_running_manifest`] sorts before hashing.
///
/// # R-04 HUMAN-REVIEW SURFACE (see module docs)
pub fn confidence_dimension_names() -> Vec<String> {
    // Exact field names of the live `ConfidenceWeights` struct
    // (infra/config.rs) — the 6-component custom-preset weight vector.
    ["base", "usage", "fresh", "help", "corr", "trust"]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

/// The ordered, versioned, enumerated retrieval-shape manifest.
///
/// Every `Vec` field is SORTED at construction. Hashed by
/// [`super::hash::compute_shape_hash`]. `#[derive(Debug)]` for diagnostics only —
/// hashing never uses `{:?}` (Debug float/format drift is the R-03 trap).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeManifest {
    /// = [`MANIFEST_VERSION`] when built live; the corpus stamp records the version
    /// it was stamped under.
    pub manifest_version: u32,
    /// Retrieval-relevant `(column_name, type)` pairs — SORTED.
    pub entry_columns: Vec<(String, String)>,
    /// Retrieval-participating `RelationType::as_str()` values — SORTED.
    pub edge_types: Vec<String>,
    /// `ConfidenceWeights`/`ConfidenceParams` field names — SORTED.
    pub confidence_dims: Vec<String>,
    /// `EmbeddingModel::model_id()` — LIVE read (R-05), never a literal.
    pub embedding_model_id: String,
    /// `EmbeddingModel::dimension()` — LIVE read (R-05), never a `384` literal.
    pub embedding_dimension: usize,
    /// `InferenceConfig.embedding_model_sha256` when set.
    pub embedding_model_sha256: Option<String>,
}

/// Build the manifest for the RUNNING schema from live inputs.
///
/// Embedding identity is read LIVE from [`EmbeddingModel`] (R-05) — a literal here
/// would silently sever OQ-3 branch (b) and reintroduce the fake-MRR-drift class.
///
/// All collections are sorted here so the source-declaration order of the `const`
/// lists above is irrelevant to the hash (the permuted-input-order determinism
/// guarantee, #2610 lineage).
pub fn build_running_manifest(
    embed: &EmbeddingModel,
    inference: &InferenceConfig,
) -> ShapeManifest {
    let mut entry_columns: Vec<(String, String)> = RETRIEVAL_RELEVANT_COLUMNS
        .iter()
        .map(|(n, t)| ((*n).to_string(), (*t).to_string()))
        .collect();
    entry_columns.sort();

    let mut edge_types: Vec<String> = RETRIEVAL_EDGE_TYPES
        .iter()
        .map(|rt| rt.as_str().to_string())
        .collect();
    edge_types.sort();

    let mut confidence_dims = confidence_dimension_names();
    confidence_dims.sort();

    ShapeManifest {
        manifest_version: MANIFEST_VERSION,
        entry_columns,
        edge_types,
        confidence_dims,
        // *** LIVE reads — R-05. NOT literals. ***
        embedding_model_id: embed.model_id().to_string(),
        embedding_dimension: embed.dimension(),
        embedding_model_sha256: inference.embedding_model_sha256.clone(),
    }
}
