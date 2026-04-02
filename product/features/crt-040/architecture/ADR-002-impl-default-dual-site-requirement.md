## ADR-002: InferenceConfig supports_cosine_threshold — Mandatory Dual-Site Default

### Context

`InferenceConfig` in `infra/config.rs` uses two independent mechanisms to encode
default values for each field:

1. A `#[serde(default = "fn_name")]` attribute on each field, backed by a private
   `fn default_fn_name() -> T { value }` function. This default fires when the field
   is absent from a TOML/JSON deserialization input.

2. An explicit `impl Default for InferenceConfig { ... }` block containing a struct
   literal with hardcoded values for each field. This default fires when Rust code
   calls `InferenceConfig::default()`.

These two paths are independent. Updating only one leaves a silent behavioral divergence:
- If only the serde fn is updated, `InferenceConfig::default()` returns the old value.
- If only the impl Default is updated, deserialization of a config.toml that omits the
  field returns the old value.

This was the root cause of the crt-038 gate-3b rework (lesson #4014): all
`default_w_*()` backing functions were updated to conf-boost-c values, but `impl Default`
retained the old literals. The AC-01 test used `toml::from_str` (serde path) and passed;
the impl Default path was silently wrong and was caught only at gate review.

Pattern #3817 (Unimatrix) documents this as "the dual-site maintenance trap" for
`InferenceConfig`. crt-040 touches `InferenceConfig` to add `supports_cosine_threshold`.

### Decision

The new `supports_cosine_threshold` field requires three simultaneous, atomic changes
in `infra/config.rs`:

1. Field declaration with serde annotation:
   ```rust
   #[serde(default = "default_supports_cosine_threshold")]
   pub supports_cosine_threshold: f32,
   ```

2. Backing function:
   ```rust
   fn default_supports_cosine_threshold() -> f32 {
       0.65
   }
   ```

3. Explicit value in `impl Default` struct literal (inside the `InferenceConfig { ... }`
   block):
   ```rust
   supports_cosine_threshold: default_supports_cosine_threshold(),
   ```

   Using a call to the backing function in the impl Default literal (rather than a
   repeated literal `0.65`) eliminates future divergence risk: there is now one source
   of truth for the value. Any future change to the default updates only the backing
   function and both paths change atomically.

Additionally: the dead field `nli_post_store_k` is removed in the same change. Its
serde backing function `default_nli_post_store_k()` and its literal in impl Default
(`nli_post_store_k: 10`) are both removed. Serde ignores unknown fields during
deserialization, so existing `config.toml` files containing `nli_post_store_k` will
silently drop the value on next load — which is correct behavior for a removed field.

A unit test MUST assert:
```rust
assert_eq!(InferenceConfig::default().supports_cosine_threshold, 0.65_f32);
```
This test exercises the impl Default path and is independent of the serde deserialization
tests that exercise the `#[serde(default = ...)]` path.

Delivery must grep the test module for all occurrences of `supports_cosine_threshold` and
verify every test assertion reflects the correct value before marking AC-09/AC-10 complete.

### Consequences

**Easier:**
- Dual-site divergence is structurally prevented for `supports_cosine_threshold` by
  delegating the impl Default literal to the backing function.
- The test requirement makes the dual-site invariant machine-checkable.
- Removing `nli_post_store_k` eliminates a dead field that has confused agents since
  crt-038 (the field's doc comment still references `run_post_store_nli` which no
  longer exists).

**Harder:**
- The serde backing function pattern must be followed for every future InferenceConfig
  field. This is a convention, not enforced by the type system. The pattern must be
  re-stated in any spec that touches InferenceConfig.
- Removing `nli_post_store_k` while adding `supports_cosine_threshold` in the same PR
  means the removal must be isolated in its own test verification step to avoid
  masking regressions (SCOPE.md SR-06).
