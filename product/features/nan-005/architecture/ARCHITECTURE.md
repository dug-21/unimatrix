# Architecture: nan-005 — Documentation & Onboarding

## System Overview

nan-005 is a documentation-only feature. It produces markdown files and protocol edits — no Rust code, no schema changes, no new MCP tools. It sits entirely outside the crate workspace, touching three areas:

1. **README.md** — rewritten as the canonical external-facing product document
2. **uni-docs agent definition** — new agent at `.claude/agents/uni/uni-docs.md` that updates docs after each shipped feature
3. **Delivery protocol modification** — adds a documentation step to `uni-delivery-protocol.md` Phase 4

The README is the sole user-facing documentation artifact. It replaces the current stale 310-line developer-focused document with a capability-first reference covering all 11 MCP tools, 14 skills, 8 knowledge categories, 5 CLI subcommands, getting started paths, and operational guidance.

### How This Feature Fits

```
External User
    ↓
README.md ← [nan-005 rewrites this]
    ↓ references
.claude/skills/ (14 skills)    ← [nan-005 documents but does not modify]
MCP tools (11 tools)           ← [nan-005 documents but does not modify]

Delivery Protocol
    ↓
Phase 4: Delivery
    ├── commit + push + PR
    ├── /review-pr
    ├── [NEW] uni-docs agent ← [nan-005 adds this step]
    └── /record-outcome
```

---

## Component Breakdown

### Component 1: README.md Rewrite

**Responsibility**: Single authoritative external-facing document covering the full product surface.

**Location**: `/README.md` (overwrites existing file)

**Scope**: 11 sections as defined in SCOPE.md. Capability-first framing — every section leads with what users can do, not implementation details.

**Content boundaries**:
- README documents concepts, capabilities, and workflow guidance
- README does NOT restate `/unimatrix-init` or `/unimatrix-seed` content — cross-references only (SR-04)
- README does NOT document internal development workflow (protocols, agent definitions, swarm orchestration)
- README does NOT include architecture deep-dives (crate internals, formula weights, detection rule logic)
- Internal dev rules stay in CLAUDE.md

**Fact verification requirement** (SR-01): Every numeric claim must be sourced from the live codebase, not from SCOPE.md counts. The implementation agent must verify before writing:

| Claim | Source | Verified Value |
|-------|--------|---------------|
| MCP tool count | `crates/unimatrix-server/src/mcp/tools.rs` — count `async fn context_` | 11 |
| Skill count | `ls .claude/skills/` — count directories | 14 |
| Crate count | `ls crates/` | 9 |
| Schema version | `crates/unimatrix-store/src/migration.rs:CURRENT_SCHEMA_VERSION` | 11 |
| Database filename | `crates/unimatrix-engine/src/project.rs` | `unimatrix.db` |
| Rust version | `Cargo.toml:rust-version` | 1.89 |
| Test count | count `#[test]` across all `.rs` files | 2131+ |
| npm package name | `packages/unimatrix/package.json` | `@dug-21/unimatrix` |
| CLI subcommands | `crates/unimatrix-server/src/main.rs:Command enum` | hook, export, import, version, model-download |

**Estimated size**: 11 sections × average 40–60 lines = 450–650 lines. Single file is tractable (SR-03). Navigability via GitHub markdown heading links.

---

### Component 2: uni-docs Agent Definition

**Responsibility**: Lightweight documentation update agent. Reads feature artifacts after delivery, identifies README sections needing updates, proposes specific edits.

**Location**: `.claude/agents/uni/uni-docs.md`

**Inputs** (reads):
- `product/features/{id}/SCOPE.md` — what was delivered, new capabilities
- `product/features/{id}/specification/SPECIFICATION.md` — exact interface changes
- Current `README.md` — sections that may need updating

**Outputs** (produces):
- Specific README edit proposals — not a full rewrite
- Identifies sections: MCP Tool Reference (new/changed tools), Skills Reference (new skills), Knowledge Categories (new categories), CLI Reference (new subcommands), operational guidance (new constraints)

**Behavioral rules**:
- Reads artifacts only — never reads implementation code (Constraint 2 from SCOPE.md)
- Proposes targeted edits to specific sections — does not rewrite unaffected sections
- Verifies claims against SCOPE.md and SPECIFICATION.md — no invention
- Commits documentation changes to the feature branch (before /review-pr)

**Pattern**: Follows the same structure as existing agents (uni-vision-guardian.md): frontmatter header, role definition, inputs/outputs, behavioral rules, self-check, knowledge stewardship block.

---

### Component 3: Delivery Protocol Modification

**Responsibility**: Adds documentation update step to Phase 4 of `uni-delivery-protocol.md`.

**Location**: Modification to `.claude/protocols/uni/uni-delivery-protocol.md`

**Insertion point**: After PR creation and before `/review-pr` invocation (per Resolved Questions #1 in SCOPE.md). This ensures documentation updates are part of the reviewed PR.

**Exact Phase 4 sequence after modification**:
```
Phase 4: Delivery
  1. Commit final artifacts (test: risk coverage + gate reports)
  2. Push feature branch + open PR
  3. [NEW] Conditionally spawn uni-docs agent (trigger criteria below)
  4. Invoke /review-pr
  5. Return SESSION 2 COMPLETE
```

**Trigger criteria** (SR-05 — mandatory vs optional):

| Feature change type | Documentation step |
|--------------------|--------------------|
| New or modified MCP tool | MANDATORY |
| New or modified skill | MANDATORY |
| New CLI subcommand or flag | MANDATORY |
| New knowledge category | MANDATORY |
| New operational constraint for users | MANDATORY |
| Schema change with user-visible behavior change | MANDATORY |
| Internal refactor (no user-visible change) | SKIP |
| Test-only feature | SKIP |
| Documentation-only feature (nan-005 itself) | SKIP |

**Gate behavior**: The documentation step has no gate. It is advisory — the Delivery Leader determines trigger criteria, spawns the agent, and includes resulting changes in the PR. If documentation update fails or produces nothing, delivery continues.

**Protocol modification is additive** — no existing phases or gates are restructured.

---

## Component Interactions

```
┌─────────────────────────────────────────────────────┐
│                   README.md (Component 1)           │
│                                                     │
│  Written once (nan-005). Updated incrementally      │
│  by uni-docs agent after future feature deliveries. │
└──────────────────────────┬──────────────────────────┘
                           │ updated by
                           ▼
┌─────────────────────────────────────────────────────┐
│              uni-docs agent (Component 2)           │
│                                                     │
│  Spawned by: Delivery Leader (Component 3)          │
│  Reads: SCOPE.md + SPECIFICATION.md                 │
│  Writes: README.md edits (committed to feature PR)  │
└──────────────────────────┬──────────────────────────┘
                           │ spawned from
                           ▼
┌─────────────────────────────────────────────────────┐
│          Delivery Protocol Phase 4 (Component 3)    │
│                                                     │
│  Trigger criteria determine when uni-docs runs.     │
│  Step placement: after PR open, before /review-pr.  │
└─────────────────────────────────────────────────────┘
```

**Data flow for future feature deliveries**:
1. Feature delivered → Stage 3a/3b/3c gates pass
2. Delivery Leader evaluates trigger criteria against feature type
3. If mandatory: spawns uni-docs with SCOPE.md + SPECIFICATION.md paths
4. uni-docs reads artifacts, reads README.md, proposes targeted edits
5. uni-docs commits changes to feature branch
6. /review-pr reviews documentation changes as part of the PR

---

## Technology Decisions

See ADRs below for full decision rationale.

| Decision | Choice | ADR |
|----------|--------|-----|
| README structure | Single file, 11 sections, capability-first | ADR-001 |
| Documentation placement in protocol | Before /review-pr, within feature PR | ADR-002 |
| Trigger enforcement model | Mandatory for tool/skill/CLI changes, skip for internal | ADR-003 |
| Content boundary: README vs CLAUDE.md | External capabilities in README, internal dev rules in CLAUDE.md | ADR-004 |

---

## Integration Points

### README.md — What It Documents

The README references these surfaces without duplicating their content:

| Surface | README treatment |
|---------|-----------------|
| `.claude/skills/` (14 skills) | Skills Reference table — name, purpose, trigger condition |
| MCP tools (11 tools) | MCP Tool Reference table — name, purpose, when-to-use |
| CLI subcommands (5) | CLI Reference table — subcommand, description, flags |
| npm package `@dug-21/unimatrix` | Getting Started primary install path |
| `/unimatrix-init` skill | Cross-reference only in operational guidance section |
| `/unimatrix-seed` skill | Cross-reference only in operational guidance section |
| CLAUDE.md | Not documented in README — internal dev rules stay in CLAUDE.md |

### uni-docs Agent — Artifact Dependencies

The agent depends on the existence and quality of feature artifacts:

| Artifact | Required | Fallback if missing |
|---------|----------|---------------------|
| `SCOPE.md` | Yes | Skip documentation step |
| `SPECIFICATION.md` | Yes (preferred) | Fall back to SCOPE.md only |
| Current `README.md` | Yes | Cannot proceed |

SR-02 from risk assessment: if artifacts are thin or missing, the agent produces nothing useful. The agent definition must include a fallback: if SPECIFICATION.md is missing, use SCOPE.md as sole source. Document this limitation explicitly in the agent's behavioral rules.

### Delivery Protocol — Integration Surface

The modification inserts a conditional block in Phase 4. The Delivery Leader spawns the agent only when trigger criteria match. The exact spawn prompt template must be included in the protocol modification so the Delivery Leader has a concrete template to follow (SR-07).

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| uni-docs agent | Agent definition file at `.claude/agents/uni/uni-docs.md` | Created by nan-005 |
| Delivery protocol Phase 4 | Modified section in `uni-delivery-protocol.md` | Modified by nan-005 |
| README.md | Markdown file at root | Rewritten by nan-005 |
| Skills reference | 14 skills documented by name and description | `.claude/skills/*/SKILL.md` frontmatter |
| MCP tool reference | 11 tools documented by name, purpose, when-to-use | `crates/unimatrix-server/src/mcp/tools.rs` |
| CLI reference | 5 subcommands (hook, export, import, version, model-download) | `crates/unimatrix-server/src/main.rs:Command enum` |
| npm package | `@dug-21/unimatrix` version `0.5.0` | `packages/unimatrix/package.json` |

---

## README Section Structure

Ordered by user-decision priority — a new user reads top-to-bottom and can stop when they have enough to decide.

```
1. Hero (3-4 sentences)
   What Unimatrix is. One key differentiator sentence. "Built in Rust. Zero cloud dependency."

2. Why Unimatrix
   Problem framing. Auditable lifecycle differentiator. Invisible delivery differentiator.
   Self-learning differentiator. 3-5 bullet points max.

3. Core Capabilities
   Grouped by user experience, not by crate.
   - Self-learning knowledge engine (MicroLoRA, confidence evolution)
   - Semantic search with confidence-aware ranking
   - Hook-driven invisible delivery (Cortical Implant)
   - Retrospective intelligence (21 detection rules)
   - Contradiction detection and correction chains

4. Getting Started
   - npm install (PRIMARY path — @dug-21/unimatrix)
   - Build from source (SECONDARY — requires Rust 1.89+, ONNX Runtime 1.20.x)
   - Configure MCP server (settings.json snippet)
   - Configure hooks (settings.json snippet)
   - Cold start: /unimatrix-seed (cross-reference)

5. Tips for Maximum Value
   Session boundaries, feature cycle naming, phase prefix format,
   /unimatrix-seed for cold start, category discipline.

6. MCP Tool Reference
   Table: Tool | Purpose | When to use
   All 11 tools. Distinguishes search vs lookup, correct vs deprecate vs quarantine.

7. Skills Reference
   Table: Skill | Purpose | When to invoke
   All 14 skills. Trigger conditions.

8. Knowledge Categories
   8 categories with description and example.
   outcome, lesson-learned, decision, convention, pattern, procedure, duties, reference.

9. CLI Reference
   Table: Subcommand | Description | Key flags
   5 subcommands: hook, export, import, version, model-download

10. Architecture Overview
    Minimal. SQLite local storage (unimatrix.db). HNSW vector index (in-memory).
    Hook integration (UDS transport). MCP transport (stdio). 9-crate workspace list only.
    Data layout with correct filenames.

11. Security Model
    Trust hierarchy, content scanning, audit trail, hash-chained corrections.
    User-facing summary only.
```

---

## Open Questions

1. **Skills without frontmatter**: `uni-git` skill lacks a `description:` frontmatter field (observed during research). The skills reference table must derive its description from the skill content or document as "Git workflow conventions." The implementation agent should verify all 14 skill descriptions before writing the table.

2. **Tool count discrepancy**: The existing README and SCOPE.md both claim 12 tools. The live codebase (`mcp/tools.rs`) has 11 `async fn context_` handlers. The implementation agent must confirm this count before writing the MCP reference table. The SCOPE.md table also lists 11 entries but the header references 12.

3. **`unimatrix-learn` crate**: Nine crates exist in the workspace but the SCOPE.md and existing README reference 8. The implementation agent must determine what `unimatrix-learn` does and include it accurately in the architecture section.

4. **Skill `uni-git` trigger condition**: The SCOPE.md skills table does not list `/uni-git` — it may be a developer-only skill (git conventions for Unimatrix contributors) rather than a user-facing skill. The implementation agent should determine whether to include it in the skills reference or note it as developer-only.
