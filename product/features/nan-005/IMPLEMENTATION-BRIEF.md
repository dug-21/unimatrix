# nan-005: Documentation & Onboarding — Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/nan-005/SCOPE.md |
| Scope Risk Assessment | product/features/nan-005/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nan-005/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-005/specification/SPECIFICATION.md |
| Risk & Test Strategy | product/features/nan-005/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-005/ALIGNMENT-REPORT.md |
| ADR-001 | product/features/nan-005/architecture/ADR-001-readme-single-file-structure.md |
| ADR-002 | product/features/nan-005/architecture/ADR-002-documentation-step-placement.md |
| ADR-003 | product/features/nan-005/architecture/ADR-003-trigger-criteria-mandatory-vs-optional.md |
| ADR-004 | product/features/nan-005/architecture/ADR-004-content-boundary-readme-vs-claudemd.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| readme-rewrite | pseudocode/readme-rewrite.md | test-plan/readme-rewrite.md |
| uni-docs-agent | pseudocode/uni-docs-agent.md | test-plan/uni-docs-agent.md |
| delivery-protocol-mod | pseudocode/delivery-protocol-mod.md | test-plan/delivery-protocol-mod.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Rewrite README.md as a comprehensive external-facing document covering all MCP tools, skills, knowledge categories, CLI subcommands, getting started paths, operational guidance, architecture overview, and security model -- enabling new users to understand and adopt Unimatrix without reading source code. Add a `uni-docs` agent definition and a conditional documentation step to the delivery protocol so documentation stays current after each shipped feature.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| README structure | Single file, 11 sections, capability-first order | SCOPE.md Resolved Q2, ADR-001 | architecture/ADR-001-readme-single-file-structure.md |
| Documentation step placement | After `gh pr create`, before `/review-pr`, within feature PR | SCOPE.md Resolved Q1, ADR-002 | architecture/ADR-002-documentation-step-placement.md |
| Trigger enforcement model | Mandatory for user-visible changes (tool/skill/CLI/category/schema); skip for internal-only | ADR-003, SR-05 mitigation | architecture/ADR-003-trigger-criteria-mandatory-vs-optional.md |
| Content boundary: README vs CLAUDE.md | External capabilities in README; internal dev rules in CLAUDE.md; cross-reference nan-003 skills, don't duplicate | ADR-004, SR-04 mitigation | architecture/ADR-004-content-boundary-readme-vs-claudemd.md |
| Stale detection approach | Incremental per-feature only (via uni-docs agent); no full validation pass | SCOPE.md Resolved Q3 | -- |
| Primary install path | npm install `@dug-21/unimatrix` (nan-004 shipped) | SCOPE.md Resolved Q4 | -- |
| `maintain` parameter behavior | Silently ignored since col-013; background tick handles maintenance | SPECIFICATION.md FR-04e | -- |
| Acknowledgments section | Preserved; credits claude-flow and ruvnet | SPECIFICATION.md FR-01e | -- |
| MicroLoRA description level | Mention by name as "adaptive embeddings that tune to project-specific usage patterns"; drop formula coefficients and technical internals (InfoNCE, EWC++) per SCOPE.md non-goals | ALIGNMENT-REPORT.md WARN #2, SCOPE.md Non-Goals | -- |
| Tool count | Verify from live codebase (`tools.rs` count); use verified number, not SCOPE.md claim of 12 | OQ-01, ALIGNMENT-REPORT.md WARN #1 | -- |

---

## Files to Create/Modify

### Modified Files

| File | Change Summary |
|------|---------------|
| `README.md` | Complete rewrite: 11 sections, capability-first, all tools/skills/categories/CLI documented |
| `.claude/protocols/uni/uni-delivery-protocol.md` | Add conditional documentation step to Phase 4 (after PR create, before /review-pr) |

### New Files

| File | Purpose |
|------|---------|
| `.claude/agents/uni/uni-docs.md` | Documentation agent definition: reads feature artifacts, proposes targeted README edits |

---

## Data Structures

Not applicable -- nan-005 is documentation-only. No Rust code, no schema changes, no new data types.

### README Section Structure (ordered by user-decision priority)

```
1.  Hero (2-3 sentences)
2.  Why Unimatrix (problem + differentiators)
3.  Core Capabilities (grouped by user experience)
4.  Getting Started (npm primary, build-from-source secondary, config)
5.  Tips for Maximum Value (operational guidance)
6.  MCP Tool Reference (table: all verified tools)
7.  Skills Reference (table: all 14 skills)
8.  Knowledge Categories (8 categories with descriptions + examples)
9.  CLI Reference (subcommands + global flags)
10. Architecture Overview (minimal: SQLite, hooks, MCP, 9 crates, data layout)
11. Security Model (trust hierarchy, scanning, audit, corrections)
```

---

## Function Signatures

Not applicable -- no Rust code produced.

### uni-docs Agent Interface

The uni-docs agent is spawned by the Delivery Leader with these inputs:
- Feature ID (e.g., `col-015`)
- Path to `SCOPE.md`
- Path to `SPECIFICATION.md` (optional; falls back to SCOPE.md only)
- Path to `README.md`

The agent reads artifacts, identifies README sections needing updates, proposes targeted edits, and commits to the feature branch with prefix `docs:`.

### Delivery Protocol Step Template

```
If feature adds/changes MCP tool, skill, CLI subcommand, knowledge category, or schema version:
  Spawn uni-docs agent with feature ID, SCOPE.md path, SPECIFICATION.md path, README.md path
  uni-docs reads artifacts and current README
  uni-docs commits targeted README edits to feature branch
  Continue to /review-pr
```

---

## Constraints

- **C-01**: README.md is the sole documentation artifact. No docs/ subdirectory, no static site. (SCOPE.md)
- **C-02**: uni-docs agent reads feature artifacts only, never source code. (SCOPE.md)
- **C-03**: Protocol changes are additive. No existing phases/gates restructured. (SCOPE.md, NFR-06)
- **C-04**: No runtime changes. Markdown files and protocol edits only. No Rust code, no MCP tools, no schema changes. (SCOPE.md)
- **C-05**: Skills are Claude Code platform-native. README documents them but cannot replace SKILL.md files.
- **C-06**: Operational guidance cross-references `/unimatrix-init` and `/unimatrix-seed` rather than restating their content. (ADR-004, SR-04 mitigation)
- **C-07**: Documentation step has no gate. Advisory only, does not block delivery. (SPECIFICATION.md FR-12f)
- **C-08**: Every numeric claim in README must be sourced from live codebase verification at authoring time. (SPECIFICATION.md FR-01c, NFR-01)
- **C-09**: Architecture section target is 20-40 lines. No crate internals, formula weights, or detection rule logic. (NFR-05)
- **C-10**: No aspirational/future features documented as current. No OAuth, HTTPS transport, `_meta` agent identity. (SPECIFICATION.md FR-09g)

---

## Fact Verification Checklist (must be completed before authoring README)

| Claim | Verification Command | Expected | Notes |
|-------|---------------------|----------|-------|
| MCP tool count | `grep -c '#\[tool(' crates/unimatrix-server/src/mcp/tools.rs` | Verify (11 per architecture) | OQ-01; use verified count |
| Skill count | `ls .claude/skills/ \| wc -l` | 14 | |
| Crate count | `ls crates/ \| wc -l` | 9 | includes unimatrix-learn |
| Schema version | `grep CURRENT_SCHEMA_VERSION crates/unimatrix-store/src/migration.rs` | 11 | |
| SQLite table count | `grep -c 'CREATE TABLE IF NOT EXISTS' crates/unimatrix-store/src/db.rs` | 19 | |
| Rust version | `grep rust-version Cargo.toml` | 1.89 | |
| npm package name | `cat packages/unimatrix/package.json \| jq .name` | @dug-21/unimatrix | |
| Test count | `grep -r '#\[test\]' crates/ \| wc -l` | 2131+ | state as approximate |
| Storage backend | `grep 'rusqlite' crates/unimatrix-store/Cargo.toml` | present | confirms SQLite |
| Database filename | `grep 'unimatrix\.db' crates/unimatrix-engine/src/project.rs` | present | |
| Hook event names | `grep -h 'UserPromptSubmit\|PreCompact\|Stop\|PostToolUse\|PreToolUse' crates/unimatrix-server/src/uds/hook.rs` | all 5 present | |
| `maintain` behavior | `grep -A5 'maintain' crates/unimatrix-server/src/mcp/tools.rs` | silently ignored | col-013 |
| CLI subcommands | Check `Command` enum in `crates/unimatrix-server/src/main.rs` | hook, export, import, version, model-download | |

---

## Dependencies

### No New Runtime Dependencies

nan-005 introduces no new Rust crates, npm packages, or external services.

### External Verification Dependencies (read-only, not modified)

| File | Purpose |
|------|---------|
| `crates/unimatrix-server/src/mcp/tools.rs` | Verify tool count, names, parameters |
| `crates/unimatrix-server/src/infra/categories.rs` | Verify 8 category names |
| `crates/unimatrix-server/src/main.rs` | Verify CLI subcommands and flags |
| `crates/unimatrix-store/src/migration.rs` | Verify schema version |
| `crates/unimatrix-store/src/db.rs` | Verify table count |
| `crates/unimatrix-engine/src/project.rs` | Verify data layout paths |
| `Cargo.toml` | Verify workspace crate count and rust-version |
| `packages/unimatrix/package.json` | Verify npm package name |
| `.claude/skills/*/SKILL.md` | Verify skill names and trigger conditions |
| `.claude/protocols/uni/uni-delivery-protocol.md` | Understand Phase 4 insertion point |

---

## NOT in Scope

- API documentation / rustdoc (internal code docs)
- Tutorial or walkthrough content (reference-grade only)
- Documentation website or static site generator
- Changelog automation (nan-004 handles CHANGELOG.md)
- Duplicating nan-003 onboarding content (`/unimatrix-init`, `/unimatrix-seed` referenced, not restated)
- Documenting internal development workflow (protocols, agents, swarms)
- Architecture deep-dives (scoring formula weights, HNSW parameters, table schemas, detection rules)
- Future/unimplemented capabilities (OAuth, HTTPS, `_meta`, Graph Enablement, Activity Intelligence)
- Crate-level internal documentation (no Rust doc comments)
- ADR creation (no architectural decisions; structure is prescribed)

---

## Alignment Status

**Overall: PASS** -- 0 FAIL, 0 VARIANCE, 2 advisory WARNs.

### WARN #1: Tool Count Not Resolved in Source Documents

SCOPE.md states 12 MCP tools; ARCHITECTURE.md fact table says 11; SPECIFICATION.md raises OQ-01 noting the discrepancy. **Resolution**: the implementation agent must verify from the live codebase (`grep -c '#\[tool(' tools.rs`) and use the verified count. Do not use 12 from SCOPE.md without verification.

### WARN #2: FR-02a Contains Implementation-Level Detail

SPECIFICATION.md FR-02a specifies exact formula coefficients (0.85, 0.15, 0.03) for the Core Capabilities section, conflicting with SCOPE.md's framing directive ("what users DO, not what was built") and explicit non-goal ("scoring formula weights are implementation details"). **Resolution**: drop numeric weights from the README. Describe confidence scoring in user-facing terms: "combines usage signals, correction quality, creator trust, and co-access patterns into a composite score." The implementation agent follows SCOPE.md framing, not FR-02a formula details.

### Additional Notes from Alignment Report

- Near-duplicate detection threshold (cosine similarity >= 0.92) added to operational guidance is a sensible addition to SCOPE.md's 6 constraints (now 7).
- `maintain` parameter behavior correction (silently ignored since col-013) is an appropriate codebase-reality correction, not a scope addition.
- Acknowledgments section preservation (FR-01e) is a low-risk addition not in SCOPE.md but appropriate to prevent accidental removal during rewrite.

---

## Key Risks (from RISK-TEST-STRATEGY.md)

| Priority | Risk | Mitigation |
|----------|------|------------|
| Critical | R-01: README contains verifiable factual error at ship time | Fact Verification Checklist must be completed and verified before authoring |
| Critical | R-02: Fact verification step skipped | Pseudocode phase produces completed checklist; review gate verifies |
| High | R-04: Tool count discrepancy shipped | Count `#[tool(` annotations; match README table row count |
| High | R-03: `maintain` parameter misdocumented | Document as silently ignored per FR-04e |
| High | R-05: uni-docs agent behavioral gaps | Verify fallback, scope boundary, no-source-code constraints in agent def |
| High | R-06: Protocol step at wrong position | Verify after `gh pr create`, before `/review-pr` |
| Med | R-07: Trigger criteria absent/ambiguous | Include mandatory/skip decision table in protocol text |
| Med | R-08: Aspirational content in README | Grep for "will/planned/future"; verify security section against FR-09g |
| Med | R-09: Inconsistent terminology | Enforce NFR-07: "Unimatrix" not "UniMatrix"; `context_search` not `contextSearch` |

---

## Open Questions for Implementation Agent

1. **OQ-01 (Tool Count)**: Run `grep -c '#\[tool(' crates/unimatrix-server/src/mcp/tools.rs` to confirm the exact MCP tool count. Use the verified number throughout the README.

2. **OQ-02 (MicroLoRA Detail Level)**: Per alignment resolution, use user-facing framing only: "adaptive embeddings that tune to project-specific usage patterns." Do not include InfoNCE, EWC++, or formula coefficients.

3. **OQ-03 (`unimatrix-learn` Crate)**: Verify what `unimatrix-learn` exports (check `crates/unimatrix-learn/src/lib.rs`) and include it with an accurate description in the architecture section's 9-crate list.

4. **OQ-04 (`/uni-git` Scope)**: Determine if `/uni-git` is user-facing or developer-only. Include it in the skills reference table regardless (all 14 skills documented) but note its scope if developer-focused.
