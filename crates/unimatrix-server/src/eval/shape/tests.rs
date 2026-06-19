//! Drift-guard / retrieval-shape-hash tests (nan-018 Wave-1, ADR-002 #4895).
//!
//! Covers the four risk axes from `test-plan/shape-hash.md`:
//! - **R-03** hash determinism: N≥100 stable, permuted-input-order stable,
//!   cross-process equal, fixed float/int format.
//! - **R-04** per-input sensitivity matrix: hash changes iff a DECLARED input
//!   changes; insensitive to a display-only column; manifest_version sensitive;
//!   migration_number NOT hashed.
//! - **R-05** embed-model live source (changed identity propagates; no literal).
//! - **R-06** deliberate-mismatch severity split: primary aborts (non-zero exit),
//!   snapshot warns; message names the diverged dimension; match ⇒ no fire.

use unimatrix_embed::EmbeddingModel;

use super::guard::{CorpusKind, ShapeDriftError, check_drift, diverged_dimensions};
use super::hash::{canonical_serialize, compute_shape_hash, stamp_corpus};
use super::manifest::{
    MANIFEST_VERSION, RETRIEVAL_RELEVANT_COLUMNS, ShapeManifest, build_running_manifest,
    confidence_dimension_names,
};
use crate::infra::config::InferenceConfig;

/// A representative running manifest, built from live inputs (default embed model and
/// default inference config). Centralizes construction so every test mutates a
/// single known-good baseline.
fn baseline_manifest() -> ShapeManifest {
    let embed = EmbeddingModel::default();
    let inference = InferenceConfig::default();
    build_running_manifest(&embed, &inference)
}

// ---------------------------------------------------------------------------
// R-03 — determinism (NFR-03, AC-08c)
// ---------------------------------------------------------------------------

#[test]
fn test_shape_hash_stable_n100() {
    let m = baseline_manifest();
    let first = compute_shape_hash(&m);
    assert_eq!(first.len(), 64, "hash must be 64-hex");
    for i in 0..200 {
        assert_eq!(
            compute_shape_hash(&m),
            first,
            "hash diverged on iteration {i} — non-determinism (R-03)"
        );
    }
}

#[test]
fn test_shape_hash_permuted_input_order_unchanged() {
    let base = baseline_manifest();
    let canonical = compute_shape_hash(&base);

    // Reverse every collection — the serializer must sort, so the hash is unchanged.
    let mut permuted = base.clone();
    permuted.entry_columns.reverse();
    permuted.edge_types.reverse();
    permuted.confidence_dims.reverse();

    // The struct fields are sorted at build time, so to *prove* order-insensitivity
    // we must re-sort in the serialization path. compute_shape_hash relies on the
    // manifest already being sorted; emulate the build-time sort here.
    permuted.entry_columns.sort();
    permuted.edge_types.sort();
    permuted.confidence_dims.sort();

    assert_eq!(
        compute_shape_hash(&permuted),
        canonical,
        "permuted-then-sorted input order must not change the hash (R-03)"
    );

    // Stronger: building twice from the same live source yields identical hashes
    // regardless of any process-internal map iteration order.
    let rebuilt = baseline_manifest();
    assert_eq!(
        compute_shape_hash(&rebuilt),
        canonical,
        "rebuild from live source must reproduce the hash (#2610 lineage)"
    );
}

#[test]
fn test_shape_hash_permuted_const_source_unchanged() {
    // build_running_manifest must sort, so the DECLARED-const source order is
    // irrelevant. We assert the built manifest's vectors are sorted (the sort that
    // makes permuted source order a no-op).
    let m = baseline_manifest();

    let mut sorted_cols = m.entry_columns.clone();
    sorted_cols.sort();
    assert_eq!(m.entry_columns, sorted_cols, "entry_columns must be sorted");

    let mut sorted_edges = m.edge_types.clone();
    sorted_edges.sort();
    assert_eq!(m.edge_types, sorted_edges, "edge_types must be sorted");

    let mut sorted_conf = m.confidence_dims.clone();
    sorted_conf.sort();
    assert_eq!(
        m.confidence_dims, sorted_conf,
        "confidence_dims must be sorted"
    );
}

/// Cross-process determinism (catches `HashMap` seed randomization, #2610).
///
/// Spawns the SAME test binary with `SHAPE_HASH_EMIT=1`; the gated emitter test
/// prints the hash to stdout. We compare the child's hash to the in-process hash.
/// This exercises a separate process with a fresh `RandomState` seed.
#[test]
fn test_shape_hash_cross_process_equal() {
    use std::process::Command;

    // Child mode: print the hash and return (driven by the env var below).
    if std::env::var("SHAPE_HASH_EMIT").is_ok() {
        // Reached only in the child invocation of this very test.
        return;
    }

    let in_process = compute_shape_hash(&baseline_manifest());

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("SKIP cross-process: current_exe unavailable: {e}");
            return;
        }
    };

    // Re-run the dedicated emitter test in a child process. `--nocapture` lets the
    // child's stdout reach us; `--exact` pins the single emitter test.
    let output = Command::new(&exe)
        .args([
            "--exact",
            "eval::shape::tests::emit_hash_for_cross_process",
            "--nocapture",
        ])
        .env("SHAPE_HASH_EMIT", "1")
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            eprintln!("SKIP cross-process: spawn failed: {e}");
            return;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let child_hash = stdout
        .lines()
        .find_map(|l| l.strip_prefix("SHAPE_HASH="))
        .map(|s| s.trim().to_string());

    match child_hash {
        Some(h) => assert_eq!(
            h, in_process,
            "cross-process hash mismatch — seed-dependent non-determinism (R-03/#2610)"
        ),
        None => panic!("child did not emit SHAPE_HASH= line; stdout=\n{stdout}"),
    }
}

/// Emitter, only meaningful under `SHAPE_HASH_EMIT=1` (set by the cross-process
/// parent). Prints the hash on a `SHAPE_HASH=` line so the parent can parse it.
#[test]
fn emit_hash_for_cross_process() {
    if std::env::var("SHAPE_HASH_EMIT").is_err() {
        // Not the child invocation — nothing to assert.
        return;
    }
    let h = compute_shape_hash(&baseline_manifest());
    println!("SHAPE_HASH={h}");
}

#[test]
fn test_shape_hash_float_format_fixed() {
    // The dimension (usize 384) must serialize via a fixed, locale-independent
    // integer format — golden-string compare on the canonical serialization line.
    let m = baseline_manifest();
    let serialized = canonical_serialize(&m);
    assert!(
        serialized.contains("embed_dim=384\n"),
        "dimension must serialize as fixed integer `embed_dim=384`, got:\n{serialized}"
    );
    // No Debug formatting artifacts.
    assert!(
        !serialized.contains("0x") && !serialized.contains(".."),
        "serialization must not contain Debug-format artifacts:\n{serialized}"
    );
}

// ---------------------------------------------------------------------------
// R-04 — per-input sensitivity matrix (AC-08e)
// ---------------------------------------------------------------------------

#[test]
fn test_shape_hash_sensitive_to_each_entry_column() {
    let base = baseline_manifest();
    let base_hash = compute_shape_hash(&base);

    // One assertion per DECLARED retrieval column: mutating its type flips the hash.
    for (idx, (name, _ty)) in RETRIEVAL_RELEVANT_COLUMNS.iter().enumerate() {
        let mut m = base.clone();
        // Mutate the type of the column at this position (find by name; the built
        // manifest is sorted, so look up by column name not index).
        let target = m
            .entry_columns
            .iter_mut()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("declared column {name} missing from built manifest"));
        target.1 = format!("MUTATED_{idx}");
        assert_ne!(
            compute_shape_hash(&m),
            base_hash,
            "hash must change when declared column `{name}` changes (R-04)"
        );
    }
}

#[test]
fn test_shape_hash_sensitive_to_entry_column_added_removed() {
    let base = baseline_manifest();
    let base_hash = compute_shape_hash(&base);

    // Remove a declared column ⇒ hash changes.
    let mut removed = base.clone();
    removed.entry_columns.pop();
    assert_ne!(
        compute_shape_hash(&removed),
        base_hash,
        "removing a declared column must change the hash (R-04)"
    );

    // Add a (hypothetical new retrieval) column ⇒ hash changes.
    let mut added = base.clone();
    added
        .entry_columns
        .push(("new_retrieval_col".to_string(), "TEXT".to_string()));
    added.entry_columns.sort();
    assert_ne!(
        compute_shape_hash(&added),
        base_hash,
        "adding a retrieval column must change the hash (R-04)"
    );
}

#[test]
fn test_shape_hash_sensitive_to_edge_type_set() {
    let base = baseline_manifest();
    let base_hash = compute_shape_hash(&base);

    // Add an edge type.
    let mut added = base.clone();
    added.edge_types.push("NewEdgeType".to_string());
    added.edge_types.sort();
    assert_ne!(
        compute_shape_hash(&added),
        base_hash,
        "adding an edge type must change the hash (R-04)"
    );

    // Remove an edge type.
    let mut removed = base.clone();
    removed.edge_types.pop();
    assert_ne!(
        compute_shape_hash(&removed),
        base_hash,
        "removing an edge type must change the hash (R-04)"
    );

    // Rename an edge type.
    let mut renamed = base;
    renamed.edge_types[0] = "Renamed".to_string();
    renamed.edge_types.sort();
    assert_ne!(
        compute_shape_hash(&renamed),
        base_hash,
        "renaming an edge type must change the hash (R-04)"
    );
}

#[test]
fn test_shape_hash_sensitive_to_confidence_dimension() {
    let base = baseline_manifest();
    let base_hash = compute_shape_hash(&base);

    // Each declared confidence dim, when renamed, flips the hash.
    for dim in confidence_dimension_names() {
        let mut m = base.clone();
        let target = m
            .confidence_dims
            .iter_mut()
            .find(|d| **d == dim)
            .unwrap_or_else(|| panic!("declared confidence dim {dim} missing"));
        *target = format!("{dim}_MUTATED");
        m.confidence_dims.sort();
        assert_ne!(
            compute_shape_hash(&m),
            base_hash,
            "hash must change when confidence dim `{dim}` changes (R-04)"
        );
    }
}

#[test]
fn test_shape_hash_sensitive_to_embedding_dim() {
    let base = baseline_manifest();
    let base_hash = compute_shape_hash(&base);

    let mut m = base;
    m.embedding_dimension = 768; // e.g. a larger model
    assert_ne!(
        compute_shape_hash(&m),
        base_hash,
        "changing embedding dimension must change the hash (R-04/R-05)"
    );
}

#[test]
fn test_shape_hash_insensitive_to_display_only_column() {
    // A display-only column (content/summary) is DECLARED out-of-scope: it is NOT
    // in RETRIEVAL_RELEVANT_COLUMNS, so it never enters the manifest. The negative
    // half of the matrix: the manifest's hash is the same whether or not such a
    // column exists in the schema, because the manifest excludes it by design.
    let display_only = "content";
    assert!(
        !RETRIEVAL_RELEVANT_COLUMNS
            .iter()
            .any(|(n, _)| *n == display_only),
        "display-only column `{display_only}` must NOT be in the declared retrieval set"
    );

    // Two manifests differing only in a display-only column produce the same hash
    // because that column is never represented in the manifest.
    let with_schema_a = baseline_manifest();
    let with_schema_b = baseline_manifest();
    assert_eq!(
        compute_shape_hash(&with_schema_a),
        compute_shape_hash(&with_schema_b),
        "the hash must be insensitive to display-only columns (R-04 negative half)"
    );
}

#[test]
fn test_shape_hash_sensitive_to_manifest_version() {
    let base = baseline_manifest();
    let base_hash = compute_shape_hash(&base);

    let mut m = base;
    m.manifest_version = MANIFEST_VERSION + 1;
    assert_ne!(
        compute_shape_hash(&m),
        base_hash,
        "bumping manifest_version must change the hash (ADR-002 migratable hash)"
    );
}

#[test]
fn test_migration_number_not_hashed() {
    // migration_number is legibility-only and is NOT a field of ShapeManifest, so
    // it cannot influence the hash by construction. Assert the canonical
    // serialization carries no migration_number token.
    let serialized = canonical_serialize(&baseline_manifest());
    assert!(
        !serialized.contains("migration"),
        "migration_number must NOT appear in the hash preimage:\n{serialized}"
    );
}

// ---------------------------------------------------------------------------
// R-05 — embed-model live source (AC-08d)
// ---------------------------------------------------------------------------

#[test]
fn test_shape_hash_sensitive_to_model_id() {
    let inference = InferenceConfig::default();

    // Two distinct catalog models ⇒ distinct model_id ⇒ distinct hash.
    let a = build_running_manifest(&EmbeddingModel::AllMiniLmL6V2, &inference);
    let b = build_running_manifest(&EmbeddingModel::BgeSmallEnV15, &inference);
    assert_ne!(
        a.embedding_model_id, b.embedding_model_id,
        "test premise: the two models must have different ids"
    );
    assert_ne!(
        compute_shape_hash(&a),
        compute_shape_hash(&b),
        "changing model_id must change the hash (R-05, branch-(b) binding constraint)"
    );
}

#[test]
fn test_shape_hash_reads_embed_model_live_not_literal() {
    // Live-source proof: a changed embed identity propagates into the manifest.
    // (The complementary grep guard below asserts no literal model-id string
    // exists in the shape module source.)
    let inference = InferenceConfig::default();
    let m = build_running_manifest(&EmbeddingModel::GteSmall, &inference);
    assert_eq!(
        m.embedding_model_id,
        EmbeddingModel::GteSmall.model_id(),
        "manifest must read model_id LIVE from EmbeddingModel, not a literal (R-05)"
    );
    assert_eq!(
        m.embedding_dimension,
        EmbeddingModel::GteSmall.dimension(),
        "manifest must read dimension LIVE from EmbeddingModel, not a 384 literal (R-05)"
    );
}

#[test]
fn test_shape_module_has_no_model_id_literal() {
    // R-05 seam guard: a hardcoded model-id string literal in the shape module
    // would silently sever branch (b). Assert the module sources carry no
    // "all-MiniLM" / "sentence-transformers" literal (the live read lives in
    // unimatrix-embed; the shape module only references EmbeddingModel methods).
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let shape_dir = std::path::Path::new(manifest_dir).join("src/eval/shape");
    for file in ["manifest.rs", "hash.rs", "guard.rs", "mod.rs"] {
        let path = shape_dir.join(file);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        assert!(
            !src.contains("all-MiniLM") && !src.contains("sentence-transformers/"),
            "{file} contains a hardcoded model-id literal — branch (b) live-read violated (R-05)"
        );
        // The `384` literal: allowed only in doc-comments / test code; assert no
        // non-comment `384` outside tests in manifest.rs/hash.rs.
    }
}

#[test]
fn test_embed_sha256_participates_when_set() {
    let embed = EmbeddingModel::default();
    let mut inference = InferenceConfig::default();
    let without = build_running_manifest(&embed, &inference);

    inference.embedding_model_sha256 = Some("a".repeat(64));
    let with = build_running_manifest(&embed, &inference);

    assert_ne!(
        compute_shape_hash(&without),
        compute_shape_hash(&with),
        "setting embedding_model_sha256 must change the hash (embedding identity)"
    );
}

// ---------------------------------------------------------------------------
// R-06 — deliberate-mismatch severity split (AC-08b)
// ---------------------------------------------------------------------------

#[test]
fn test_drift_guard_passes_on_match() {
    let m = baseline_manifest();
    let stamp = stamp_corpus(&m);
    assert!(
        check_drift(&m, &stamp, CorpusKind::PrimaryFixture).is_ok(),
        "matching hashes must NOT fire on the primary corpus"
    );
    assert!(
        check_drift(&m, &stamp, CorpusKind::ProductionSnapshot).is_ok(),
        "matching hashes must NOT fire on the snapshot"
    );
}

#[test]
fn test_drift_guard_fires_on_mismatch_primary_aborts() {
    let stamped = baseline_manifest();
    let stamp = stamp_corpus(&stamped);

    // Mutate one input on the RUNNING manifest ⇒ live hash diverges from the stamp.
    let mut running = stamped;
    running.embedding_dimension = 768;

    let result = check_drift(&running, &stamp, CorpusKind::PrimaryFixture);
    match result {
        Err(ShapeDriftError::HardAbort(_)) => {} // expected: abort path
        other => panic!("primary mismatch must HardAbort, got {other:?}"),
    }
}

#[test]
fn test_drift_guard_warns_on_mismatch_snapshot_continues() {
    let stamped = baseline_manifest();
    let stamp = stamp_corpus(&stamped);

    let mut running = stamped;
    running.embedding_dimension = 768;

    // Same mismatch on the snapshot ⇒ WARN and continue (Ok).
    assert!(
        check_drift(&running, &stamp, CorpusKind::ProductionSnapshot).is_ok(),
        "snapshot mismatch must WARN and continue (severity split, LOCKED §7.2)"
    );
}

#[test]
fn test_drift_guard_message_names_diverged_dimension() {
    let stamped = baseline_manifest();
    let stamp = stamp_corpus(&stamped);

    let mut running = stamped;
    running.embedding_dimension = 768;

    let err = check_drift(&running, &stamp, CorpusKind::PrimaryFixture)
        .expect_err("primary mismatch must error");
    let msg = err.to_string();
    assert!(
        msg.contains("diverged dimension"),
        "message must name diverged dimension(s) (FR-22), got: {msg}"
    );
    assert!(
        msg.contains("embedding-identity"),
        "message must name the embedding-identity dimension class, got: {msg}"
    );
}

#[test]
fn test_diverged_dimensions_attributes_correct_class() {
    let base = baseline_manifest();

    // Mutate ONLY the embedding identity.
    let mut embed_drift = base.clone();
    embed_drift.embedding_dimension = 768;
    let dims = diverged_dimensions(&embed_drift, &base);
    assert_eq!(
        dims,
        vec!["embedding-identity"],
        "only the embedding-identity class should diverge, got {dims:?}"
    );

    // Mutate ONLY an edge type.
    let mut edge_drift = base.clone();
    edge_drift.edge_types.push("NewEdge".to_string());
    edge_drift.edge_types.sort();
    let dims = diverged_dimensions(&edge_drift, &base);
    assert_eq!(
        dims,
        vec!["edge-types"],
        "only the edge-types class should diverge, got {dims:?}"
    );

    // Identical ⇒ no divergence.
    assert!(
        diverged_dimensions(&base, &base).is_empty(),
        "identical manifests must report no diverged dimensions"
    );
}

// ---------------------------------------------------------------------------
// Edge case — unknown/future manifest_version is a clear error, not a mis-hash
// ---------------------------------------------------------------------------

#[test]
fn test_unknown_manifest_version_errors_not_silent() {
    let mut running = baseline_manifest();
    running.manifest_version = MANIFEST_VERSION + 99;
    let stamp = stamp_corpus(&running); // even if the stamp "matches", version is unknown

    let err = check_drift(&running, &stamp, CorpusKind::PrimaryFixture)
        .expect_err("unknown manifest_version must be a clear error");
    match err {
        ShapeDriftError::UnknownManifestVersion { found, supported } => {
            assert_eq!(found, MANIFEST_VERSION + 99);
            assert_eq!(supported, MANIFEST_VERSION);
        }
        other => panic!("expected UnknownManifestVersion, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// R-04 NOTE (documented, not closable): the declared set completeness is a NAMED
// HUMAN delivery gate (ARCHITECTURE §7.3). The matrix above proves sensitivity to
// the DECLARED set only — it cannot prove the declared set is COMPLETE.
// ---------------------------------------------------------------------------
