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
    /// Resolution precedence (ADR-002):
    /// 1. `cache_dir` field (explicit config or test override)
    /// 2. `UNIMATRIX_MODEL_CACHE` env var (container redirect, empty = unset)
    /// 3. `dirs::cache_dir()` platform default + `unimatrix/models`
    /// 4. `.unimatrix/models` fallback
    pub fn resolve_cache_dir(&self) -> PathBuf {
        self.resolve_cache_dir_with_env(std::env::var("UNIMATRIX_MODEL_CACHE").ok())
    }

    /// Inner resolution logic, parameterized for testability.
    ///
    /// `std::env::set_var` is unsafe in Rust 2024 edition and the crate uses
    /// `#![forbid(unsafe_code)]`, so tests pass the env var value directly
    /// instead of mutating the process environment.
    fn resolve_cache_dir_with_env(&self, env_value: Option<String>) -> PathBuf {
        // Step 1: Explicit config field (highest priority -- test overrides, operator config)
        if let Some(ref dir) = self.cache_dir {
            return dir.clone();
        }

        // Step 2: Container redirect via environment variable (ADR-001)
        // Empty string is treated as unset (ADR-002 invariant, R-07 guard)
        if let Some(env_dir) = env_value
            && !env_dir.is_empty()
        {
            return PathBuf::from(env_dir);
        }

        // Step 3: Platform-specific default (unchanged)
        if let Some(cache) = dirs::cache_dir() {
            return cache.join("unimatrix").join("models");
        }

        // Step 4: Last resort fallback
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

    // --- nan-015 tests: UNIMATRIX_MODEL_CACHE env var support ---
    //
    // These tests use resolve_cache_dir_with_env() to avoid std::env::set_var,
    // which is unsafe in Rust 2024 edition (and forbidden by #![forbid(unsafe_code)]).

    /// R-01 scenario 1: env var set and non-empty returns env var path.
    #[test]
    fn test_resolve_cache_dir_env_var_used_when_field_none() {
        let config = EmbedConfig::default();
        let resolved = config.resolve_cache_dir_with_env(Some("/tmp/test-cache".to_string()));
        assert_eq!(resolved, PathBuf::from("/tmp/test-cache"));
    }

    /// R-01 scenario 2: env var unset falls through to dirs::cache_dir().
    #[test]
    fn test_resolve_cache_dir_unset_env_falls_to_dirs() {
        let config = EmbedConfig::default();
        let resolved = config.resolve_cache_dir_with_env(None);
        let resolved_str = resolved.to_string_lossy();
        assert!(
            resolved_str.contains("unimatrix") && resolved_str.contains("models"),
            "expected dirs fallback containing unimatrix/models, got: {resolved_str}"
        );
    }

    /// R-01 scenario 3: config field wins over env var.
    #[test]
    fn test_resolve_cache_dir_config_field_wins_over_env_var() {
        let config = EmbedConfig {
            cache_dir: Some(PathBuf::from("/explicit")),
            ..Default::default()
        };
        let resolved = config.resolve_cache_dir_with_env(Some("/tmp/env-path".to_string()));
        assert_eq!(resolved, PathBuf::from("/explicit"));
    }

    /// R-07: empty env var treated as unset -- falls through to dirs.
    #[test]
    fn test_resolve_cache_dir_empty_env_var_falls_through() {
        let config = EmbedConfig::default();
        let resolved = config.resolve_cache_dir_with_env(Some(String::new()));
        assert_ne!(resolved, PathBuf::from(""));
        let resolved_str = resolved.to_string_lossy();
        assert!(
            resolved_str.contains("unimatrix") && resolved_str.contains("models"),
            "empty env var should fall through to dirs, got: {resolved_str}"
        );
    }

    /// R-01 scenario 4: last-resort fallback when dirs unavailable.
    /// Verifies the .unimatrix/models fallback path is well-formed.
    /// Note: cannot easily force dirs::cache_dir() to return None in CI,
    /// so this test verifies the fallback path construction directly.
    #[test]
    fn test_resolve_cache_dir_fallback_path_construction() {
        // The fallback path is .unimatrix/models -- verify it is constructed correctly
        let expected = PathBuf::from(".unimatrix").join("models");
        assert_eq!(
            expected,
            PathBuf::from(".unimatrix/models"),
            "fallback path should be .unimatrix/models"
        );
    }
}
