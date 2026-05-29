## ADR-001: Environment Variable for Container Model Cache Redirect

### Context

ONNX models (~166 MB) are moving from baked-in image layers to a shared named volume (`unimatrix-shared`) mounted at `/shared`. The model cache path must be redirected from `/data/.cache/unimatrix/models/` to `/shared/models/` inside the container, while non-container usage (developers running `unimatrix` natively) must remain unchanged.

Three approaches were evaluated in SCOPE.md:

- **Option A (ENV-based)**: New environment variable `UNIMATRIX_MODEL_CACHE=/shared/models` set in the Dockerfile. `resolve_cache_dir()` checks this before `dirs::cache_dir()`. Explicit, self-documenting, no side effects.
- **Option B (Config-based)**: Set `EmbedConfig.cache_dir = Some("/shared/models")` via config.toml default in the container. Requires config to load before model download; the model-download CLI path does not load config.toml.
- **Option C (HOME/XDG-based)**: Set `XDG_CACHE_HOME=/shared` to redirect `dirs::cache_dir()`. Side effect: redirects ALL cache operations for any crate using `dirs::cache_dir()`, not just model storage.

Prior ADR: nxs-003 ADR-004 (Unimatrix #70) established `dirs::cache_dir()` + `unimatrix/models` as the default cache path. This decision extends that chain without replacing it.

### Decision

Use `UNIMATRIX_MODEL_CACHE` environment variable, checked by `EmbedConfig::resolve_cache_dir()` when the `cache_dir` field is `None`.

Implementation in `resolve_cache_dir()`:

```rust
pub fn resolve_cache_dir(&self) -> PathBuf {
    // 1. Explicit config field (highest priority — test overrides, operator config)
    if let Some(ref dir) = self.cache_dir {
        return dir.clone();
    }

    // 2. Container redirect via environment variable
    if let Ok(env_dir) = std::env::var("UNIMATRIX_MODEL_CACHE") {
        if !env_dir.is_empty() {
            return PathBuf::from(env_dir);
        }
    }

    // 3. Platform-specific default (unchanged)
    if let Some(cache) = dirs::cache_dir() {
        return cache.join("unimatrix").join("models");
    }

    // 4. Last resort fallback
    PathBuf::from(".unimatrix").join("models")
}
```

The Dockerfile sets `UNIMATRIX_MODEL_CACHE=/shared/models` in the runtime stage ENV block. The env var is not set outside the container, so steps 3-4 govern non-container behavior exactly as before.

The env var name uses the `UNIMATRIX_` prefix to make it container-internal by convention (SR-08). If a developer accidentally sets it in their shell, the variable name is self-documenting about its purpose.

### Consequences

**Easier:**
- All call sites that use `EmbedConfig::default().resolve_cache_dir()` (seven non-test sites) are redirected without any call-site changes. SR-06 is resolved structurally.
- The model-download CLI works correctly inside the container without needing config.toml.
- Non-container usage is completely unchanged — no env var means no redirect.
- Self-documenting: `docker inspect` shows the env var; operators can override it.

**Harder:**
- One more env var in the container environment. The `UNIMATRIX_` prefix avoids collision but adds to the surface operators must understand.
- If an operator unsets the env var in the container without providing an alternative, models fall back to `/data/.cache/unimatrix/models/` (the old behavior), mixing models back into the data volume. This is safe but defeats the purpose of the separation.
