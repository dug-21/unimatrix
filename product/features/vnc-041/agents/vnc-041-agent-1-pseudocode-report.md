# Agent Report — vnc-041-agent-1-pseudocode

## Task
Stage 3a per-component pseudocode for vnc-041 (config seeding + seam-level WARN). Decompose into the five
brief-enumerated components (C1–C5) and produce one pseudocode file per component plus OVERVIEW.md.

## Deliverables
- `product/features/vnc-041/pseudocode/OVERVIEW.md`
- `product/features/vnc-041/pseudocode/seed-write-primitive.md` (C1)
- `product/features/vnc-041/pseudocode/per-slug-seed-renderer.md` (C2)
- `product/features/vnc-041/pseudocode/per-slug-seed-writer.md` (C3)
- `product/features/vnc-041/pseudocode/global-serve-seed.md` (C4)
- `product/features/vnc-041/pseudocode/seam-warn.md` (C5)

## Components covered
C1 seed-write primitive, C2 per-slug seed renderer, C3 per-slug seed writer (register State B+C),
C4 global serve-time seed (http.enabled gate), C5 locked-key seam WARN.

## Invariants encoded (all verified against live code)
- Crate `unimatrix-server` throughout; `unimatrix-engine/src/project.rs` read-only.
- C1: `write_if_absent` via `OpenOptions::create_new(true)`; `AlreadyExists` silent no-op; no `.exists()`
  precheck; `write_default_config_if_absent` delegates the `force=false` branch, keeps the `force=true`
  overwrite branch.
- C2: exhaustive `match` on `OverlayDisposition` (no catch-all); legend keyed on registry key string +
  disposition (field-less locks render "managed globally" with no knob); reuses `DEFAULT_CONFIG_TOML`.
- C3: seed at BOTH State B (after ensure_project_stanza, line 305) and State C (line 342); writes ONLY (b)
  via `per_slug_data_dir`; best-effort; never touches (a)≡(c).
- C4: gated by `if config.http.enabled` (NOT base_dir); local else branch has no seed call site.
- C5: WARN derives from `is_per_slug_overlayable==false` over keys the file SETS (raw `toml::Value` table);
  WARN-only; content-free (key+slug, never value); once per locked key per resolution==per boot.

## Open questions / gaps flagged (none blocking)
1. **`write_if_absent` visibility (C1↔C3):** declare `pub(crate)` so `projects.rs` can call it with the
   C2-rendered body. Signature unchanged — just the modifier. Confirm in 3b.
2. **C5 text-read coordination with `load_single_config`:** lower-risk default is a separate
   `read_to_string` for the WARN pass, leaving the typed load untouched; a read error in the WARN pass must
   not pre-empt the canonical error. Confirm in 3b — does not affect WARN-only behavior.
3. **C5 `flatten_present_keys` nesting depth:** top-level + one sub-level covers the registry's dotted keys;
   confirm no classified key needs deeper nesting in 3b.
4. **OQ-A render granularity (resolved by ADR-003):** implemented the legend-block shape per architect
   recommendation; inline-tag alternative noted but not required.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern, "config seeding write_if_absent create_new no-clobber")
  — low-relevance results (#646 serde-default config extension, #4070/#3773/#2396 config patterns); none
  directly applicable to the seed-write primitive. context_briefing not invoked separately — the brief
  already merged the ADR decisions and the five ADR files were read in full.
- Deviations from established patterns: none. ADR-001 reuses the existing proven `create_new` discipline;
  C5 mirrors the existing `*_sha256` global-wins WARN precedent and the #4749 content-free-logging pattern.
