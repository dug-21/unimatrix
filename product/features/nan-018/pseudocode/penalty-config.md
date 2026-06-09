# Component: Penalty config — `penalty-config.md`

**Wave**: 1
**Location**: `crates/unimatrix-server/src/infra/config.rs` (modify)
**ADR**: ADR-001 (#4897), ADR-006 (#4894 — eval-only boundary). **Risks**: R-02 (dual-default), R-13 (multiplier).

## Purpose

Surface the 7 crt-014 levers + an optional `multiplier` overlay as `UnimatrixConfig` fields,
deserialized from the profile TOML `[graph_penalty]` section. Defaults reproduce the engine
consts exactly (additive, C-02, ADR-006: eval-only — not a deployment re-tune surface).
Resolve to a `GraphPenaltyParams` for the search layer.

## New: `GraphPenaltyConfig`

```
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]                       // omitted [graph_penalty] => all defaults
pub struct GraphPenaltyConfig {
    #[serde(default = "default_orphan")]               pub orphan: f64,
    #[serde(default = "default_clean_replacement")]    pub clean_replacement: f64,
    #[serde(default = "default_hop_decay")]            pub hop_decay: f64,
    #[serde(default = "default_partial_supersession")] pub partial_supersession: f64,
    #[serde(default = "default_dead_end")]             pub dead_end: f64,
    #[serde(default = "default_fallback")]             pub fallback: f64,
    #[serde(default = "default_max_traversal_depth")]  pub max_traversal_depth: usize,
    #[serde(default)]                                  pub multiplier: Option<f64>,  // default None
}
```

### Dual-default discipline (#4064 — LOAD-BEARING, R-02)

Each `default_*()` fn AND the `impl Default` field BOTH reference the **engine const** —
one numeric source of truth. NEVER inline a literal `0.75` here; import from the engine.

```
use unimatrix_engine::graph::{
    ORPHAN_PENALTY, CLEAN_REPLACEMENT_PENALTY, HOP_DECAY_FACTOR,
    PARTIAL_SUPERSESSION_PENALTY, DEAD_END_PENALTY, FALLBACK_PENALTY, MAX_TRAVERSAL_DEPTH,
};

fn default_orphan()               -> f64   { ORPHAN_PENALTY }
fn default_clean_replacement()    -> f64   { CLEAN_REPLACEMENT_PENALTY }
fn default_hop_decay()            -> f64   { HOP_DECAY_FACTOR }
fn default_partial_supersession() -> f64   { PARTIAL_SUPERSESSION_PENALTY }
fn default_dead_end()             -> f64   { DEAD_END_PENALTY }
fn default_fallback()             -> f64   { FALLBACK_PENALTY }
fn default_max_traversal_depth()  -> usize { MAX_TRAVERSAL_DEPTH }

impl Default for GraphPenaltyConfig {
    fn default() -> Self {
        GraphPenaltyConfig {
            orphan: default_orphan(),
            clean_replacement: default_clean_replacement(),
            hop_decay: default_hop_decay(),
            partial_supersession: default_partial_supersession(),
            dead_end: default_dead_end(),
            fallback: default_fallback(),
            max_traversal_depth: default_max_traversal_depth(),
            multiplier: None,
        }
    }
}
```

## `UnimatrixConfig` field

```
pub struct UnimatrixConfig {
    // ... existing fields ...
    #[serde(default)]
    pub graph_penalty: GraphPenaltyConfig,
}
```
Omitting `[graph_penalty]` in TOML ⇒ `GraphPenaltyConfig::default()` ⇒ engine consts (NFR-02).

## Resolution: `GraphPenaltyConfig` → `GraphPenaltyParams`

Convenience method (called once in `with_rate_config`, see `search-threading.md`):

```
impl GraphPenaltyConfig {
    pub fn resolve_params(&self) -> GraphPenaltyParams {
        // Start from per-field values (already defaulted to consts when unset).
        let mut p = GraphPenaltyParams {
            orphan: self.orphan,
            clean_replacement: self.clean_replacement,
            hop_decay: self.hop_decay,                      // SHAPE — never multiplier-scaled
            partial_supersession: self.partial_supersession,
            dead_end: self.dead_end,
            fallback: self.fallback,
            max_traversal_depth: self.max_traversal_depth,  // SHAPE — never multiplier-scaled
        };

        // Multiplier overlay (OQ-2): scales SEVERITIES toward harsher (lower).
        // Per-field overrides win — multiplier only applies where the field is at its DEFAULT.
        if let Some(m) = self.multiplier {
            // For each severity, apply m ONLY if the field equals its const default
            // (i.e. the author did not explicitly override it). Explicit override wins (R-13).
            if self.orphan               == default_orphan()               { p.orphan               *= m; }
            if self.clean_replacement    == default_clean_replacement()    { p.clean_replacement    *= m; }
            if self.partial_supersession == default_partial_supersession() { p.partial_supersession *= m; }
            if self.dead_end             == default_dead_end()             { p.dead_end             *= m; }
            if self.fallback             == default_fallback()             { p.fallback             *= m; }
            // hop_decay, max_traversal_depth: NOT scaled (shape, not severity).
        }
        p
    }
}
```

### Multiplier semantics (OQ-2, R-13)

- `multiplier = None` ⇒ no scaling; per-field (or default) values pass through verbatim.
- `multiplier = Some(m)`, `m ∈ (0,1]` ⇒ scales the FIVE severities (orphan, clean_replacement,
  partial_supersession, dead_end, fallback) by `*m` toward harsher.
- `hop_decay` and `max_traversal_depth` are SHAPE params — NEVER scaled.
- **Per-field override wins**: if the author set an explicit value on a severity, the multiplier
  does NOT also touch it (multiplier is a convenience overlay, never a replacement). Implemented
  via the "equals-default ⇒ scalable" rule above. (Delivery may instead carry an explicit
  `Option<f64>` per field to distinguish "set to the default value" from "unset"; if so, prefer
  that — it removes the equals-default ambiguity. Flagged: the equals-default heuristic treats a
  deliberate set-to-default as unset. Document the chosen mechanism in Band-2.)

## Validation (reuse `eval/profile/validation.rs` path; security R-TEST)

Add range validation for the new fields in `validate()`:

```
fn validate_graph_penalty(cfg: &GraphPenaltyConfig) -> Result<(), ConfigError> {
    for (name, v) in [("orphan", cfg.orphan), ("clean_replacement", cfg.clean_replacement),
                      ("hop_decay", cfg.hop_decay), ("partial_supersession", cfg.partial_supersession),
                      ("dead_end", cfg.dead_end), ("fallback", cfg.fallback)] {
        if !v.is_finite() || v < 0.0 || v > 1.0 {
            return Err(ConfigError::OutOfRange { field: name, value: v });   // reject NaN, <0, >1
        }
    }
    if cfg.max_traversal_depth == 0 { return Err(ConfigError::OutOfRange { field: "max_traversal_depth", value: 0.0 }); }
    if let Some(m) = cfg.multiplier {
        if !m.is_finite() || m <= 0.0 || m > 1.0 { return Err(ConfigError::OutOfRange { field: "multiplier", value: m }); }
    }
    Ok(())
}
```
An out-of-range value (e.g. 99.0, NaN) is rejected at config load — never silently used or clamped.

## Data flow

- **Input**: profile TOML `[graph_penalty]` section (or absent ⇒ defaults).
- **Output**: `GraphPenaltyConfig` on `UnimatrixConfig`; `resolve_params()` → `GraphPenaltyParams`.

## Error handling

- Deserialization of malformed/oversized TOML errors cleanly via existing serde path (no panic).
- Range/NaN failures surface as `ConfigError` at load — caller aborts the eval run with a clear message.

## Key test scenarios

- **Dual-default triangulation (R-02)**: for each lever, assert
  `default_<x>() == GraphPenaltyConfig::default().<x> == GraphPenaltyParams::default().<x> == <ENGINE_CONST>`.
- **Empty-TOML equivalence (R-01.3, NFR-02)**: TOML omitting `[graph_penalty]` ⇒
  `resolve_params() == GraphPenaltyParams::default()`.
- **Multiplier exclusion (R-13.1)**: `multiplier = Some(0.5)`; assert `hop_decay` and
  `max_traversal_depth` UNCHANGED while the 5 severities are scaled.
- **Precedence (R-13.2)**: set both `multiplier` and an explicit `orphan` override ⇒ explicit wins.
- **Multiplier None (R-13.3)**: `None` ⇒ no scaling.
- **Validation**: out-of-range (99.0), NaN, `multiplier` outside (0,1], `max_traversal_depth = 0`
  all rejected at load.
