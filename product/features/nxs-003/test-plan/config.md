# C2: Config Module -- Test Plan

## Tests

```
test_default_values:
    config = EmbedConfig::default()
    ASSERT matches!(config.model, EmbeddingModel::AllMiniLmL6V2)
    ASSERT config.cache_dir.is_none()
    ASSERT config.batch_size == 32
    ASSERT config.separator == ": "

test_custom_config:
    config = EmbedConfig {
        model: EmbeddingModel::BgeSmallEnV15,
        cache_dir: Some(PathBuf::from("/tmp/custom")),
        batch_size: 16,
        separator: " - ".to_string(),
    }
    ASSERT matches!(config.model, EmbeddingModel::BgeSmallEnV15)
    ASSERT config.cache_dir == Some(PathBuf::from("/tmp/custom"))
    ASSERT config.batch_size == 16
    ASSERT config.separator == " - "

test_config_clone:
    config = EmbedConfig::default()
    cloned = config.clone()
    ASSERT cloned.batch_size == config.batch_size
    ASSERT cloned.separator == config.separator

test_config_debug:
    config = EmbedConfig::default()
    debug_str = format!("{:?}", config)
    ASSERT debug_str contains "EmbedConfig"
    ASSERT debug_str contains "32"   // batch_size

test_resolve_cache_dir_custom:
    config = EmbedConfig {
        cache_dir: Some(PathBuf::from("/tmp/models")),
        ..Default::default()
    }
    resolved = config.resolve_cache_dir()
    ASSERT resolved == PathBuf::from("/tmp/models")

test_resolve_cache_dir_default:
    config = EmbedConfig::default()
    resolved = config.resolve_cache_dir()
    // On Linux, should end with "unimatrix/models"
    ASSERT resolved.ends_with("unimatrix/models") OR resolved.ends_with("unimatrix\\models")
```

## Risks Covered

- R-10: Cache path resolution (default and custom).
- AC-13: Configuration supports model, cache_dir, batch_size, separator.
