//! Model-availability guard for server-level pipeline tests (#723).
//!
//! `skip_if_no_model()` decides whether the `pipeline_e2e` / `graph_subgraph_*`
//! integration tests can run against a real ONNX model. It MUST derive the model
//! directory the same way the downloader (`ensure_model`) and loader
//! (`OnnxProvider::new`) do — via [`EmbeddingModel::cache_subdir`].
//!
//! Born wrong in col-015 (#161): the guard re-derived the dir name with a `--`
//! separator while the canonical helper uses `_`. The drifted path never existed,
//! so every guarded test silently skipped and reported a vacuous green (#723).
//!
//! This module makes the path derivation a single source of truth and replaces
//! the fail-green behavior with a three-state classifier that fails LOUDLY when a
//! model dir for the target id is present but at a non-canonical (drifted) path.

use std::path::{Path, PathBuf};

use unimatrix_embed::{EmbedConfig, EmbeddingModel};

/// Canonical on-disk model directory for `model` under `cache_dir`.
///
/// Single source of truth for the guard's path derivation: delegates to
/// [`EmbeddingModel::cache_subdir`] — the SAME helper the downloader and loader
/// use. Exposed as a standalone fn (not inlined) so the structural regression
/// test can assert guard-path == `cache_subdir()`-path WITHOUT a model on disk.
fn canonical_model_dir(cache_dir: &Path, model: EmbeddingModel) -> PathBuf {
    cache_dir.join(model.cache_subdir())
}

/// Three-state classification of a model's on-disk presence under `cache_dir`.
enum ModelDirStatus {
    /// Canonical path exists with the ONNX file — tests may run.
    Present,
    /// No directory for THIS model id exists under `cache_dir` — skip cleanly.
    Absent,
    /// A directory resolving to THIS model id exists, but NOT at the canonical
    /// path (separator drift). Carries (found, canonical) for the loud-fail
    /// message. NOT a skip — a hard failure.
    Mismatched { found: PathBuf, canonical: PathBuf },
}

/// Plausible sanitized directory names for `model_id`'s org/repo `/` boundary.
///
/// `cache_subdir()` replaces `/` with `_`; historical drift used `--` (#723);
/// `-` is included for completeness. These are the only forms that resolve to
/// THIS model id. Matching against this exact set — rather than a fuzzy
/// normalization — guarantees a sibling model dir (e.g. the NLI model's
/// `nli-minilm2-l6-h768`) can never trigger a false mismatch: it is not in the set.
fn drift_candidate_names(model_id: &str) -> Vec<String> {
    ["_", "--", "-"]
        .iter()
        .map(|sep| model_id.replace('/', sep))
        .collect()
}

/// Classify the model's on-disk state.
///
/// Parameterized over `cache_dir` / `model` / `onnx_filename` so it is testable
/// against a temp dir with no real model.
fn classify_model_dir(
    cache_dir: &Path,
    model: EmbeddingModel,
    onnx_filename: &str,
) -> ModelDirStatus {
    let canonical = canonical_model_dir(cache_dir, model);
    if canonical.join(onnx_filename).exists() {
        return ModelDirStatus::Present;
    }

    // Canonical path has no model. Check whether a DRIFTED dir for THIS model id
    // exists (separator mismatch) before concluding the model is absent.
    let canonical_name = model.cache_subdir();
    let candidates = drift_candidate_names(model.model_id());
    if let Ok(entries) = std::fs::read_dir(cache_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // Skip the canonical name itself — its missing ONNX file is a genuine
            // "absent" signal, not drift.
            if name == canonical_name {
                continue;
            }
            if candidates.iter().any(|c| c == name.as_ref()) {
                return ModelDirStatus::Mismatched {
                    found: entry.path(),
                    canonical,
                };
            }
        }
    }

    ModelDirStatus::Absent
}

/// Check if the ONNX model is available.
///
/// Returns `true` if the model is NOT found (i.e., tests should skip).
///
/// Distinguishes three states (#723):
/// 1. Canonical path present → returns `false`, tests run.
/// 2. Genuinely absent → returns `true`, tests skip cleanly (model-less CI).
/// 3. A dir for THIS model id present but at a NON-canonical (drifted) path →
///    **panics** with the found vs expected paths. This is the fail-green guard
///    that masked #723; it now fails loudly instead of skipping.
pub fn skip_if_no_model() -> bool {
    let config = EmbedConfig::default();
    let cache_dir = config.resolve_cache_dir();
    let onnx_filename = config.model.onnx_filename();

    match classify_model_dir(&cache_dir, config.model, onnx_filename) {
        ModelDirStatus::Present => false,
        ModelDirStatus::Absent => {
            eprintln!(
                "ONNX model not found at {}, skipping pipeline_e2e test",
                canonical_model_dir(&cache_dir, config.model)
                    .join(onnx_filename)
                    .display()
            );
            true
        }
        ModelDirStatus::Mismatched { found, canonical } => {
            panic!(
                "model dir separator mismatch (#723): a directory for `{}` exists at \
                 {} but the canonical path is {}. The downloader/loader use \
                 EmbeddingModel::cache_subdir() (`_`-separated); align the on-disk dir \
                 (or remove the drifted alias) rather than letting the guard skip silently.",
                config.model.model_id(),
                found.display(),
                canonical.display(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (b) STRUCTURAL REGRESSION TEST for #723.
    ///
    /// The directory the guard checks MUST be byte-identical to the directory the
    /// downloader/loader use — both derived from `cache_subdir()`. This is a pure
    /// path-construction assertion: it runs WITHOUT a model on disk and fails if
    /// anyone re-introduces separator drift (the col-015/#161 defect).
    #[test]
    fn test_canonical_model_dir_matches_cache_subdir_no_drift() {
        let cache_dir = Path::new("/tmp/unimatrix-test-cache");
        let model = EmbeddingModel::default();

        let guard_dir = canonical_model_dir(cache_dir, model);
        // The downloader/loader path: cache_dir.join(model.cache_subdir()).
        let downloader_dir = cache_dir.join(model.cache_subdir());

        assert_eq!(
            guard_dir, downloader_dir,
            "guard model dir drifted from cache_subdir()-derived downloader dir (#723)"
        );
        // Explicitly assert the canonical `_` separator, NOT the historical `--`.
        assert_eq!(
            guard_dir,
            cache_dir.join("sentence-transformers_all-MiniLM-L6-v2"),
            "default model dir must use the canonical `_` separator"
        );
    }

    /// State 1: canonical path with the ONNX file present → not a skip.
    #[test]
    fn test_classify_model_dir_present_returns_present() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let model = EmbeddingModel::default();
        let canonical = canonical_model_dir(tmp.path(), model);
        std::fs::create_dir_all(&canonical).expect("create canonical dir");
        std::fs::write(canonical.join(model.onnx_filename()), b"onnx").expect("write onnx");

        assert!(matches!(
            classify_model_dir(tmp.path(), model, model.onnx_filename()),
            ModelDirStatus::Present
        ));
    }

    /// State 2: nothing on disk → skip cleanly.
    #[test]
    fn test_classify_model_dir_absent_returns_absent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let model = EmbeddingModel::default();

        assert!(matches!(
            classify_model_dir(tmp.path(), model, model.onnx_filename()),
            ModelDirStatus::Absent
        ));
    }

    /// State 2 (no false-fail): a SIBLING model dir (the NLI model) present must
    /// NOT trigger a mismatch for the embedding model.
    #[test]
    fn test_classify_model_dir_sibling_nli_dir_does_not_false_fail() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let model = EmbeddingModel::default();
        // NLI sibling cache dir — must be ignored by the embedding-model guard.
        std::fs::create_dir_all(tmp.path().join("nli-minilm2-l6-h768")).expect("create nli dir");

        assert!(
            matches!(
                classify_model_dir(tmp.path(), model, model.onnx_filename()),
                ModelDirStatus::Absent
            ),
            "a sibling NLI model dir must not be read as embedding-model separator drift"
        );
    }

    /// State 3: a `--`-drifted dir for THIS model id present, canonical absent →
    /// loud-fail (Mismatched), naming both paths.
    #[test]
    fn test_classify_model_dir_drifted_returns_mismatched() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let model = EmbeddingModel::default();
        // Historical `--` drift dir, populated, with NO canonical `_` dir.
        let drifted = tmp.path().join("sentence-transformers--all-MiniLM-L6-v2");
        std::fs::create_dir_all(&drifted).expect("create drifted dir");
        std::fs::write(drifted.join(model.onnx_filename()), b"onnx").expect("write onnx");

        match classify_model_dir(tmp.path(), model, model.onnx_filename()) {
            ModelDirStatus::Mismatched { found, canonical } => {
                assert_eq!(found, drifted, "found path must name the drifted dir");
                assert_eq!(
                    canonical,
                    canonical_model_dir(tmp.path(), model),
                    "canonical path must be the cache_subdir()-derived dir"
                );
            }
            _ => panic!("drifted model dir must classify as Mismatched, not skip (#723)"),
        }
    }

    /// State 1 precedence: when BOTH the canonical dir (with model) and a drifted
    /// alias exist, the canonical presence wins — no false loud-fail.
    #[test]
    fn test_classify_model_dir_canonical_present_with_drift_alias_is_present() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let model = EmbeddingModel::default();
        let canonical = canonical_model_dir(tmp.path(), model);
        std::fs::create_dir_all(&canonical).expect("create canonical dir");
        std::fs::write(canonical.join(model.onnx_filename()), b"onnx").expect("write onnx");
        std::fs::create_dir_all(tmp.path().join("sentence-transformers--all-MiniLM-L6-v2"))
            .expect("create drift alias");

        assert!(matches!(
            classify_model_dir(tmp.path(), model, model.onnx_filename()),
            ModelDirStatus::Present
        ));
    }

    /// `drift_candidate_names` covers the canonical and historical separators and
    /// excludes any NLI-style name.
    #[test]
    fn test_drift_candidate_names_includes_canonical_and_historical() {
        let names = drift_candidate_names("sentence-transformers/all-MiniLM-L6-v2");
        assert!(names.contains(&"sentence-transformers_all-MiniLM-L6-v2".to_string()));
        assert!(names.contains(&"sentence-transformers--all-MiniLM-L6-v2".to_string()));
        assert!(!names.iter().any(|n| n.contains("nli-minilm2")));
    }
}
