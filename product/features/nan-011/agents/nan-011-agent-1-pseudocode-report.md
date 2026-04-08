# Agent Report: nan-011-agent-1-pseudocode

## Task

Produce per-component pseudocode files for nan-011 (Release Preparation: Documentation,
Configuration, and Distribution). All five components are file editing tasks — no Rust code.

## Files Produced

- `product/features/nan-011/pseudocode/OVERVIEW.md`
- `product/features/nan-011/pseudocode/readme-vision.md`
- `product/features/nan-011/pseudocode/config-toml.md`
- `product/features/nan-011/pseudocode/skills-audit.md`
- `product/features/nan-011/pseudocode/protocols-dir.md`
- `product/features/nan-011/pseudocode/npm-package.md`

## Components Covered

1. README + PRODUCT-VISION.md Repair (readme-vision.md)
2. config.toml Full Rewrite (config-toml.md)
3. Skills MCP Format Audit — 4 files changed, 10 no-op (skills-audit.md)
4. protocols/ Directory — 4 copies + README.md created (protocols-dir.md)
5. npm Package Update — package.json + skills/uni-retro/SKILL.md (npm-package.md)

## Dependency Ordering for Wave Planning

Wave 1 (parallel, no deps): Components 1 and 2
Wave 2 (source fixes): Component 3 — must complete before Waves 3/4
Wave 3 (copies, parallel after Wave 2): Components 4 and 5

The critical ordering constraint is source-before-copy: `.claude/skills/uni-retro/SKILL.md`
must be fixed (Component 3) before it is copied to `skills/uni-retro/SKILL.md` (Component 5).
Same for `.claude/protocols/uni/` → `protocols/` (Component 4).

## Open Questions

None — all architectural decisions are resolved (ADR-001 through ADR-004). ARCHITECTURE.md
open questions 1, 2, 3, 4 are all addressed in ADR decisions and spec:
- Q1 (skills/ at repo root): check for existence; create if absent — encoded in npm-package.md
- Q2 (uni-init skill list): all 14 skills listed — encoded in skills-audit.md Operation I1-2
- Q3 (protocols/README.md depth): minimal (< 150 lines) with three-call example — encoded in protocols-dir.md
- Q4 (SR-06 qualifier): FR-1.2 qualifier sentence is required — encoded in readme-vision.md

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found nan-005 ADR (#1254, capability-first README),
  dual-site config default pattern (#3817, directly informs R-01/R-02 severity), InferenceConfig
  hidden sites lesson (#4148, confirms rayon_pool_size formula requirement). No results were
  specifically about documentation-only feature pseudocode patterns.
- Queried: `mcp__unimatrix__context_search` for "documentation update skill file editing patterns" —
  found #1118 (Versioned Sentinel Markers for Idempotent File Mutation), #552 (Skill File as Single
  Source of Truth). Pattern #552 directly validates the source-before-copy constraint encoded in
  all component files.
- Queried: `mcp__unimatrix__context_search` for "nan-011 architectural decisions" — found #4265
  (ADR-001), #4266 (ADR-002), #4267 (ADR-003) confirming ADRs are stored in Unimatrix.
- Deviations from established patterns: none. The source-before-copy ordering follows #552.
  The dual-site default problem (#3817) is explicitly documented in config-toml.md.

## Material Drift Note (Vision Entries #4163/#4164)

Per IMPLEMENTATION-BRIEF.md constraint 10, Unimatrix vision entries #4163 and #4164 are
out of scope for nan-011 delivery. Implementer should note any drift between these entries
and the updated PRODUCT-VISION.md in their delivery report for the post-merge uni-zero session.
