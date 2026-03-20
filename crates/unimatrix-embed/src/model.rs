/// Pre-configured catalog of 384-d sentence-transformer models.
///
/// All models produce 384-dimensional embeddings compatible with
/// nxs-002's `VectorConfig { dimension: 384 }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmbeddingModel {
    /// sentence-transformers/all-MiniLM-L6-v2 (default)
    #[default]
    AllMiniLmL6V2,
    /// sentence-transformers/all-MiniLM-L12-v2
    AllMiniLmL12V2,
    /// sentence-transformers/multi-qa-MiniLM-L6-cos-v1
    MultiQaMiniLmL6,
    /// sentence-transformers/paraphrase-MiniLM-L6-v2
    ParaphraseMiniLmL6V2,
    /// BAAI/bge-small-en-v1.5
    BgeSmallEnV15,
    /// intfloat/e5-small-v2
    E5SmallV2,
    /// thenlper/gte-small
    GteSmall,
}

impl EmbeddingModel {
    /// HuggingFace model repository ID.
    pub fn model_id(&self) -> &'static str {
        match self {
            Self::AllMiniLmL6V2 => "sentence-transformers/all-MiniLM-L6-v2",
            Self::AllMiniLmL12V2 => "sentence-transformers/all-MiniLM-L12-v2",
            Self::MultiQaMiniLmL6 => "sentence-transformers/multi-qa-MiniLM-L6-cos-v1",
            Self::ParaphraseMiniLmL6V2 => "sentence-transformers/paraphrase-MiniLM-L6-v2",
            Self::BgeSmallEnV15 => "BAAI/bge-small-en-v1.5",
            Self::E5SmallV2 => "intfloat/e5-small-v2",
            Self::GteSmall => "thenlper/gte-small",
        }
    }

    /// Output embedding dimension (always 384 for all catalog models).
    pub fn dimension(&self) -> usize {
        384
    }

    /// Maximum input sequence length in word-piece tokens.
    pub fn max_seq_length(&self) -> usize {
        match self {
            Self::AllMiniLmL6V2 => 256,
            Self::AllMiniLmL12V2 => 256,
            Self::MultiQaMiniLmL6 => 256,
            Self::ParaphraseMiniLmL6V2 => 256,
            Self::BgeSmallEnV15 => 512,
            Self::E5SmallV2 => 512,
            Self::GteSmall => 512,
        }
    }

    /// ONNX model path within the HuggingFace repository.
    ///
    /// Most sentence-transformer models store ONNX models under `onnx/model.onnx`.
    pub fn onnx_repo_path(&self) -> &'static str {
        "onnx/model.onnx"
    }

    /// Local filename for the cached ONNX model.
    pub fn onnx_filename(&self) -> &'static str {
        "model.onnx"
    }

    /// Sanitized directory name for cache (slash replaced with underscore).
    pub fn cache_subdir(&self) -> String {
        self.model_id().replace('/', "_")
    }
}

/// Catalog of known NLI cross-encoder ONNX model variants.
///
/// Mirrors `EmbeddingModel` conventions: `model_id`, `onnx_repo_path`,
/// `onnx_filename`, `cache_subdir`. Used by `NliProvider` and `ensure_nli_model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NliModel {
    /// `cross-encoder/nli-MiniLM2-L6-H768`
    ///
    /// ~85 MB, Apache 2.0. ONNX export confirmed available. Primary model.
    NliMiniLM2L6H768,
    /// `cross-encoder/nli-deberta-v3-small`
    ///
    /// ~180 MB. ONNX availability must be verified at implementation time (SR-01, ADR-003).
    /// `onnx_filename()` returns `"model.onnx"` as best-effort; implementer must confirm
    /// actual filename when testing against a downloaded copy.
    NliDebertaV3Small,
}

impl NliModel {
    /// Resolve from a config string identifier. Returns `None` for unrecognized values.
    ///
    /// Called by `InferenceConfig::validate()`; `None` triggers startup abort (AC-17, R-15).
    ///
    /// Recognized names (case-sensitive): `"minilm2"`, `"deberta"`.
    pub fn from_config_name(name: &str) -> Option<Self> {
        match name {
            "minilm2" => Some(Self::NliMiniLM2L6H768),
            "deberta" => Some(Self::NliDebertaV3Small),
            _ => None,
        }
    }

    /// HuggingFace model repository ID.
    pub fn model_id(&self) -> &'static str {
        match self {
            Self::NliMiniLM2L6H768 => "cross-encoder/nli-MiniLM2-L6-H768",
            Self::NliDebertaV3Small => "cross-encoder/nli-deberta-v3-small",
        }
    }

    /// Repo path for the ONNX file download via `hf-hub`.
    ///
    /// Both cross-encoder variants export ONNX under `onnx/model.onnx` (confirmed on HF hub).
    pub fn onnx_repo_path(&self) -> &'static str {
        match self {
            Self::NliMiniLM2L6H768 => "onnx/model.onnx",
            Self::NliDebertaV3Small => "onnx/model.onnx",
        }
    }

    /// Local ONNX filename. Both variants use the standard optimum-exported filename.
    pub fn onnx_filename(&self) -> &'static str {
        "model.onnx"
    }

    /// Local cache subdirectory name. No slashes — safe for filesystem paths (R-18).
    pub fn cache_subdir(&self) -> &'static str {
        match self {
            Self::NliMiniLM2L6H768 => "nli-minilm2-l6-h768",
            Self::NliDebertaV3Small => "nli-deberta-v3-small",
        }
    }
}

#[cfg(test)]
mod nli_model_tests {
    use super::*;

    #[test]
    fn test_from_config_name_minilm2() {
        assert_eq!(
            NliModel::from_config_name("minilm2"),
            Some(NliModel::NliMiniLM2L6H768)
        );
    }

    #[test]
    fn test_from_config_name_deberta() {
        assert_eq!(
            NliModel::from_config_name("deberta"),
            Some(NliModel::NliDebertaV3Small)
        );
    }

    #[test]
    fn test_from_config_name_unknown_returns_none() {
        assert_eq!(NliModel::from_config_name("gpt4"), None);
        assert_eq!(NliModel::from_config_name(""), None);
        // Case-sensitive: uppercase should not match
        assert_eq!(NliModel::from_config_name("MINILM2"), None);
        assert_eq!(NliModel::from_config_name("MiniLM2"), None);
    }

    #[test]
    fn test_nli_minilm2_model_id() {
        assert_eq!(
            NliModel::NliMiniLM2L6H768.model_id(),
            "cross-encoder/nli-MiniLM2-L6-H768"
        );
    }

    #[test]
    fn test_nli_deberta_model_id() {
        assert_eq!(
            NliModel::NliDebertaV3Small.model_id(),
            "cross-encoder/nli-deberta-v3-small"
        );
    }

    #[test]
    fn test_nli_model_onnx_filename_returns_model_onnx() {
        assert_eq!(NliModel::NliMiniLM2L6H768.onnx_filename(), "model.onnx");
        assert_eq!(NliModel::NliDebertaV3Small.onnx_filename(), "model.onnx");
    }

    #[test]
    fn test_nli_model_methods_return_non_empty() {
        for model in [NliModel::NliMiniLM2L6H768, NliModel::NliDebertaV3Small] {
            assert!(!model.model_id().is_empty(), "{model:?} has empty model_id");
            assert!(
                !model.onnx_repo_path().is_empty(),
                "{model:?} has empty onnx_repo_path"
            );
            assert!(
                !model.onnx_filename().is_empty(),
                "{model:?} has empty onnx_filename"
            );
            assert!(
                !model.cache_subdir().is_empty(),
                "{model:?} has empty cache_subdir"
            );
        }
    }

    #[test]
    fn test_nli_model_cache_subdirs_distinct() {
        // R-18: distinct subdirs prevent tokenizer cross-contamination.
        let minilm_dir = NliModel::NliMiniLM2L6H768.cache_subdir();
        let deberta_dir = NliModel::NliDebertaV3Small.cache_subdir();
        assert_ne!(
            minilm_dir, deberta_dir,
            "cache_subdir must differ between model variants to prevent tokenizer confusion"
        );
    }

    #[test]
    fn test_nli_model_cache_subdirs_no_slash() {
        // R-18: no slashes in cache subdir (safe for filesystem paths).
        for model in [NliModel::NliMiniLM2L6H768, NliModel::NliDebertaV3Small] {
            let subdir = model.cache_subdir();
            assert!(
                !subdir.contains('/'),
                "{model:?} cache_subdir contains slash: {subdir}"
            );
        }
    }

    #[test]
    fn test_nli_model_derives() {
        let model = NliModel::NliMiniLM2L6H768;
        let cloned = model;
        assert_eq!(model, cloned);
        let debug = format!("{model:?}");
        assert!(debug.contains("NliMiniLM2L6H768"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_MODELS: [EmbeddingModel; 7] = [
        EmbeddingModel::AllMiniLmL6V2,
        EmbeddingModel::AllMiniLmL12V2,
        EmbeddingModel::MultiQaMiniLmL6,
        EmbeddingModel::ParaphraseMiniLmL6V2,
        EmbeddingModel::BgeSmallEnV15,
        EmbeddingModel::E5SmallV2,
        EmbeddingModel::GteSmall,
    ];

    #[test]
    fn test_default_model() {
        let model = EmbeddingModel::default();
        assert_eq!(model, EmbeddingModel::AllMiniLmL6V2);
    }

    #[test]
    fn test_all_models_have_model_id() {
        for model in &ALL_MODELS {
            let id = model.model_id();
            assert!(!id.is_empty(), "{model:?} has empty model_id");
            assert!(id.contains('/'), "{model:?} model_id missing slash: {id}");
        }
    }

    #[test]
    fn test_all_models_dimension_384() {
        for model in &ALL_MODELS {
            assert_eq!(model.dimension(), 384, "{model:?} has wrong dimension");
        }
    }

    #[test]
    fn test_max_seq_length_values() {
        assert_eq!(EmbeddingModel::AllMiniLmL6V2.max_seq_length(), 256);
        assert_eq!(EmbeddingModel::AllMiniLmL12V2.max_seq_length(), 256);
        assert_eq!(EmbeddingModel::MultiQaMiniLmL6.max_seq_length(), 256);
        assert_eq!(EmbeddingModel::ParaphraseMiniLmL6V2.max_seq_length(), 256);
        assert_eq!(EmbeddingModel::BgeSmallEnV15.max_seq_length(), 512);
        assert_eq!(EmbeddingModel::E5SmallV2.max_seq_length(), 512);
        assert_eq!(EmbeddingModel::GteSmall.max_seq_length(), 512);
    }

    #[test]
    fn test_onnx_filename() {
        for model in &ALL_MODELS {
            assert_eq!(model.onnx_filename(), "model.onnx");
        }
    }

    #[test]
    fn test_onnx_repo_path() {
        for model in &ALL_MODELS {
            assert_eq!(model.onnx_repo_path(), "onnx/model.onnx");
        }
    }

    #[test]
    fn test_cache_subdir_sanitization() {
        assert_eq!(
            EmbeddingModel::AllMiniLmL6V2.cache_subdir(),
            "sentence-transformers_all-MiniLM-L6-v2"
        );
        assert_eq!(
            EmbeddingModel::BgeSmallEnV15.cache_subdir(),
            "BAAI_bge-small-en-v1.5"
        );
        for model in &ALL_MODELS {
            let subdir = model.cache_subdir();
            assert!(
                !subdir.contains('/'),
                "{model:?} cache_subdir contains slash: {subdir}"
            );
        }
    }

    #[test]
    fn test_model_id_known_values() {
        assert_eq!(
            EmbeddingModel::AllMiniLmL6V2.model_id(),
            "sentence-transformers/all-MiniLM-L6-v2"
        );
        assert_eq!(
            EmbeddingModel::AllMiniLmL12V2.model_id(),
            "sentence-transformers/all-MiniLM-L12-v2"
        );
        assert_eq!(
            EmbeddingModel::MultiQaMiniLmL6.model_id(),
            "sentence-transformers/multi-qa-MiniLM-L6-cos-v1"
        );
        assert_eq!(
            EmbeddingModel::ParaphraseMiniLmL6V2.model_id(),
            "sentence-transformers/paraphrase-MiniLM-L6-v2"
        );
        assert_eq!(
            EmbeddingModel::BgeSmallEnV15.model_id(),
            "BAAI/bge-small-en-v1.5"
        );
        assert_eq!(EmbeddingModel::E5SmallV2.model_id(), "intfloat/e5-small-v2");
        assert_eq!(EmbeddingModel::GteSmall.model_id(), "thenlper/gte-small");
    }

    #[test]
    fn test_model_enum_derives() {
        let model = EmbeddingModel::AllMiniLmL6V2;
        let cloned = model;
        assert_eq!(model, cloned);
        let debug = format!("{model:?}");
        assert!(debug.contains("AllMiniLmL6V2"));
    }

    #[test]
    fn test_seven_variants_exist() {
        assert_eq!(ALL_MODELS.len(), 7);
    }
}
