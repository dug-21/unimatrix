# Release Preparation: Documentation, Configuration, and Distribution (nan-011)

GH Issue: TBD

---

## Problem Statement

Unimatrix is approaching a release milestone. The user-facing surface — README, vision
statement, default `config.toml`, skills, and reference protocols — has drifted
significantly from the current implementation across multiple shipping cycles.

Specific inaccuracies in the current public surface:

| Location | Problem |
|----------|---------|
| README | "Semantic Search with NLI Re-ranking" described as a core capability — NLI removed in crt-038 (task mismatch, zero MRR contribution) |
| README | "Contradiction Detection and NLI Edge Classification" section — NLI removed |
| README | Binary path `target/release/unimatrix-server` — binary renamed to `unimatrix` (nan-004, ADR-002) |
| README | PPR expander, behavioral signal delivery, phase-conditioned category affinity, and domain-agnostic observation pipeline (col-023) are entirely absent |
| README | Opening "what is Unimatrix" definition does not reflect current product positioning |
| PRODUCT-VISION.md | W1-5 still marked "IN PROGRESS" — col-023 shipped as PR #332 |
| PRODUCT-VISION.md | `HookType enum tied to Claude Code events` still marked "In progress" — resolved in col-023 |
| `config.toml` | Only `[retention]` section present (26 lines); 7 configurable sections are entirely undiscovered by operators |
| Skills | Unimatrix tool calls in skills use bare names (`context_store`, `context_search`, etc.) in some files — the required format is `mcp__unimatrix__context_*`; bare names fail in certain agent execution contexts |
| Distribution | Reference protocols (design, delivery, bugfix, routing) are not included in the npm package; operators have no access to the `context_cycle` workflow integration pattern |
| Distribution | `uni-retro` skill is not in the npm package; retro reports are a high-value showcase capability for new users |

---

## Goals

1. **Rewrite the "what is Unimatrix" definition** in README and PRODUCT-VISION.md to
   accurately describe Unimatrix as a workflow-aware, self-learning knowledge engine,
   with the static/dynamic knowledge layering mental model and the explicit "not an
   orchestration engine" positioning.

2. **Repair README accuracy**: remove all stale content (NLI re-ranking, NLI
   contradiction detection). Add accurate summaries of Wave 1A capabilities.
   Fix binary name references throughout.

3. **Ship a comprehensive default `config.toml`** covering all user-facing
   configuration sections with descriptive comments. Separate the operator-facing
   surface from the advanced tuning surface. The config file should be the primary
   reference for operators configuring a new deployment.

4. **Audit all 14 skills for correct MCP tool call format**: every Unimatrix tool
   call in every skill must use the full `mcp__unimatrix__context_*` prefix. Bare
   tool names cause "tool not found" errors in certain agent execution contexts.

5. **Update `uni-release` skill** to include the reference protocols and `uni-retro`
   skill in the npm package as part of the standard release process.

6. **Ship reference protocols**: create a `protocols/` directory at the repo root,
   validate all 4 protocols for accuracy, and include them in the distributed package.
   Provide a `protocols/README.md` documenting the `context_cycle` workflow integration
   pattern.

7. **Ship `uni-retro` skill in the npm package**: `uni-retro` produces feature
   retrospective reports and is a key showcase capability for new users. It belongs
   in the distributed package alongside the protocols.

8. **Update `uni-seed` skill** to reflect the current blank-DB installation path,
   schema v22, and current category allowlist. Ensure all tool calls use the full
   MCP prefix format.

---

## Non-Goals

- No code changes to the Unimatrix binary, MCP server, or any Rust crate.
- No schema migrations.
- No new capabilities or MCP tool implementations.
- No rewrite of skills that are format-correct and factually accurate — only skills
  with MCP format errors or material inaccuracies are changed.
- No changes to the protocols' workflow choreography — only accuracy corrections,
  stale-reference removal, and format fixes are in scope.
- The `uni-release` skill itself is internal tooling and is NOT included in the
  distributed npm package. Only the artifacts it packages (protocols, uni-retro) are
  distributed.
- No changes to `InferenceConfig` defaults or any compiled-in Rust defaults.
- The "Invisible Delivery" README bullet ("Agents do not need to ask for context")
  oversells the hook-driven injection capability. The standalone bullet is not changed
  in this delivery, but the vision statement section in the README receives a one-sentence
  qualifier (FR-1.2) scoping "before agents need to ask for it" to workflow-phase-conditioned
  delivery — resolving SR-06 without touching the bullet directly.
- Unimatrix knowledge base vision entries (#4163, #4164) are NOT updated in this
  delivery. They require `context_correct` in a uni-zero session after this feature
  merges. The implementer should note any detected drift in their agent report.

---

## Background

### The Static/Dynamic Knowledge Distinction

The core mental model this feature must communicate accurately:

**Static layer** (managed in the tool, changes infrequently): workflow definitions
(protocols), agent definitions, skill definitions. These are stable once established
for a project.

**Dynamic layer** (changes with every feature delivery): architecture decisions,
patterns, lessons learned, test harness conventions, integration contracts. Every new
feature potentially introduces a new ADR, a new pattern, a new lesson from a gate
failure. This layer cannot be static — it must grow, evolve, and be correctable.

Unimatrix was designed to manage the dynamic layer. That is the differentiation.
Static tooling belongs in `.claude/`. Dynamic knowledge belongs in Unimatrix.

### MCP Tool Call Format

Unimatrix registers its tools with the `mcp__unimatrix__` prefix. In agent execution
contexts where the MCP server name is part of tool resolution, skills that call bare
names fail with "tool not found" errors. The correct form for all Unimatrix tool calls
in skill files is the full prefix form:

```
mcp__unimatrix__context_search
mcp__unimatrix__context_store
mcp__unimatrix__context_get
mcp__unimatrix__context_lookup
mcp__unimatrix__context_correct
mcp__unimatrix__context_deprecate
mcp__unimatrix__context_status
mcp__unimatrix__context_briefing
mcp__unimatrix__context_enroll
mcp__unimatrix__context_quarantine
mcp__unimatrix__context_cycle
mcp__unimatrix__context_cycle_review
```

### Config Structure

`UnimatrixConfig` has 8 top-level TOML sections. The shipped `config.toml` documents
only `[retention]`. The full user-facing surface and intended exposure:

| TOML Section | User-Facing Fields | Advanced (commented-out block) |
|---|---|---|
| `[profile]` | `preset` (collaborative / authoritative / operational / empirical) | `preset = "custom"` with custom weights note |
| `[knowledge]` | `categories`, `boosted_categories`, `adaptive_categories` | `freshness_half_life_hours` |
| `[server]` | `instructions` | — |
| `[agents]` | `default_trust`, `session_capabilities` | — |
| `[retention]` | `activity_detail_retention_cycles`, `audit_log_retention_days` | `max_cycles_per_tick` |
| `[observation]` | Full `[[observation.domain_packs]]` example | — |
| `[confidence]` | — | Full 6-component custom weights block (only applies when `preset = "custom"`) |
| `[inference]` | `rayon_pool_size`, `phase_freq_lookback_days`, `min_phase_session_pairs` | NLI sub-block (opt-in, requires external model); all other fields omitted or in a clearly-marked "do not change" block |

The `[inference]` NLI fields (`nli_enabled`, `nli_model_name`, `nli_model_path`,
`nli_model_sha256`, `nli_top_k`, `nli_entailment_threshold`, `nli_contradiction_threshold`)
are opt-in and must be present but fully commented out with a note that they require
an external ONNX model file not bundled with Unimatrix.

PPR parameters, graph inference parameters, Informs detection thresholds, and fusion
weight fine-tuning fields in `[inference]` are internal tuning knobs. They may be
omitted entirely from the default config or present in a clearly-marked block stating
"Internal tuning — do not change unless directed by a support issue."

All default values shown in `config.toml` must exactly match the compiled defaults in
`crates/unimatrix-server/src/infra/config.rs`. The implementer must verify each value
by reading the `default_*` functions in that file.

### Wave 1A Capabilities Missing from README

The following shipped capabilities have no presence in the current README and must
be added:

- **Graph-Enhanced Retrieval** (crt-042/044/045, crt-050): Unimatrix combines semantic
  search, graph traversal, and SQL-backed ranking. HNSW seeds feed a Personalized
  PageRank (PPR) expansion step that walks the co-access graph to surface cross-category
  entries that pure vector search misses. Phase-conditioned category affinity stratifies
  results by workflow phase, so briefings at a design transition surface different
  knowledge than briefings mid-implementation. Co-access ranking promotes entries that
  agents have historically retrieved together. The three layers compose: semantic
  similarity locates candidates, graph expansion broadens the pool, phase/co-access
  ranking orders it by workflow context. (+0.0122 MRR confirmed on PPR expansion alone.)
- **Behavioral Signal Delivery** (Group 6, crt-046): cycle outcomes written as graph
  edges; goal-conditioned briefing from prior cycle patterns.
- **Domain-Agnostic Observation Pipeline** (W1-5, col-023): `source_domain` guard on
  all detection rules; domain pack registration via config; "claude-code" pack always
  active; any domain's event stream connects without code changes.

---

## Proposed Approach

### Deliverable 1 — Vision Statement and README

**Approved vision statement — use verbatim:**

> Unimatrix is a workflow-aware, self-learning knowledge engine built for agentic
> software delivery. It captures the knowledge that emerges from doing work —
> decisions, patterns, lessons, conventions — and makes it trustworthy, retrievable,
> and continuously improving. As agents move through delivery cycles, Unimatrix learns
> what matters at each phase and delivers the right knowledge dynamically, before
> agents need to ask for it. Knowledge retention becomes a first-class citizen of the
> delivery process, not a side effect.
>
> Unimatrix is not an orchestration engine. It does not coordinate agents, schedule
> work, or manage workflows. It is a knowledge engine that understands workflow context
> — your current phase, what your team has been doing, what comes next — and uses that
> understanding to surface relevant knowledge at exactly the right moment.
>
> The key mental model: workflow definitions, agent definitions, and skill definitions
> are static — they live in your tooling and change infrequently. Architecture
> decisions, patterns, and lessons-learned are dynamic — they evolve with every
> feature, every delivery, every failure. Unimatrix was designed to manage the dynamic
> layer. Every architectural pivot, every hard-won lesson, every reusable pattern is
> captured, attributed, and made available to every future agent that needs it.
>
> Built for agentic software delivery. Configurable for any workflow-centric domain.

This replaces the current opening paragraph in both README and PRODUCT-VISION.md.

**README sections to remove entirely:**
- "Semantic Search with NLI Re-ranking" sub-section — replace with a new
  "Graph-Enhanced Retrieval" section (see below)
- "Contradiction Detection and NLI Edge Classification" sub-section; replace with
  one-paragraph accurate description: cosine Supports detection (threshold ≥ 0.65),
  contradiction_density Lambda dimension using the periodic scan, manual contradiction
  management via `context_correct`

**README sections to add (one paragraph each):**
- **Graph-Enhanced Retrieval** — the unified story: semantic search (HNSW vector
  similarity) + graph expansion (PPR co-access traversal for cross-category
  surfacing) + phase/co-access ranking (workflow-phase-conditioned category
  affinity, co-access promotion). Position Unimatrix as a platform that chooses
  semantic, graph, or SQL access where each fits best. PPR may be named as the
  graph technique under this heading.
- Behavioral signal delivery and goal-conditioned briefing
- Domain-agnostic observation pipeline

**Binary name fix:** all references to `unimatrix-server` in README update to
`unimatrix`. Build output path: `target/release/unimatrix`.

**PRODUCT-VISION.md targeted fixes** (two lines — already partially applied; verify
and complete):
- Mark W1-5 as COMPLETE (`col-023`, PR #332, GH #331)
- Domain Coupling gap table: `HookType enum` row → Fixed

### Deliverable 2 — Default `config.toml`

Full rewrite of `config.toml` at the repo root. Document the sections in this order:
`[profile]`, `[knowledge]`, `[server]`, `[agents]`, `[retention]`, `[observation]`,
then an `## Advanced Configuration` block containing `[confidence]` and the
operator-relevant `[inference]` fields, followed by an optional internal block.

Requirements:
- Every user-facing field has a comment explaining its purpose, effect, accepted
  values or range, and default.
- The four `preset` values must be listed with one-line descriptions of each.
- The `[[observation.domain_packs]]` section must show a complete commented example
  with all four fields: `source_domain`, `event_types`, `categories`, `rule_file`.
  Include a note that the built-in "claude-code" pack is always active regardless of
  this section.
- The `[confidence]` custom weights block lists all six components (base, usage,
  fresh, help, corr, trust) with a note that they must sum to 0.92 and are only
  active when `preset = "custom"`.
- The NLI sub-block is commented out with: "Requires an external ONNX NLI
  cross-encoder model file. Not bundled. See documentation for model acquisition."
- All default values must match compiled defaults in `config.rs` exactly.
- The file must be valid TOML (all uncommented fields parse correctly).

### Deliverable 3 — Skills MCP Format Audit

Audit all 14 skills in `.claude/skills/`:

```
uni-git, uni-release, uni-review-pr, uni-init, uni-seed,
uni-store-lesson, uni-store-adr, uni-store-pattern, uni-store-procedure,
uni-knowledge-lookup, uni-knowledge-search, uni-query-patterns,
uni-zero, uni-retro
```

**Format audit (all 14):** grep each SKILL.md for bare Unimatrix tool names used as
function calls. Any bare name invocation must be updated to the full
`mcp__unimatrix__context_*` form. Prose references in descriptive text (e.g., "call
`context_search` to find entries") do not require the prefix in prose but all
code-block and invocation-format tool calls do.

**Accuracy audit (targeted — these 4 skills require content review beyond format):**

- `uni-release`: Update for binary rename. Add protocol packaging step and
  `uni-retro` packaging step (see Deliverable 4). Verify the release process steps
  are accurate against the current `uni-release` workflow. This skill is NOT
  shipped in the npm package.
- `uni-init`: Verify the CLAUDE.md block it appends lists all 14 current skills
  accurately by name. Verify any Unimatrix tool call examples use full prefix format.
- `uni-retro`: Remove any references to `HookType`, closed-enum event types, or
  predecessor col-023 concepts. Verify the retro invocation pattern works with the
  current domain-agnostic pipeline. This skill IS shipped in the npm package (see
  Deliverable 4).
- `uni-seed`: See Deliverable 5.

### Deliverable 4 — Protocol and uni-retro Packaging

**Create `protocols/` directory at repo root** containing:
- `uni-design-protocol.md` (source: `.claude/protocols/uni/uni-design-protocol.md`)
- `uni-delivery-protocol.md`
- `uni-bugfix-protocol.md`
- `uni-agent-routing.md`
- `README.md` — one page; see content requirements below

**`protocols/README.md` must cover:**
- What the protocols are and how they relate to `context_cycle`
- How `context_cycle(type: "start" | "phase" | "stop")` works and why it enables
  workflow-conditioned knowledge delivery
- A minimal illustrative example of a two-phase cycle (design → delivery) showing
  the `context_cycle` calls at start, phase transition, and stop
- A note that these are Claude Code + Unimatrix reference implementations; the
  `context_cycle` pattern generalizes to any workflow-centric domain

**Validate all 4 protocols for accuracy:**
- Remove any references to NLI, MicroLoRA, `unimatrix-server` (old binary name)
- Verify `context_cycle` calls match the current MCP tool signature
- Verify agent IDs and tool call formats are current
- No changes to choreography logic, phase structure, or gate definitions

**Update `uni-release` SKILL.md and `package.json`:**
- Add a step to the release process that copies `protocols/` into the npm package
- Add `uni-retro` to the npm package (copy `.claude/skills/uni-retro/SKILL.md` to
  a distributable location, e.g., `skills/uni-retro/SKILL.md` at repo root, and
  include in `package.json` files array)
- Add `"protocols"` and `"skills"` (or the specific skill path) to the `files` array
  in `package.json`
- Verify via `npm pack --dry-run` that both artifacts appear in the package manifest

**Note on uni-retro distribution rationale:** Retrospective reports are a high-value
demonstration of Unimatrix's self-learning capability. A new user who installs
Unimatrix, runs a delivery session, and then runs `/uni-retro` receives a structured
analysis of what happened — knowledge reuse metrics, patterns extracted, friction
points identified. This is a concrete showcase of value that is well-suited for
sharing. Shipping uni-retro in the package removes the friction of finding it.

### Deliverable 5 — uni-seed Update

`uni-seed` is the first-run population skill for new blank-database installations. A
fresh Unimatrix install starts with an empty database; `uni-seed` provides an initial
curated knowledge set.

Update requirements:
- All tool calls use `mcp__unimatrix__context_store` (full prefix)
- Seed entry categories match the current `INITIAL_CATEGORIES` list from
  `crates/unimatrix-server/src/infra/categories.rs` — implementer must read this
  file and verify each seeded entry's category is present in the allowlist
- Any seed entries referencing removed features (NLI, MicroLoRA) are removed or
  updated to reflect what actually shipped
- Skill description clearly states: "Run once per new project before the first
  delivery session. Do not re-run on an established installation — seed entries
  will duplicate existing knowledge."
- The skill accurately describes what it seeds and why each category of seed content
  is useful for a new installation

---

## Acceptance Criteria

- AC-01: README and PRODUCT-VISION.md opening section uses the approved vision
  statement verbatim.
- AC-02: README contains zero mentions of "NLI re-ranking", "NLI cross-encoder",
  "NLI contradiction", or any variant used to describe an active or shipped feature.
  (References in the context of the opt-in NLI config block or future roadmap are
  permitted.) The "Adaptive Embeddings (MicroLoRA)" section is retained — MicroLoRA
  is shipped and active.
- AC-03: README contains a "Graph-Enhanced Retrieval" section (or equivalent heading)
  that describes the semantic + graph + SQL access model, covering PPR expansion,
  phase-conditioned category affinity, and co-access ranking as a unified capability.
  README also contains at least one paragraph each for behavioral signal delivery and
  domain-agnostic observation pipeline.
- AC-04: All binary name references in README use `unimatrix`. Build path shown as
  `target/release/unimatrix`.
- AC-05: PRODUCT-VISION.md correctly marks W1-5 as COMPLETE and the HookType domain
  coupling gap as Fixed.
- AC-06: `config.toml` covers all 7 sections beyond `[retention]`:
  `[profile]`, `[knowledge]`, `[server]`, `[agents]`, `[observation]`, and the
  advanced block with `[confidence]` and `[inference]`. Every user-facing field has
  a comment.
- AC-07: `config.toml` contains a complete commented `[[observation.domain_packs]]`
  example showing `source_domain`, `event_types`, `categories`, and `rule_file` with
  explanatory comments on each field.
- AC-08: `config.toml` is valid TOML. Every uncommented field parses without error.
  All shown default values match compiled defaults in `config.rs`.
- AC-09: The NLI sub-block in `[inference]` is present but fully commented out with
  a note that it requires an external model file not bundled with Unimatrix.
- AC-10: `grep` for bare Unimatrix tool name invocations across all skill files
  returns zero matches. Specifically, the following patterns must not appear as
  function calls (without `mcp__unimatrix__` prefix) in any `.claude/skills/**/*.md`
  file: `context_search(`, `context_store(`, `context_get(`, `context_lookup(`,
  `context_correct(`, `context_deprecate(`, `context_status(`, `context_briefing(`,
  `context_enroll(`, `context_quarantine(`, `context_cycle(`, `context_cycle_review(`.
- AC-11: `uni-init` SKILL.md lists all 14 current skills by name, accurately and
  completely.
- AC-12: `uni-retro` SKILL.md contains no references to `HookType`, closed-enum event
  type matching, or any col-023 predecessor concept.
- AC-13: `uni-release` SKILL.md includes steps to (a) copy `protocols/` into the npm
  package and (b) include `uni-retro` in the npm package. `package.json` `files`
  array includes both artifacts. `npm pack --dry-run` output confirms both appear.
- AC-14: `protocols/` directory exists at repo root containing all 4 protocol files
  and a `README.md`. The README includes a `context_cycle` usage example.
- AC-15: All 4 protocols contain zero references to removed features (NLI, MicroLoRA,
  `unimatrix-server`). `context_cycle` call signatures in protocols match the current
  MCP tool.
- AC-16: `uni-seed` SKILL.md uses `mcp__unimatrix__context_store` for all tool calls,
  describes the blank-installation use case, and warns against re-running on an
  established installation.
- AC-17: All seed entries in `uni-seed` use categories present in the current
  `INITIAL_CATEGORIES` list in `crates/unimatrix-server/src/infra/categories.rs`.

---

## Constraints

- **No Rust code changes.** Documentation, configuration, and skill files only.
  If an acceptance criterion requires a code change, file a separate issue.
- **`config.toml` default values must match compiled defaults.** Implementer must
  read `default_*` functions in `config.rs` directly. Any discrepancy between the
  config file and compiled default is a bug.
- **`config.toml` must be valid TOML.** All fields shown (even commented-out examples)
  must be syntactically valid TOML when uncommented. Types matter: strings use quotes,
  integers are bare, floats use decimal notation, arrays use `[...]`, tables-of-tables
  use `[[...]]`.
- **Protocols: choreography unchanged.** The protocols define wave structure, agent
  spawning order, and gate logic. These are not in scope for change. Only factual
  inaccuracies and removed-feature references are corrected.
- **`uni-release` is not distributed.** It must not appear in the npm `files` array.
- **Vision entries in Unimatrix (#4163, #4164) are out of scope for this delivery.**
  Update via `context_correct` in a uni-zero session after merge. Implementer should
  note any material drift between these entries and the updated PRODUCT-VISION.md in
  their agent report.
- **`protocols/` files are copies, not symlinks.** Symlinks do not survive `npm pack`.
  The protocol files in `protocols/` are independent copies of the files in
  `.claude/protocols/uni/`. Both copies should be updated if corrections are needed.

---

## Open Questions

None — all decisions made in design session with project owner.

| Decision | Resolution |
|----------|-----------|
| Vision statement | Approved verbatim (see Proposed Approach) |
| Protocol shipping mechanism | Copy to `protocols/` dir; include in npm `package.json` files array via `uni-release` update |
| `uni-release` distribution | NOT shipped in npm package; internal tooling only |
| `uni-retro` distribution | YES shipped in npm package; key showcase capability |
| Skills audit scope | All 14; MCP format is primary check; uni-retro, uni-init, uni-release, uni-seed also require accuracy review |
| `config.toml` `[observation]` domain pack section | Included with full example |
| `config.toml` NLI fields | Present but fully commented out in advanced block |
| `config.toml` internal inference fields | Omitted or in clearly-marked "do not change" block |

---

## Tracking

GH Issue: https://github.com/dug-21/unimatrix/issues/546
Feature directory: `product/features/nan-011/`
