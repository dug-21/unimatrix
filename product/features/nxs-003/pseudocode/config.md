# C2: Config Module -- Pseudocode

## Purpose

Define `EmbedConfig` struct with sensible defaults.

## File: `crates/unimatrix-embed/src/config.rs`

```
USE std::path::PathBuf
USE crate::model::EmbeddingModel

#[derive(Debug, Clone)]
STRUCT EmbedConfig:
    pub model: EmbeddingModel           // default: AllMiniLmL6V2
    pub cache_dir: Option<PathBuf>      // default: None -> ~/.cache/unimatrix/models/
    pub batch_size: usize               // default: 32
    pub separator: String               // default: ": "

IMPL Default for EmbedConfig:
    fn default() -> Self:
        EmbedConfig {
            model: EmbeddingModel::default(),
            cache_dir: None,
            batch_size: 32,
            separator: ": ".to_string(),
        }

IMPL EmbedConfig:
    /// Resolve the cache directory. If cache_dir is None, use platform default.
    pub fn resolve_cache_dir(&self) -> PathBuf:
        IF let Some(ref dir) = self.cache_dir:
            return dir.clone()

        // Platform-specific via dirs crate
        IF let Some(cache) = dirs::cache_dir():
            return cache.join("unimatrix").join("models")

        // Fallback: current directory
        PathBuf::from(".unimatrix").join("models")
```

## Design Notes

- `cache_dir: Option<PathBuf>` -- `None` means use platform default at runtime.
- Resolution uses `dirs::cache_dir()` which returns `~/.cache` on Linux, `~/Library/Caches` on macOS.
- Fallback to `.unimatrix/models` in current directory if `dirs::cache_dir()` returns None.
- `resolve_cache_dir` is a method rather than computing at construction time so the struct remains a pure data type.
- `batch_size` of 32 balances memory usage and inference throughput.
- `separator` is configurable but convenience functions use ": " by default.
