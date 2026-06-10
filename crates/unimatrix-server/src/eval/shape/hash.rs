//! Deterministic retrieval-shape hash (nan-018, ADR-002 #4895).
//!
//! Determinism is STRUCTURAL, not incidental (R-03 / NFR-03):
//! - every collection is ALREADY sorted in [`ShapeManifest`] (no `HashMap`
//!   iteration — the #2610 / #1099 / #3752 non-determinism lineage);
//! - serialization uses fixed, locale-independent string/integer formatting;
//! - NO `{:?}` Debug formatting and NO raw `f64` (the dimension is a `usize`).
//!   If an `f64` ever enters the manifest it MUST use a fixed `{:.N}` format.
//!
//! The same manifest produces a byte-identical canonical string in-process,
//! across permuted source order, and across separate process invocations (no
//! seed-dependent map iteration), which the SHA-256 then collapses to 64-hex.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use super::manifest::ShapeManifest;

/// Canonical, deterministic serialization of a [`ShapeManifest`].
///
/// Fixed field order; each pre-sorted collection emitted line-by-line with a
/// stable label prefix. This string is the hash preimage and is also the
/// golden-string surface for the float/int-format determinism test.
pub fn canonical_serialize(m: &ShapeManifest) -> String {
    let mut buf = String::new();
    // `write!` to a String is infallible; the Result is discarded deliberately.
    let _ = writeln!(buf, "manifest_version={}", m.manifest_version);
    for (name, ty) in &m.entry_columns {
        let _ = writeln!(buf, "col:{name}={ty}");
    }
    for et in &m.edge_types {
        let _ = writeln!(buf, "edge:{et}");
    }
    for cd in &m.confidence_dims {
        let _ = writeln!(buf, "conf:{cd}");
    }
    let _ = writeln!(buf, "embed_model_id={}", m.embedding_model_id);
    // usize via Display — fixed, locale-independent integer format (no Debug).
    let _ = writeln!(buf, "embed_dim={}", m.embedding_dimension);
    if let Some(sha) = &m.embedding_model_sha256 {
        let _ = writeln!(buf, "embed_sha={sha}");
    }
    buf
}

/// SHA-256 of `bytes`, lowercase hex (64 chars).
fn sha256_lowercase_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        // `{:02x}` — fixed lowercase 2-hex-digit format, locale-independent.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Compute the deterministic retrieval-shape hash (64-hex SHA-256).
pub fn compute_shape_hash(m: &ShapeManifest) -> String {
    sha256_lowercase_hex(canonical_serialize(m).as_bytes())
}

/// Stamp a corpus: the hash an authoring/migration step writes into the corpus
/// manifest stamp (`eval/corpus/fixtures/manifest.toml`). Identical to
/// [`compute_shape_hash`]; named for call-site legibility at authoring time.
pub fn stamp_corpus(running: &ShapeManifest) -> String {
    compute_shape_hash(running)
}
