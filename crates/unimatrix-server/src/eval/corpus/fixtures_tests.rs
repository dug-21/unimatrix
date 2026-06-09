//! Tests for the SHIPPED primary fixture corpus + manifest stamp (nan-018, ADR-004 §5).
//!
//! Distinct from `tests.rs` (which tests the loader GATE with synthetic inputs):
//! this module audits the **shipped assets** under `fixtures/` and proves the
//! end-to-end embed-at-load -> search path is non-vacuous (R-15 / AC-14 floor).
//!
//! Owns two of the three non-negotiable Wave-1 backstop tests:
//! - R-09 static corpus audit (zero literal-id, zero null across shipped corpus).
//! - R-15 non-empty-results smoke test (an AC-14 scenario search is non-empty).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;
use unimatrix_core::{VectorConfig, VectorIndex};
use unimatrix_embed::{EmbeddingProvider, prepare_text};

use super::assertions::RawFixture;
use super::embed::{embed_and_write_vectors, load_fixture_corpus_with_embeddings};
use super::loader::load_fixture_corpus;

/// Embedding dimension for the catalog models (all 384-d).
const DIM: usize = 384;

/// Absolute path to the shipped primary-corpus fixtures directory.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/eval/corpus/fixtures")
}

/// Read + parse every shipped `*.toml` fixture (skips the manifest stamp).
fn shipped_fixtures() -> Vec<(String, RawFixture)> {
    let dir = fixtures_dir();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("read fixtures dir") {
        let entry = entry.expect("dirent");
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".toml") || name == "manifest.toml" {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).expect("read fixture");
        let fixture: RawFixture =
            toml::from_str(&text).unwrap_or_else(|e| panic!("parse {name}: {e}"));
        out.push((name, fixture));
    }
    out
}

/// Deterministic, model-free embedding provider (mirrors `MockProvider`).
///
/// Keeps the smoke test free of ONNX model files: same text -> same 384-d
/// L2-normalized vector. The model-free unit-test path the brief mandates.
struct DeterministicProvider;

impl EmbeddingProvider for DeterministicProvider {
    fn embed(&self, text: &str) -> unimatrix_embed::Result<Vec<f32>> {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let seed = hasher.finish();
        let mut v = vec![0.0_f32; DIM];
        for (i, slot) in v.iter_mut().enumerate() {
            let h = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(i as u64);
            *slot = ((h as f32) / (u64::MAX as f32)) * 2.0 - 1.0;
        }
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        Ok(v)
    }

    fn embed_batch(&self, texts: &[&str]) -> unimatrix_embed::Result<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    fn dimension(&self) -> usize {
        DIM
    }

    fn name(&self) -> &str {
        "deterministic-test"
    }
}

// ---------------------------------------------------------------------------
// R-09 — static corpus audit (Wave-1 backstop #1, MAY NOT be deferred)
// ---------------------------------------------------------------------------

#[test]
fn test_primary_corpus_audit_zero_literal_id_zero_null() {
    let fixtures = shipped_fixtures();
    assert!(!fixtures.is_empty(), "shipped corpus must contain fixtures");

    let mut scenario_count = 0usize;
    for (file, fixture) in &fixtures {
        for scenario in &fixture.scenarios {
            scenario_count += 1;
            // ZERO literal-id `expected`.
            assert!(
                !scenario.has_literal_expected(),
                "{file}: scenario carries a literal-id `expected` (banned, R-09/C-04)"
            );
            // ZERO null ground truth — every scenario carries a real assertion set.
            assert!(
                !scenario.is_null_ground_truth(),
                "{file}: scenario has null ground truth (no property assertion, R-09)"
            );
            // Only the three property families are used (structurally guaranteed by
            // the `ExpectedAssertions` shape; assert non-empty for clarity).
            let a = scenario
                .assertions
                .as_ref()
                .expect("non-null scenario has assertions");
            assert!(
                !a.is_empty(),
                "{file}: assertion set must carry >=1 property assertion"
            );
        }
    }
    assert!(
        scenario_count >= 4,
        "corpus must ship >=4 scenarios (one per required shape), got {scenario_count}"
    );
}

// ---------------------------------------------------------------------------
// AC-06 — the four required shapes are present and each loads + searches
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_corpus_contains_required_four_shapes() {
    let dir = fixtures_dir();
    let out = TempDir::new().unwrap();
    let target = out.path().join("snap.db");
    let corpus = load_fixture_corpus(&dir, &target)
        .await
        .expect("shipped corpus loads");

    // Each required shape is identified by a representative alias that must resolve.
    for (shape, anchor) in [
        ("multi-correction chain", "jwt.head"),
        ("dangling deprecated", "cache.stale"),
        ("superseded-but-Active", "retry.legacy"),
        ("deprecated-but-connected", "db.dep1"),
    ] {
        assert!(
            corpus.alias_map.resolve(anchor).is_some(),
            "required shape '{shape}' missing (anchor alias '{anchor}' did not resolve)"
        );
    }

    // The multi-correction chain head redirects its superseded predecessors.
    let members = corpus.alias_map.head_members("jwt.head");
    assert!(
        members.len() >= 3,
        "multi-correction chain head must redirect >=3 predecessors (depth>1), got {}",
        members.len()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_each_shape_loads_and_searches() {
    let dir = fixtures_dir();
    let out = TempDir::new().unwrap();
    let target = out.path().join("snap.db");

    // Load + embed-at-load so the snapshot is searchable.
    let corpus = load_fixture_corpus_with_embeddings(&dir, &target, &DeterministicProvider)
        .await
        .expect("shipped corpus loads + embeds");

    // The sibling vector dir exists with a populated index (from_profile loads it).
    let vector_dir = target.parent().unwrap().join("vector");
    assert!(
        vector_dir.join("unimatrix-vector.meta").exists(),
        "embed-at-load must produce a vector index beside the snapshot"
    );

    // Every scenario's query returns a non-empty ranked list via direct index search
    // (the model-free search seam: same provider for query + corpus).
    let store = Arc::new(
        unimatrix_store::SqlxStore::open_readonly(&corpus.db_path)
            .await
            .expect("reopen snapshot"),
    );
    let index = VectorIndex::load(store, VectorConfig::default(), &vector_dir)
        .await
        .expect("load dumped index");
    assert!(index.point_count() > 0, "index must carry fixture vectors");

    for scenario in &corpus.scenarios {
        let q = DeterministicProvider
            .embed(&scenario.query)
            .expect("embed query");
        let hits = index.search(&q, 10, 32).expect("search");
        assert!(
            !hits.is_empty(),
            "scenario '{}' search returned empty — AC-14 would be vacuous",
            scenario.id
        );
    }
}

// ---------------------------------------------------------------------------
// AC-14 floor — a non-empty-results smoke test for one AC-14 scenario (R-15)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_ac14_scenario_search_returns_non_empty_ranked_list() {
    let dir = fixtures_dir();
    let out = TempDir::new().unwrap();
    let target = out.path().join("snap.db");
    let corpus = load_fixture_corpus_with_embeddings(&dir, &target, &DeterministicProvider)
        .await
        .expect("load + embed");

    let vector_dir = target.parent().unwrap().join("vector");
    let store = Arc::new(
        unimatrix_store::SqlxStore::open_readonly(&corpus.db_path)
            .await
            .expect("reopen"),
    );
    let index = VectorIndex::load(store, VectorConfig::default(), &vector_dir)
        .await
        .expect("load index");

    // The deprecated-connected rank-below scenario: BOTH a deprecated anchor and a
    // weak-active anchor must be present in the result set so a rank_below(A,B)
    // assertion is evaluated NON-vacuously (the canonical AC-14 non-vacuous case).
    let dep = corpus.alias_map.resolve("db.dep1").expect("dep1 resolves");
    let act = corpus
        .alias_map
        .resolve("db.actWeak")
        .expect("actWeak resolves");

    let query = "how should I size the database connection pool";
    let q = DeterministicProvider.embed(query).expect("embed query");
    // k larger than corpus so both anchors are reachable in the result set.
    let hits = index.search(&q, 64, 64).expect("search");
    assert!(!hits.is_empty(), "AC-14 smoke search must be non-empty");

    let returned: Vec<u64> = hits.iter().map(|h| h.entry_id).collect();
    assert!(
        returned.contains(&dep) && returned.contains(&act),
        "non-vacuous AC-14: both rank_below anchors must be present in results \
         (dep={dep}, act={act}, returned={returned:?})"
    );
}

// ---------------------------------------------------------------------------
// ADR-004 §5 — deprecated-connected crossover sits in a BRACKETED RANGE
// ---------------------------------------------------------------------------

#[test]
fn test_deprecated_connected_crossover_is_bracketed() {
    // The §5 obligation: the deprecated-but-connected shape must carry enough
    // variation that the steepness crossover is a RANGE, not a single exemplar.
    // We assert the structural precondition that makes a bracket possible:
    //   * >=4 distinct connected-deprecated entries (the swept candidates), and
    //   * >=3 distinct active entries (a weakest-active band, not one line),
    //   * spanning a spread of chain-depth (standalone, depth-1, depth-2) so the
    //     penalty a sweep applies lands at a sequence of distinct points.
    let fixtures = shipped_fixtures();
    let (_, depconn) = fixtures
        .iter()
        .find(|(name, _)| name == "deprecated_connected.toml")
        .expect("deprecated_connected shape present");

    let deprecated: Vec<&str> = depconn
        .entries
        .iter()
        .filter(|e| e.status == "Deprecated")
        .map(|e| e.alias.as_str())
        .collect();
    let active: Vec<&str> = depconn
        .entries
        .iter()
        .filter(|e| e.status == "Active")
        .map(|e| e.alias.as_str())
        .collect();

    assert!(
        deprecated.len() >= 4,
        "crossover bracket needs >=4 connected-deprecated candidates, got {}: {deprecated:?}",
        deprecated.len()
    );
    assert!(
        active.len() >= 3,
        "weakest-active band needs >=3 active entries, got {}: {active:?}",
        active.len()
    );

    // Connectivity spread: at least one standalone-deprecated (no successor) AND at
    // least one chained-deprecated (has a successor) so depth varies across the band.
    let standalone = depconn
        .entries
        .iter()
        .any(|e| e.status == "Deprecated" && e.superseded_by.is_empty());
    let chained = depconn
        .entries
        .iter()
        .any(|e| e.status == "Deprecated" && !e.superseded_by.is_empty());
    assert!(
        standalone && chained,
        "deprecated-connected must span standalone AND chained connectivity \
         (standalone={standalone}, chained={chained})"
    );
}

// ---------------------------------------------------------------------------
// Manifest stamp — shape_hash matches the hash computed over THIS corpus's schema
// ---------------------------------------------------------------------------

/// Parsed manifest stamp (`fixtures/manifest.toml`).
#[derive(Debug, serde::Deserialize)]
struct ManifestStamp {
    manifest_version: u32,
    #[allow(dead_code)]
    migration_number: u32,
    shape_hash: String,
}

fn read_manifest_stamp() -> ManifestStamp {
    let path = fixtures_dir().join("manifest.toml");
    let text = std::fs::read_to_string(&path).expect("read manifest stamp");
    toml::from_str(&text).expect("parse manifest stamp")
}

/// The running manifest the drift guard computes at eval start for this corpus.
fn running_manifest() -> crate::eval::shape::ShapeManifest {
    use crate::infra::config::InferenceConfig;
    use unimatrix_embed::EmbeddingModel;
    crate::eval::shape::build_running_manifest(
        &EmbeddingModel::default(),
        &InferenceConfig::default(),
    )
}

#[test]
fn test_manifest_stamp_matches_computed_shape_hash() {
    let stamp = read_manifest_stamp();
    let computed = crate::eval::shape::compute_shape_hash(&running_manifest());

    assert_eq!(
        stamp.manifest_version,
        crate::eval::shape::MANIFEST_VERSION,
        "stamp manifest_version must match the live MANIFEST_VERSION"
    );
    assert_eq!(
        stamp.shape_hash.len(),
        64,
        "shape_hash must be 64 lowercase hex chars"
    );
    assert!(
        stamp
            .shape_hash
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "shape_hash must be lowercase hex"
    );
    assert_eq!(
        stamp.shape_hash, computed,
        "corpus manifest stamp is STALE — recompute via stamp_corpus and update fixtures/manifest.toml"
    );
}

// ---------------------------------------------------------------------------
// Dead-end edge — embed step does not panic on a no-Active-terminal chain
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_dead_end_chain_loads_and_embeds_without_panic() {
    let dir = fixtures_dir();
    let out = TempDir::new().unwrap();
    let target = out.path().join("snap.db");
    let corpus = load_fixture_corpus_with_embeddings(&dir, &target, &DeterministicProvider)
        .await
        .expect("dead-end chain present in corpus loads + embeds cleanly");
    // The dead-end terminal resolves (it is a real entry) but is Deprecated, so its
    // redirect assertion is a DEFINED FAIL at eval — not a load/embed panic here.
    assert!(corpus.alias_map.resolve("flag.terminal").is_some());
}

// ---------------------------------------------------------------------------
// Helper presence (silences dead-code lints in non-stamp builds)
// ---------------------------------------------------------------------------

#[test]
fn test_embed_separator_prepare_text_roundtrip() {
    // Guards that the embed text shape used by embed-at-load is what we expect.
    let t = prepare_text("Title", "Body", ": ");
    assert_eq!(t, "Title: Body");
    let _ = embed_and_write_vectors; // path-import liveness
}
