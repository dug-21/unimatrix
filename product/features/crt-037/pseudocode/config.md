# Component: config.rs (unimatrix-server)

## Purpose

Add three new fields to `InferenceConfig` controlling the `Informs` detection pass:
`informs_category_pairs`, `nli_informs_cosine_floor`, and `nli_informs_ppr_weight`. Add
corresponding serde default functions and `validate()` range checks.

Wave 1. No I/O. Pure struct extension.

## Files Modified

`crates/unimatrix-server/src/infra/config.rs`

## Context: InferenceConfig Pattern

Existing fields follow a consistent pattern (observed from crt-029 and crt-034 additions):
1. Field declaration with doc comment in the `InferenceConfig` struct body
2. `#[serde(default = "default_fn_name")]` attribute
3. Private `fn default_fn_name() -> T` below the struct
4. Entry in `InferenceConfig::default()` using the same function or inline value
5. Range check in `InferenceConfig::validate()` with a structured `ConfigError` variant

All three new fields follow this exact pattern.

## New/Modified Functions

### default_informs_category_pairs()

```
fn default_informs_category_pairs() -> Vec<[String; 2]>:
    return vec![
        ["lesson-learned", "decision"],
        ["lesson-learned", "convention"],
        ["pattern", "decision"],
        ["pattern", "convention"],
    ]
    // Four pairs only — frozen at v1 (C-10, SR-04)
    // These are the ONLY locations in the codebase where these domain strings appear
    // (C-12: domain vocab must not appear in nli_detection_tick.rs)
```

### default_nli_informs_cosine_floor()

```
fn default_nli_informs_cosine_floor() -> f32:
    return 0.45_f32
    // Distinct from supports_candidate_threshold (0.50)
    // Captures the 0.45-0.50 band invisible to the Supports scan
    // Inclusive floor: similarity >= floor means candidate (Phase 4b uses >=, not >)
```

### default_nli_informs_ppr_weight()

```
fn default_nli_informs_ppr_weight() -> f32:
    return 0.6_f32
    // Weight multiplier: Informs edge weight = cosine * ppr_weight (both f32)
    // Separate from Supports edge weight (which uses scores.entailment directly)
```

### InferenceConfig struct additions

Three fields added to the `InferenceConfig` struct, in a new section after the PPR fields
(last existing field: `ppr_max_expand`). Follow the section comment pattern:

```
// -----------------------------------------------------------------------
// Informs edge detection fields (crt-037)
// -----------------------------------------------------------------------

/// Category pairs eligible for Informs detection.
///
/// Each element [lhs, rhs] means: entries with category `lhs` may Inform entries
/// with category `rhs`. Domain vocabulary lives ONLY here — not in detection logic
/// (C-12 / AC-22). Detection receives this list as a runtime config value.
///
/// Default: four software-engineering pairs (frozen at v1, C-10 / SR-04).
/// An empty list disables Informs detection without error.
#[serde(default = "default_informs_category_pairs")]
pub informs_category_pairs: Vec<[String; 2]>,

/// HNSW cosine similarity floor for Informs candidate pre-filter.
///
/// Phase 4b includes pairs with similarity >= nli_informs_cosine_floor.
/// Inclusive floor (>= not >) — pairs at exactly 0.45 are valid candidates (AC-17, AC-18).
/// Distinct from supports_candidate_threshold (Phase 4 uses strict >; Phase 4b uses >=).
///
/// Default: 0.45. Range: (0.0, 1.0) exclusive (>0.0, <1.0).
#[serde(default = "default_nli_informs_cosine_floor")]
pub nli_informs_cosine_floor: f32,

/// PPR edge weight multiplier for Informs edges.
///
/// Informs edge weight = candidate.cosine * nli_informs_ppr_weight (both f32).
/// Controls how strongly institutional memory influences PPR traversal relative to
/// Supports edges (which use the NLI entailment score as weight directly).
/// Weight must be finite — NaN/±Inf rejected before any write (C-13, NF-08).
///
/// Default: 0.6. Range: [0.0, 1.0] inclusive (0.0 disables PPR contribution; 1.0 is max).
#[serde(default = "default_nli_informs_ppr_weight")]
pub nli_informs_ppr_weight: f32,
```

### InferenceConfig::default() additions

```
fn default() -> Self:
    InferenceConfig {
        // ... all existing fields unchanged ...
        // crt-037: Informs edge detection fields
        informs_category_pairs: default_informs_category_pairs(),
        nli_informs_cosine_floor: default_nli_informs_cosine_floor(),
        nli_informs_ppr_weight: default_nli_informs_ppr_weight(),
    }
```

### InferenceConfig::validate() additions

Add after the existing PPR field range checks (or at the end of the method body, before
the final `Ok(())`). Two new checks — no cross-field invariant between these and any
existing field:

```
fn validate(&self, path: &Path) -> Result<(), ConfigError>:
    // ... all existing checks unchanged ...

    // nli_informs_cosine_floor: (0.0, 1.0) exclusive
    if self.nli_informs_cosine_floor <= 0.0 || self.nli_informs_cosine_floor >= 1.0:
        return Err(ConfigError::NliFieldOutOfRange {
            path: path.to_path_buf(),
            field: "nli_informs_cosine_floor",
            value: self.nli_informs_cosine_floor as f64,
            min: 0.0,
            max: 1.0,
        })
        // Rejects exactly 0.0 and exactly 1.0 (exclusive bounds — AC-10)

    // nli_informs_ppr_weight: [0.0, 1.0] inclusive
    if self.nli_informs_ppr_weight < 0.0 || self.nli_informs_ppr_weight > 1.0:
        return Err(ConfigError::NliFieldOutOfRange {
            path: path.to_path_buf(),
            field: "nli_informs_ppr_weight",
            value: self.nli_informs_ppr_weight as f64,
            min: 0.0,
            max: 1.0,
        })
        // Accepts exactly 0.0 and exactly 1.0 (inclusive bounds — AC-11)
        // Rejects -0.01 and 1.01

    // Note: informs_category_pairs has no range check (empty is valid — disables detection)

    Ok(())
```

Determine which `ConfigError` variant to use by inspecting the existing validate() body.
If `NliFieldOutOfRange` does not exist but a similar pattern does (e.g., a generic
out-of-range variant), use that. Do not invent a new variant — match the existing error
taxonomy. Flag if no suitable variant exists.

## State Machines

None. `InferenceConfig` is a pure data struct — no lifecycle state.

## Initialization Sequence

Config is loaded at server startup via `load_config`. `validate()` is called after
deserialization. The default `InferenceConfig` must pass `validate()` (AC-12).

No startup side effects from the new fields. `informs_category_pairs` is read by
`nli_detection_tick.rs` at tick invocation time — not at startup.

## Data Flow

```
TOML config file
  --deserialize via serde--> InferenceConfig.informs_category_pairs (Vec<[String; 2]>)
                             InferenceConfig.nli_informs_cosine_floor (f32)
                             InferenceConfig.nli_informs_ppr_weight (f32)
  --validate()--> Ok or Err(ConfigError)
  --pass as &InferenceConfig to run_graph_inference_tick
```

## Error Handling

`validate()` returns `Err(ConfigError::...)` for out-of-range values. Upstream caller
(server startup) maps this to a fatal startup error with a user-readable message.

No new `ConfigError` variants needed if an existing out-of-range variant is present. If
the exact variant shape differs, use the closest existing one and note it in the
implementation.

## Key Test Scenarios

AC-07: Parse an empty TOML string for `InferenceConfig`. Assert `informs_category_pairs`
equals the four default pairs. Use `toml::from_str("")` or equivalent test pattern.

AC-08: Parse empty TOML. Assert `nli_informs_cosine_floor == 0.45_f32`.

AC-09: Parse empty TOML. Assert `nli_informs_ppr_weight == 0.6_f32`.

AC-10: `validate()` with `nli_informs_cosine_floor = 0.0` returns `Err`.
       `validate()` with `nli_informs_cosine_floor = 1.0` returns `Err`.
       `validate()` with `nli_informs_cosine_floor = 0.45` returns `Ok`.
       `validate()` with `nli_informs_cosine_floor = 0.001` returns `Ok` (inside exclusive range).
       `validate()` with `nli_informs_cosine_floor = 0.999` returns `Ok`.

AC-11: `validate()` with `nli_informs_ppr_weight = -0.01` returns `Err`.
       `validate()` with `nli_informs_ppr_weight = 1.01` returns `Err`.
       `validate()` with `nli_informs_ppr_weight = 0.0` returns `Ok` (inclusive).
       `validate()` with `nli_informs_ppr_weight = 1.0` returns `Ok` (inclusive).

AC-12: `InferenceConfig::default().validate(path)` returns `Ok(())`.

TOML round-trip: Serialize default config to TOML, re-parse, assert field values are
preserved. Confirms serde default functions are wired correctly.

Empty informs_category_pairs: `validate()` with `informs_category_pairs = []` returns
`Ok(())` — empty list disables detection without error.

## Constraints

- C-10: Default list is frozen at four entries. Do not add a fifth entry.
- C-12: Domain vocabulary strings (`"lesson-learned"`, `"decision"`, `"pattern"`,
  `"convention"`) must appear ONLY in `default_informs_category_pairs()`. They must not
  appear in `nli_detection_tick.rs`.
- C-08: No new top-level cap field. `max_graph_inference_per_tick` remains the sole throttle.
- No cross-field invariant between `nli_informs_cosine_floor` and
  `supports_candidate_threshold` — they govern independent scans.
