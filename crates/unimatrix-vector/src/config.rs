/// Configuration for the vector index.
///
/// All fields have sensible defaults for 384-dimension text embeddings
/// (all-MiniLM-L6-v2 model).
#[derive(Debug, Clone)]
pub struct VectorConfig {
    /// Embedding vector dimension. Default: 384.
    pub dimension: usize,
    /// Max connections per HNSW node (M parameter). Default: 16.
    pub max_nb_connection: usize,
    /// Construction beam width. Default: 200.
    pub ef_construction: usize,
    /// Pre-allocation hint for max elements. Default: 10,000.
    /// Not a hard cap; the index grows dynamically beyond this.
    pub max_elements: usize,
    /// Maximum HNSW graph layers. Default: 16.
    pub max_layer: usize,
    /// Default search beam width (overridable per query). Default: 32.
    pub default_ef_search: usize,
}

impl Default for VectorConfig {
    fn default() -> Self {
        VectorConfig {
            dimension: 384,
            max_nb_connection: 16,
            ef_construction: 200,
            max_elements: 10_000,
            max_layer: 16,
            default_ef_search: 32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = VectorConfig::default();
        assert_eq!(config.dimension, 384);
        assert_eq!(config.max_nb_connection, 16);
        assert_eq!(config.ef_construction, 200);
        assert_eq!(config.max_elements, 10_000);
        assert_eq!(config.max_layer, 16);
        assert_eq!(config.default_ef_search, 32);
    }

    #[test]
    fn test_custom_config() {
        let config = VectorConfig {
            dimension: 768,
            max_nb_connection: 32,
            ef_construction: 400,
            max_elements: 50_000,
            max_layer: 24,
            default_ef_search: 64,
        };
        assert_eq!(config.dimension, 768);
        assert_eq!(config.max_nb_connection, 32);
        assert_eq!(config.ef_construction, 400);
        assert_eq!(config.max_elements, 50_000);
        assert_eq!(config.max_layer, 24);
        assert_eq!(config.default_ef_search, 64);
    }

    #[test]
    fn test_config_clone() {
        let config = VectorConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.dimension, config.dimension);
        assert_eq!(cloned.max_nb_connection, config.max_nb_connection);
        assert_eq!(cloned.ef_construction, config.ef_construction);
        assert_eq!(cloned.max_elements, config.max_elements);
        assert_eq!(cloned.max_layer, config.max_layer);
        assert_eq!(cloned.default_ef_search, config.default_ef_search);
    }

    #[test]
    fn test_config_debug() {
        let config = VectorConfig::default();
        let debug_str = format!("{config:?}");
        assert!(debug_str.contains("384"), "expected '384' in: {debug_str}");
        assert!(
            debug_str.contains("VectorConfig"),
            "expected 'VectorConfig' in: {debug_str}"
        );
    }
}
