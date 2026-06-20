# C5 — Locked-key seam WARN (`resolve_slug_config` file-present arm)

> ADR-005. Crate: `unimatrix-server`, file `http_provision.rs`. Drives AC-04, addresses SR-02/06/07, R-04, R-07, R-08, R-12.

## Purpose

When a per-slug file (b) SETS a key that is global-locked (`is_per_slug_overlayable == false`),
`resolve_slug_config` emits ONE `tracing::warn` per locked key per boot, naming key + slug — replacing the
current silent ignore (R-13). WARN-ONLY: the value is already ignored by the merge; the resolution output,
return type, and all error paths are UNCHANGED. The locked surface DERIVES from the Feature A registry at
runtime — no hand-list in B.

## Anchor (verified, http_provision.rs:310–354)

`resolve_slug_config` (signature UNCHANGED):
- **No-file arm** (lines 322–328): `Cow::Borrowed(global)` byte-for-byte fallthrough — **UNTOUCHED** by C5.
- **File-present arm** (lines 330–353): `load_single_config` (typed) → `validate_config` (per-file) →
  `merge_configs` → `validate_config` (post-merge) → `Cow::Owned(merged)`. C5 adds a WARN pass at the head
  of this arm, alongside (not replacing) the typed parse. `config_err(slug, path, detail)` exists for
  slug-named errors. `PROJECT_CONFIG_NAME` and the path-join are already in this arm.

## The detection problem (why a raw parse is needed)

`load_single_config` deserializes straight into typed `UnimatrixConfig`. Once typed, a key set-to-its-default
is indistinguishable from an absent key — the typed struct cannot tell whether the operator SET a locked key.
Detecting "the file SETS a locked key" requires inspecting which keys are PRESENT in the raw TOML. So C5
parses the file text once into a raw `toml::Value` table, alongside the typed parse.

## New / modified functions

### `warn_locked_keys` (NEW — private helper)

```
fn warn_locked_keys(text: &str, slug: &ProjectSlug):
    // 1. Raw parse — INDEPENDENT of the typed parse; degrade silently on failure (R-07 #3).
    //    The WARN pass MUST NOT introduce a new error path. If the raw parse fails, return —
    //    the existing load_single_config in the same arm already surfaces the loud, slug-named
    //    ServerError::Config; the WARN pass never converts a parseable file into an error.
    raw = match toml::from_str::<toml::Value>(text):
        Ok(v)  => v
        Err(_) => return    // no WARN; do NOT error

    // 2. Enumerate PRESENT keys as dotted identifiers matching the registry's `key` strings
    //    (e.g. "inference.embedding_model_sha256", "tls", "permissive", "knowledge.categories").
    //    Walk the top-level table; for a sub-table, emit "section.key"; for a leaf at top level,
    //    emit "key". Section granularity mirrors PER_SLUG_CONFIG_CLASSIFICATION dotted keys.
    present_keys = flatten_present_keys(raw)   // see helper below

    // 3. For each present key, consult the registry. is_per_slug_overlayable(key) == false ⇒
    //    GlobalLocked (or unknown/non-seam key — the conservative default also returns false).
    //    DERIVES from the registry at runtime; no key list restated in B (SR-02/SR-07).
    for key in present_keys:
        if !config::is_per_slug_overlayable(&key):
            tracing::warn!(
                slug = %slug, key = %key,
                "per-slug config sets a global-locked key; value is ignored (managed globally)"
            )
            // CONTENT-FREE (C-11, #4749): log key + slug only — NEVER the operator's set VALUE.
```

### `flatten_present_keys` (NEW — private helper)

```
fn flatten_present_keys(raw: toml::Value) -> Vec<String>:
    keys = Vec::new()
    if let toml::Value::Table(top) = raw:
        for (name, value) in top:
            match value:
                toml::Value::Table(sub) =>
                    for (sub_name, _) in sub:
                        keys.push(format!("{name}.{sub_name}"))   // "section.key"
                _ =>
                    keys.push(name)                                // top-level leaf, e.g. "permissive"
    return keys
    // NOTE: one level of nesting matches the registry's dotted-key shape (section.key). Deeper
    // nesting is not part of the per-slug classification surface; top-level + one sub-level
    // covers every PER_SLUG_CONFIG_CLASSIFICATION key (confirm against the registry in 3b).
```

### `resolve_slug_config` change (signature UNCHANGED — additive)

```
// FILE-PRESENT ARM (after confirming is_file, before/at the head of the load):
// read the text ONCE, reuse it for both the WARN pass and (where possible) the typed load.
let text = std::fs::read_to_string(&path).map_err(|e| config_err(slug, &path, &e.to_string()))?;

warn_locked_keys(&text, slug);                 // ◄── NEW (C5). Pure observation; no `?`, never errors.

// existing flow continues, UNCHANGED in behavior:
let slug_file = load_single_config(&path)?;    // typed parse (existing error source)
//   ... validate / merge / post-merge validate / Cow::Owned(merged) ...
```

IMPLEMENTATION NOTE (do NOT change resolution behavior): the WARN pass reads the file text and parses raw.
`load_single_config` currently reads the path itself. Two acceptable shapes — confirm in 3b without altering
the typed load's error semantics:
  (a) read `text` once here, call `warn_locked_keys(&text, slug)`, keep `load_single_config(&path)` as-is
      (one extra `read_to_string` — negligible, boot-only); OR
  (b) if `load_single_config` exposes a from-text variant, feed it the same `text`.
Shape (a) is the lower-risk default (leaves `load_single_config` untouched). EITHER WAY the typed parse
remains the SOLE error source; the raw pass NEVER adds an error.

## State machine / dedup (FR-11, OQ-C, R-08)

ADR-005 / brief OQ-C: the resolver runs **once per slug per boot** (the per-slug loop, main.rs:1089), so
once-per-resolution IS once-per-boot. No persistent dedup structure is required: a single `resolve_slug_config`
call emits at most one warn per locked key it finds (the `for` loop visits each present key once; a key
appears once in the raw table). This is naturally once-per-(slug, key)-per-boot.

- **Do NOT** add cross-boot persistence (state must reset every boot — R-08 #3).
- **Do NOT** add cross-slug shared state (one slug's WARN must not suppress another's — R-08 #2; the warn is
  keyed on the `slug` argument, so each slug's resolution warns independently).
- If a future caller invokes `resolve_slug_config` multiple times per boot for the same slug, the loop would
  re-warn. Per ADR-005 that is not the current call pattern; if it changes, scope any dedup set per
  (slug, key) per boot — never process-global-across-boots. (Flagged; current pattern needs none.)

## Initialization sequence

None. C5 is an inline pass in an existing function.

## Data flow

- **Inputs:** the per-slug file text, `&slug`, `is_per_slug_overlayable` (registry predicate).
- **Output:** zero or more `tracing::warn` lines. NO change to the function's return value.
- **Transformations:** text → raw `toml::Value` → flattened dotted keys → filter by
  `!is_per_slug_overlayable` → warn.

## WARN-only invariant (SR-06, R-07) — the dominant scope-creep risk

- The merge/validate flow, the return type (`Cow<UnimatrixConfig>`), and every error path are UNCHANGED.
  The locked value remains IGNORED exactly as Feature A does today — the merged value is the global, not
  the per-slug, value.
- The raw parse adds NO failure mode: on a raw-parse error it returns (no warn, no error); the existing
  `load_single_config` is the sole error source and still surfaces the loud slug-named `ServerError::Config`.
- The no-file arm is NOT touched — vnc-040's byte-for-byte fallthrough sentinel stays green.
- The pre-existing `*_sha256` divergence warn inside `merge_configs` is left UNCHANGED. For a
  set-and-diverging `*_sha256`, both warns may fire — acceptable complementary signal, not a defect.

## Error handling

- The single `?` in the new code is on `read_to_string` (shape (a)) — but that path is ALREADY read by
  `load_single_config`; to avoid introducing a new error before the typed parse, prefer reading text and on
  a read error, skip the WARN pass and let `load_single_config` produce the canonical error. (Confirm in 3b:
  ensure the WARN read does not pre-empt or alter the typed load's error.) The WARN logic itself
  (`warn_locked_keys`, `flatten_present_keys`) is infallible and emits only logs.
- No `.unwrap()`, no panic, no new `ServerError` variant (NFR-07, R-07).

## Security (RISK-TEST-STRATEGY Security Risks)

- (b) is operator-authored file input parsed twice (typed + raw). A malformed/hostile TOML must not crash
  the resolver via the WARN pass — `toml::from_str` returning `Err` degrades to no-warn (R-07).
- The slug is a validated `ProjectSlug` newtype (vnc-038); registry keys are static. The WARN names bounded
  identifiers only — NEVER the operator's set value (C-11, #4749 content-free logging).

## Key test scenarios (hints — see RISK-TEST-STRATEGY R-04, R-07, R-08, R-12)

- **R-04 #2 / AC-04:** for each `GlobalLocked` key set in (b), a WARN fires naming key + slug; for each
  `PerSlugOverlayable` key set, NO WARN.
- **R-04 #3 (flip test):** flip one key's `OverlayDisposition` and assert the WARN behavior for that key
  flips — proves the WARN derives from the registry at runtime (pairs with C2's flip test).
- **R-04 #4 / edge case:** an unknown/typo'd key set in (b) ALSO warns (conservative
  `is_per_slug_overlayable == false` default) — explicitly assert and document.
- **R-07 #1 (FR-12 equivalence — CRITICAL):** resolution output (`Cow<UnimatrixConfig>`) for a (b) with a
  global-locked override present is value-identical WITH and WITHOUT the WARN code path — only logs differ.
- **R-07 #2:** the locked override value remains ignored after the WARN — merged value is the global.
- **R-07 #3:** a malformed (b) the raw pass cannot inspect adds NO new error — `load_single_config` is the
  sole error source; a malformed file still surfaces the existing loud slug-named `ServerError::Config`.
- **R-07 #4:** a slug with no (b) ⇒ no WARN, byte-for-byte fallthrough (no-file arm untouched).
- **R-08 #1:** repeated `resolve_slug_config` for the same slug+key in one boot ⇒ at most one WARN per key.
- **R-08 #2:** two slugs each setting the same locked key ⇒ a distinct WARN per slug (no cross-slug suppression).
- **R-12 #1:** (b) setting `permissive` / `tls` / `*_sha256` / `rayon_pool_size` ⇒ WARN fires (all are
  `GlobalLocked` in the registry; uniform treatment, no special-casing).
- Empty (b) (zero keys) ⇒ no WARN; (b) setting only overlayable keys ⇒ no WARN.

## Open questions / gaps

- **Text-read coordination with `load_single_config`.** Whether to read the file text once in C5 and reuse
  it, or read twice, depends on whether `load_single_config` exposes a from-text path and whether it carries
  the 64 KiB cap / `0o022` permission check on the read. The lower-risk default (shape (a): separate
  `read_to_string` for the WARN pass, leave `load_single_config(&path)` untouched) preserves the typed load's
  existing hardening and error semantics. Flagged for the C5 implementer to confirm in 3b — it does NOT
  change the WARN-only behavior, only how the bytes are obtained. If shape (a) is chosen, ensure a read error
  in the WARN pass does NOT pre-empt the canonical `load_single_config` error (skip the WARN on read error,
  let the typed load error speak).
- **`flatten_present_keys` nesting depth.** Top-level + one sub-level covers the current registry's dotted
  keys. Confirm against `PER_SLUG_CONFIG_CLASSIFICATION` in 3b that no classified key needs deeper nesting.
```
