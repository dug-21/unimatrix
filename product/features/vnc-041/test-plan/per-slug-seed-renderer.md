# C2 — Per-Slug Seed Renderer Test Plan (`render_per_slug_seed_toml`)

> File: `infra/config.rs`. ADR-003 (#5237). Risks: **R-04 (Critical)**, R-06, R-12, R-14.
> ACs: **AC-03** (annotations match classification field-for-field; flip flips the annotation).
> Tests live in a new `infra/config.rs` mod-test (or a sibling `*_tests.rs` module, mirroring
> `slug_config_classification_tests.rs`).

## What this component is

```rust
pub fn render_per_slug_seed_toml() -> String;
```
Body = (1) a **classification-derived legend block** produced by iterating
`PER_SLUG_CONFIG_CLASSIFICATION` and `match`ing on `OverlayDisposition`
(`PerSlugOverlayable` ⇒ "editable here"; `GlobalLocked` ⇒ "managed globally; value IGNORED"),
PREPENDED to (2) the reused `DEFAULT_CONFIG_TOML` knob template verbatim. No new serializer; no
hand-list. The `match` MUST be exhaustive (no catch-all) — a new `OverlayDisposition` variant is an
intended compile break (R-06 forcing function).

## Unit tests

### R-04 / AC-03 — full-registry annotation coverage (no key outside/missing the registry)
- `test_render_legend_covers_every_registry_entry`
  Arrange: render once. Act: for EVERY `entry` in `PER_SLUG_CONFIG_CLASSIFICATION`, search the legend.
  Assert: the legend contains a line naming `entry.key`. Assert the legend line count == registry length
  (no extra keys, none missing). This binds the legend to the registry mechanically.
- `test_render_overlayable_keys_render_editable_line`
  For each `PerSlugOverlayable` entry, assert its legend line contains the "editable here" marker.
- `test_render_locked_keys_render_managed_globally_line`
  For each `GlobalLocked` entry, assert its legend line is commented-out AND contains "managed globally"
  (+ "value ignored" per AC-03/ADR-003). Verifies AC-03 "global-locked keys INCLUDED but commented-out".
- `test_render_legend_lines_keyed_on_registry_disposition`
  For each entry, derive expected marker from `entry.disposition` and assert the rendered line matches —
  the render is a pure function of the registry, no second source.

### R-04 / R-06 / AC-03 — the FLIP TEST (proven, not restated) — load-bearing centerpiece
- `test_render_legend_flips_when_disposition_flips`
  This is the AC-03 "proven, not restated" proof. Approach (delivery picks the cleanest mechanism that
  does NOT mutate the production registry across tests):
  - Preferred: factor the legend render to accept a `&[ConfigKeyClass]` slice (or render a single
    `ConfigKeyClass`), so the test can pass a one-entry slice with disposition `PerSlugOverlayable`,
    assert the line is "editable here", then the same key with `GlobalLocked`, assert it flips to
    "managed globally". One flip → the rendered annotation flips. This proves the legend derives from
    the disposition at render time, not a hardcoded string per key.
  - If the public surface is only `render_per_slug_seed_toml() -> String`, expose a
    test-visible `render_legend_line(entry: &ConfigKeyClass) -> String` (or `#[cfg(test)]` helper) and
    flip-test that. The whole-string render is then covered by the full-registry tests above.
  NOTE: this flip test pairs with the C5 WARN flip test (seam-warn.md). ONE conceptual flip must move
  BOTH behaviors; they may share a flip harness but each asserts its own surface.

### R-06 — exhaustiveness forcing function (compile-time)
- `test_render_match_is_exhaustive_over_overlay_disposition` (documentation + structural)
  The `match` on `OverlayDisposition` in the renderer has no catch-all `_` arm. Verified structurally at
  Stage 3c (adding a third variant must fail to compile in the renderer). Document as a build-time
  invariant. (Mirrors the Feature A exhaustiveness discipline in `slug_config_classification_tests.rs`.)

### R-12 — field-less / shape-mismatched locks render safely (no panic, no bogus knob)
- `test_render_fieldless_locks_render_managed_globally_no_knob`
  Assert `permissive`, `tls`, `http`, and the `*_sha256` descriptors
  (`inference.embedding_model_sha256`, `inference.nli_model_sha256`) each appear in the legend as
  "managed globally; value ignored" with NO editable knob line emitted for them. (They are keyed on the
  registry `key` string + disposition, never a struct field — ADR-003 / R-12.)
- `test_render_does_not_panic_for_any_registry_entry`
  Render the full body; assert it completes (no panic) — covers field-less entries that have no
  `UnimatrixConfig` field. The legend never dereferences a struct field.
- `test_render_legend_lists_exactly_registry_dotted_keys`
  Assert the legend's key set == the registry's dotted-key set (e.g. `inference.w_sim`, `permissive`),
  count matches registry length (R-12 scenario 3).

### R-14 — seeded body is valid TOML (the body half of the round-trip)
- `test_render_output_parses_as_valid_toml`
  Act: `toml::from_str::<toml::Value>(&render_per_slug_seed_toml())`. Assert: Ok. The legend lines are
  comments; the body is the proven `DEFAULT_CONFIG_TOML`. (Mirrors the existing
  `test_write_default_config_creates_file_when_absent` parse check.)
- `test_render_output_deserializes_to_default_unimatrix_config`
  Act: parse render output as `UnimatrixConfig`. Assert: equals compiled defaults (all knob lines are
  comments; legend sets nothing) — confirms a pristine seed overlays NOTHING (R-14, no WARN later).

## Integration test (cross-component, R-14)

- `test_render_seed_resolve_roundtrip_pristine_no_warn` (threads C2 → C3-style write → C5 resolve)
  Arrange: write `render_per_slug_seed_toml()` to `{base}/{slug}/config.toml`. Act: `resolve_slug_config`.
  Assert: Ok, resolved config equals the global (seed overlays nothing), AND no WARN emitted (all locked
  keys commented out). This is the R-14 coverage requirement: a pristine seed parses, resolves cleanly,
  emits no WARN. (May live in the C5 module since it exercises the resolver; cross-referenced here.)

## Coverage requirement (RISK-TEST-STRATEGY R-04, R-12, R-14)

Annotation binds to the registry at runtime (full-registry coverage + flip test moves the annotation);
no hand-enumerated key list in B; every field-less lock renders "managed globally" with no knob and no
panic; the rendered body is valid TOML that resolves to defaults and emits no WARN.
