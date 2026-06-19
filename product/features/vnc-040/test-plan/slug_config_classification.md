# Test Plan — `slug_config_classification` (ADR-004 canonical registry)

> Component: `PER_SLUG_CONFIG_CLASSIFICATION: &[ConfigKeyClass]` +
> `enum OverlayDisposition { PerSlugOverlayable, GlobalLocked }` + `fn is_per_slug_overlayable(key)`,
> colocated with `merge_configs` in `infra/config.rs`. **DATA-ONLY** — does NOT rewrite `merge_configs`.
> Owns: **R-14 (AC-11 drift-guard)**, R-07 (closed checklist), R-05 (`*_sha256` global-wins), R-02
> (inference-arm coverage), R-08 (nli params overlayable), R-13 (single-owner doc).
> Tests live in the `infra/config.rs` test module (Rust `#[test]`, pure — no I/O).

## Unit Test Expectations

### AC-11 / R-14 — Machine-checked drift-guard (the centerpiece, MANDATORY)

`test_classification_drift_guard_every_entry_matches_merge_configs`
- **Arrange:** iterate `PER_SLUG_CONFIG_CLASSIFICATION`. For EACH `ConfigKeyClass { key, disposition }`,
  build a `global` and a `per_slug` `UnimatrixConfig` that differ ONLY on `key` (per-slug sets a
  distinct, non-default value; all other fields equal).
- **Act:** `let merged = merge_configs(&global, &per_slug);`
- **Assert:**
  - `PerSlugOverlayable` ⇒ the merged value at `key` == the per-slug value (slug wins).
  - `GlobalLocked` ⇒ the merged value at `key` == the global value (global wins) — INCLUDING the
    `inference.embedding_model_sha256` / `inference.nli_model_sha256` `*_sha256` carve-out (a differing
    per-slug pin does NOT win).
- **Why:** a flipped `merge_configs` arm, or a registry entry classified one way but merged the other,
  fails loudly naming the offending key. This is the crt-031 anti-divergence binding.

`test_classification_registry_exhaustive_vs_validate_config_fields`
- **Assert:** every config key/section that `validate_config` (`infra/config.rs:3413`) reads/constrains
  AND that is reachable at the `build_project_server` call site has a matching
  `PER_SLUG_CONFIG_CLASSIFICATION` entry — no seam-relevant key omitted (architect's open question,
  carry-item 9). A new field in `validate_config`'s set with no registry row fails the test.
- **Note for 3c:** derive the field set mechanically where possible (e.g. a field enumeration helper or
  an explicit `EXPECTED_CLASSIFIED_KEYS` const cross-checked against the registry) so the test breaks on
  addition rather than silently under-covering.

`test_is_per_slug_overlayable_matches_registry_disposition`
- **Assert:** `is_per_slug_overlayable(key)` returns `true` iff the registry classifies `key` as
  `PerSlugOverlayable`, for every registry key, and a stable `false`/panic-free behavior for an unknown
  key (document which — spec/pseudocode decides; the test pins it).

### AC-07 / R-07 — Closed full-call-site checklist (the row-set guard)

`test_verdict_rowset_equals_live_call_site_arguments`
- **Assert:** the verdict row-set (the classified call-site inputs) is EXACTLY the set of
  config-relevant `build_project_server` arguments — the 9 crt-056 params + `embed_handle` +
  `permissive` + `instructions` + the `[embedding]` section — with NONE absent and NO extra. A future
  added `build_project_server` argument with no row fails the test (this risk MATERIALIZED TWICE at the
  design gate: first `embed_handle`, then `instructions`/`permissive`).
- **Per-row disposition assertions:**
  - GLOBAL-LOCKED, proven not overlayable: `embed_handle`, `rayon_pool`, `nli_handle`, `permissive`,
    `[embedding]` section, `*_sha256` pins.
  - OVERLAYABLE, proven overlayable: `nli_top_k`, `nli_enabled`, overlayable `inference.*` weights,
    `confidence.weights`, `knowledge.categories`, `knowledge.boosted_categories`,
    `observation.domain_packs`, `server.instructions`.

### AC-05 / R-05 — Hash-pin global-wins + warn

`test_sha256_pins_global_wins_under_per_slug_pairing`
- **Arrange:** global sets `embedding_model_sha256` / `nli_model_sha256`; per-slug sets DIFFERING pins.
- **Act:** `merge_configs(&global, &per_slug)`.
- **Assert:** merged pins == global pins (per-slug pin does NOT win); a `tracing::warn` naming the
  divergence is emitted (capture via a tracing test subscriber). 

`test_no_global_pin_plus_per_slug_pin_does_not_become_authoritative`
- **Arrange:** global pin unset; per-slug pin set.
- **Assert:** the per-slug pin does NOT silently become authoritative for a model the handle is not
  (descriptor lock) — merged pin behavior matches the documented global-wins/absence semantics, no
  second model described (corroborates AC-04 at the merge level).

### R-02 — Inference-arm field coverage under the C6 call shape

`test_inference_arm_every_field_overlays_or_falls_through`
- **Arrange:** for EACH overlayable `[inference]` field, a per-slug override; a non-overridden sibling
  left global.
- **Assert:** overridden field == per-slug; non-overridden sibling == global (per-key, AC-03). This
  exercises the inline `InferenceConfig {…}` literal (#4070 — the grep-for-spread-misses site) for the
  global→per-slug call shape.
- **Recorded obligation (checked in report, not a behavioral test):** confirm the inline literal lists
  every field explicitly or ends `..InferenceConfig::default()`, and that global→per-slug exercises the
  SAME arm as global→project (no project-only assumption). This is the SR-02/A1 re-audit gate.

### R-08 — nli_top_k / nli_enabled overlayable-as-runtime-param

`test_nli_top_k_and_nli_enabled_overlay_without_model_coupling`
- **Assert:** a per-slug override of `nli_top_k` / `nli_enabled` is reflected in the merged config; the
  registry classifies both `PerSlugOverlayable`; neither selects/reloads a model (handle untouched —
  the behavioral half is in `per_slug_loop`/N=2 harness).

### R-13 — Single-owner / B-renders-from-A (doc-asserted, no behavioral guard here)

`test_classification_is_single_source_of_truth_doc_assertion`
- **Assert (review-grade, recorded in report):** the §9 verdict table and Spec FR-11 table render FROM
  `PER_SLUG_CONFIG_CLASSIFICATION` (no second hand-authored split); Feature B's seed annotations are
  named a CONSUMER (one-way A→B). The behavioral drift guard is AC-11 above; R-13's own residual
  (no runtime warn for an ignored global-locked key) stays documented, NOT test-gated.

## Integration Test Expectations (MCP interface)

**None.** The registry and `merge_configs` overlay-vs-lock behavior are pure Rust, invisible at the
single-server MCP surface. The drift-guard is the authoritative proof (see OVERVIEW §5b). No
`suites/*.py` addition.

## Edge Cases (from Risk Strategy)

- `Option` field set in global, unset in per-slug → global retained via `.or()` (R-02 sibling) — assert.
- List field (`categories`/`boosted_categories`) override → REPLACE not append (#2286, AC-03) — assert.
- Per-slug file empty / all-default → merged == global (degenerate; merge yields global per key).
- A key classified but not actually merged by `merge_configs` (or vice-versa) → drift-guard FAILS
  loudly (the whole point).

## Assertions Summary (concrete)

- `merge_configs(g, s).at(key) == s.at(key)` for every `PerSlugOverlayable` key.
- `merge_configs(g, s).at(key) == g.at(key)` for every `GlobalLocked` key (incl. both `*_sha256`).
- verdict row-set `==` live `build_project_server` config-relevant argument set (closed, none absent).
- `tracing::warn` emitted on `*_sha256` divergence.
- registry keys ⊇ `validate_config`'s seam-relevant field set (exhaustiveness).
