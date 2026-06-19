## ADR-003: Re-validate the MERGED per-slug config, not only each per-slug file

### Context
SR-01 (High/High) is the highest-rated scope risk. AC-08 as scoped validates each per-slug file
**independently**. Unimatrix #3905 proved that a two-level merge can pass per-file validation yet
produce a merged struct that violates a **cross-field** invariant: `InferenceConfig`'s sum-of-six
fusion weights (`w_sim + w_nli + w_conf + w_coac + w_util + w_prov ≤ 1.0`). Each file sets a
*different* subset of weights non-default; both validate alone; the field-by-field merge combines
them into a sum > 1.0. #3905's fix was to call `validate_config(&merged, …)` after
`merge_configs()` in `load_config`. vnc-040 adds a **THIRD** precedence layer (global → project →
per-slug all feed one merged struct), widening this surface.

### Decision
`resolve_slug_config` MUST call `validate_config(&merged, &path)` immediately after
`merge_configs(&global, &slug_file)` and before the merged config is returned or consumed — in
addition to the per-file `validate_config(&slug_file, &path)` that runs before the merge. Both
validations use the EXISTING `validate_config`; vnc-040 adds **no new validation logic**, only a
second invocation site on the new merge result.

Cross-field constraints the post-merge call re-checks (spec to enumerate exhaustively from
`validate_config`; known classes): fusion-weight sum-of-six ≤ 1.0; PPR/confidence weight
constraints; the custom-preset cross-level inheritance prohibition (#3923, enforced in
`validate_config`); category/instruction size and well-formedness bounds. The design test for the
spec, per #3905: for every sum/cross-field constraint, ask "can a global override of field A
combined with a per-slug override of field B violate it?" — and rely on the merged-config
validation to catch it.

This makes the post-merge re-validation an explicit acceptance criterion (strengthening AC-08),
not merely the per-file check.

### Consequences
- **Easier:** the third-layer cross-level invariant gap is closed at the single point where the
  merge happens; an invalid merged per-slug config fails loud at startup naming the slug file.
- **Cost:** one extra `validate_config` call per slug that has a config file — startup-only,
  negligible.
- **Necessary, not optional:** per-file validation alone is provably insufficient for cross-field
  invariants (#3905); skipping the merged-config validation would re-open exactly the bug #3905
  documented, now on a wider third-layer surface.
- Depends on ADR-001 (the helper that owns the merge) and complements ADR-002 (the model
  invariants that hold by construction, separate from these cross-field value invariants).
