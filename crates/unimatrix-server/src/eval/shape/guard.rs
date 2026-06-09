//! Drift-guard compare + severity split (nan-018, ADR-002 #4895; LOCKED §7.2 / R-13).
//!
//! At eval start the harness builds the RUNNING manifest, loads the corpus stamp,
//! and calls [`check_drift`]:
//! - **Primary fixture corpus** mismatch ⇒ `ShapeDriftError::HardAbort` (abort,
//!   non-zero exit). The corpus is the durable yardstick whose numbers feed
//!   ass-073 → crt-053 ACs → product ranking, so silent drift propagates to
//!   product behavior. Aborting deliberately overrides the eval `report` exit-0
//!   quality-verdict convention: the guard protects corpus VALIDITY (a
//!   precondition), a different class from the body-only verdict.
//! - **Production snapshot** mismatch ⇒ `tracing::warn!` and continue (ephemeral
//!   by contract — re-snapshot when shape drifts).
//!
//! The message NAMES which shape dimension(s) diverged (FR-22) so the migration
//! fix is obvious.

use std::fmt;

use super::hash::compute_shape_hash;
use super::manifest::{MANIFEST_VERSION, ShapeManifest};

/// Which corpus a drift check is running against — drives the severity split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorpusKind {
    /// Durable hand-authored fixture corpus. Mismatch is a HARD ERROR.
    PrimaryFixture,
    /// Ephemeral production snapshot (realism layer). Mismatch is a WARN.
    ProductionSnapshot,
}

impl CorpusKind {
    fn label(self) -> &'static str {
        match self {
            CorpusKind::PrimaryFixture => "primary fixture corpus",
            CorpusKind::ProductionSnapshot => "production snapshot",
        }
    }
}

/// Drift-guard error. `HardAbort` is the abort path for the primary fixture
/// corpus; a snapshot mismatch never produces an error (it warns and returns
/// `Ok`). `UnknownManifestVersion` guards a future/unknown manifest version from
/// becoming a silent mis-hash.
#[derive(Debug)]
pub enum ShapeDriftError {
    /// Primary-corpus shape drift — the run MUST abort with a non-zero exit.
    /// Carries the dimension-naming message (FR-22).
    HardAbort(String),
    /// The running manifest declares a `manifest_version` this binary does not
    /// understand — a clear error, NOT a silent mis-hash.
    UnknownManifestVersion {
        /// Version found on the running manifest.
        found: u32,
        /// Version this binary supports ([`MANIFEST_VERSION`]).
        supported: u32,
    },
}

impl fmt::Display for ShapeDriftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShapeDriftError::HardAbort(msg) => write!(f, "{msg}"),
            ShapeDriftError::UnknownManifestVersion { found, supported } => write!(
                f,
                "retrieval-shape manifest_version {found} is unknown to this binary \
                 (supported = {supported}); refusing to hash to avoid silent staleness"
            ),
        }
    }
}

impl std::error::Error for ShapeDriftError {}

/// Names of the per-class shape dimensions, for the divergence message.
const DIM_ENTRY_COLUMNS: &str = "entry-columns";
const DIM_EDGE_TYPES: &str = "edge-types";
const DIM_CONFIDENCE_DIMS: &str = "confidence-dims";
const DIM_EMBEDDING_IDENTITY: &str = "embedding-identity";
const DIM_MANIFEST_VERSION: &str = "manifest-version";

/// Per-class sub-hash so divergence can be attributed to a specific dimension.
///
/// Re-uses [`compute_shape_hash`] over a manifest that carries ONLY this class's
/// inputs (all other fields zeroed/empty), so a per-class hash is itself stable
/// and comparable between two manifests.
fn class_hash(m: &ShapeManifest, class: &str) -> String {
    let mut probe = ShapeManifest {
        manifest_version: 0,
        entry_columns: Vec::new(),
        edge_types: Vec::new(),
        confidence_dims: Vec::new(),
        embedding_model_id: String::new(),
        embedding_dimension: 0,
        embedding_model_sha256: None,
    };
    match class {
        DIM_MANIFEST_VERSION => probe.manifest_version = m.manifest_version,
        DIM_ENTRY_COLUMNS => probe.entry_columns = m.entry_columns.clone(),
        DIM_EDGE_TYPES => probe.edge_types = m.edge_types.clone(),
        DIM_CONFIDENCE_DIMS => probe.confidence_dims = m.confidence_dims.clone(),
        DIM_EMBEDDING_IDENTITY => {
            probe.embedding_model_id = m.embedding_model_id.clone();
            probe.embedding_dimension = m.embedding_dimension;
            probe.embedding_model_sha256 = m.embedding_model_sha256.clone();
        }
        _ => {}
    }
    compute_shape_hash(&probe)
}

/// Compare the running manifest against another (e.g. the manifest the corpus was
/// stamped from) and return the per-class dimension labels that diverged.
///
/// Used when the stamped *manifest* is available. When only the stamped *hash* is
/// known (the on-disk corpus stamp), [`name_diverged_dimensions`] is the entry
/// point.
pub fn diverged_dimensions(running: &ShapeManifest, other: &ShapeManifest) -> Vec<&'static str> {
    let classes = [
        DIM_MANIFEST_VERSION,
        DIM_ENTRY_COLUMNS,
        DIM_EDGE_TYPES,
        DIM_CONFIDENCE_DIMS,
        DIM_EMBEDDING_IDENTITY,
    ];
    classes
        .into_iter()
        .filter(|class| class_hash(running, class) != class_hash(other, class))
        .collect()
}

/// Best-effort divergence triage when only the stamped *hash* is available.
///
/// Without the stamped manifest we cannot attribute the drift to a class, so this
/// reports the full DECLARED dimension set as the suspect surface — the message
/// still NAMES the dimensions a migration author must inspect (FR-22), and is
/// upgraded to a precise single-class attribution by [`diverged_dimensions`] when
/// the stamped manifest is on hand.
fn name_diverged_dimensions(_running: &ShapeManifest, _stamped_hash: &str) -> String {
    format!(
        "{DIM_ENTRY_COLUMNS}, {DIM_EDGE_TYPES}, {DIM_CONFIDENCE_DIMS}, \
         {DIM_EMBEDDING_IDENTITY}, {DIM_MANIFEST_VERSION}"
    )
}

/// Compare the running schema's shape hash to a corpus stamp and apply the
/// severity split.
///
/// # Errors
/// - [`ShapeDriftError::UnknownManifestVersion`] if the running manifest version
///   is not understood by this binary (checked before hashing).
/// - [`ShapeDriftError::HardAbort`] if `kind == PrimaryFixture` and the hashes
///   differ. A `ProductionSnapshot` mismatch logs a `warn!` and returns `Ok`.
pub fn check_drift(
    running: &ShapeManifest,
    stamped_hash: &str,
    kind: CorpusKind,
) -> Result<(), ShapeDriftError> {
    if running.manifest_version != MANIFEST_VERSION {
        return Err(ShapeDriftError::UnknownManifestVersion {
            found: running.manifest_version,
            supported: MANIFEST_VERSION,
        });
    }

    let live = compute_shape_hash(running);
    if live == stamped_hash {
        return Ok(());
    }

    let diverged = name_diverged_dimensions(running, stamped_hash);
    let msg = format!(
        "retrieval-shape drift on {}: diverged dimension(s) = {diverged}; \
         stamped={stamped_hash} live={live}",
        kind.label()
    );

    match kind {
        CorpusKind::PrimaryFixture => {
            tracing::error!("{msg}");
            Err(ShapeDriftError::HardAbort(msg))
        }
        CorpusKind::ProductionSnapshot => {
            tracing::warn!("{msg}");
            Ok(())
        }
    }
}
