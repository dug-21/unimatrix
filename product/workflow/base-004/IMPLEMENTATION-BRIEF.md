# base-004 Implementation Brief: Mandatory Knowledge Stewardship

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/workflow/base-004/SCOPE.md |
| Scope Risk Assessment | product/workflow/base-004/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/workflow/base-004/architecture/ARCHITECTURE.md |
| Specification | product/workflow/base-004/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/workflow/base-004/RISK-TEST-STRATEGY.md |
| Alignment Report | product/workflow/base-004/ALIGNMENT-REPORT.md |
| ADR-001 | product/workflow/base-004/architecture/ADR-001-stewardship-section-template.md |
| ADR-002 | product/workflow/base-004/architecture/ADR-002-agent-report-stewardship-block.md |
| ADR-003 | product/workflow/base-004/architecture/ADR-003-validator-composite-check.md |
| ADR-004 | product/workflow/base-004/architecture/ADR-004-store-pattern-skill.md |
| ADR-005 | product/workflow/base-004/architecture/ADR-005-bugfix-causal-linkage.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| C1: Agent Definition Stewardship Sections | pseudocode/agent-stewardship-sections.md | test-plan/agent-stewardship-sections.md |
| C2: Agent Report Stewardship Block | pseudocode/report-stewardship-block.md | test-plan/report-stewardship-block.md |
| C3: Validator Gate Check Integration | pseudocode/validator-gate-checks.md | test-plan/validator-gate-checks.md |
| C4: /store-pattern Skill | pseudocode/store-pattern-skill.md | test-plan/store-pattern-skill.md |
| C5: Retro Stewardship Quality Pass | pseudocode/retro-quality-pass.md | test-plan/retro-quality-pass.md |
| C6: Bugfix Protocol Linkage | pseudocode/bugfix-protocol-linkage.md | test-plan/bugfix-protocol-linkage.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Close the knowledge feedback loop in Unimatrix by making every swarm agent responsible for storing findings back (or explicitly declining), enforcing compliance through validator gate checks, curating quality through a retro stewardship pass, and providing a dedicated `/store-pattern` skill for implementation-level patterns. After this feature, no agent can silently omit knowledge storage -- the system enforces that every agent makes a stewardship decision.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Stewardship section template | Three-tier structure: Active (10-15 lines), Read-only (6-8 lines), Exempt (2 lines). Active agents get Before/After/Report subsections. Quality enforcement in skills, not agent defs. | ADR-001 + SR-01 | architecture/ADR-001-stewardship-section-template.md |
| Agent report stewardship block | Structured `## Knowledge Stewardship` heading with bullet-list format: `- Stored:`, `- Queried:`, `- Declined:`. Validator parses heading + bullet prefixes. | ADR-002 + Variance #1 resolution | architecture/ADR-002-agent-report-stewardship-block.md |
| Validator check integration | One composite stewardship check per gate (3a #5, 3b #7, 3c #5). REWORKABLE FAIL if block missing; WARN if reason missing after "nothing novel". | ADR-003 | architecture/ADR-003-validator-composite-check.md |
| /store-pattern skill design | Separate skill with required what/why/scope fields. `why` minimum 10 chars, `what` maximum 200 chars. Category always `pattern`. Dedup via context_search before storing. | ADR-004 + SR-04 | architecture/ADR-004-store-pattern-skill.md |
| Bugfix causal linkage | `caused_by_feature:{feature-id}` tag on bugfix outcomes and lessons. Optional -- investigator judgment. No validator enforcement on this specific tag. | ADR-005 | architecture/ADR-005-bugfix-causal-linkage.md |
| Report heading string | `## Knowledge Stewardship` (not `## Stewardship`). All agent defs, report blocks, and validator parsing use this exact heading. | Human-approved variance #1 | -- |
| Retro quality pass timing | Phase 1b (before pattern extraction). Review agent-stored entries first, then retro extracts its own patterns. | Human-approved variance #2 | -- |
| uni-specification tier | Read-only. No storage expected. Spec decisions are feature-specific, not generalizable. Retro can promote any that generalize. | Human-approved variance #3 | -- |
| Report format | Bullet-list format (`- Stored:`, `- Queried:`, `- Declined:`) NOT table format. Architecture's format wins over Specification's table. | Human-approved variance #4 | -- |

## Files to Create/Modify

### New Files

| File | Summary |
|------|---------|
| `.claude/skills/store-pattern/SKILL.md` | New skill for implementation-level patterns with what/why/scope template, dedup check, and pattern-vs-lesson decision rule |

### Modified Files

| File | Summary |
|------|---------|
| `.claude/agents/uni/uni-rust-dev.md` | Add Knowledge Stewardship section: active-storage tier, crate-as-topic, /store-pattern skill, example patterns, self-check item |
| `.claude/agents/uni/uni-tester.md` | Add Knowledge Stewardship section: active-storage tier, testing/crate topic, /store-pattern or /store-procedure skill, self-check item |
| `.claude/agents/uni/uni-validator.md` | Strengthen existing stewardship section + add composite stewardship check to Gate 3a (#5), 3b (#7), 3c (#5) check sets + self-check item |
| `.claude/agents/uni/uni-risk-strategist.md` | Strengthen existing stewardship section: active-storage tier, risk topic, /store-pattern skill, self-check item |
| `.claude/agents/uni/uni-researcher.md` | Strengthen existing stewardship section: active-storage tier, research area topic, /store-pattern skill, self-check item |
| `.claude/agents/uni/uni-bug-investigator.md` | Strengthen existing stewardship section: active-storage tier, crate topic, /store-lesson skill, causal feature guidance, self-check item |
| `.claude/agents/uni/uni-vision-guardian.md` | Add Knowledge Stewardship section: active-storage tier, vision topic, /store-pattern skill, recurring misalignment patterns, self-check item |
| `.claude/agents/uni/uni-security-reviewer.md` | Add Knowledge Stewardship section: active-storage tier, security/crate topic, /store-lesson skill, self-check item |
| `.claude/agents/uni/uni-specification.md` | Add Knowledge Stewardship section: read-only tier, queries patterns before working, no storage expected, self-check item |
| `.claude/agents/uni/uni-pseudocode.md` | Add Knowledge Stewardship section: read-only tier, queries patterns before designing, reports deviations, self-check item |
| `.claude/agents/uni/uni-synthesizer.md` | Add Knowledge Stewardship section: exempt tier, "no storage or query expected" with rationale |
| `.claude/agents/uni/uni-architect.md` | No changes needed -- already has gold-standard stewardship section |
| `.claude/skills/retro/SKILL.md` | Add Phase 1b: Stewardship Quality Review between Phase 1 and Phase 2 |
| `.claude/protocols/uni/uni-bugfix-protocol.md` | Add stewardship requirements: investigator causal linkage, rust-dev pattern storage, caused_by_feature tag in /record-outcome, bugfix validator stewardship check |

## Data Structures

No Rust data structures. The key data contracts are markdown formats:

### Agent Report Stewardship Block

```markdown
## Knowledge Stewardship

- Queried: /query-patterns for {area} -- {findings summary or "no results"}
- Stored: entry #{id} "{title}" via /store-pattern (or "nothing novel to store -- {reason}")
- Declined: {category} -- {reason}
```

### /store-pattern Content Assembly

```
What: {one-sentence pattern}
Why: {what goes wrong without it}
Scope: {crate, module, or context where it applies}
```

### Bugfix Causal Tag

```
tags: ["bugfix", "caused_by_feature:{originating-feature-id}"]
```

## Function Signatures

No Rust functions. The interface contracts are MCP tool calls used by skills:

| Skill | MCP Tool | Key Parameters |
|-------|----------|---------------|
| `/store-pattern` | `context_store` | title, content (assembled what/why/scope), topic, category: "pattern", tags (incl. feature_cycle) |
| `/store-pattern` (dedup) | `context_search` | query: "{what}", category: "pattern", k: 3 |
| `/store-pattern` (supersede) | `context_correct` | original_id, content, reason |
| Retro Phase 1b | `context_search` | query: "{feature-id}", k: 20 |
| Retro Phase 1b (deprecate) | `context_deprecate` | entry_id, reason |
| Bugfix outcome | `/record-outcome` | type: bugfix, tags: ["caused_by_feature:{id}"] |

## Constraints

1. **File-only changes**: All modifications to `.claude/agents/uni/*.md`, `.claude/skills/*/SKILL.md`, `.claude/skills/retro/SKILL.md`, `.claude/protocols/uni/uni-bugfix-protocol.md`. No Rust code, Cargo.toml, or schema changes.
2. **Backward compatibility**: Agent definitions must remain valid for mid-session agents. Section additions only -- no structural changes to existing sections, no removal of existing content.
3. **Context window budget (NFR-01)**: Active-storage agents 10-15 lines, read-only 6-8 lines, exempt 2 lines for stewardship section (excluding heading and self-check).
4. **Self-check format**: `- [ ] {statement}` matching existing convention.
5. **Skill structure**: `/store-pattern` follows same directory/SKILL.md conventions as `/store-lesson`, `/store-procedure`.
6. **Validator check format**: New gate checks follow existing numbered check format.
7. **Heading contract**: `## Knowledge Stewardship` is the exact heading used everywhere -- agent defs, report blocks, and validator parsing. No variations.
8. **Atomic deployment (NFR-04)**: All agent definition changes in a single commit.
9. **No advisory rollout**: Stewardship checks are REWORKABLE FAIL from day one. The bar is low (store or explicitly decline).

## Dependencies

| Dependency | Type | Used By |
|------------|------|---------|
| Unimatrix MCP `context_store` | Existing tool | /store-pattern skill |
| Unimatrix MCP `context_search` | Existing tool | /store-pattern dedup, retro Phase 1b |
| Unimatrix MCP `context_correct` | Existing tool | /store-pattern supersede, retro recategorize |
| Unimatrix MCP `context_deprecate` | Existing tool | Retro Phase 1b quality curation |
| `/store-lesson` skill | Existing skill | Bug-investigator, security-reviewer, validator |
| `/store-procedure` skill | Existing skill | Tester (alternative to /store-pattern) |
| `/store-adr` skill | Existing skill | Architect (unchanged) |
| `/query-patterns` skill | Existing skill | All non-exempt agents (query before work) |
| `/record-outcome` skill | Existing skill | Bugfix protocol (add caused_by_feature tag) |
| 12 agent definitions | Existing files | `.claude/agents/uni/*.md` (uni-init and uni-scrum-master excluded) |
| Retro skill | Existing file | `.claude/skills/retro/SKILL.md` |
| Bugfix protocol | Existing file | `.claude/protocols/uni/uni-bugfix-protocol.md` |

## NOT in Scope

1. No Rust code changes -- no crate modifications, no Cargo.toml, no schema migrations
2. No deliberate retrieval confidence boost (tracked in #199)
3. No MCP tool signature changes -- existing tools are sufficient
4. No automated storage -- agents decide; the system enforces a decision was made
5. No CLAUDE.md changes
6. No uni-init changes
7. No advisory/warn-only rollout period
8. No auto-injection of feature_cycle tags -- agents responsible, skill recommends
9. No changes to uni-architect stewardship section (already gold standard)
10. No changes to uni-scrum-master or uni-init (not participant agents)

## Alignment Status

**Overall: PASS with resolved variances.**

Vision alignment is strong -- this feature directly supports the "self-learning expertise engine" vision by closing the knowledge feedback loop. All 4 variances identified in the Alignment Report have been resolved by human approval:

| Variance | Resolution |
|----------|-----------|
| Heading mismatch (Architecture `## Knowledge Stewardship` vs Specification `## Stewardship`) | Use `## Knowledge Stewardship` everywhere. This is the parsing contract. |
| Retro phase insertion point (Architecture Phase 2b vs Specification Phase 1b) | Phase 1b -- review agent-stored entries before retro extracts its own patterns. |
| uni-specification tier (Architecture read-only vs Specification active-storage) | Read-only. No storage expected. Spec decisions are feature-specific. |
| Report format (Architecture bullet-list vs Specification table) | Bullet-list format (`- Stored:`, `- Queried:`, `- Declined:`). Simpler to produce and parse. |

Scope additions noted as WARN in alignment report (bugfix protocol elaboration FR-08, report format change FR-04) are accepted -- they are reasonable elaborations of scope intent.

## Agent Tier Classification (Authoritative)

This table is the single source of truth for tier assignments, resolving all cross-document inconsistencies.

| Agent | Tier | Gate | Store Expectation | Query Expectation |
|-------|------|------|-------------------|-------------------|
| uni-architect | Active | 3a | ADRs + patterns via /store-adr | /query-patterns |
| uni-risk-strategist | Active | 3a | Risk patterns via /store-pattern | /query-patterns |
| uni-pseudocode | Read-only | 3a | None | /query-patterns |
| uni-rust-dev | Active | 3b | Implementation patterns via /store-pattern | /query-patterns |
| uni-vision-guardian | Active | 3b (if spawned) | Alignment patterns via /store-pattern | /query-patterns |
| uni-tester | Active | 3c | Test patterns via /store-pattern or /store-procedure | /knowledge-search |
| uni-validator | Active | N/A (is the validator) | Gate failure patterns via /store-lesson | /query-patterns |
| uni-researcher | Active | N/A (design session) | Research patterns via /store-pattern | /query-patterns |
| uni-bug-investigator | Active | Bugfix gate | Root cause lessons via /store-lesson | /query-patterns |
| uni-security-reviewer | Active | N/A (PR review) | Security anti-patterns via /store-lesson | /query-patterns |
| uni-specification | Read-only | N/A (design session) | None | /query-patterns |
| uni-synthesizer | Exempt | N/A | None | None |

## Critical Implementation Notes

1. **Heading consistency is the #1 risk** (R-01, Critical). Every file that references the report heading must use exactly `## Knowledge Stewardship`. Grep all modified files after implementation to verify zero variations.

2. **Bullet prefix format must be exact**: `- Stored:`, `- Queried:`, `- Declined:`, `- Updated:`. These are the parsing tokens. Case-sensitive, colon-terminated, dash-prefixed.

3. **The validator checks stewardship from agent reports, not from Unimatrix**. Agents must include evidence in their report. The validator never calls context_search to verify entries exist.

4. **feature_cycle tag is recommended, not enforced**. The retro quality pass query should search by feature ID in content/title as fallback, not only by tag (R-05 mitigation).

5. **Pattern vs lesson decision rule** must appear in both `/store-pattern` and `/store-lesson` skill files to prevent inconsistent categorization (SR-04).

6. **Retro Phase 1b assesses by category-appropriate template**: what/why/scope for patterns, what-happened/root-cause/takeaway for lessons, numbered steps for procedures. Do not apply one template to all categories (R-12 mitigation).
