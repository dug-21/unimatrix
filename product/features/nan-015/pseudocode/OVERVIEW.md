# nan-015 Pseudocode Overview

## Components

| Component | File | Scope |
|-----------|------|-------|
| cache-path-resolution | `crates/unimatrix-embed/src/config.rs` | Add `UNIMATRIX_MODEL_CACHE` env var to `resolve_cache_dir()` |
| dockerfile | `Dockerfile` | Remove model bake-in, add `/shared` volume, set env var |
| compose-config | `docker-compose.yml` | Add `unimatrix-shared` named volume |
| documentation | `product/PRODUCT-VISION.md`, `product/WAVE2-ROADMAP.md` | Update W2-1 to two-volume model |

## Data Flow

```
Dockerfile ENV                    docker-compose.yml
  UNIMATRIX_MODEL_CACHE=            unimatrix-shared volume
  /shared/models                    mounted at /shared
        |                                  |
        v                                  v
  EmbedConfig::resolve_cache_dir()    /shared/models/ on disk
        |
        +---> OnnxProvider::new()       (embed startup)
        +---> NliConfig.cache_dir       (NLI startup)
        +---> model-download CLI        (manual download)
        |
        v
  ensure_model() / ensure_nli_model()
        |
        v
  /shared/models/{model-dirs}/model.onnx
```

## Shared Types

No new types introduced. The only modified function signature is:

- `EmbedConfig::resolve_cache_dir(&self) -> PathBuf` -- signature unchanged, body modified

The env var name `UNIMATRIX_MODEL_CACHE` is a shared constant string that must be identical in:
1. `config.rs` -- `std::env::var("UNIMATRIX_MODEL_CACHE")`
2. `Dockerfile` -- `ENV UNIMATRIX_MODEL_CACHE=/shared/models`

## Sequencing Constraints

1. **cache-path-resolution first** -- the Rust code change is the foundation; Dockerfile and compose depend on the env var being consumed correctly
2. **dockerfile second** -- depends on cache-path-resolution being ready (sets the env var that Rust code reads)
3. **compose-config third** -- depends on Dockerfile declaring the volume mount point
4. **documentation last** -- purely descriptive, no runtime dependency

## V-01 Alignment Status

SPECIFICATION C-04 text (lines 258-266) lists the correct precedence: config field > env var > dirs > fallback. This matches the architecture. V-01 is **resolved** -- no correction needed.
