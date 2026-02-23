/// Pre-configured catalog of 384-d sentence-transformer models.
///
/// All models produce 384-dimensional embeddings compatible with
/// nxs-002's `VectorConfig { dimension: 384 }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingModel {
    /// sentence-transformers/all-MiniLM-L6-v2 (default)
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

impl Default for EmbeddingModel {
    fn default() -> Self {
        Self::AllMiniLmL6V2
    }
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

    /// ONNX model filename within the repository.
    pub fn onnx_filename(&self) -> &'static str {
        "model.onnx"
    }

    /// Sanitized directory name for cache (slash replaced with underscore).
    pub fn cache_subdir(&self) -> String {
        self.model_id().replace('/', "_")
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
        assert_eq!(
            EmbeddingModel::E5SmallV2.model_id(),
            "intfloat/e5-small-v2"
        );
        assert_eq!(
            EmbeddingModel::GteSmall.model_id(),
            "thenlper/gte-small"
        );
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
