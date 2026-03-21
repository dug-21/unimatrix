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
///
/// Quantized variants (`*Q8`) use the same HuggingFace repo and cache directory as
/// their FP32 counterparts — `tokenizer.json` is shared. Only the ONNX filename
/// differs (`model_qint8_avx512.onnx` vs `model.onnx`). Both files can coexist
/// in the same cache subdirectory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NliModel {
    /// `cross-encoder/nli-MiniLM2-L6-H768` — FP32, ~313 MB.
    NliMiniLM2L6H768,
    /// `cross-encoder/nli-MiniLM2-L6-H768` — INT8 quantized (`model_qint8_avx512.onnx`), ~50 MB.
    ///
    /// Same repo and cache directory as `NliMiniLM2L6H768`. Requires AVX-512 at runtime.
    NliMiniLM2L6H768Q8,
    /// `cross-encoder/nli-deberta-v3-small` — FP32, ~541 MB.
    NliDebertaV3Small,
    /// `cross-encoder/nli-deberta-v3-small` — INT8 quantized (`model_qint8_avx512.onnx`), ~180 MB.
    ///
    /// Same repo and cache directory as `NliDebertaV3Small`. Requires AVX-512 at runtime.
    NliDebertaV3SmallQ8,
}

impl NliModel {
    /// Resolve from a config string identifier. Returns `None` for unrecognized values.
    ///
    /// Called by `InferenceConfig::validate()`; `None` triggers startup abort (AC-17, R-15).
    ///
    /// Recognized names (case-sensitive): `"minilm2"`, `"minilm2-q8"`, `"deberta"`, `"deberta-q8"`.
    pub fn from_config_name(name: &str) -> Option<Self> {
        match name {
            "minilm2"    => Some(Self::NliMiniLM2L6H768),
            "minilm2-q8" => Some(Self::NliMiniLM2L6H768Q8),
            "deberta"    => Some(Self::NliDebertaV3Small),
            "deberta-q8" => Some(Self::NliDebertaV3SmallQ8),
            _ => None,
        }
    }

    /// HuggingFace model repository ID.
    ///
    /// Q8 quantized variants use the same repo as their FP32 counterpart.
    pub fn model_id(&self) -> &'static str {
        match self {
            Self::NliMiniLM2L6H768 | Self::NliMiniLM2L6H768Q8 => "cross-encoder/nli-MiniLM2-L6-H768",
            Self::NliDebertaV3Small | Self::NliDebertaV3SmallQ8 => "cross-encoder/nli-deberta-v3-small",
        }
    }

    /// Repo path for the ONNX file download via `hf-hub`.
    ///
    /// FP32 variants use `onnx/model.onnx`; Q8 variants use `onnx/model_qint8_avx512.onnx`.
    pub fn onnx_repo_path(&self) -> &'static str {
        match self {
            Self::NliMiniLM2L6H768 | Self::NliDebertaV3Small => "onnx/model.onnx",
            Self::NliMiniLM2L6H768Q8 | Self::NliDebertaV3SmallQ8 => "onnx/model_qint8_avx512.onnx",
        }
    }

    /// Local ONNX filename in the cache directory.
    ///
    /// Q8 quantized variants use a distinct filename so they can coexist with FP32
    /// in the same cache subdirectory (the tokenizer is shared between FP32 and Q8).
    pub fn onnx_filename(&self) -> &'static str {
        match self {
            Self::NliMiniLM2L6H768 | Self::NliDebertaV3Small => "model.onnx",
            Self::NliMiniLM2L6H768Q8 | Self::NliDebertaV3SmallQ8 => "model_qint8_avx512.onnx",
        }
    }

    /// Local cache subdirectory name. No slashes — safe for filesystem paths (R-18).
    ///
    /// Q8 variants share the same cache subdirectory as their FP32 counterpart so the
    /// tokenizer is not downloaded twice.
    pub fn cache_subdir(&self) -> &'static str {
        match self {
            Self::NliMiniLM2L6H768 | Self::NliMiniLM2L6H768Q8 => "nli-minilm2-l6-h768",
            Self::NliDebertaV3Small | Self::NliDebertaV3SmallQ8 => "nli-deberta-v3-small",
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
    fn test_from_config_name_q8_variants() {
        assert_eq!(NliModel::from_config_name("minilm2-q8"), Some(NliModel::NliMiniLM2L6H768Q8));
        assert_eq!(NliModel::from_config_name("deberta-q8"), Some(NliModel::NliDebertaV3SmallQ8));
    }

    #[test]
    fn test_from_config_name_unknown_returns_none() {
        assert_eq!(NliModel::from_config_name("gpt4"), None);
        assert_eq!(NliModel::from_config_name(""), None);
        // Case-sensitive: uppercase should not match
        assert_eq!(NliModel::from_config_name("MINILM2"), None);
        assert_eq!(NliModel::from_config_name("MiniLM2"), None);
        assert_eq!(NliModel::from_config_name("minilm2-Q8"), None);
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
    fn test_nli_model_onnx_filename() {
        assert_eq!(NliModel::NliMiniLM2L6H768.onnx_filename(), "model.onnx");
        assert_eq!(NliModel::NliDebertaV3Small.onnx_filename(), "model.onnx");
        assert_eq!(NliModel::NliMiniLM2L6H768Q8.onnx_filename(), "model_qint8_avx512.onnx");
        assert_eq!(NliModel::NliDebertaV3SmallQ8.onnx_filename(), "model_qint8_avx512.onnx");
    }

    #[test]
    fn test_nli_model_q8_shares_cache_subdir_with_fp32() {
        // Q8 variants share the tokenizer directory with their FP32 counterpart.
        assert_eq!(
            NliModel::NliMiniLM2L6H768.cache_subdir(),
            NliModel::NliMiniLM2L6H768Q8.cache_subdir()
        );
        assert_eq!(
            NliModel::NliDebertaV3Small.cache_subdir(),
            NliModel::NliDebertaV3SmallQ8.cache_subdir()
        );
    }

    #[test]
    fn test_nli_model_q8_uses_quantized_repo_path() {
        assert_eq!(NliModel::NliMiniLM2L6H768Q8.onnx_repo_path(), "onnx/model_qint8_avx512.onnx");
        assert_eq!(NliModel::NliDebertaV3SmallQ8.onnx_repo_path(), "onnx/model_qint8_avx512.onnx");
    }

    #[test]
    fn test_nli_model_methods_return_non_empty() {
        for model in [NliModel::NliMiniLM2L6H768, NliModel::NliMiniLM2L6H768Q8, NliModel::NliDebertaV3Small, NliModel::NliDebertaV3SmallQ8] {
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
        // R-18: MiniLM2 and DeBERTa must use distinct cache subdirs.
        assert_ne!(
            NliModel::NliMiniLM2L6H768.cache_subdir(),
            NliModel::NliDebertaV3Small.cache_subdir(),
            "cache_subdir must differ between model families to prevent tokenizer confusion"
        );
        assert_ne!(
            NliModel::NliMiniLM2L6H768Q8.cache_subdir(),
            NliModel::NliDebertaV3SmallQ8.cache_subdir(),
        );
    }

    #[test]
    fn test_nli_model_cache_subdirs_no_slash() {
        // R-18: no slashes in cache subdir (safe for filesystem paths).
        for model in [NliModel::NliMiniLM2L6H768, NliModel::NliMiniLM2L6H768Q8, NliModel::NliDebertaV3Small, NliModel::NliDebertaV3SmallQ8] {
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
