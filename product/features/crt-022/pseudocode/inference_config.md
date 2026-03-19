# Component Pseudocode: `InferenceConfig`

**File to modify**: `crates/unimatrix-server/src/infra/config.rs`

---

## Purpose

Add the `[inference]` config section to `UnimatrixConfig`. The section holds
`rayon_pool_size: usize` for W1-2. W1-4 (NLI model path, thresholds) and W2-4
(GGUF pool parameters) will extend this same section without renaming it (C-08).

`InferenceConfig` follows the exact same `#[serde(default)]` pattern used by all
other config section structs in this file:
- `ProfileConfig`
- `KnowledgeConfig`
- `ServerConfig`
- `AgentsConfig`
- `ConfidenceConfig`

---

## New / Modified Functions and Types

### `InferenceConfig` struct (new)

```
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct InferenceConfig {
    /// Thread count for the ML inference rayon pool.
    ///
    /// Default: `(num_cpus::get() / 2).max(4).min(8)` (ADR-003 pool floor = 4).
    /// Valid range: [1, 64]. Out-of-range aborts startup with a structured error.
    ///
    /// Operators on resource-constrained deployments may set this as low as 1.
    /// Operators with large knowledge bases (>1000 entries) should increase this
    /// beyond 4 to reduce queuing latency during contradiction scans.
    ///
    /// W1-4 (NLI) and W2-4 (GGUF) will add fields to this section without renaming it.
    pub rayon_pool_size: usize,
}
```

### `Default` implementation for `InferenceConfig`

```
impl Default for InferenceConfig {
    fn default() -> Self {
        // ADR-003: floor = 4 (supersedes SCOPE.md floor = 2).
        // Reasoning:
        //   1 thread max: contradiction scan
        //   1 thread max: quality-gate embedding loop (concurrent with scan)
        //   2 threads min: concurrent MCP inference calls
        //   Total minimum: 4
        //
        // On single-core: num_cpus = 1; 1/2 = 0 (integer division); max(0, 4) = 4.
        // On dual-core:   num_cpus = 2; 2/2 = 1; max(1, 4) = 4.
        // On octa-core:   num_cpus = 8; 8/2 = 4; max(4, 4) = 4; min(4, 8) = 4.
        // On 20-core:     num_cpus = 20; 20/2 = 10; max(10, 4) = 10; min(10, 8) = 8.
        InferenceConfig {
            rayon_pool_size: (num_cpus::get() / 2).max(4).min(8),
        }
    }
}
```

### `InferenceConfig::validate`

```
pub fn validate(&self, path: &Path) -> Result<(), ConfigError>
```

Algorithm:

```
1. if self.rayon_pool_size < 1 OR self.rayon_pool_size > 64:
2.     return Err(ConfigError::InferencePoolSizeOutOfRange {
3.         path: path.to_path_buf(),
4.         value: self.rayon_pool_size,
5.     })
6. return Ok(())
```

Note: range check is `[1, 64]` inclusive on both ends. Value 0 fails (< 1).
Value 64 passes. Value 65 fails (> 64). The range is intentionally wide to allow
operator tuning; the default formula caps the default at 8 for safety.

### New `ConfigError` variant

Add to the `ConfigError` enum:

```
InferencePoolSizeOutOfRange {
    path: PathBuf,
    value: usize,
},
```

Add to the `Display` impl for `ConfigError`:

```
ConfigError::InferencePoolSizeOutOfRange { path, value } => write!(
    f,
    "config error in {}: [inference] rayon_pool_size is {}; \
     must be in the range [1, 64] inclusive",
    path.display(),
    value
),
```

The display message names the section (`[inference]`), the field (`rayon_pool_size`),
the bad value, and the valid range. This matches the pattern of other `ConfigError`
variants in this file (e.g., `TooManyCategories`, `HalfLifeOutOfRange`).

### `UnimatrixConfig` extension

Add to the `UnimatrixConfig` struct (after the `confidence` field):

```
#[serde(default)]
pub inference: InferenceConfig,
```

The struct currently derives `Default` — the new `inference` field will be covered by
`InferenceConfig`'s `Default` impl automatically.

`UnimatrixConfig::default()` continues to work without change.

### `validate_config` extension

In the existing `validate_config(config, path)` function, add a call to
`InferenceConfig::validate` after the `[agents]` section validation and before the
`[profile]` preset validation (to follow the field declaration order):

```
// --- Validate [inference] rayon_pool_size ---
config.inference.validate(path)?;
```

This integrates `InferenceConfig` validation into the existing validation pass that
runs on every loaded config file.

### `merge_configs` extension

In `merge_configs(global, project)`, add the inference field merge following the same
replace-semantics pattern as other fields:

```
inference: InferenceConfig {
    rayon_pool_size: if project.inference.rayon_pool_size
        != default.inference.rayon_pool_size
    {
        project.inference.rayon_pool_size
    } else {
        global.inference.rayon_pool_size
    },
},
```

This means: if the per-project config explicitly sets a non-default `rayon_pool_size`,
it wins. Otherwise, the global config value is used.

---

## `num_cpus` Dependency

The `num_cpus` crate is used in the `Default` impl. Check whether it is already
present in `unimatrix-server/Cargo.toml`. If not, add:

```
num_cpus = "1"
```

This crate has no unsafe code and a minimal footprint. It is a dev dependency in
several other workspace crates — verify with `cargo tree` before adding.

---

## Initialization Sequence

In `main.rs`, after `load_config` returns the merged `UnimatrixConfig` and before
constructing the rayon pool:

```
1. config = load_config(...) or UnimatrixConfig::default()
   // validate_config runs per-file inside load_config via load_single_config.
   // InferenceConfig::validate is called as part of validate_config.
   // On validation failure, load_config returns Err(ConfigError).
   // The caller logs the error and falls back to UnimatrixConfig::default().

2. // validate_config was called per-file; a default config bypasses it.
   // If using default config (no file loaded), InferenceConfig::validate is not called.
   // Default values are always valid (rayon_pool_size in [4..8] range).
   // No additional validate call needed in the default path.

3. pool = RayonPool::new(config.inference.rayon_pool_size, "ml_inference_pool")
   // on Err → ServerStartupError::InferencePoolInit(e)
```

Gap: if `load_config` fails and falls back to `UnimatrixConfig::default()`, the
default `rayon_pool_size` is always valid (4 to 8), so startup proceeds safely.
If an invalid `rayon_pool_size` is in the config file, `validate_config` catches it
before `load_config` returns, preventing the bad value from reaching `RayonPool::new`.

---

## Error Handling

| Scenario | Behaviour |
|----------|-----------|
| `rayon_pool_size = 0` | `ConfigError::InferencePoolSizeOutOfRange { value: 0 }` |
| `rayon_pool_size = 65` | `ConfigError::InferencePoolSizeOutOfRange { value: 65 }` |
| `rayon_pool_size = 1` | validation passes (lower bound inclusive) |
| `rayon_pool_size = 64` | validation passes (upper bound inclusive) |
| Absent `[inference]` section | default applied; `rayon_pool_size = (cpus/2).max(4).min(8)` |
| Config load fails entirely | falls back to `UnimatrixConfig::default()`; default is always valid |

---

## Key Test Scenarios (AC-11, R-07)

1. **Valid lower bound** (AC-11 #5): `InferenceConfig { rayon_pool_size: 1 }.validate(path)` → `Ok`.

2. **Valid upper bound** (AC-11 #6): `InferenceConfig { rayon_pool_size: 64 }.validate(path)` → `Ok`.

3. **Rejects 0** (AC-11 #7): `InferenceConfig { rayon_pool_size: 0 }.validate(path)` →
   `Err(ConfigError::InferencePoolSizeOutOfRange { value: 0, .. })`.

4. **Rejects 65** (AC-11 #8): `InferenceConfig { rayon_pool_size: 65 }.validate(path)` →
   `Err(ConfigError::InferencePoolSizeOutOfRange { value: 65, .. })`.

5. **Default formula produces valid value on single-core** (R-07 scenario 5, edge case):
   simulate `num_cpus = 1`; default formula yields 4; `validate` returns `Ok`.

6. **Error message names field and range** (R-07 diagnostic requirement): assert the
   `Display` of `InferencePoolSizeOutOfRange` contains `"rayon_pool_size"`, the bad value,
   and `"[1, 64]"`.

7. **Absent `[inference]` section** (AC-09 default): parse a TOML string without an
   `[inference]` section; assert `config.inference.rayon_pool_size` equals the default
   formula result.

8. **Integration: startup rejects `rayon_pool_size = 0`** (R-07 scenario 6): server
   integration test with config `[inference]\nrayon_pool_size = 0`; assert server exits
   with non-zero status and a structured error message.

9. **Two-level merge: per-project wins over global** (merge semantics): global has
   `rayon_pool_size = 4`; per-project has `rayon_pool_size = 8`; merged config has `8`.

10. **Two-level merge: global wins when per-project is default** (merge semantics):
    global has `rayon_pool_size = 6`; per-project has no `[inference]` section (default
    applied); merged config has `6`.
