# C3: Config Module -- Test Plan

## Tests

```
test_default_values:
    config = VectorConfig::default()
    ASSERT config.dimension == 384
    ASSERT config.max_nb_connection == 16
    ASSERT config.ef_construction == 200
    ASSERT config.max_elements == 10_000
    ASSERT config.max_layer == 16
    ASSERT config.default_ef_search == 32

test_custom_config:
    config = VectorConfig {
        dimension: 768,
        max_nb_connection: 32,
        ef_construction: 400,
        max_elements: 50_000,
        max_layer: 24,
        default_ef_search: 64,
    }
    ASSERT config.dimension == 768
    ASSERT config.max_nb_connection == 32

test_config_clone:
    config = VectorConfig::default()
    cloned = config.clone()
    ASSERT cloned.dimension == config.dimension
    ASSERT cloned.max_nb_connection == config.max_nb_connection

test_config_debug:
    config = VectorConfig::default()
    debug_str = format!("{:?}", config)
    ASSERT debug_str contains "384"
    ASSERT debug_str contains "VectorConfig"
```

## Risks Covered
None directly. Config correctness underpins all other operations.
