//! Retrieval-shape drift guard (nan-018 Wave-1, ADR-002 #4895; LOCKED §7.2).
//!
//! The **triple linchpin**: one deterministic hash serves (1) the mechanical
//! drift guard, (2) the OQ-5 protocol-trigger predicate, and (3) the OQ-3
//! embed-model-dependence question (branch (b) — embedding identity is in the
//! hash, so embed-at-load is safe with NO frozen vector sidecar).
//!
//! Pipeline:
//! 1. [`build_running_manifest`] — assemble the ordered, versioned, ENUMERATED
//!    manifest from the LIVE schema (embedding identity read live — R-05).
//! 2. [`compute_shape_hash`] — deterministic SHA-256 (64-hex) over a canonical
//!    serialization (sorted vectors, fixed int/string format — R-03 / NFR-03).
//! 3. [`check_drift`] — compare to the corpus stamp with the severity split:
//!    primary corpus mismatch = HARD ERROR (abort); snapshot mismatch = WARN.
//!
//! ## R-04 — column-manifest completeness is a NAMED HUMAN delivery gate
//!
//! The per-input sensitivity tests prove the hash changes iff a *declared*
//! manifest input changes (and is insensitive to display-only columns). They
//! CANNOT prove the declared set is COMPLETE. The declared set
//! ([`manifest::RETRIEVAL_RELEVANT_COLUMNS`], [`manifest::RETRIEVAL_EDGE_TYPES`],
//! [`manifest::confidence_dimension_names`]) is the human-review surface a named
//! reviewer must certify against the live retrieval/ranking path before delivery
//! (ARCHITECTURE §7.3 LOCKED). This module flags the gate; it cannot close it.

pub mod guard;
pub mod hash;
pub mod manifest;

pub use guard::{CorpusKind, ShapeDriftError, check_drift, diverged_dimensions};
pub use hash::{canonical_serialize, compute_shape_hash, stamp_corpus};
pub use manifest::{
    MANIFEST_VERSION, RETRIEVAL_EDGE_TYPES, RETRIEVAL_RELEVANT_COLUMNS, ShapeManifest,
    build_running_manifest, confidence_dimension_names,
};

#[cfg(test)]
mod tests;
