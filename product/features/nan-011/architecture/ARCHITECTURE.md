# Architecture: nan-011 — Release Preparation

## System Overview

nan-011 is a documentation and distribution feature. It contains no Rust code changes. The feature touches five surfaces: README.md, PRODUCT-VISION.md, config.toml, the 14 skill files in `.claude/skills/`, and the npm packaging layer (`packages/unimatrix/`). It also creates two new directories at repo root: `protocols/` (distributed) and `skills/` (partially distributed).

The feature's purpose is to synchronize the user-facing surface with the current implementation state after several shipping cycles that added Wave 1A capabilities, removed NLI from the active pipeline, renamed the binary from `unimatrix-server` to `unimatrix`, and externalized configuration to `config.toml`.

## Component Breakdown

### Component 1 — README + PRODUCT-VISION.md Repair

**Responsibility**: Bring external documentation into alignment with the current implementation.

Files modified:
- `README.md` (at repo root)
- `product/PRODUCT-VISION.md`

Changes:
- Replace opening section with the approved vision statement (verbatim, from SCOPE.md)
- Remove "Semantic Search with NLI Re-ranking" section
- Remove "Contradiction Detection and NLI Edge Classification" section
- Add "Graph-Enhanced Retrieval" section (PPR + phase affinity + co-access ranking)
- Add "Behavioral Signal Delivery" paragraph (cycle outcomes as graph edges)
- Add "Domain-Agnostic Observation Pipeline" paragraph (source_domain guard, domain packs)
- Fix all `unimatrix-server` → `unimatrix` binary name references
- Fix build path `target/release/unimatrix-server` → `target/release/unimatrix`
- PRODUCT-VISION.md: mark W1-5 COMPLETE (col-023, PR #332)
- PRODUCT-VISION.md: mark HookType domain coupling gap as Fixed

Section order is canonical per ADR-001.

### Component 2 — config.toml Full Rewrite

**Responsibility**: Document all 8 configurable sections with verified defaults.

File modified: `config.toml` (at repo root)

The current file documents only `[retention]` (26 lines). The rewrite adds 7 additional sections. All default values are verified against `crates/unimatrix-server/src/infra/config.rs` `default_*` functions and `Default` impls (see ADR-002).

Section order: `[profile]` → `[knowledge]` → `[server]` → `[agents]` → `[retention]` → `[observation]` → `[confidence]` (advanced block) → `[inference]` (operator fields, then NLI opt-in block, then internal tuning block).

### Component 3 — Skills MCP Format Audit

**Responsibility**: Ensure all 14 skill files use the `mcp__unimatrix__context_*` prefix for all tool invocations.

Files reviewed: all 14 SKILL.md files in `.claude/skills/`

Files requiring changes: 4 (uni-seed format fix, uni-retro format fix, uni-init content update, uni-release content update). See ADR-004 for the exact audit results.

### Component 4 — protocols/ Directory

**Responsibility**: Create the distributable reference protocols directory.

Files created:
- `protocols/uni-design-protocol.md` (copy from `.claude/protocols/uni/`)
- `protocols/uni-delivery-protocol.md`
- `protocols/uni-bugfix-protocol.md`
- `protocols/uni-agent-routing.md`
- `protocols/README.md` (new, covers context_cycle integration pattern)

All 4 protocol files validated for accuracy: NLI references removed, `unimatrix-server` replaced with `unimatrix`, `context_cycle` call signatures verified against current MCP tool. Choreography logic is not changed.

### Component 5 — npm Package Update

**Responsibility**: Include protocols/ and uni-retro in the distributed package.

Files modified:
- `packages/unimatrix/package.json` — add `"protocols/"` to `files` array
- `skills/uni-retro/SKILL.md` (new file at repo root, copy of `.claude/skills/uni-retro/SKILL.md`)
- `.claude/skills/uni-release/SKILL.md` — add packaging steps

The `files` array already contains `"skills/"`. Adding `"protocols/"` is the only `package.json` change. The `skills/uni-retro/SKILL.md` file is created at repo root to be picked up by the existing `"skills/"` entry.

## Component Interactions

```
nan-011 has no runtime component interactions.
All components are static artifacts — files written once, read by humans or tools.

Content flow:
  .claude/protocols/uni/ ──copy──► protocols/           ──include──► npm package
  .claude/skills/uni-retro/ ──copy──► skills/uni-retro/ ──include──► npm package
  config.rs default_* fns ──verify──► config.toml
  SCOPE.md vision statement ──verbatim──► README.md, PRODUCT-VISION.md
```

The `uni-release` skill orchestrates the copy steps at release time, not at delivery time. During delivery, the implementer creates the initial copies manually.

## Technology Decisions

See ADR-001 (README section order), ADR-002 (config defaults), ADR-003 (distribution packaging), ADR-004 (skills audit).

## Integration Points

### config.rs → config.toml (SR-01 risk)

The config.toml must reflect compiled defaults from `crates/unimatrix-server/src/infra/config.rs`. Two default sites exist:
- `#[serde(default = "fn")]` annotations — govern TOML omission behavior
- `Default` impls — govern programmatic construction

For `boosted_categories` and `adaptive_categories`, these two sites disagree: the serde default returns `["lesson-learned"]` while `Default::default()` returns `[]`. The config.toml must show the serde default value because that governs what happens when a user omits the field.

### protocols/ ↔ .claude/protocols/uni/ (SR-03 risk)

Dual-copy maintenance. Source of truth: `.claude/protocols/uni/`. Copy direction: source → `protocols/`. The `uni-release` skill enforces a diff-verification step.

### package.json files array → npm dist

Current `files` array: `["bin/", "lib/", "skills/", "postinstall.js"]`

After nan-011: `["bin/", "lib/", "skills/", "postinstall.js", "protocols/"]`

The `skills/` entry already covers `skills/uni-retro/SKILL.md`. No second entry for skills is needed.

## Integration Surface

| Integration Point | Type / Details | Source |
|---|---|---|
| `Preset` enum values | `lowercase` serde: `collaborative`, `authoritative`, `operational`, `empirical`, `custom` | `config.rs:1583` |
| `INITIAL_CATEGORIES` | `["lesson-learned","decision","convention","pattern","procedure"]` (5 items) | `categories/mod.rs:15` |
| `AgentsConfig` defaults | `default_trust = "permissive"`, `session_capabilities = ["Read","Write","Search"]` | `config.rs:202` |
| `RetentionConfig` defaults | `activity_detail_retention_cycles = 50`, `audit_log_retention_days = 180`, `max_cycles_per_tick = 10` | `config.rs:1508-1515` |
| `InferenceConfig.rayon_pool_size` | Dynamic: `(num_cpus::get() / 2).max(4).min(8)` — cannot be shown as a single integer | `config.rs:702` |
| `nli_enabled` default | `false` | `config.rs:766` |
| `ppr_expander_enabled` default | `false` | `config.rs:874` |
| `boosted_categories` serde default | `["lesson-learned"]` (serde fn); `[]` (Rust Default) — show serde value in config.toml | `config.rs:128` |
| `DomainPackConfig` required fields | `source_domain`, `event_types`, `categories` — no defaults; omission = parse error | `config.rs:100` |
| `ConfidenceWeights` sum constraint | `base + usage + fresh + help + corr + trust = 0.92 ± 1e-9` | `config.rs:227` |
| npm `files` array location | `packages/unimatrix/package.json` | filesystem |
| Protocol source files | `.claude/protocols/uni/{uni-design,uni-delivery,uni-bugfix,uni-agent-routing}-protocol.md` | filesystem |
| uni-retro source | `.claude/skills/uni-retro/SKILL.md` | filesystem |
| uni-retro distribution target | `skills/uni-retro/SKILL.md` (repo root) | filesystem |

## Open Questions

1. **`skills/` directory at repo root**: The current `packages/unimatrix/package.json` lists `"skills/"` in the `files` array but it is unclear whether a `skills/` directory exists at repo root today (as opposed to `.claude/skills/`). The implementer must check before creating `skills/uni-retro/SKILL.md`. If no `skills/` directory exists, the directory must be created and the `files` array entry is already correct.

2. **`uni-init` skill list**: The CLAUDE.md block appended by `uni-init` currently lists only 2 skills (`/uni-init`, `/uni-seed`). Updating it to list all 14 skills could produce a very long table. The spec writer should decide: update the table to all 14 with one-line descriptions, or limit to the most commonly invoked skills (uni-store-adr, uni-store-lesson, uni-store-pattern, uni-store-procedure, uni-knowledge-search, uni-knowledge-lookup, uni-query-patterns, uni-retro, uni-zero). An incomplete list is an accuracy defect.

3. **`protocols/README.md` content depth**: The SCOPE.md requires a context_cycle usage example. The spec writer should confirm whether this README is a single page (under 150 lines) with minimal prose, or a more complete integration guide. The architecture constrains it to cover: what protocols are, how context_cycle works, a minimal two-phase example, and a note on generalizability.

4. **SR-06 — Vision statement "before agents need to ask"**: The approved vision statement includes language that describes proactive delivery. The spec should require a companion callout in the README that clarifies this is workflow-phase-conditioned delivery, not autonomous hook injection (per SR-06 recommendation). This is a spec-level decision, not an architectural one, but the implementer should be made aware of it.
