use std::path::PathBuf;

use crate::model::EmbeddingModel;

/// Configuration for the embedding pipeline.
#[derive(Debug, Clone)]
pub struct EmbedConfig {
    /// Model to use. Default: AllMiniLmL6V2.
    pub model: EmbeddingModel,

    /// Cache directory for model files.
    /// Default: None (resolved to platform-specific path at runtime).
    pub cache_dir: Option<PathBuf>,

    /// Maximum batch size for `embed_batch`. Default: 32.
    pub batch_size: usize,

    /// Separator for title+content concatenation. Default: ": ".
    pub separator: String,
}

impl Default for EmbedConfig {
    fn default() -> Self {
        Self {
            model: EmbeddingModel::default(),
            cache_dir: None,
            batch_size: 32,
            separator: ": ".to_string(),
        }
    }
}

impl EmbedConfig {
    /// Resolve the cache directory.
    ///
    /// If `cache_dir` is `Some`, returns it directly.
    /// Otherwise, uses `dirs::cache_dir()` to get the platform-specific cache path,
    /// appending `unimatrix/models`. Falls back to `.unimatrix/models` in the
    /// current directory if `dirs::cache_dir()` returns `None`.
    pub fn resolve_cache_dir(&self) -> PathBuf {
        if let Some(ref dir) = self.cache_dir {
            return dir.clone();
        }

        if let Some(cache) = dirs::cache_dir() {
            return cache.join("unimatrix").join("models");
        }

        // Fallback: current directory
        PathBuf::from(".unimatrix").join("models")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = EmbedConfig::default();
        assert_eq!(config.model, EmbeddingModel::AllMiniLmL6V2);
        assert!(config.cache_dir.is_none());
        assert_eq!(config.batch_size, 32);
        assert_eq!(config.separator, ": ");
    }

    #[test]
    fn test_custom_config() {
        let config = EmbedConfig {
            model: EmbeddingModel::BgeSmallEnV15,
            cache_dir: Some(PathBuf::from("/tmp/custom")),
            batch_size: 16,
            separator: " - ".to_string(),
        };
        assert_eq!(config.model, EmbeddingModel::BgeSmallEnV15);
        assert_eq!(config.cache_dir, Some(PathBuf::from("/tmp/custom")));
        assert_eq!(config.batch_size, 16);
        assert_eq!(config.separator, " - ");
    }

    #[test]
    fn test_config_clone() {
        let config = EmbedConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.batch_size, config.batch_size);
        assert_eq!(cloned.separator, config.separator);
    }

    #[test]
    fn test_config_debug() {
        let config = EmbedConfig::default();
        let debug_str = format!("{config:?}");
        assert!(debug_str.contains("EmbedConfig"));
        assert!(debug_str.contains("32"));
    }

    #[test]
    fn test_resolve_cache_dir_custom() {
        let config = EmbedConfig {
            cache_dir: Some(PathBuf::from("/tmp/models")),
            ..Default::default()
        };
        let resolved = config.resolve_cache_dir();
        assert_eq!(resolved, PathBuf::from("/tmp/models"));
    }

    #[test]
    fn test_resolve_cache_dir_default() {
        let config = EmbedConfig::default();
        let resolved = config.resolve_cache_dir();
        // On Linux, should contain "unimatrix/models"
        let resolved_str = resolved.to_string_lossy();
        assert!(
            resolved_str.contains("unimatrix") && resolved_str.contains("models"),
            "resolved cache dir should contain unimatrix/models, got: {resolved_str}"
        );
    }
}
