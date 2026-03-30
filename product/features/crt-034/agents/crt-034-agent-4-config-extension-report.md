# Agent Report: crt-034-agent-4-config-extension

## Task
Add `max_co_access_promotion_per_tick: usize` to `InferenceConfig` in
`crates/unimatrix-server/src/infra/config.rs`, mirroring the exact pattern of
`max_graph_inference_per_tick`.

## Files Modified

- `crates/unimatrix-server/src/infra/config.rs`

## Changes Made

Five modifications to `InferenceConfig`, all following the `max_graph_inference_per_tick`
pattern exactly per ADR-004 (#3826):

1. **Struct field** — added after `graph_inference_k`, before the heal-pass section,
   with `#[serde(default = "default_max_co_access_promotion_per_tick")]` and doc comment
   explaining default (200), range ([1, 10000]), and why it differs from NLI's cap (100).

2. **Serde default fn** — `fn default_max_co_access_promotion_per_tick() -> usize { 200 }`
   inserted after `default_max_graph_inference_per_tick`.

3. **`validate()` range check** — `[1, 10000]` guard using `ConfigError::NliFieldOutOfRange`
   with `field: "max_co_access_promotion_per_tick"` (satisfies AC-10).

4. **`Default` impl stanza** — `max_co_access_promotion_per_tick: 200` added in the
   crt-029 fields comment group.

5. **`merge_configs()` stanza** — project-overrides-global logic using `!=` comparison
   against default value, inserted between `graph_inference_k` and `heal_pass_batch_size`.

## Tests

6 unit tests added in `infra::config::tests`:

| Test | Covers |
|------|--------|
| `test_max_co_access_promotion_per_tick_default` | AC-06(a): absent field → 200 |
| `test_max_co_access_promotion_per_tick_validation_zero` | AC-06(b), AC-10: 0 rejected, error names field |
| `test_max_co_access_promotion_per_tick_validation_over_limit` | AC-06(c): 10001 rejected, error names field |
| `test_max_co_access_promotion_per_tick_validation_boundary_values` | ADR-004: 1 and 10000 both pass |
| `test_merge_configs_project_overrides_global_co_access_cap` | AC-06(d), R-07: project 50 wins over global 200 |
| `test_merge_configs_global_only_co_access_cap` | R-07 secondary: global 300 preserved when project is default |

**Result: 6/6 pass. Full `unimatrix-server` test suite: all pass (0 new failures).**

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] `cargo test --workspace` passes (no new failures)
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in changed code
- [x] Only `crates/unimatrix-server/src/infra/config.rs` modified
- [x] Error handling uses `ConfigError::NliFieldOutOfRange` with context (no `.unwrap()` in non-test code)
- [x] No new structs (field added to existing struct; no `#[derive]` change needed)
- [x] Code follows validated pseudocode — no deviations
- [x] All 6 test cases match component test plan expectations
- [x] No file exceeds 500-line limit (config.rs is a large pre-existing file; no split required for this component's additions)

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` -- returned entry #3826 (ADR-004, authoritative for this component), confirming the exact pattern to follow; also surfaced #3822 (near-threshold oscillation pattern) and #3821 (write_pool_server pattern) for context.
- Stored: nothing novel to store -- the `max_graph_inference_per_tick` mirror pattern was already documented in ADR-004 (#3826); this implementation is a mechanical application of that decision with no new gotchas discovered.
