# Pseudocode: config-loader

**File**: `crates/unimatrix-server/src/infra/config.rs` (new file)

## Purpose

Owns the entire config lifecycle for dsn-001: struct definitions, TOML
deserialization, validation, two-level merge, preset resolution, and
permission enforcement. Produces a `UnimatrixConfig` consumed by `main.rs`
startup wiring. Has no runtime state — it produces values and is done.

---

## New Structs and Enum

```
// Top-level config. All sections are optional in TOML — absent sections
// use compiled defaults via #[serde(default)].
#[derive(Debug, Default, Clone, PartialEq, serde::Deserialize)]
struct UnimatrixConfig {
    #[serde(default)] profile:    ProfileConfig,
    #[serde(default)] knowledge:  KnowledgeConfig,
    #[serde(default)] server:     ServerConfig,
    #[serde(default)] agents:     AgentsConfig,
    #[serde(default)] confidence: ConfidenceConfig,
    // CycleConfig is intentionally absent (ADR-004: removed stub).
}

#[derive(Debug, Default, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
struct ProfileConfig {
    preset: Preset,   // default: Preset::Collaborative
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
struct KnowledgeConfig {
    categories:                Vec<String>,  // default: INITIAL_CATEGORIES as Vec
    boosted_categories:        Vec<String>,  // default: ["lesson-learned"]
    freshness_half_life_hours: Option<f64>,  // default: None (use preset built-in)
}
impl Default for KnowledgeConfig {
    fn default() -> Self {
        KnowledgeConfig {
            categories: INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect(),
            boosted_categories: vec!["lesson-learned".to_string()],
            freshness_half_life_hours: None,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
struct ServerConfig {
    instructions: Option<String>,  // None = use compiled default
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
struct AgentsConfig {
    default_trust:        String,       // default: "permissive"
    session_capabilities: Vec<String>,  // default: ["Read", "Write", "Search"]
}
impl Default for AgentsConfig {
    fn default() -> Self {
        AgentsConfig {
            default_trust: "permissive".to_string(),
            session_capabilities: vec![
                "Read".to_string(),
                "Write".to_string(),
                "Search".to_string(),
            ],
        }
    }
}

// Active only when preset = "custom". Ignored for named presets.
#[derive(Debug, Default, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
struct ConfidenceConfig {
    weights: Option<ConfidenceWeights>,
}

// Six-component weight vector. Required for custom preset.
// Does NOT derive Default — must not be silently zero-initialized.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct ConfidenceWeights {
    base:  f64,
    usage: f64,
    fresh: f64,
    help:  f64,
    corr:  f64,
    trust: f64,
}

// Preset enum. #[serde(rename_all = "lowercase")] maps TOML strings to variants.
// An unknown string fails serde deserialization before validate_config runs (AC-26).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Preset {
    Authoritative,
    Operational,
    Empirical,
    Collaborative,
    Custom,
}
impl Default for Preset {
    fn default() -> Self { Preset::Collaborative }
}
```

---

## ConfigError Variants (exact names, all 17)

```
enum ConfigError {
    FileTooLarge     { path: PathBuf, size: usize },
    WorldWritable    { path: PathBuf },
    MalformedToml    { path: PathBuf, detail: String },
    InvalidCategoryChar     { path: PathBuf, category: String },
    TooManyCategories       { path: PathBuf, count: usize },
    InvalidCategoryLength   { path: PathBuf, category: String, len: usize },
    BoostedCategoryNotInAllowlist { path: PathBuf, category: String },
    InvalidHalfLifeValue    { path: PathBuf, value: f64 },
    HalfLifeOutOfRange      { path: PathBuf, value: f64 },
    InstructionsTooLong     { path: PathBuf, len: usize },
    InstructionsInjection   { path: PathBuf, pattern_category: String },
    InvalidDefaultTrust     { path: PathBuf, value: String },
    InvalidSessionCapability { path: PathBuf, value: String },
    CustomPresetMissingWeights  { path: PathBuf },
    CustomPresetMissingHalfLife { path: PathBuf },
    CustomWeightOutOfRange  { path: PathBuf, field: String, value: f64 },
    CustomWeightSumInvariant { path: PathBuf, sum: f64 },
}

// impl std::fmt::Display for ConfigError:
//   Every variant must include: (a) file path, (b) specific field/constraint,
//   (c) valid values or range where applicable.
//   Example: "config error in /home/user/.unimatrix/config.toml:
//             [confidence] weights sum is 0.9500000000; must equal 0.92 exactly
//             (tolerance 1e-9)"
```

---

## Functions

### `load_config`

```
// Reads, size-caps, permission-checks, validates, and merges global + per-project config.
// Returns compiled defaults when no config files are present.
//
// ORDERING INVARIANT: ContentScanner::global() is called at the TOP of this function,
// before any validate_config call that may invoke scan_title(). The ContentScanner
// singleton is a OnceLock — this explicit warm call ensures it is initialized
// before validation begins. Do not remove this call.
pub fn load_config(home_dir: &Path, data_dir: &Path) -> Result<UnimatrixConfig, ConfigError>

BODY:
    // Step 0: Warm ContentScanner singleton BEFORE any validate_config call.
    // ORDERING INVARIANT: must be first. scan_title() in validate_config requires
    // ContentScanner::global() to be initialized. This explicit call documents the
    // dependency and prevents silent breakage if the OnceLock ever changes behavior.
    let _scanner = ContentScanner::global();

    // Step 1: Load global config (~/.unimatrix/config.toml).
    let global_path = home_dir.join(".unimatrix").join("config.toml");
    let global_config = match global_path.exists() {
        false => {
            tracing::debug!("global config not found; using compiled defaults");
            UnimatrixConfig::default()
        }
        true => load_single_config(&global_path)?
    };

    // Step 2: Load per-project config (~/.unimatrix/{hash}/config.toml).
    let project_path = data_dir.join("config.toml");
    let project_config = match project_path.exists() {
        false => UnimatrixConfig::default(),
        true  => load_single_config(&project_path)?
    };

    // Step 3: Merge (per-project fields win over global, global wins over compiled defaults).
    let merged = merge_configs(global_config, project_config);

    Ok(merged)
```

### `load_single_config` (private helper)

```
// Reads one config file: permission check → size cap → deserialize → validate.
fn load_single_config(path: &Path) -> Result<UnimatrixConfig, ConfigError>

BODY:
    // Permission check (Unix only).
    #[cfg(unix)]
    check_permissions(path)?;

    // Read to buffer with 64 KB cap.
    let bytes = std::fs::read(path)
        .map_err(|e| ConfigError::MalformedToml { path: path.into(), detail: e.to_string() })?;

    if bytes.len() > 65536 {
        return Err(ConfigError::FileTooLarge { path: path.into(), size: bytes.len() });
    }

    // Deserialize — unknown preset string fails here before validate_config.
    let text = String::from_utf8_lossy(&bytes);
    let config: UnimatrixConfig = toml::from_str(&text)
        .map_err(|e| ConfigError::MalformedToml { path: path.into(), detail: e.to_string() })?;

    // Validate all fields.
    validate_config(&config, path)?;

    Ok(config)
```

### `validate_config`

```
// Post-parse field validation for a single config file.
// Independently testable: no tokio, no store, no scanner dependency beyond
// ContentScanner::global() (warmed before this is called from load_config).
// When called directly in tests, the caller must call ContentScanner::global() first.
pub fn validate_config(config: &UnimatrixConfig, path: &Path) -> Result<(), ConfigError>

BODY:
    // --- Validate [knowledge] categories ---
    if config.knowledge.categories.len() > 64 {
        return Err(ConfigError::TooManyCategories {
            path: path.into(), count: config.knowledge.categories.len()
        });
    }
    for cat in &config.knowledge.categories {
        if cat.len() > 64 {
            return Err(ConfigError::InvalidCategoryLength {
                path: path.into(), category: cat.clone(), len: cat.len()
            });
        }
        for ch in cat.chars() {
            if !matches!(ch, 'a'..='z' | '0'..='9' | '_' | '-') {
                return Err(ConfigError::InvalidCategoryChar {
                    path: path.into(), category: cat.clone()
                });
            }
        }
    }

    // --- Validate [knowledge] boosted_categories ---
    let category_set: HashSet<&str> = config.knowledge.categories.iter().map(|s| s.as_str()).collect();
    for boosted in &config.knowledge.boosted_categories {
        if !category_set.contains(boosted.as_str()) {
            return Err(ConfigError::BoostedCategoryNotInAllowlist {
                path: path.into(), category: boosted.clone()
            });
        }
    }

    // --- Validate [knowledge] freshness_half_life_hours ---
    if let Some(v) = config.knowledge.freshness_half_life_hours {
        if v.is_nan() || v.is_infinite() || v <= 0.0 {
            return Err(ConfigError::InvalidHalfLifeValue { path: path.into(), value: v });
        }
        // Note: -0.0 in IEEE 754 — v <= 0.0 is true for -0.0, so -0.0 is rejected above.
        if v > 87600.0 {
            return Err(ConfigError::HalfLifeOutOfRange { path: path.into(), value: v });
        }
        // v == 87600.0 passes (inclusive upper bound, per EC-04).
    }

    // --- Validate [server] instructions ---
    if let Some(ref instructions) = config.server.instructions {
        // Length check BEFORE scanner (security invariant: length short-circuits injection scan).
        if instructions.len() > 8192 {
            return Err(ConfigError::InstructionsTooLong {
                path: path.into(), len: instructions.len()
            });
        }
        // Injection scan using the already-warmed ContentScanner singleton.
        let scanner = ContentScanner::global();
        if let Err(scan_result) = scanner.scan_title(instructions) {
            return Err(ConfigError::InstructionsInjection {
                path: path.into(),
                pattern_category: scan_result.category.to_string(),
            });
        }
    }

    // --- Validate [agents] default_trust ---
    match config.agents.default_trust.as_str() {
        "permissive" | "strict" => {},
        other => return Err(ConfigError::InvalidDefaultTrust {
            path: path.into(), value: other.to_string()
        }),
    }

    // --- Validate [agents] session_capabilities ---
    // Allowlist: only Read, Write, Search. Admin is explicitly excluded (SR-SEC-02).
    const VALID_CAPS: &[&str] = &["Read", "Write", "Search"];
    for cap_str in &config.agents.session_capabilities {
        if !VALID_CAPS.contains(&cap_str.as_str()) {
            return Err(ConfigError::InvalidSessionCapability {
                path: path.into(), value: cap_str.clone()
            });
        }
    }

    // --- Validate [profile] preset + [confidence] weights interaction ---
    match config.profile.preset {
        Preset::Custom => {
            // custom requires both weights and freshness_half_life_hours.
            // Weights absence is detected here; half_life absence detected after.
            match &config.confidence.weights {
                None => return Err(ConfigError::CustomPresetMissingWeights { path: path.into() }),
                Some(w) => {
                    // Validate each weight in [0.0, 1.0] and finite.
                    let weight_fields = [
                        ("base",  w.base),
                        ("usage", w.usage),
                        ("fresh", w.fresh),
                        ("help",  w.help),
                        ("corr",  w.corr),
                        ("trust", w.trust),
                    ];
                    for (name, val) in weight_fields {
                        if val.is_nan() || val.is_infinite() || val < 0.0 || val > 1.0 {
                            return Err(ConfigError::CustomWeightOutOfRange {
                                path: path.into(), field: name.to_string(), value: val
                            });
                        }
                    }
                    // Sum invariant: (sum - 0.92).abs() < 1e-9.
                    // NOT sum <= 1.0 — SCOPE.md comment is incorrect; ADR-005 governs.
                    let sum = w.base + w.usage + w.fresh + w.help + w.corr + w.trust;
                    if (sum - 0.92).abs() >= 1e-9 {
                        return Err(ConfigError::CustomWeightSumInvariant {
                            path: path.into(), sum
                        });
                    }
                }
            }
            // freshness_half_life_hours is required for custom preset.
            if config.knowledge.freshness_half_life_hours.is_none() {
                return Err(ConfigError::CustomPresetMissingHalfLife { path: path.into() });
            }
        }
        _ => {
            // Named presets: warn if [confidence] weights present, then ignore.
            if config.confidence.weights.is_some() {
                tracing::warn!(
                    path = %path.display(),
                    preset = ?config.profile.preset,
                    "[confidence] weights present but preset is not 'custom'; weights will be ignored"
                );
            }
            // No validation of weight values for named presets — they are not used.
        }
    }

    Ok(())
```

### `resolve_confidence_params`

```
// Single resolution site: converts preset selection into a fully-populated ConfidenceParams.
// ADR-006: this is the ONLY place that determines which confidence parameters to use.
// Called once at startup; result wrapped in Arc and passed to background tick.
//
// SR-02: always returns a ConfidenceParams with all six weights populated.
// SR-11: single site prevents half_life precedence confusion.
// SR-13: the returned struct is the W3-1 cold-start vector.
pub fn resolve_confidence_params(config: &UnimatrixConfig) -> Result<ConfidenceParams, ConfigError>

BODY:
    // W3-1 extension point (not implemented in dsn-001):
    // Priority 0: if load_learned_weights(data_dir) returns Some(learned), return it.
    // dsn-001 skips this check; W3-1 inserts it here.

    match config.profile.preset {
        Preset::Collaborative => {
            // Collaborative = compiled defaults. Apply optional [knowledge] override.
            let mut params = ConfidenceParams::default();
            if let Some(override_half_life) = config.knowledge.freshness_half_life_hours {
                params.freshness_half_life_hours = override_half_life;
            }
            Ok(params)
        }

        Preset::Authoritative | Preset::Operational | Preset::Empirical => {
            // Named preset: use ADR-005 weight table.
            let mut params = confidence_params_from_preset(config.profile.preset);
            // Apply optional [knowledge] freshness_half_life_hours override.
            if let Some(override_half_life) = config.knowledge.freshness_half_life_hours {
                // Operator explicitly overrides the preset's built-in half_life.
                params.freshness_half_life_hours = override_half_life;
            }
            // If absent, params.freshness_half_life_hours already carries the preset's
            // built-in value from confidence_params_from_preset (correct behavior).
            Ok(params)
        }

        Preset::Custom => {
            // validate_config already verified both fields are present.
            // If we reach here, config.confidence.weights is Some and half_life is Some.
            // Errors here indicate a logic gap in validate_config — treat as internal error.
            let weights = config.confidence.weights.as_ref()
                .ok_or_else(|| ConfigError::CustomPresetMissingWeights {
                    path: PathBuf::from("<merged config>")
                })?;
            let half_life = config.knowledge.freshness_half_life_hours
                .ok_or_else(|| ConfigError::CustomPresetMissingHalfLife {
                    path: PathBuf::from("<merged config>")
                })?;

            Ok(ConfidenceParams {
                w_base:  weights.base,
                w_usage: weights.usage,
                w_fresh: weights.fresh,
                w_help:  weights.help,
                w_corr:  weights.corr,
                w_trust: weights.trust,
                freshness_half_life_hours: half_life,
                alpha0: COLD_START_ALPHA,
                beta0:  COLD_START_BETA,
            })
        }
    }
```

### `confidence_params_from_preset`

```
// Constructs ConfidenceParams for a named preset from the ADR-005 weight table.
// Used by resolve_confidence_params internally and by the SR-10 mandatory test.
//
// PANICS on Preset::Custom — calling with Custom is a logic error.
// Only resolve_confidence_params handles the Custom path.
// No direct calls to confidence_params_from_preset(Preset::Custom) anywhere.
pub fn confidence_params_from_preset(preset: Preset) -> ConfidenceParams

BODY:
    match preset {
        Preset::Collaborative => {
            // Must equal ConfidenceParams::default() exactly (SR-10 invariant).
            ConfidenceParams::default()
        }
        Preset::Authoritative => ConfidenceParams {
            w_base:  0.14, w_usage: 0.14, w_fresh: 0.10,
            w_help:  0.14, w_corr:  0.18, w_trust: 0.22,
            freshness_half_life_hours: 8760.0,
            alpha0: COLD_START_ALPHA,
            beta0:  COLD_START_BETA,
        },
        Preset::Operational => ConfidenceParams {
            w_base:  0.14, w_usage: 0.18, w_fresh: 0.24,
            w_help:  0.08, w_corr:  0.18, w_trust: 0.10,
            freshness_half_life_hours: 720.0,
            alpha0: COLD_START_ALPHA,
            beta0:  COLD_START_BETA,
        },
        Preset::Empirical => ConfidenceParams {
            w_base:  0.12, w_usage: 0.16, w_fresh: 0.34,
            w_help:  0.04, w_corr:  0.06, w_trust: 0.20,
            freshness_half_life_hours: 24.0,
            alpha0: COLD_START_ALPHA,
            beta0:  COLD_START_BETA,
        },
        Preset::Custom => {
            // Logic error: Custom preset does not have built-in weights.
            // Use resolve_confidence_params() to handle the Custom path.
            panic!("confidence_params_from_preset(Preset::Custom) is a logic error; \
                    use resolve_confidence_params() instead");
        }
    }
```

### `merge_configs`

```
// Merges global and per-project configs using replace semantics (ADR-003).
// Per-project field wins over global when it differs from the compiled default.
// Per-project field absent (== compiled default) falls through to global value.
// List fields replace entirely — no append.
//
// NOTE: Cross-level custom preset weight inheritance is PROHIBITED (ADR-003).
// If per-project sets preset=custom but has no [confidence] weights, and global
// has weights, validate_config (called before merge) will have already aborted.
// This function does not need to enforce the prohibition — it is enforced during
// per-file validation before merge is called.
fn merge_configs(global: UnimatrixConfig, project: UnimatrixConfig) -> UnimatrixConfig

BODY:
    // Helper: pick project field if it differs from default, else use global field.
    // This is the "non-default detection" merge (ADR-003).

    UnimatrixConfig {
        profile: ProfileConfig {
            preset: if project.profile.preset != ProfileConfig::default().preset {
                project.profile.preset
            } else {
                global.profile.preset
            },
        },
        knowledge: KnowledgeConfig {
            categories: if project.knowledge.categories != KnowledgeConfig::default().categories {
                project.knowledge.categories
            } else {
                global.knowledge.categories
            },
            boosted_categories: if project.knowledge.boosted_categories
                    != KnowledgeConfig::default().boosted_categories {
                project.knowledge.boosted_categories
            } else {
                global.knowledge.boosted_categories
            },
            freshness_half_life_hours: project.knowledge.freshness_half_life_hours
                .or(global.knowledge.freshness_half_life_hours),
            // Option: Some from project wins; fallback to global Some; else None.
        },
        server: ServerConfig {
            instructions: project.server.instructions.or(global.server.instructions),
        },
        agents: AgentsConfig {
            default_trust: if project.agents.default_trust != AgentsConfig::default().default_trust {
                project.agents.default_trust
            } else {
                global.agents.default_trust
            },
            session_capabilities: if project.agents.session_capabilities
                    != AgentsConfig::default().session_capabilities {
                project.agents.session_capabilities
            } else {
                global.agents.session_capabilities
            },
        },
        confidence: ConfidenceConfig {
            weights: project.confidence.weights.or(global.confidence.weights),
            // Note: For custom preset, per-project weights are required in the per-project
            // file (ADR-003). The merge here is a simple Option::or — but validate_config
            // has already verified that if the merged preset is "custom", the merged file
            // either has per-project weights or raises CustomPresetMissingWeights. The merge
            // function does not need to re-enforce this; validation gates it.
            //
            // IMPORTANT: The "cross-level inheritance prohibition" means:
            // If the merged preset comes from the project (custom) and project has no weights,
            // validate_config for the project file already aborted. So we never reach merge
            // in that state. But if global has custom+weights and project has no-preset,
            // global.confidence.weights will be present — this is fine because the merged
            // preset will be custom from global, and validate_config already verified global.
        },
    }
```

### `check_permissions`

```
// Unix-only: checks file permissions before reading.
// World-writable → abort (security risk: attacker can write the file we read).
// Group-writable → warn and continue.
// Uses metadata() (not symlink_metadata()) so symlinks are followed to target.
// There is no yield point between check and read — the file is read immediately
// after this function returns in load_single_config (TOCTOU mitigation).
#[cfg(unix)]
fn check_permissions(path: &Path) -> Result<(), ConfigError>

BODY:
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::metadata(path)
        .map_err(|e| ConfigError::MalformedToml { path: path.into(), detail: e.to_string() })?;
    let mode = metadata.permissions().mode();

    if mode & 0o002 != 0 {
        // World-writable: abort startup.
        return Err(ConfigError::WorldWritable { path: path.into() });
    }
    if mode & 0o020 != 0 {
        // Group-writable: warn and continue.
        tracing::warn!(
            path = %path.display(),
            mode = format!("{:o}", mode),
            "config file is group-writable; consider restricting permissions to 0600"
        );
    }
    Ok(())
```

---

## Constants (module-level)

```
// File size cap before TOML parse.
const CONFIG_MAX_BYTES: usize = 65536; // 64 KB

// Maximum instructions length before ContentScanner runs.
const INSTRUCTIONS_MAX_BYTES: usize = 8192; // 8 KB

// Maximum freshness_half_life_hours value (10 years).
const HALF_LIFE_MAX_HOURS: f64 = 87600.0;

// Preset weight table constants (ADR-005, all rows verified sum to 0.92).
// These are the authoritative values — all validation and resolution code uses these.
// NOTE: Sum invariant is (sum - 0.92).abs() < 1e-9, NOT sum <= 1.0 (SCOPE.md is wrong).
const SUM_INVARIANT: f64 = 0.92;
const SUM_TOLERANCE: f64 = 1e-9;
```

---

## `dirs::home_dir() = None` Handling

The startup wiring in `main.rs` resolves `home_dir` before calling `load_config`.
If `dirs::home_dir()` returns `None`, `main.rs` emits a `tracing::warn!` and calls
`load_config` with a fallback path that does not exist — this means both config files
are absent and `UnimatrixConfig::default()` is returned. No panic. See startup-wiring.md.

---

## State Machine: Config Load Lifecycle

```
START
  │
  ├─ home_dir = None → WARN + use UnimatrixConfig::default() → DONE
  │
  ├─ global_path not found → skip (no error)
  │     per-project not found → skip (no error)
  │     → merge(default, default) = default → DONE
  │
  ├─ global_path found:
  │     check_permissions → Err(WorldWritable) → ABORT
  │     read → Err(io) → ABORT(MalformedToml with io detail)
  │     len > 65536 → ABORT(FileTooLarge)
  │     toml::from_str → Err → ABORT(MalformedToml with parse detail)
  │     validate_config → Err → ABORT(specific variant)
  │     → global_config: OK
  │
  ├─ per-project path found: (same flow as global)
  │     → project_config: OK
  │
  └─ merge_configs(global_config, project_config) → merged → DONE
```

---

## Key Test Scenarios

1. **No config files** — `load_config` returns `UnimatrixConfig::default()`. `resolve_confidence_params(&default)` returns `ConfidenceParams::default()`.

2. **SR-10 mandatory test** — `confidence_params_from_preset(Preset::Collaborative) == ConfidenceParams::default()`. Comment verbatim: "SR-10: If this test fails, fix the weight table, not the test."

3. **Custom preset — all four missing-field permutations** (AC-25):
   - Both present → OK
   - Weights absent → `CustomPresetMissingWeights`
   - Half-life absent → `CustomPresetMissingHalfLife`
   - Both absent → `CustomPresetMissingWeights` (detected first in validate_config order)

4. **Weight sum invariant** — custom weights summing to `0.95` abort with `CustomWeightSumInvariant`. Weights summing to `0.92` pass. Both sides of `1e-9` boundary tested (0.920000001 fails, 0.919999999 fails).

5. **Named preset immunity to [confidence]** — `resolve_confidence_params` with `Preset::Authoritative` and `confidence.weights = Some(...)` returns the authoritative table values, not the custom weights.

6. **freshness_half_life_hours precedence** (all four ADR-006 cases):
   - named + absent → preset built-in
   - named + present → override value
   - custom + absent → `CustomPresetMissingHalfLife`
   - custom + present → override value

7. **File too large** — 65537-byte file → `FileTooLarge`. 65536-byte file → passes size check.

8. **World-writable** — mode 0o666 → `WorldWritable`. Mode 0o664 → warn + OK.

9. **Instructions length-before-scan ordering** — 8193-byte string → `InstructionsTooLong` (not `InstructionsInjection`). 8193-byte injection string must return `InstructionsTooLong`.

10. **`session_capabilities` Admin exclusion** — `["Admin"]` → `InvalidSessionCapability`. `["Read", "Admin"]` → same. `["Read", "Write", "Search"]` → OK.

11. **Two-level merge** — global `categories = ["a", "b"]`, per-project `categories = ["c"]` → effective `["c"]` only.

12. **Cross-level custom preset prohibition** (R-10) — `merge_configs(global_with_weights, project_custom_no_weights)` followed by `validate_config` for the project file aborts with `CustomPresetMissingWeights` (because validate_config runs per-file before merge).

---

## Error Handling

All `ConfigError` variants implement `std::fmt::Display`. Every message includes:
- The file path (PathBuf from each variant)
- The specific field or constraint violated
- Valid values or valid range

`load_config` propagates all errors upward via `?`. The caller (`main.rs`) converts them to a `Box<dyn std::error::Error>` which terminates startup with a descriptive message.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — found pattern #2298 (TOML key semantic divergence for dsn-001) and #646 (serde(default) backward-compatible extension). Both relevant: serde(default) pattern confirmed for all sub-structs.
- Deviations from established patterns: none. Using `#[serde(default)]` on all sub-structs follows pattern #646.
