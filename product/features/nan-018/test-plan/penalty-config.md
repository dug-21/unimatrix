# Test Plan — Penalty config (`GraphPenaltyConfig`)

**Component**: `crates/unimatrix-server/src/infra/config.rs` — new `GraphPenaltyConfig` struct + `UnimatrixConfig.graph_penalty` field (`#[serde(default)]`).
**Wave**: 1. **Primary risks**: R-02 (dual-default divergence, High), R-13 (multiplier), security (range validation).

## Unit test expectations

### R-02 — dual-default triangulation (the #4070 "five-site atomic" lesson) — AC-01

For **each of the 7 levers** (`orphan`, `clean_replacement`, `hop_decay`, `partial_supersession`, `dead_end`, `fallback`, `max_traversal_depth`):

- `test_graph_penalty_config_dual_default_{lever}_triangulates`: assert
  `default_{lever}() == GraphPenaltyConfig::default().{lever} == GraphPenaltyParams::default().{lever} == <engine const>`.
  Concrete const values: `orphan=0.75`, `clean_replacement=0.40`, `hop_decay=0.60`, `partial_supersession=0.60`, `dead_end=0.65`, `fallback=0.70`, `max_traversal_depth=10`.
- A single triangulation test may cover all 7 in one body, but each lever gets its own explicit `assert_eq!` (do not loop opaquely — name the const).

### Empty / omitted-section deserialization (NFR-02) — AC-01(a)

- `test_config_omits_graph_penalty_section_deserializes_to_defaults`: a TOML with **no** `[graph_penalty]` section deserializes to `GraphPenaltyConfig::default()` (every field = const). Asserts `#[serde(default)]` at both the section and field level.
- `test_config_partial_graph_penalty_section_fills_rest_from_default`: a TOML setting only `clean_replacement = 0.30` leaves all other 6 fields at their const defaults.

### Multiplier overlay field (R-13) — AC-01

- `test_graph_penalty_config_multiplier_defaults_none`: `GraphPenaltyConfig::default().multiplier == None`.
- `test_config_multiplier_some_parsed`: TOML `multiplier = 0.5` deserializes to `Some(0.5)`. (Resolution semantics — which fields it scales — are tested in `search-threading.md`/`engine-penalty.md`, since resolution lives there.)

### Range validation (security — RISK-TEST-STRATEGY §Security) — reuses `eval/profile/validation.rs`

- `test_config_penalty_out_of_range_rejected`: a penalty field of `99.0` (or negative) is rejected at validation, not silently used.
- `test_config_penalty_nan_rejected`: `NaN` / non-finite penalty is rejected.
- `test_config_max_traversal_depth_zero_rejected_or_defined`: `max_traversal_depth = 0` either errors or has documented defined behavior (assert the chosen rule).
- `test_config_multiplier_out_of_range_rejected`: an absurd multiplier (e.g. negative, or driving severities out of `[0,1]`) is range-validated.

## Concrete assertions / expected behaviors
- The engine `const`s remain the **single source of truth**; `impl Default` and each serde `default_{lever}()` fn **reference the const**, never re-state the literal. The triangulation test is the regression sentinel for this.
- Serde round-trip uses **non-trivial values** (#3557): set each field to a non-default value and assert it survives `toml::to_string` → `from_str`.

## Edge cases
- Future manifest/config version mismatch is a shape-hash concern (see `shape-hash.md`), not config.
- Out-of-range value must error at **config load** (`validation.rs`), never reach `graph_penalty_with`.
