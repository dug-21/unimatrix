## ADR-002: NliStoreConfig Deleted Entirely — No Partial Field Retention

Feature: crt-038 — conf-boost-c formula and NLI dead-code removal. Status: Accepted.

### Context

`NliStoreConfig` is a `pub(crate) struct` in `store_ops.rs` with five fields:
- `enabled: bool`
- `nli_post_store_k: usize`
- `nli_entailment_threshold: f32`
- `nli_contradiction_threshold: f32`
- `max_contradicts_per_tick: usize`

The struct is constructed in `services/mod.rs` (~line 435) from `InferenceConfig`
fields of the same names, then stored as `nli_cfg: NliStoreConfig` on the store ops
context. Its sole consumer is the `run_post_store_nli` spawn block (~line 313 of
store_ops.rs).

The SCOPE.md Background section listed these fields as potentially retained, creating
an apparent conflict with AC-14, which requires the struct to be deleted entirely.
SR-04 identifies this contradiction explicitly.

The SCOPE.md also notes that `InferenceConfig` retains fields of the same names for
use by `run_graph_inference_tick` and operator tooling. These are separate structs.

### Decision

**AC-14 is authoritative**: `NliStoreConfig` is deleted entirely from `store_ops.rs`.
All five fields are removed. The struct, its `impl Default`, the `nli_cfg` context
field, and the constructor parameter are all deleted.

`InferenceConfig` retains its same-named fields independently — they are used by
`run_graph_inference_tick` and are outside this feature's removal scope. The two
structs are named differently (`NliStoreConfig` vs `InferenceConfig`) and serve
different purposes; their field name overlap is coincidental.

The partial-retention alternative (keeping some `NliStoreConfig` fields for
hypothetical future consumers) is rejected because:
1. No current consumer of `NliStoreConfig` fields exists after `run_post_store_nli`
   is removed.
2. Dead fields in a struct are indistinguishable from live fields at a glance;
   retaining them creates confusion about what is actually used.
3. `InferenceConfig` already provides these values at any point where the store ops
   context is constructed; a future consumer can read them directly from there.

### Consequences

Easier:
- `store_ops.rs` has no dead struct definitions after the change.
- The constructor for the store ops context has fewer parameters.
- `services/mod.rs` does not need to import or instantiate `NliStoreConfig`.

Harder:
- Any future feature that wants per-call NLI config in the store path will need to
  re-introduce a struct (or pass `InferenceConfig` directly). This is a minor
  re-derivation cost, not a blocking concern given the NLI dead-code context.
