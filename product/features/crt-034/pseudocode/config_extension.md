# config_extension — Pseudocode

## Component: `InferenceConfig` extension in `infra/config.rs`

### Purpose

Add `max_co_access_promotion_per_tick: usize` to `InferenceConfig`, following the exact
pattern established by `max_graph_inference_per_tick` (ADR-004, #3826). This field is
the per-tick cap on how many co_access pairs the promotion tick processes. It participates
in serde deserialization (with a default fn), range validation in `validate()`, the
`Default` impl stanza, and the `merge_configs()` project-overrides-global logic.

No new config section. No new struct. No changes to any other field.

---

## File to Modify

**`crates/unimatrix-server/src/infra/config.rs`**

### Modification 1: Struct field in `InferenceConfig`

**Location**: After the existing crt-029 fields block (after `graph_inference_k`), before
the `// -----------------------------------------------------------------------` separator
for the heal pass fields (bugfix-444). Insert as a new section block:

```
// -----------------------------------------------------------------------
// co_access promotion tick fields (crt-034)
// -----------------------------------------------------------------------
/// Maximum number of co_access pairs to promote per background tick.
///
/// Controls how many qualifying pairs (count >= CO_ACCESS_GRAPH_MIN_COUNT = 3)
/// are fetched and processed per tick invocation. Highest-count pairs are
/// selected first (ORDER BY count DESC), so the cap prioritizes high-signal
/// pairs when the qualifying set exceeds the budget.
///
/// Default: 200. Higher than max_graph_inference_per_tick (100) because
/// co_access promotion is pure SQL with no CPU-bound ML inference cost.
/// Valid range: [1, 10000]. Out-of-range aborts startup with a structured
/// error naming the field.
#[serde(default = "default_max_co_access_promotion_per_tick")]
pub max_co_access_promotion_per_tick: usize,
```

### Modification 2: Serde default function

**Location**: In the default value functions section, after `fn default_max_graph_inference_per_tick()`:

```
fn default_max_co_access_promotion_per_tick() -> usize {
    200
}
```

### Modification 3: `Default` impl stanza

**Location**: In `impl Default for InferenceConfig`, inside the `InferenceConfig { ... }`
block, in the crt-029 fields comment group (after `max_graph_inference_per_tick: 100`
and `graph_inference_k: 10`). Add a new comment + field:

```
// Current in Default impl:
//     // crt-029: background graph inference tick fields
//     supports_candidate_threshold: 0.5,
//     supports_edge_threshold: 0.6,
//     max_graph_inference_per_tick: 100,
//     graph_inference_k: 10,

// Modified — add after graph_inference_k:
            // crt-034: co_access promotion tick fields
            max_co_access_promotion_per_tick: 200,
```

### Modification 4: `validate()` range check

**Location**: In `impl InferenceConfig`, in the `validate()` function, after the existing
`max_graph_inference_per_tick` range check block. Add:

```
// -- crt-034: max_co_access_promotion_per_tick range check [1, 10000] --
if self.max_co_access_promotion_per_tick < 1
    || self.max_co_access_promotion_per_tick > 10000
{
    return Err(ConfigError::NliFieldOutOfRange {
        path: path.to_path_buf(),
        field: "max_co_access_promotion_per_tick",
        value: self.max_co_access_promotion_per_tick.to_string(),
        reason: "must be in range [1, 10000]",
    });
}
```

Note: `ConfigError::NliFieldOutOfRange` is the existing error variant used by
`max_graph_inference_per_tick` — reuse it here; the variant name is generic enough
despite its "Nli" prefix (it exists for out-of-range config fields broadly).

### Modification 5: `merge_configs()` stanza

**Location**: In `merge_configs()`, inside the `inference: InferenceConfig { ... }` block,
after the `graph_inference_k` stanza (line ~2067-2073) and before the `// bugfix-444`
heal pass comment. Add:

```
// crt-034: co_access promotion tick
max_co_access_promotion_per_tick: if project.inference.max_co_access_promotion_per_tick
    != default.inference.max_co_access_promotion_per_tick
{
    project.inference.max_co_access_promotion_per_tick
} else {
    global.inference.max_co_access_promotion_per_tick
},
```

Pattern mirrors `max_graph_inference_per_tick` (lines 2060-2066) exactly. `usize`
comparison uses `!=` (not float epsilon) — same as `max_graph_inference_per_tick`.

---

## Data Flow

```
TOML config file (operator)
  │  [inference]
  │  max_co_access_promotion_per_tick = N   (optional; defaults to 200 via serde fn)
  │
  ▼
InferenceConfig::max_co_access_promotion_per_tick: usize
  │
  ├─ validate() checks [1, 10000]; returns ConfigError::NliFieldOutOfRange if out of range
  │
  ├─ merge_configs(): project value wins over global if != default
  │
  └─ passed as &InferenceConfig to run_co_access_promotion_tick()
       used as LIMIT ?2 in the batch SELECT
```

---

## Error Handling

**Validation error**: `validate()` returns `Err(ConfigError::NliFieldOutOfRange { ... })`
when the value is 0 or > 10000. The error message must contain the literal string
`"max_co_access_promotion_per_tick"` so operators can identify the field from the message
alone (AC-10). The existing `NliFieldOutOfRange` variant's `{field}` interpolation handles
this automatically.

**Startup abort**: `validate()` failure propagates to startup and aborts the server process
with a structured error (same behavior as existing out-of-range fields). No runtime
degradation mode.

---

## Key Test Scenarios

**AC-06a**: Default deserialization produces `max_co_access_promotion_per_tick = 200`.
- TOML: `[inference]\n` (field absent) → `config.inference.max_co_access_promotion_per_tick == 200`

**AC-06b**: Project-level override replaces global value.
- global: `max_co_access_promotion_per_tick = 200` (default)
- project: `max_co_access_promotion_per_tick = 50`
- merged: `max_co_access_promotion_per_tick == 50`

**AC-06c**: global value is used when project does not override.
- global: `max_co_access_promotion_per_tick = 300`
- project: (field absent → default 200)
- merged: `max_co_access_promotion_per_tick == 300`

**AC-10**: `validate()` rejects `max_co_access_promotion_per_tick = 0` with error
message containing `"max_co_access_promotion_per_tick"`.

**R-07a**: Boundary 1 is valid — `validate()` accepts value = 1.

**R-07b**: Boundary 10000 is valid — `validate()` accepts value = 10000.

**R-07c**: Boundary 10001 is rejected — `validate()` returns Err for value = 10001.

**R-07d (structural)**: Confirm field is present in `merge_configs()` stanza by running
project-override test (AC-06b). If the field is absent from merge_configs, the project
override silently fails — this is the entire point of the test.

**Default impl test**: `InferenceConfig::default().max_co_access_promotion_per_tick == 200`
and `InferenceConfig::default().validate(...)` succeeds.
