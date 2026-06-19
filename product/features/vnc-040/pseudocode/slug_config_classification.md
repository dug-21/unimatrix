# Component 1 — `slug_config_classification` (ADR-004 canonical registry)

> NEW, **DATA-ONLY**. Colocated with `merge_configs`/`validate_config` in `infra/config.rs`.
> ADR-004 (#5210), FR-16, AC-11, R-14. **Does NOT rewrite `merge_configs`** — no generic merge
> engine, no new config knob. The registry is the single owner of the per-slug-vs-global split; the
> §9 verdict table, `merge_configs`' real behavior (via the drift-guard test), and Feature B's seed
> annotations all RENDER from it (one-way: A owns, B consumes).

## Purpose

Declare exactly ONE authoritative classification of every call-site-relevant config key/section as
`PerSlugOverlayable` or `GlobalLocked`, plus a lookup predicate. Bind `merge_configs`' actual
overlay-vs-lock behavior to it with a machine-checked drift-guard test (the anti-divergence
guarantee). Retires R-13's "unowned split."

## New types (verbatim shapes from ADR-004)

```
pub enum OverlayDisposition { PerSlugOverlayable, GlobalLocked }   // derive Debug, Clone, Copy, PartialEq, Eq

pub struct ConfigKeyClass {
    pub key: &'static str,                 // stable id, e.g. "knowledge.categories"
    pub disposition: OverlayDisposition,
}
```

## `PER_SLUG_CONFIG_CLASSIFICATION` — the canonical slice

```
pub const PER_SLUG_CONFIG_CLASSIFICATION: &[ConfigKeyClass] = &[
    // ---- PerSlugOverlayable (verdict rows 3-9, I; FR-03/FR-14) ----
    { "knowledge.categories",           PerSlugOverlayable },   // row 7  (AC-01)
    { "knowledge.boosted_categories",   PerSlugOverlayable },   // row 9
    { "confidence.weights",             PerSlugOverlayable },   // row 6
    { "observation.domain_packs",       PerSlugOverlayable },   // row 8
    { "inference.nli_top_k",            PerSlugOverlayable },   // row 3  (runtime param, not model id)
    { "inference.nli_enabled",          PerSlugOverlayable },   // row 4
    // overlayable inference weights (replace arm) — enumerate each weight key that merge_configs
    // overlays so the drift-guard covers them individually, e.g.:
    { "inference.w_sim",                PerSlugOverlayable },
    { "inference.w_nli",                PerSlugOverlayable },
    { "inference.w_conf",               PerSlugOverlayable },
    { "inference.w_coac",               PerSlugOverlayable },
    { "inference.w_util",               PerSlugOverlayable },
    { "inference.w_prov",               PerSlugOverlayable },
    // PPR overlayable weights merge_configs overlays (rust-dev: align to the real field names):
    { "inference.ppr_alpha",            PerSlugOverlayable },
    { "inference.ppr_blend_weight",     PerSlugOverlayable },
    // ... (rust-dev: every inference.* weight key merge_configs replaces gets a row — exhaustive)
    { "server.instructions",            PerSlugOverlayable },   // row I  (#785, FR-14)

    // ---- GlobalLocked (verdict rows 0-2, P, embedding section, transport; FR-04/05/06/09/15) ----
    { "inference.embedding_model_sha256", GlobalLocked },       // hash pin, global-wins (#4655)
    { "inference.nli_model_sha256",       GlobalLocked },       // hash pin, global-wins (#4649)
    { "inference.rayon_pool_size",        GlobalLocked },       // row 1 (pool global)
    { "permissive",                       GlobalLocked },       // row P (FR-15)
    { "server.tls",                       GlobalLocked },       // transport (AC-06)
    { "http",                             GlobalLocked },       // transport (http.enabled etc.)
    // auth/host transport keys merge_configs locks global — enumerate the real key ids:
    // { "server.auth", GlobalLocked }, { "server.host", GlobalLocked }, ...
];
```

> **EXHAUSTIVENESS obligation (carry-item 9, architect open question).** The key list MUST be
> exhaustive against `validate_config`'s / `merge_configs`' field set for every seam-relevant key:
> no overlayable field merged but unclassified, no locked field merged but unclassified. The
> `*_sha256` global-wins carve-out, `permissive`, and transport sections are explicitly present.
> rust-dev + tester reconcile the literal key strings against the real struct field paths
> (`InferenceConfig` weight + PPR field names, `ServerConfig` transport field names) when
> implementing — the strings above are the disposition map, the EXACT identifiers are bound by the
> drift-guard test below. A key in the merge but absent here (or classified one way, merged the
> other) breaks the build via the drift-guard.

## `is_per_slug_overlayable` predicate

```
pub fn is_per_slug_overlayable(key: &str) -> bool:
    for entry in PER_SLUG_CONFIG_CLASSIFICATION:
        if entry.key == key:
            return entry.disposition == PerSlugOverlayable
    // Unknown key: not in the seam classification. Conservative default = false (treat as locked /
    // not-overlayable). rust-dev: a *seam-relevant* key missing here is a drift-guard failure, so an
    // unknown key reaching this predicate at runtime is by construction a non-seam key.
    return false
```

## Data flow

- **Input:** a `&str` key id (for the predicate) / the const slice itself (for renderers + test).
- **Output:** `bool` (predicate); `&[ConfigKeyClass]` (the canonical data).
- **Transformations:** none — pure data + linear lookup. No I/O, no config mutation, no merge.

## Error handling

- None at runtime — the registry is `const`, the predicate is total (returns `bool`, never panics,
  no `.unwrap()`). The ONLY failure surface is COMPILE/TEST time: the drift-guard test fails the
  build if the classification disagrees with `merge_configs`.

## Drift-guard test (AC-11 / R-14 — MANDATORY, lives in `infra/config.rs` `#[cfg(test)]` mod)

```
#[test] fn classification_matches_merge_configs_behavior():
    for entry in PER_SLUG_CONFIG_CLASSIFICATION:
        // Build two configs differing ONLY on entry.key:
        let global = UnimatrixConfig::default()    with entry.key set to value_A
        let slug   = UnimatrixConfig::default()    with entry.key set to value_B  (value_B != value_A)
        // NOTE: merge_configs CONSUMES both args (owned) → pass clones / fresh builds.
        let merged = merge_configs(global.clone(), slug.clone())
        match entry.disposition:
            PerSlugOverlayable => assert merged.<key> == value_B    // overlay won (slug value)
            GlobalLocked       => assert merged.<key> == value_A    // lock held (global value)
        // *_sha256 carve-out is a GlobalLocked row → asserted global-wins here, covering AC-05's
        // merge half (the tracing::warn half is asserted in resolve/merge-level tests, R-05).
```

Key-string → struct-field accessor mapping is a per-entry match inside the test (the test is the
binding that ties `entry.key` strings to real fields). A classification entry with no corresponding
field accessor — or a merge arm that disagrees — fails this test.

## Key test scenarios (hints for tester)

1. **AC-11 / R-14 #1** — every registry entry drives `merge_configs`; overlayable⇒slug wins,
   locked⇒global wins. (the test above)
2. **R-05 carve-out** — `inference.embedding_model_sha256` + `inference.nli_model_sha256` rows
   assert global-wins under a differing per-slug pin (the merge half of AC-05).
3. **Exhaustiveness** — a meta-assertion that every seam-relevant `merge_configs` field has a
   classification row (closed-set discipline mirroring R-07): adding a merged field without a
   registry entry fails. (tester: derive the field set from the live merge/validate surface.)
4. **`is_per_slug_overlayable` predicate** — returns `true` for a sampled overlayable key, `false`
   for a sampled locked key, `false` for an unknown key.
5. **R-13 ownership** — assert (doc/review-level) the §9 verdict table and Feature B's future seed
   render FROM this slice, not a hand-authored copy.

## Anti-patterns guarded

- DO NOT add merge logic here — data-only (ADR-004 §Decision).
- DO NOT let the §9 table or Feature B re-state the split — they render from this slice (R-13, #4869
  one-way A→B contract).
