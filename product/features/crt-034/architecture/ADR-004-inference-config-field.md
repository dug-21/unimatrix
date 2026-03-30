## ADR-004: `max_co_access_promotion_per_tick` Added to `InferenceConfig`

### Context

The promotion tick must be capped per tick to bound tail latency. `InferenceConfig`
already holds `max_graph_inference_per_tick: usize` (default 100, range [1, 1000]) for
the NLI background tick (crt-029). The new field follows the same pattern.

The question is whether to:
1. Add `max_co_access_promotion_per_tick` to `InferenceConfig` following the exact
   `max_graph_inference_per_tick` pattern (serde default fn, validate() range check,
   merge_configs stanza, Default impl stanza).
2. Use a separate config section (e.g., `[graph]` section).
3. Derive the cap from an existing field (e.g., reuse `max_graph_inference_per_tick`).

Option 3 is rejected: NLI inference has ML cost (ONNX forward pass per pair); the NLI
default of 100 reflects that cost. Co_access promotion is pure SQL with no ML cost;
200 is appropriate. Sharing the cap conflates two unrelated cost centers.

Option 2 is rejected: a new section adds TOML ceremony for a single field. The existing
`[inference]` section is already the home for background tick configuration parameters.
Structurally, the promotion tick is a peer of the NLI tick.

Option 1 is the established pattern. All tick-related throttle and threshold parameters
live in `[inference]`. The config merge contract (ADR-003 dsn-001, entry #2286) applies:
project-level override wins if it differs from `Default`, else falls through to global.

### Decision

Add `max_co_access_promotion_per_tick: usize` to `InferenceConfig` in
`crates/unimatrix-server/src/infra/config.rs` following the `max_graph_inference_per_tick`
pattern exactly:

**Field declaration** (in `InferenceConfig` struct, after the PPR fields):
```rust
/// Maximum number of co_access pairs to promote per background tick.
///
/// Qualifying pairs (count >= CO_ACCESS_GRAPH_MIN_COUNT) are fetched in descending
/// count order; only the top N are processed. Setting this lower bounds tail latency
/// at the cost of slower catch-up after a cold start or #409 GC run.
/// Default: 200. Valid range: [1, 10000].
#[serde(default = "default_max_co_access_promotion_per_tick")]
pub max_co_access_promotion_per_tick: usize,
```

**Private serde default fn**:
```rust
fn default_max_co_access_promotion_per_tick() -> usize {
    200
}
```

**`Default` impl stanza** (in `impl Default for InferenceConfig`):
```rust
max_co_access_promotion_per_tick: default_max_co_access_promotion_per_tick(),
```

**`validate()` range check** (after the `max_graph_inference_per_tick` check):
```rust
// crt-034: max_co_access_promotion_per_tick range check [1, 10000]
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

**`merge_configs()` stanza** (after the `max_graph_inference_per_tick` stanza):
```rust
max_co_access_promotion_per_tick: if project.inference.max_co_access_promotion_per_tick
    != default.inference.max_co_access_promotion_per_tick
{
    project.inference.max_co_access_promotion_per_tick
} else {
    global.inference.max_co_access_promotion_per_tick
},
```

Default 200 vs NLI's 100: appropriate because co_access promotion is pure SQL with no
ML inference cost. The larger default allows faster catch-up without penalty.

Range [1, 10000] vs NLI's [1, 1000]: the upper bound is wider because SQL SELECT+INSERT
pairs are O(1) per pair with no rayon work, and co_access tables in practice remain
small (~0.34 MB cited in SCOPE.md).

### Consequences

- Config consumers get a predictable, documented throttle knob.
- All existing `InferenceConfig` tooling (TOML deserialization, validation errors,
  config merge, test patterns) applies unchanged.
- The zero-value rejection (AC-10: `max_co_access_promotion_per_tick = 0`) is enforced
  by the validate() range check `< 1` without additional code.
- Implementation agents MUST add a test for the range-check boundary (0 rejected, 1
  accepted, 10000 accepted, 10001 rejected) following the pattern in config.rs tests for
  `max_graph_inference_per_tick`.
