//! Fixture-corpus sweep entry point (nan-018 Wave-1 AC-14 capstone).
//!
//! `run_fixture_sweep` is the proof-by-use spine: it wires the three Wave-1
//! components into one correlated steepness sweep over the durable hand-authored
//! fixture corpus, so a single run reports — for the same scenarios — trust
//! outcomes (AC-02/03) AND P@5/MRR (AC-04) AND token-weighted cost (AC-09).
//!
//! It performs the three capstone wiring jobs:
//! 1. **Drift guard on load (AC-14 cond. 5, LOCKED §7.2)** — before any replay, it
//!    builds the RUNNING retrieval-shape manifest from live inputs and
//!    [`check_drift`]s it against the primary fixture corpus stamp. A mismatch is a
//!    HARD ERROR (`PrimaryFixture` ⇒ `ShapeDriftError::HardAbort`) that aborts the
//!    sweep with a non-zero result — the guard ACTUALLY fires here.
//! 2. **Alias-map threading on the fixture path (AC-14 cond. 1)** — it loads the
//!    corpus via [`load_fixture_corpus_with_embeddings`] (ADR-002 branch (b)) so the
//!    snapshot is end-to-end searchable, then replays each profile threading
//!    `Some(&alias_map)` so property-based trust evaluates NON-vacuously against a
//!    non-empty result set. (Log-sourced JSONL runs keep `None` — see `mod.rs`.)
//! 3. **The lever is live (AC-14 cond. 3)** — each profile's `[graph_penalty]` TOML
//!    is resolved and threaded into its `SearchService` at the eval profile layer
//!    (`eval/profile/layer.rs`), so two profiles whose TOML differs in a penalty
//!    lever produce DIFFERENT penalties ⇒ an observable ranking/penalty delta.
//!
//! ## Provider injection (R-15 non-vacuous)
//!
//! The corpus is embedded with the caller-supplied [`EmbeddingProvider`]. The
//! SearchService embeds the *query* through each layer's embed handle, so the SAME
//! provider is injected into that handle ([`EmbedServiceHandle::set_ready_with_provider`]);
//! otherwise query and corpus vectors come from different models and search returns
//! empty (the vacuous-proof failure). Production passes the real ONNX provider; the
//! AC-14 proof test passes the deterministic in-memory provider used by the fixtures
//! smoke tests (model-free, offline, reproducible).

use std::path::Path;
use std::sync::Arc;

use unimatrix_embed::{EmbeddingModel, EmbeddingProvider};

use crate::eval::corpus::{LoadedCorpus, load_fixture_corpus_with_embeddings};
use crate::eval::profile::{EvalProfile, EvalServiceLayer};
use crate::eval::shape::{CorpusKind, ShapeManifest, build_running_manifest, check_drift};
use crate::infra::config::InferenceConfig;

use super::layer;
use super::replay;

/// Outcome of a fixture-corpus sweep: where the results live + the loaded corpus
/// (its `alias_map` and `scenarios` let the caller assert AC-14 conditions).
#[derive(Debug)]
pub struct SweepOutcome {
    /// The materialized + embedded fixture corpus (db path, alias map, scenarios).
    pub corpus: LoadedCorpus,
}

/// The primary fixture-corpus manifest stamp (`fixtures/manifest.toml`).
///
/// `migration_number` is human-legibility ONLY — never a hash input (ADR-002).
#[derive(Debug, serde::Deserialize)]
struct ManifestStamp {
    #[allow(dead_code)]
    manifest_version: u32,
    #[allow(dead_code)]
    migration_number: u32,
    shape_hash: String,
}

/// Read + parse the primary fixture corpus manifest stamp from `corpus_dir`.
fn read_manifest_stamp(corpus_dir: &Path) -> Result<ManifestStamp, Box<dyn std::error::Error>> {
    let path = corpus_dir.join("manifest.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("read corpus manifest stamp {}: {e}", path.display()))?;
    let stamp: ManifestStamp = toml::from_str(&text)
        .map_err(|e| format!("parse corpus manifest stamp {}: {e}", path.display()))?;
    Ok(stamp)
}

/// Build the RUNNING retrieval-shape manifest from live inputs (R-05).
///
/// Embedding identity (`model_id` + `dimension`) is read LIVE from
/// [`EmbeddingModel::default`] — never a literal — so embed-model drift trips the
/// guard (ADR-002 branch (b)).
fn running_manifest() -> ShapeManifest {
    build_running_manifest(&EmbeddingModel::default(), &InferenceConfig::default())
}

/// Run the deterministic drift guard against the primary fixture corpus stamp.
///
/// HARD ERROR (abort, propagated as `Err`) on mismatch (`PrimaryFixture` severity,
/// LOCKED §7.2). The guard ACTUALLY fires from here — it is not advisory.
fn guard_corpus_shape(corpus_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let stamp = read_manifest_stamp(corpus_dir)?;
    let running = running_manifest();
    check_drift(&running, &stamp.shape_hash, CorpusKind::PrimaryFixture)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    Ok(())
}

/// Run a correlated steepness sweep over the primary fixture corpus.
///
/// Steps (the AC-14 capstone spine):
/// 1. **Drift guard** — `guard_corpus_shape`: HARD ERROR on shape mismatch BEFORE any
///    work (AC-14 cond. 5).
/// 2. **Load + embed-at-load** — `load_fixture_corpus_with_embeddings` with the injected
///    `provider` so the snapshot is searchable (R-15 non-vacuous).
/// 3. **Build one layer per profile** — `EvalServiceLayer::from_profile`. The profile's
///    `[graph_penalty]` resolves into the SearchService (lever live, AC-14 cond. 3).
///    The SAME `provider` is injected into each layer's embed handle so the query is
///    embedded with the model the corpus was embedded with.
/// 4. **Replay** threading `Some(&alias_map)` so trust evaluates NON-vacuously
///    (AC-14 cond. 1) and writes one JSON result per scenario to `out`.
///
/// `profiles` is ordered: the first profile is the BASELINE (default penalties); the
/// rest are swept candidates. `corpus_dir` defaults via [`default_fixtures_dir`] when
/// the caller has no override.
pub async fn run_fixture_sweep(
    corpus_dir: &Path,
    target_db: &Path,
    profiles: &[EvalProfile],
    k: usize,
    out: &Path,
    provider: Arc<dyn EmbeddingProvider>,
    project_dir: Option<&Path>,
) -> Result<SweepOutcome, Box<dyn std::error::Error>> {
    if profiles.is_empty() {
        return Err("run_fixture_sweep: at least one profile is required".into());
    }
    if k == 0 {
        return Err("run_fixture_sweep: k must be >= 1".into());
    }

    // 1. Drift guard (AC-14 cond. 5, LOCKED §7.2) — fires BEFORE any replay.
    guard_corpus_shape(corpus_dir)?;

    // 2. Load + embed-at-load (ADR-002 branch (b)) so search is non-vacuous (R-15).
    let corpus =
        load_fixture_corpus_with_embeddings(corpus_dir, target_db, provider.as_ref()).await?;

    std::fs::create_dir_all(out)?;

    // 3. Build one EvalServiceLayer per profile. The profile's graph_penalty.resolve_params()
    //    is threaded into the SearchService at from_profile (lever live, AC-14 cond. 3).
    let mut layers: Vec<EvalServiceLayer> = Vec::with_capacity(profiles.len());
    for profile in profiles {
        let layer = EvalServiceLayer::from_profile(&corpus.db_path, profile, project_dir).await?;

        // Inject the SAME provider into the search path so the query embedding matches
        // the corpus embedding (R-15). Without this, query vectors (ONNX) and corpus
        // vectors (injected provider) diverge and search returns empty.
        layer
            .embed_handle()
            .set_ready_with_provider(Arc::clone(&provider))
            .await;

        // Confirm readiness through the same poll the JSONL path uses (defensive).
        layer::wait_for_embed_model(&layer.embed_handle(), &profile.name).await?;

        layers.push(layer);
    }

    // 4. Replay threading Some(&alias_map) so trust evaluates non-vacuously (AC-14 cond. 1).
    replay::run_replay_loop(
        profiles,
        &layers,
        &corpus.scenarios,
        k,
        out,
        Some(&corpus.alias_map),
    )
    .await?;

    Ok(SweepOutcome { corpus })
}

/// Absolute path to the shipped primary-corpus fixtures directory.
///
/// The canonical location the drift-guarded corpus + manifest stamp live at
/// (`crates/unimatrix-server/src/eval/corpus/fixtures/`). Callers may override with
/// their own `corpus_dir`; this is the default the CLI and AC-14 proof use.
pub fn default_fixtures_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/eval/corpus/fixtures")
}
