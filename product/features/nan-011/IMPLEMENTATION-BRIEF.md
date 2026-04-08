# nan-011 Implementation Brief
# Release Preparation: Documentation, Configuration, and Distribution

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-011/SCOPE.md |
| Architecture | product/features/nan-011/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-011/specification/SPECIFICATION.md |
| Risk Strategy | product/features/nan-011/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-011/ALIGNMENT-REPORT.md |

---

## Goal

Synchronize the Unimatrix user-facing surface — README, PRODUCT-VISION.md, default
`config.toml`, 14 skill files, and reference protocols — with the current implementation
after multiple shipping cycles that added Wave 1A capabilities, removed NLI from the
active pipeline, renamed the binary, and externalized configuration. No Rust code
changes. The output is a set of static artifact edits and two new directory trees
(`protocols/` and `skills/uni-retro/`) that are included in the npm distribution.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| README + PRODUCT-VISION.md Repair | pseudocode/readme-vision.md | test-plan/readme-vision.md |
| config.toml Full Rewrite | pseudocode/config-toml.md | test-plan/config-toml.md |
| Skills MCP Format Audit | pseudocode/skills-audit.md | test-plan/skills-audit.md |
| protocols/ Directory | pseudocode/protocols-dir.md | test-plan/protocols-dir.md |
| npm Package Update | pseudocode/npm-package.md | test-plan/npm-package.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| README section order after nan-011 | Canonical 7-section order: Vision → How It Works → Capabilities (6 sub-sections) → Configuration → Installation → Quick Start → MCP Tool Reference. NLI sections removed; Graph-Enhanced Retrieval, Behavioral Signal Delivery, Domain-Agnostic Observation Pipeline added. | SCOPE.md §Deliverable 1, ADR-001 | product/features/nan-011/architecture/ADR-001-readme-section-order.md |
| config.toml default value authority | `config.rs` `default_*` functions govern TOML omission behavior; `Default` impl governs programmatic construction. For `boosted_categories` and `adaptive_categories`, these two sites disagree — config.toml must show the serde default (`["lesson-learned"]`), not the Rust Default (`[]`). Implementer must read every `default_*` fn directly. | SR-01, ADR-002 | product/features/nan-011/architecture/ADR-002-config-toml-defaults.md |
| Distribution packaging — protocols/ and uni-retro | `protocols/` created at repo root as independent copies (no symlinks). `skills/uni-retro/SKILL.md` created at repo root. `package.json` gains `"protocols/"` in its `files` array; `"skills/"` already present covers uni-retro. `uni-release` SKILL.md gets Steps 7a/7b for copy + diff-verification. `uni-release` itself is NOT distributed. | SR-03, ADR-003 | product/features/nan-011/architecture/ADR-003-distribution-packaging.md |
| Skills MCP format audit pattern | Two-pass grep: (1) backtick-wrapped bare names, (2) `context_[a-z_]*(` without `mcp__unimatrix__` prefix. Prose references exempt. Confirmed bare invocations: `uni-seed` line ~49, `uni-retro` lines ~146 and ~161. 10 of 14 skills require no changes. | SR-04, ADR-004 | product/features/nan-011/architecture/ADR-004-skills-mcp-format-audit.md |
| FR-1.2 qualifier sentence | Accepted by project owner (WARN resolved). A one-sentence qualifier immediately follows the vision statement in README.md to clarify that "before agents need to ask for it" describes workflow-phase-conditioned delivery, not unconditional hook injection. SCOPE.md updated to reflect. | SR-06, ALIGNMENT-REPORT.md | — |

---

## Files to Create / Modify

### Modified Files

| File | Change Summary |
|------|---------------|
| `README.md` | Replace opening with approved vision statement + FR-1.2 qualifier; remove NLI sections; add Graph-Enhanced Retrieval, Behavioral Signal Delivery, Domain-Agnostic Observation Pipeline sections; fix all `unimatrix-server` → `unimatrix` binary references |
| `product/PRODUCT-VISION.md` | Replace opening Vision paragraph with approved statement; mark W1-5 COMPLETE (col-023, PR #332, GH #331); mark HookType domain coupling gap Fixed |
| `config.toml` | Full rewrite: 8 sections (`[profile]`, `[knowledge]`, `[server]`, `[agents]`, `[retention]`, `[observation]`, advanced `[confidence]`, advanced `[inference]`); all defaults verified against config.rs |
| `.claude/skills/uni-seed/SKILL.md` | Fix bare `context_status()` → `mcp__unimatrix__context_status({})`; add idempotency warning before first tool call; verify categories against INITIAL_CATEGORIES; describe blank-install use case |
| `.claude/skills/uni-retro/SKILL.md` | Fix bare `context_search(` and `context_store(` in spawn-prompt strings (~lines 146, 161); verify no HookType/col-023 predecessor references |
| `.claude/skills/uni-init/SKILL.md` | Update CLAUDE.md append block to list all 14 current skills; fix `unimatrix-server` → `unimatrix` in Prerequisites section |
| `.claude/skills/uni-release/SKILL.md` | Add Step 7a (copy protocols/ + diff verify) and Step 7b (copy uni-retro); update Step 7 git add; update Step 10 summary |
| `.claude/protocols/uni/uni-design-protocol.md` | Remove any NLI/MicroLoRA/`unimatrix-server`/HookType references; verify context_cycle call signatures; no choreography changes |
| `.claude/protocols/uni/uni-delivery-protocol.md` | Same validation and correction scope as uni-design-protocol.md |
| `.claude/protocols/uni/uni-bugfix-protocol.md` | Same validation and correction scope |
| `.claude/protocols/uni/uni-agent-routing.md` | Same validation and correction scope |
| `packages/unimatrix/package.json` | Add `"protocols/"` to `files` array |

### Created Files

| File | Purpose |
|------|---------|
| `protocols/uni-design-protocol.md` | Distributable copy of `.claude/protocols/uni/uni-design-protocol.md` (corrected) |
| `protocols/uni-delivery-protocol.md` | Distributable copy of uni-delivery-protocol.md |
| `protocols/uni-bugfix-protocol.md` | Distributable copy of uni-bugfix-protocol.md |
| `protocols/uni-agent-routing.md` | Distributable copy of uni-agent-routing.md |
| `protocols/README.md` | context_cycle integration guide with two-phase example |
| `skills/uni-retro/SKILL.md` | Distributable copy of corrected `.claude/skills/uni-retro/SKILL.md` |

---

## Data Structures

### UnimatrixConfig TOML Sections (config.rs)

```
[profile]
  preset: Preset enum — collaborative | authoritative | operational | empirical | custom
  Default: "collaborative" (#[default] on Preset::Collaborative)

[knowledge]
  categories: Vec<String> — ["lesson-learned","decision","convention","pattern","procedure"]
  boosted_categories: Vec<String> — ["lesson-learned"]   ← serde default_fn (NOT Rust Default=[])
  adaptive_categories: Vec<String> — ["lesson-learned"]  ← serde default_fn (NOT Rust Default=[])
  freshness_half_life_hours: Option<f64> — absent = None

[server]
  instructions: Option<String> — absent = None (uses compiled SERVER_INSTRUCTIONS)

[agents]
  default_trust: String — "permissive"
  session_capabilities: Vec<String> — ["Read","Write","Search"]  ← case-sensitive capitals

[retention]
  activity_detail_retention_cycles: u32 — 50
  audit_log_retention_days: u32 — 180
  max_cycles_per_tick: u32 — 10

[[observation.domain_packs]]
  source_domain: String — REQUIRED (no default, parse error if absent)
  event_types: Vec<String> — REQUIRED
  categories: Vec<String> — REQUIRED
  rule_file: Option<PathBuf> — absent = None

[confidence]  (only active when preset = "custom")
  weights.base + weights.usage + weights.fresh + weights.help + weights.corr + weights.trust
  Sum MUST equal 0.92 ± 1e-9. No Default impl — all six required when preset = "custom".

[inference]  — operator-facing
  rayon_pool_size: usize — dynamic: (num_cpus / 2).max(4).min(8) — NOT a fixed integer
  phase_freq_lookback_days: u32 — 30
  min_phase_session_pairs: u32 — 5

[inference]  — NLI opt-in block (fully commented out)
  nli_enabled: bool — false
  nli_model_name / nli_model_path / nli_model_sha256 / nli_top_k / nli_entailment_threshold
  /nli_contradiction_threshold  — all commented; require external ONNX model not bundled

[inference]  — internal tuning (omit or "do not change" block)
  ppr_alpha=0.85, ppr_iterations=20, ppr_inclusion_threshold=0.05, ppr_blend_weight=0.15,
  ppr_max_expand=50, ppr_expander_enabled=false, w_sim=0.50, w_conf=0.35,
  w_phase_histogram=0.02, w_phase_explicit=0.05, supports_cosine_threshold=0.65
```

### INITIAL_CATEGORIES (categories/mod.rs — authority at delivery time)

```
["lesson-learned", "decision", "convention", "pattern", "procedure"]
```

### package.json files array (after nan-011)

```
["bin/", "lib/", "skills/", "postinstall.js", "protocols/"]
```

---

## Function Signatures / Invocation Formats

### Correct MCP prefix form (all skill files must use this)

```
mcp__unimatrix__context_search(...)
mcp__unimatrix__context_store(...)
mcp__unimatrix__context_get(...)
mcp__unimatrix__context_lookup(...)
mcp__unimatrix__context_correct(...)
mcp__unimatrix__context_deprecate(...)
mcp__unimatrix__context_status(...)
mcp__unimatrix__context_briefing(...)
mcp__unimatrix__context_enroll(...)
mcp__unimatrix__context_quarantine(...)
mcp__unimatrix__context_cycle(...)
mcp__unimatrix__context_cycle_review(...)
```

### context_cycle call signature (for protocols/README.md example)

```
mcp__unimatrix__context_cycle({ "feature": "<id>", "type": "start" })
mcp__unimatrix__context_cycle({ "feature": "<id>", "type": "phase", "phase": "<name>" })
mcp__unimatrix__context_cycle({ "feature": "<id>", "type": "stop" })
```

### AC-10 Verification Grep Patterns (two-pass from ADR-004)

```bash
# Pass 1 — backtick-wrapped bare invocations
grep -rn '`context_[a-z_]*(' .claude/skills/*/SKILL.md

# Pass 2 — any bare invocation line without prefix
grep -rn 'context_[a-z_]*(' .claude/skills/*/SKILL.md | grep -v 'mcp__unimatrix__'
```

Both passes must return zero uninvestigated matches. Apply same passes to
`skills/uni-retro/SKILL.md` (repo root copy) independently.

---

## Constraints

1. **No Rust code changes.** Documentation, configuration, and skill files only. If an
   acceptance criterion would require a code change, file a separate issue.
2. **config.toml default values must match compiled defaults.** `config.rs` is the
   authority. Implementer must read every `default_*` function directly — no inferred
   values. Discrepancy = bug.
3. **config.toml must be valid TOML.** All uncommented fields must parse cleanly. Run
   `python3 -c "import tomllib; tomllib.load(open('config.toml','rb'))"` before PR.
4. **Protocols: choreography unchanged.** Wave structure, agent spawning order, and gate
   logic are not in scope. Only factual inaccuracies and removed-feature references are
   corrected.
5. **`uni-release` is not distributed.** Must not appear in `package.json` `files` array
   or npm pack output.
6. **`protocols/` files are copies, not symlinks.** Symlinks do not survive `npm pack`.
7. **`boosted_categories` / `adaptive_categories` serde default vs. Rust Default:** the
   serde `default_*` function returns `["lesson-learned"]`; `Default::default()` returns
   `[]`. The config.toml must show `["lesson-learned"]` with a comment explaining the
   two-site distinction.
8. **`rayon_pool_size` is dynamic.** Must not appear as a fixed integer. Show the formula
   `(num_cpus / 2).max(4).min(8)` in a comment.
9. **Source-before-copy ordering for distributions.** Fix `.claude/skills/uni-retro/SKILL.md`
   first, then copy to `skills/uni-retro/SKILL.md`. Fix protocols in `.claude/protocols/uni/`
   first, then copy to `protocols/`. Never copy then fix.
10. **Vision entries #4163/#4164 are out of scope.** Update via `context_correct` in a
    uni-zero session after merge. Implementer should note any material drift in their agent
    report.

---

## Dependencies

- `python3` with `tomllib` (stdlib, Python ≥ 3.11) — for TOML validity check (R-06)
- `node` + `npm` — for `npm pack --dry-run` verification (AC-13, R-08); confirm available
  before delivery
- `diff` / `git diff` — for dual-copy drift verification (R-04, R-12)
- `grep` — for bare invocation audit (AC-10) and NLI stale reference check (R-11)

No new crate dependencies. No schema migrations. No Rust compilation required.

---

## NOT in Scope

- Rust code changes to any crate
- Schema migrations
- New MCP tools or capabilities
- Workflow choreography changes in protocols (phase order, gate logic, agent spawning)
- Skills with correct MCP format and accurate content (10 of 14 skills require no changes)
- Unimatrix knowledge base vision entries #4163 and #4164 (deferred to post-merge uni-zero session)
- Minimum compatible Unimatrix version note on distributed `uni-retro` (SR-07 deferral,
  filed as follow-on concern)
- `uni-release` skill inclusion in npm package (internal tooling; only artifacts it packages
  are distributed)
- `InferenceConfig` compiled-in Rust defaults changes
- Any "Invisible Delivery" README bullet copy correction beyond the FR-1.2 qualifier sentence

---

## Alignment Status

Overall: **PASS** with 1 resolved WARN.

| Dimension | Status | Notes |
|-----------|--------|-------|
| Vision Alignment | PASS | Feature corrects documentation to match shipped vision; no strategic contradiction |
| Milestone Fit | PASS | Nanoprobes phase; documentation and distribution packaging fit cleanly |
| Architecture Consistency | PASS | All five SCOPE.md deliverables map to architecture components |
| Risk Completeness | PASS | All 15 risks trace to scope risks; coverage table consistent |
| Scope Gap (FR-1.2) | WARN — RESOLVED | FR-1.2 qualifier sentence was absent from SCOPE.md non-goals but present in spec. Accepted by project owner per spawn prompt; SCOPE.md updated. The qualifier is a single explanatory sentence preventing the vision statement from being read as a claim of hook-injected context delivery. |

**Vision entries #4163/#4164 drift:** These Unimatrix entries are not updated in this
delivery. The implementer must note any material drift between these entries and the
updated PRODUCT-VISION.md in their agent report for the post-merge uni-zero session.

**SR-07 (versioning contract for distributed uni-retro):** Acknowledged and deferred.
Distributing `uni-retro` in npm creates a versioning contract. Filed as a follow-on
concern. Minimum version documentation is not in scope for nan-011.
