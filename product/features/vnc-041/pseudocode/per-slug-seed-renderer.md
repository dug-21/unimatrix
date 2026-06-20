# C2 — Per-slug seed renderer (`render_per_slug_seed_toml`)

> ADR-003. Crate: `unimatrix-server`, file `infra/config.rs`. Drives AC-03, addresses SR-02/03/07, R-04, R-12, R-14.

## Purpose

Render the body written to the per-slug file (b). The body has two parts:
1. A **classification-derived legend block** — one commented line per entry in
   `PER_SLUG_CONFIG_CLASSIFICATION`, tagged editable / managed-globally by `OverlayDisposition`.
2. The **reused `DEFAULT_CONFIG_TOML`** template appended verbatim as the editable-knob body.

The legend is the AC-03 contract surface: it is produced by iterating the registry and `match`-ing on
`OverlayDisposition`, so a classification flip mechanically flips the rendered annotation ("proven, not
restated"). B restates NO key list and adds NO new serializer.

## New / modified functions

### `render_per_slug_seed_toml` (NEW — `pub`)

```
pub fn render_per_slug_seed_toml() -> String:
    out = String::new()

    // --- Per-slug header (static comment block; documents intent) ---
    out += "# Per-slug config overlay for this project.\n"
    out += "# Edit the per-slug-overlayable keys below; they overlay the daemon-global config\n"
    out += "# on the next restart (no hot-reload). Keys marked 'managed globally' are ignored\n"
    out += "# here — set them in the global config.toml instead.\n"
    out += "#\n"
    out += "# --- key classification (derived from the daemon's per-slug overlay registry) ---\n"

    // --- Legend block: iterate the registry IN ORDER; exhaustive match, NO catch-all (R-06) ---
    for entry in PER_SLUG_CONFIG_CLASSIFICATION:
        line = match entry.disposition:
            OverlayDisposition::PerSlugOverlayable =>
                format!("# {} — editable here (overlays the global per-slug)\n", entry.key)
            OverlayDisposition::GlobalLocked =>
                format!("# {} — managed globally; a value here is IGNORED\n", entry.key)
            // NO `_ =>` arm. A future OverlayDisposition variant is an INTENDED compile break
            // (ADR-003 forcing function, R-06) — the renderer must classify every disposition.
        out += line

    out += "#\n"
    out += "# --- editable knobs (defaults shown; uncomment and edit to override) ---\n"
    out += "\n"

    // --- Reused template body, verbatim. No struct serialization (Non-Goal). ---
    out += DEFAULT_CONFIG_TOML

    return out
```

## State machine / lifecycle

None. Pure deterministic render — same registry + template ⇒ same output.

## Initialization sequence

None. Free function; reads two `const`/`static` items already in `infra/config.rs`.

## Data flow

- **Inputs:** `PER_SLUG_CONFIG_CLASSIFICATION` (the registry), `DEFAULT_CONFIG_TOML` (the template).
  No parameters — the inputs are module-level constants.
- **Output:** `String` — the complete per-slug seed body.
- **Transformations:** each `ConfigKeyClass { key, disposition }` → one commented legend line keyed on
  `disposition`. Template appended verbatim.
- **Consumer:** C3 passes this `String` to C1's `write_if_absent(path, &body)`.

## Field-less / heterogeneous locks (SR-03, OQ-B, R-12)

The legend is keyed on the registry's `key` **string** + `disposition` — NEVER on a `UnimatrixConfig`
struct field. Therefore:
- `permissive` (no `UnimatrixConfig` field), `tls` / `http` (transport, never read at the seam),
  `*_sha256` descriptors, `rayon_pool_size` all render as `GlobalLocked` legend lines
  ("managed globally; value ignored") with **no editable knob** emitted for them.
- The renderer never dereferences a struct field, so a field-less entry cannot panic and cannot produce
  a bogus editable knob (R-12). Treatment is uniform: every entry is a string + disposition.

## Output validity (R-14)

The legend lines are all `#`-prefixed comments; the appended `DEFAULT_CONFIG_TOML` is the proven, already
parseable template. The full body therefore parses as TOML and, fed through `resolve_slug_config`, resolves
cleanly with NO WARN (a pristine seed sets no global-locked key — everything is commented). This is the
round-trip guarantee for W2/AC-02.

## Error handling

Infallible — returns `String`, no I/O, no fallible calls. No `.unwrap()` (none needed; pure string build).

## Key test scenarios (hints — see RISK-TEST-STRATEGY R-04, R-12, R-14)

- **R-04 #1 (AC-03 field-for-field):** parse/inspect the rendered output; for EVERY entry in
  `PER_SLUG_CONFIG_CLASSIFICATION`, assert overlayable ⇒ "editable here" legend line, locked ⇒
  "managed globally" line. No key outside the registry, none missing (count == registry length).
- **R-04 #3 / R-06 (flip test — "proven, not restated"):** flip one entry's `OverlayDisposition`
  (overlayable↔locked) and assert the rendered legend line for that key flips. This is the AC-03 binding proof.
- **R-06 #3 (exhaustiveness):** the `match` on `OverlayDisposition` has no catch-all — adding a variant
  fails to compile until classified (structural/compile assertion).
- **R-12:** `permissive`, `tls`, `http`, `*_sha256`, `rayon_pool_size` render as "managed globally" legend
  lines with no editable knob; renderer does not panic on any field-less entry.
- **R-14 #1:** the rendered body parses as TOML (legend = comments, body = `DEFAULT_CONFIG_TOML`).
- **R-14 #2/#3:** the rendered body fed to `resolve_slug_config` resolves with no error and no WARN;
  resolved config == global (pristine seed overlays nothing).

## Open questions / gaps

- **OQ-A (resolved by ADR-003, architect recommendation): legend-block granularity.** This pseudocode
  implements the legend-block shape (registry-derived commented lines PREPENDED to the reused template),
  which keeps the proven static template intact and makes the flip test trivial. If a later spec decision
  requires per-key tags woven INLINE into the template body, the same principle holds (tags MUST still be
  produced by iterating the registry, never a hand-list) but the render is heavier. Flagged, not blocking —
  ADR-003 endorses the legend block.
```
