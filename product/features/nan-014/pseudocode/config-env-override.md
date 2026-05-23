# config-env-override: UNIMATRIX_CONFIG Environment Variable

## Purpose

Add `UNIMATRIX_CONFIG` as the highest-priority config file source, checked before `dirs::config_dir()` and the existing `HOME`-based fallback. Required because `HOME=/data` in the container causes `dirs::config_dir()` to resolve to `/data/.config/`, missing the bind-mounted `/etc/unimatrix/config.toml`.

Per ADR-005: the Dockerfile sets `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml`. If the file exists at that path, it is loaded. If not (no bind mount), the daemon proceeds with defaults.

## Modified Function

**File**: `crates/unimatrix-server/src/infra/config.rs`

**Function**: `load_config`

**Current signature** (unchanged):
```
pub fn load_config(home_dir: &Path, data_dir: &Path) -> Result<UnimatrixConfig, ConfigError>
```

### Pseudocode

```
pub fn load_config(home_dir, data_dir):
    // ORDERING INVARIANT: warm ContentScanner singleton (existing).
    let _scanner = ContentScanner::global()

    // NEW: Step 0 — check UNIMATRIX_CONFIG env var (highest priority).
    // When set AND the file exists, load it as the override config.
    // This is the container config path: /etc/unimatrix/config.toml.
    // If the env var is set but the file does not exist, log debug and skip.
    // If the env var is not set, skip entirely.
    let env_config = match std::env::var("UNIMATRIX_CONFIG"):
        Ok(path_str):
            let path = PathBuf::from(&path_str)
            if path.exists():
                tracing::info!(path = %path.display(), "loading config from UNIMATRIX_CONFIG")
                Some(load_single_config(&path)?)
            else:
                tracing::debug!(
                    path = %path.display(),
                    "UNIMATRIX_CONFIG set but file not found; skipping"
                )
                None
        Err(_):
            None  // env var not set — normal non-container operation

    // Step 1: load global config (~/.unimatrix/config.toml) — existing.
    let global_path = home_dir.join(".unimatrix").join("config.toml")
    let global_config = if global_path.exists():
        load_single_config(&global_path)?
    else:
        tracing::debug!("global config not found; using compiled defaults")
        UnimatrixConfig::default()

    // Step 2: load per-project config — existing.
    let project_path = data_dir.join("config.toml")
    let project_config = if project_path.exists():
        load_single_config(&project_path)?
    else:
        UnimatrixConfig::default()

    // Step 3: merge — existing precedence: project > global > compiled defaults.
    let merged = merge_configs(global_config, project_config)

    // NEW: Step 3b — if env_config exists, merge it ON TOP (highest priority).
    // env_config fields win over project, global, and compiled defaults.
    let merged = match env_config:
        Some(env_cfg):
            merge_configs(merged, env_cfg)
        None:
            merged

    // Step 4: post-merge validation — existing.
    // Use the env config path for error reporting when present,
    // otherwise fall back to global_path (existing behavior).
    let validation_path = match std::env::var("UNIMATRIX_CONFIG"):
        Ok(p) => PathBuf::from(p),
        Err(_) => global_path,
    validate_config(&merged, &validation_path)?

    Ok(merged)
```

### Merge Precedence (after change)

```
UNIMATRIX_CONFIG (env var path)   <-- NEW, highest priority
  > per-project config (~/.unimatrix/{hash}/config.toml)
    > global config (~/.unimatrix/config.toml)
      > compiled defaults
```

### Key Design Decision

The env var config merges ON TOP of the existing global+project merge, not as a replacement. This means an operator can set a few fields in `/etc/unimatrix/config.toml` and let everything else inherit from defaults or per-project config.

## Calling Code (main.rs)

The calling code in `tokio_main_daemon` and `tokio_main_stdio` is unchanged. Both call:

```rust
load_config(&home, &paths.data_dir)
```

The `UNIMATRIX_CONFIG` env var is read inside `load_config` itself. The callers do not pass it explicitly.

**Container-specific note**: In the container, `HOME=/data`, so `dirs::home_dir()` returns `Some("/data")`. The global config path becomes `/data/.unimatrix/config.toml`. The per-project config path is in `paths.data_dir` (under `/data/.unimatrix/{hash}/`). The `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` overrides both when the bind mount exists.

## Error Handling

- **Env var set, file missing**: Debug log, skip. No error. The daemon starts with defaults or lower-priority configs.
- **Env var set, file unparseable**: `load_single_config` returns `ConfigError`. Propagated to caller. Daemon logs warning and uses compiled defaults (existing error handling in `tokio_main_daemon`).
- **Env var not set**: Entirely skipped. Zero behavioral change to existing non-container usage.

## Key Test Scenarios

1. **Env var set, file exists**: Set `UNIMATRIX_CONFIG` to a temp file with `[profile]\npreset = "minimal"`. Call `load_config`. Assert the returned config has `preset = "minimal"`, proving env var config wins.

2. **Env var set, file missing**: Set `UNIMATRIX_CONFIG` to a nonexistent path. Call `load_config`. Assert it returns `Ok` with the same config as if the env var were unset.

3. **Env var not set**: Unset `UNIMATRIX_CONFIG`. Call `load_config`. Assert behavior is identical to current (regression gate).

4. **Env var config overrides per-project**: Create a per-project config with `preset = "balanced"` and an env var config with `preset = "minimal"`. Call `load_config`. Assert `preset = "minimal"` (env var wins).

5. **Validation runs on merged result**: Set `UNIMATRIX_CONFIG` to a file that, when merged with defaults, violates a constraint (e.g., too many categories). Assert `load_config` returns `ConfigError`.
