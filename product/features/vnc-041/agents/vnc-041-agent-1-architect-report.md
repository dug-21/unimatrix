# Agent Report — vnc-041-agent-1-architect

## Task
Produce the architecture for vnc-041 — config seeding (global + per-slug) plus seam-level
WARN for global-locked keys (C17 / vnc-040 Feature B).

## Artifacts
- `product/features/vnc-041/architecture/ARCHITECTURE.md`
- `product/features/vnc-041/architecture/ADR-001-seed-write-primitive.md` (Unimatrix #5235)
- `product/features/vnc-041/architecture/ADR-002-per-slug-seed-in-register.md` (Unimatrix #5236)
- `product/features/vnc-041/architecture/ADR-003-annotations-render-from-classification.md` (Unimatrix #5237)
- `product/features/vnc-041/architecture/ADR-004-global-seed-http-enabled-gate.md` (Unimatrix #5238)
- `product/features/vnc-041/architecture/ADR-005-seam-warn-locked-keys.md` (Unimatrix #5239)

## Key decisions
- ADR-001 — seed writes use the existing `create_new(true)` no-clobber primitive
  (`write_default_config_if_absent`), never `atomic_write`/`File::create`/`fs::write`. Factor
  a content-parameterized `write_if_absent` so the per-slug seed (different body) shares it.
- ADR-002 — per-slug seed is eager in `register` (State B + C), writes ONLY file (b)
  `{base_dir}/{slug}/config.toml` via the existing `per_slug_data_dir` join; never touches
  the shared (a)≡(c) path-hash file.
- ADR-003 — per-slug annotations render from `PER_SLUG_CONFIG_CLASSIFICATION` (a derived
  legend block + reused `DEFAULT_CONFIG_TOML` body); no new serializer.
- ADR-004 — global serve-time seed gated by `if config.http.enabled` (the real container
  seam), NOT the `base_dir` arg. **Correction**: serve always passes `base_dir = None`;
  container-vs-local is decided by `dirs::home_dir()`, so the risk-assessment's recommended
  `base_dir = Some(/data)` gate would never fire.
- ADR-005 — seam WARN parses the raw TOML to see which keys the file SETS, warns one-per-
  locked-key-per-boot for any key where `is_per_slug_overlayable` is false; WARN-only.

## Risk coverage
- SR-01 → ADR-001 (create_new O_EXCL). SR-02/SR-03/SR-07 → ADR-003 + ADR-005 (both derive
  from the registry at runtime). SR-04 → ADR-004 (structural `http.enabled` gate +
  base_dir correction). SR-05/SR-09 → ADR-002 (writes only (b), reuses the single join).
  SR-06 → ADR-005 (WARN-only). SR-08 → flagged for tester (additive call sites, no
  signature change; the four `write_default_config_if_absent` tests + `Command::Version`
  match are the ripple gates).

## Open questions (for spec/dev/tester)
- OQ-A — AC-03 render granularity (legend block vs inline per-key tags); architect
  recommends legend block.
- OQ-B — `permissive`/`tls`/`http` are field-less/transport locks; render in legend, do
  not seed editable knobs; WARN still fires if a per-slug file sets them.
- OQ-C — AC-03 flip-test and the SR-08 ripple audit are the regression spine.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced vnc-040 ADR-004 (#5217, the A→B
  single-source classification), ADR-002 (#5206, byte-for-byte fallthrough + the documented
  seam-WARN residual), ADR-001 (#5209, resolver shape), vnc-038 ADR-007 (#5086, register
  write site), and patterns #5212/#5213 (resolve_slug_config gotchas). Applied directly.
- Stored: entries #5235–#5239 "ADR-001..ADR-005 vnc-041" via context_store (decision); edges
  #5236→#5235 Prerequisite, #5238→#5235 Prerequisite, #5239→#5206 Supports.
