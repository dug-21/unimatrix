## ADR-002: Cache Path Precedence Chain

### Context

SR-01 (High severity) identifies that the cache path resolution chain has multiple layers and incorrect precedence could cause models to land on the wrong volume, silently breaking backup separation. SR-06 (High severity) requires all call sites to resolve consistently through a single function.

The current `resolve_cache_dir()` has two layers:
1. `EmbedConfig.cache_dir` field (explicit override)
2. `dirs::cache_dir()` platform default

ADR-001 adds a third layer (env var). The ordering must be unambiguous and documented so future modifications do not introduce precedence bugs.

Prior decisions:
- nxs-003 ADR-004 (Unimatrix #70): Established `dirs::cache_dir()` as the default path.
- nan-014 ADR-005 (Unimatrix #4573): Documented the single-volume design being superseded by this feature.
- Lesson #4642: Hash verification must precede model loading — the path change does not affect this ordering but makes it more critical because the shared volume is writable.

### Decision

The precedence chain for `resolve_cache_dir()` is, from highest to lowest priority:

```
1. EmbedConfig.cache_dir field     — Explicit programmatic override.
                                      Used by: tests, operator config.toml,
                                      NliConfig.nli_model_path.
                                      Wins unconditionally.

2. UNIMATRIX_MODEL_CACHE env var   — Container redirect.
                                      Used by: Dockerfile ENV.
                                      Checked only when field is None.
                                      Empty string is treated as unset.

3. dirs::cache_dir() + suffix      — Platform-specific default.
                                      Linux: ~/.cache/unimatrix/models/
                                      macOS: ~/Library/Caches/unimatrix/models/
                                      Checked only when env var is absent.

4. ".unimatrix/models"             — Last resort fallback.
                                      Only when dirs::cache_dir() returns None.
```

**Invariant**: Every call site that constructs `EmbedConfig::default()` (which sets `cache_dir: None`) will hit steps 2-4. Only call sites that explicitly set `cache_dir: Some(...)` bypass the env var. This is intentional — explicit overrides must win.

**Test requirements**: Add unit tests for:
- Env var set and non-empty: returns env var path
- Env var set but empty string: falls through to dirs
- Env var unset: falls through to dirs (existing behavior preserved)
- Explicit `cache_dir` field: wins over env var (existing test, verify it still passes)

### Consequences

**Easier:**
- Precedence is documented and testable. SR-01 is resolved by explicit ordering.
- The env var cannot override an explicit config field, preventing accidental misdirection when operators set `cache_dir` in config.toml.
- Empty-string guard prevents `UNIMATRIX_MODEL_CACHE=""` from creating a relative path at the filesystem root.

**Harder:**
- Four-level precedence requires documentation. Developers modifying `resolve_cache_dir()` must understand all four levels.
- The env var is read on every call to `resolve_cache_dir()`. This is called during startup (not hot path), so performance is not a concern. If future hot-path usage emerges, cache the resolved value.
