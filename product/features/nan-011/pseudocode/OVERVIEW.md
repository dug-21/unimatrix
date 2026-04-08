# nan-011 Pseudocode Overview
# Release Preparation: Documentation, Configuration, and Distribution

## Components and Files

| Component | Pseudocode File | Modified/Created Files |
|-----------|----------------|------------------------|
| 1. README + PRODUCT-VISION.md Repair | readme-vision.md | `README.md`, `product/PRODUCT-VISION.md` |
| 2. config.toml Full Rewrite | config-toml.md | `config.toml` |
| 3. Skills MCP Format Audit | skills-audit.md | `.claude/skills/uni-seed/SKILL.md`, `.claude/skills/uni-retro/SKILL.md`, `.claude/skills/uni-init/SKILL.md`, `.claude/skills/uni-release/SKILL.md` |
| 4. protocols/ Directory | protocols-dir.md | `.claude/protocols/uni/` (4 files validated), `protocols/` (4 copies + README.md created) |
| 5. npm Package Update | npm-package.md | `packages/unimatrix/package.json`, `skills/uni-retro/SKILL.md` (new at repo root) |

---

## Dependency Ordering (Wave Planning)

Wave 1 — Independent reads and edits with no downstream dependencies:
- Component 1: README + PRODUCT-VISION.md Repair
- Component 2: config.toml Full Rewrite

Wave 2 — Source files must be corrected before copies are made:
- Component 3 (Skills MCP Format Audit) MUST complete for `uni-retro` BEFORE Component 4 and Component 5
  - `.claude/skills/uni-retro/SKILL.md` is the source for `skills/uni-retro/SKILL.md` (npm dist)
  - `.claude/protocols/uni/` files are the source for `protocols/` (npm dist)

Wave 3 — Copies and packaging (depends on Wave 2):
- Component 4 (protocols/ Directory): copy from corrected `.claude/protocols/uni/` sources
- Component 5 (npm Package Update): create `skills/uni-retro/SKILL.md` at repo root from corrected source; update `package.json`

Wave 4 — Verification (runs after all waves):
- TOML parse check
- Two-pass MCP format grep
- Dual-copy diff verification (protocols)
- npm pack --dry-run

---

## Critical Ordering Constraint

Source-before-copy is NON-NEGOTIABLE:

```
1. Fix .claude/skills/uni-retro/SKILL.md  (source)
2. Copy to skills/uni-retro/SKILL.md       (dist)

1. Fix .claude/protocols/uni/*.md          (source)
2. Copy to protocols/*.md                  (dist)
```

Never reverse these steps. Copy-then-fix produces a broken npm distribution.

---

## Shared Types and Values

### Approved Vision Statement (verbatim — do not paraphrase)

```
Unimatrix is a workflow-aware, self-learning knowledge engine built for agentic
software delivery. It captures the knowledge that emerges from doing work —
decisions, patterns, lessons, conventions — and makes it trustworthy, retrievable,
and continuously improving. As agents move through delivery cycles, Unimatrix learns
what matters at each phase and delivers the right knowledge dynamically, before
agents need to ask for it. Knowledge retention becomes a first-class citizen of the
delivery process, not a side effect.

Unimatrix is not an orchestration engine. It does not coordinate agents, schedule
work, or manage workflows. It is a knowledge engine that understands workflow context
— your current phase, what your team has been doing, what comes next — and uses that
understanding to surface relevant knowledge at exactly the right moment.

The key mental model: workflow definitions, agent definitions, and skill definitions
are static — they live in your tooling and change infrequently. Architecture
decisions, patterns, and lessons-learned are dynamic — they evolve with every
feature, every delivery, every failure. Unimatrix was designed to manage the dynamic
layer. Every architectural pivot, every hard-won lesson, every reusable pattern is
captured, attributed, and made available to every future agent that needs it.

Built for agentic software delivery. Configurable for any workflow-centric domain.
```

### FR-1.2 Qualifier Sentence (verbatim)

```
This workflow-phase-conditioned delivery means knowledge is surfaced at phase
transitions based on what the engine has learned about each phase — it is not
unconditional injection into every prompt.
```

### Canonical README Section Order (ADR-001)

1. Vision statement + FR-1.2 qualifier
2. How It Works
3. Capabilities:
   a. Knowledge Lifecycle
   b. Graph-Enhanced Retrieval (new)
   c. Adaptive Embeddings / MicroLoRA (retained)
   d. Behavioral Signal Delivery (new)
   e. Contradiction Detection (updated — no NLI claim)
   f. Domain-Agnostic Observation Pipeline (new)
4. Configuration
5. Installation
6. Quick Start / Usage
7. MCP Tool Reference

### INITIAL_CATEGORIES (from categories/mod.rs — authority at delivery time)

```
["lesson-learned", "decision", "convention", "pattern", "procedure"]
```

### Canonical 14-Skill List (for uni-init CLAUDE.md block)

```
uni-git, uni-release, uni-review-pr, uni-init, uni-seed,
uni-store-lesson, uni-store-adr, uni-store-pattern, uni-store-procedure,
uni-knowledge-lookup, uni-knowledge-search, uni-query-patterns,
uni-zero, uni-retro
```

### config.toml Verified Defaults (from ADR-002)

| Field | Serde Default | Notes |
|-------|--------------|-------|
| `preset` | `"collaborative"` | Preset enum lowercase |
| `categories` | `["lesson-learned","decision","convention","pattern","procedure"]` | |
| `boosted_categories` | `["lesson-learned"]` | SERDE DEFAULT — Rust Default is `[]` |
| `adaptive_categories` | `["lesson-learned"]` | SERDE DEFAULT — Rust Default is `[]` |
| `freshness_half_life_hours` | absent (None) | Option<f64> |
| `instructions` | absent (None) | Option<String> |
| `default_trust` | `"permissive"` | |
| `session_capabilities` | `["Read","Write","Search"]` | Capital R/W/S |
| `activity_detail_retention_cycles` | `50` | u32 |
| `audit_log_retention_days` | `180` | u32 |
| `max_cycles_per_tick` | `10` | u32 |
| `rayon_pool_size` | `(num_cpus / 2).max(4).min(8)` | Dynamic — never show bare integer |
| `phase_freq_lookback_days` | `30` | u32 |
| `min_phase_session_pairs` | `5` | u32 |
| `nli_enabled` | `false` | Fully commented out |
| `ppr_expander_enabled` | `false` | Internal tuning block |

### package.json files Array (after nan-011)

```json
["bin/", "lib/", "skills/", "postinstall.js", "protocols/"]
```

### Correct MCP Tool Invocation Prefix

All skill files must use: `mcp__unimatrix__context_*` (12 tools total)

Prohibited bare forms (invocation context only — prose exempt):
`context_search(`, `context_store(`, `context_get(`, `context_lookup(`,
`context_correct(`, `context_deprecate(`, `context_status(`,
`context_briefing(`, `context_enroll(`, `context_quarantine(`,
`context_cycle(`, `context_cycle_review(`

---

## Shared Verification Commands

Run these after all edits, before opening PR:

```bash
# TOML validity
python3 -c "import tomllib; tomllib.load(open('config.toml','rb'))"

# MCP format — Pass 1 (backtick-wrapped bare invocations)
grep -rn '`context_[a-z_]*(' .claude/skills/*/SKILL.md

# MCP format — Pass 2 (any bare invocation without prefix)
grep -rn 'context_[a-z_]*(' .claude/skills/*/SKILL.md | grep -v 'mcp__unimatrix__'

# MCP format — same two passes on the npm dist copy
grep -n '`context_[a-z_]*(' skills/uni-retro/SKILL.md
grep -n 'context_[a-z_]*(' skills/uni-retro/SKILL.md | grep -v 'mcp__unimatrix__'

# Dual-copy diff (protocols)
diff .claude/protocols/uni/uni-design-protocol.md protocols/uni-design-protocol.md
diff .claude/protocols/uni/uni-delivery-protocol.md protocols/uni-delivery-protocol.md
diff .claude/protocols/uni/uni-bugfix-protocol.md protocols/uni-bugfix-protocol.md
diff .claude/protocols/uni/uni-agent-routing.md protocols/uni-agent-routing.md

# Stale reference checks
grep "unimatrix-server" README.md
grep -i "nli re-rank\|nli cross-encoder\|nli contradiction\|nli re-ranker\|nli sort" README.md
grep -rn "NLI\|MicroLoRA\|unimatrix-server\|HookType" protocols/
grep -rn "HookType\|closed.enum\|UserPromptSubmit\|SubagentStart\|PreCompact\|PreToolUse\|PostToolUse\|Stop hook" .claude/skills/uni-retro/SKILL.md

# npm pack
cd packages/unimatrix && npm pack --dry-run
# Confirm output includes: protocols/README.md, skills/uni-retro/SKILL.md
# Confirm output DOES NOT include: uni-release/SKILL.md
```
