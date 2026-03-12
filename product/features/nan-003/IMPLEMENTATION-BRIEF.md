# nan-003: Unimatrix Onboarding Skills — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-003/SCOPE.md |
| Architecture | product/features/nan-003/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-003/specification/SPECIFICATION.md |
| Risk Strategy | product/features/nan-003/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-003/ALIGNMENT-REPORT.md |

---

## Goal

Deliver two Claude Code skill files — `/unimatrix-init` and `/unimatrix-seed` — that establish the three-layer Unimatrix chain (CLAUDE.md awareness → skill invocation → agent behavior) in new repositories. `/unimatrix-init` appends a self-contained Unimatrix block to CLAUDE.md and produces a read-only agent orientation report; `/unimatrix-seed` guides developers through human-directed, gated knowledge seeding to populate Unimatrix with foundational repo conventions, patterns, and procedures from day one.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| `/unimatrix-init` skill | pseudocode/unimatrix-init.md | test-plan/unimatrix-init.md |
| `/unimatrix-seed` skill | pseudocode/unimatrix-seed.md | test-plan/unimatrix-seed.md |
| CLAUDE.md block template | pseudocode/claude-md-template.md | test-plan/claude-md-template.md |
| Agent scan algorithm | pseudocode/agent-scan.md | test-plan/agent-scan.md |
| Seed state machine | pseudocode/seed-state-machine.md | test-plan/seed-state-machine.md |
| Entry quality gate | pseudocode/entry-quality-gate.md | test-plan/entry-quality-gate.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Seed state machine gates | Model `/unimatrix-seed` as an explicit state machine with hard STOP gates using bold "STOP. Wait for human response." phrasing at every level transition. No auto-advance. | SR-01 / SCOPE.md Goal 5 | architecture/ADR-001-hard-stop-gates-seed-state-machine.md |
| Idempotency sentinel | Paired versioned sentinel: `<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->` + `<!-- end unimatrix-init v1 -->`. Check entire file; for files >200 lines also explicitly check last 30 lines (head-check fallback). | SR-02 / SCOPE.md Resolved Decision 4 | architecture/ADR-002-versioned-sentinel-idempotency.md |
| MCP availability check | `/unimatrix-seed` calls `context_status()` as its absolute first action, before any file reads. Halt with actionable error if unavailable. `/unimatrix-init` requires no MCP (file ops only). | SR-06 | architecture/ADR-003-context-status-preflight.md |
| Agent recommendation output | Terminal-only. No file written. Recommendations become stale immediately as agents evolve; re-run (`--dry-run`) is free. | SCOPE.md Resolved Decision 2 | architecture/ADR-004-terminal-only-recommendation-output.md |
| CLAUDE.md block skills scope | Block lists only `unimatrix-*` prefixed skills (2 skills). Existing 11 skills (`store-adr`, `retro`, etc.) target experienced users; they appear in agent defs, not the onboarding block. | SCOPE.md Goal 4 / AC-11 | architecture/ADR-005-claude-md-block-unimatrix-skills-only.md |
| Seed entry categories + quality gate | Seed may only use categories: `convention`, `pattern`, `procedure`. Excluded: `decision`, `outcome`, `lesson-learned`, `duties`. Every entry must pass What/Why/Scope gate (≤200 char what, ≥10 char why, scope present) before human presentation. | SCOPE.md Goal 5 | architecture/ADR-006-seed-entry-categories-and-quality-gate.md |

---

## Files to Create

| File | Summary |
|------|---------|
| `.claude/skills/unimatrix-init/SKILL.md` | `/unimatrix-init` skill: pre-flight → agent scan → CLAUDE.md append. YAML frontmatter + markdown. |
| `.claude/skills/unimatrix-seed/SKILL.md` | `/unimatrix-seed` skill: MCP preflight → existing-check → Level 0 → gated levels 1/2 → done. YAML frontmatter + markdown. |

No Rust code, no schema changes, no crate modifications. Both deliverables are markdown files only.

---

## Data Structures

### CLAUDE.md Block Template (Component 3)

```markdown
<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->
## Unimatrix

Knowledge engine (MCP server). Makes agent expertise searchable, trustworthy, and self-improving.

### Available Skills

| Skill | When to Use |
|-------|-------------|
| `/unimatrix-init` | First-time setup: wire CLAUDE.md and get agent recommendations |
| `/unimatrix-seed` | Populate Unimatrix with foundational repo knowledge |

### Knowledge Categories

| Category | What Goes Here |
|----------|---------------|
| `decision` | Architectural decisions (ADRs) — use `/store-adr` |
| `pattern` | Reusable implementation patterns — use `/store-pattern` |
| `procedure` | Step-by-step workflows — use `/store-procedure` |
| `convention` | Project-wide coding/process standards |
| `lesson-learned` | Post-failure takeaways — use `/store-lesson` |

### When to Invoke

- Before implementing anything new → search knowledge base
- After each architectural decision → store ADR
- After each shipped feature → run retrospective
- When a technique evolves → update procedure
<!-- end unimatrix-init v1 -->
```

> **Note on `outcome` category (WARN-1)**: SPECIFICATION.md FR-05(c) adds `outcome` to the category guide; SCOPE.md AC-01 and ARCHITECTURE.md Component 3 both list five categories only (no `outcome`). Implementation follows SCOPE.md and ARCHITECTURE.md — five categories, no `outcome` in the block.

### Seed State Machine (Component 5)

```
States: PREFLIGHT → EXISTING_CHECK → LEVEL_0 → GATE_0 → [LEVEL_1] → GATE_1 → [LEVEL_2] → GATE_2 → DONE

Depth limit: Level 0 + 2 opt-in levels maximum. Level 3 does not exist.

Approval modes:
  Level 0: batch (2-4 entries, present as group, single approve/reject)
  Level 1+: per-entry (individual approve/reject for each)
```

### Entry Quality Gate (Component 6)

| Field | Rule | Reject if |
|-------|------|-----------|
| `what` | One sentence describing the knowledge | > 200 chars; missing |
| `why` | Consequence or motivation | < 10 chars; tautological; missing |
| `scope` | Where it applies | Missing |

Categories allowed: `convention`, `pattern`, `procedure`
Categories excluded: `decision`, `outcome`, `lesson-learned`, `duties`

### Agent Scan Output (Component 4)

```
Agent Orientation Report
========================
Agent file                     | Missing                          | Suggested Addition
-------------------------------|----------------------------------|------------------------------------------
.claude/agents/custom-dev.md   | context_briefing, unimatrix-*   | Add context_briefing call at session start
.claude/agents/my-reviewer.md  | outcome reporting                | Add /record-outcome reference at session end
```

Checks per agent file:
- `context_briefing` invocation or reference
- Outcome reporting (`/record-outcome` or `context_store` with `category: "outcome"`)
- Any `unimatrix-*` skill reference

---

## Function Signatures (Integration Surface)

| Integration Point | Signature | Source |
|-------------------|-----------|--------|
| `context_status()` | MCP tool → system health object | vnc-003 |
| `context_search(query, category?, k?)` | MCP tool → ranked entries | vnc-002 |
| `context_store(title, content, topic, category, tags, agent_id)` | MCP tool → entry ID | vnc-002 |
| Glob `.claude/agents/**/*.md` | File glob → list of paths | Claude Read/Glob |
| Read `CLAUDE.md` | File read → string content | Claude Read |
| Append to `CLAUDE.md` | Edit/append → confirmation | Claude Edit (append, NOT Write/overwrite) |

**Critical**: CLAUDE.md must be appended with Edit semantics (preserve existing content), never Write/overwrite.

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | Skills are markdown files — no executable code. All file ops performed by Claude following instructions. |
| C-02 | Manual installation to target repos. No auto-install mechanism (nan-004 scope). Prerequisites section required in both SKILL.md files. |
| C-03 | `/unimatrix-seed` requires operational MCP server. Pre-flight failure → halt immediately, not mid-exploration. |
| C-04 | Sentinel idempotency assumes CLAUDE.md is a readable markdown file. Encrypted or excessively large files: known limitation. |
| C-05 | Server-side 0.92 cosine dedup catches exact duplicates; EXISTING_CHECK (≥3 active entries in seeding categories) is primary near-duplicate defense. |
| C-06 | `/unimatrix-seed` relies on Claude's conversation context for state across turns. Explicit STOP gates are the only enforcement mechanism. |
| C-07 | `/unimatrix-init` must NOT modify any `.claude/agents/` files. Recommendations terminal-only. |
| C-08 | `unimatrix-` prefix is the new production skill namespace. Existing skills (`store-adr`, `retro`, etc.) unchanged. |
| C-09 | `/unimatrix-seed` must not use `decision`, `outcome`, or `lesson-learned` categories. |
| C-10 | Neither skill handles binary installation, ONNX download, `settings.json` wiring, or agent definition creation. |

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `unimatrix-server` (MCP server) | Runtime | Required for `/unimatrix-seed`. Must be running and wired in Claude settings. |
| `context_status` MCP tool | Runtime | Pre-flight in `/unimatrix-seed` |
| `context_search` MCP tool | Runtime | Existing-entries check in `/unimatrix-seed` |
| `context_store` MCP tool | Runtime | Storing approved seed entries |
| Claude Code skill loader | Platform | Directory name → slash command. Existing mechanism, no changes. |
| `.claude/skills/{name}/SKILL.md` format | Convention | YAML frontmatter + markdown. Both deliverables follow this. |
| `uni-init` agent (`.claude/agents/uni/uni-init.md`) | Adjacent | Complementary (not competing): handles brownfield bootstrap from `.claude/` files. Disambiguation required in SKILL.md. |

---

## NOT in Scope

- Installing the Unimatrix binary, ONNX model download, `settings.json` wiring → **nan-004**
- Modifying `.claude/agents/` files (recommendation only, no edits)
- Renaming existing skills (`store-adr`, `retro`, etc.) to `unimatrix-` prefix
- Creating or modifying agent definitions in the target repo
- Seeding from `.claude/agents/` or `.claude/protocols/` files → **`uni-init` agent**
- Non-Claude-Code environments or non-MCP transports
- Deep code analysis (function signatures, type hierarchies, dependency graphs)
- Automated test harness (skills are model instructions; verification is manual)
- `/unimatrix-init --update` for stale block replacement (future; sentinel infra provided by ADR-002)
- Automated skill installation across repos (nan-004 scope)

---

## Alignment Status

**Source**: product/features/nan-003/ALIGNMENT-REPORT.md

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | **VARIANCE** | PRODUCT-VISION.md defines nan-003 as "schema, ONNX, npx unimatrix init"; actual scope is Claude Code skills only. Full installation deferred to nan-004. PRODUCT-VISION.md requires update. |
| Milestone Fit | PASS | Skills delivery (CLAUDE.md scaffolding + knowledge seeding) directly enables first multi-repo deployments per Platform Hardening milestone. |
| Scope Gaps | PASS | All 14 AC from SCOPE.md addressed across source documents. |
| Scope Additions | **WARN** | SPECIFICATION.md FR-05(c) adds `outcome` to the category guide; SCOPE.md AC-01 and ARCHITECTURE.md block template both enumerate 5 categories without `outcome`. **Implementation decision: follow SCOPE.md and ARCHITECTURE.md — 5 categories, no `outcome` in the block.** |
| Architecture Consistency | **WARN** | (1) `outcome` discrepancy resolved by implementation decision above. (2) Spec open question 2 treats ADR-002 sentinel fallback as undecided — ADR-002 has decided it (head-check fallback for files >200 lines). Implementation follows ADR-002. |
| Risk Completeness | PASS | All 7 scope risks traced; 13 test risk scenarios with scenarios mapped. |

**VARIANCE 1 Action Required**: PRODUCT-VISION.md should be updated to split nan-003 ("Onboarding Skills") and nan-004 ("Installation, packaging, schema, ONNX, npx") before this issue is closed. This is a documentation task, not a blocker for implementation.

**WARN: Existing-entries threshold** — ARCHITECTURE.md open question 3 proposes ≥3 active entries; RISK-TEST-STRATEGY.md R-10 references this threshold; SPECIFICATION.md FR-14 omits any numeric value. **Implementation must resolve: use ≥3 active entries in `convention`/`pattern`/`procedure` categories as the threshold for the re-seed warning.**
