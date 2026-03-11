# Agent Report: base-004-agent-1-architect

## Task
Design the architecture for base-004 (Mandatory Knowledge Stewardship) -- a workflow-only feature modifying agent definitions, skills, validator checks, retro skill, and bugfix protocol.

## Artifacts Produced

### ARCHITECTURE.md
- Path: `product/workflow/base-004/architecture/ARCHITECTURE.md`
- Contains: System Overview, 6-component breakdown, component interactions, integration surface, detailed design for all components

### ADR Files
| ADR | Title | File |
|-----|-------|------|
| ADR-001 | Three-Tier Stewardship Section Template | `product/workflow/base-004/architecture/ADR-001-stewardship-section-template.md` |
| ADR-002 | Structured Agent Report Stewardship Block | `product/workflow/base-004/architecture/ADR-002-agent-report-stewardship-block.md` |
| ADR-003 | One Composite Stewardship Check Per Gate | `product/workflow/base-004/architecture/ADR-003-validator-composite-check.md` |
| ADR-004 | Separate /store-pattern Skill with What/Why/Scope Template | `product/workflow/base-004/architecture/ADR-004-store-pattern-skill.md` |
| ADR-005 | Bugfix Causal Linkage via caused_by_feature Tag | `product/workflow/base-004/architecture/ADR-005-bugfix-causal-linkage.md` |

## Key Design Decisions

1. **Three-tier agent classification** (ADR-001): Active storage (8 agents, 10-15 lines), read-only (2 agents, 6-8 lines), exempt (1 agent, 2 lines). Bounds context window cost at ~1,200 tokens total vs. ~4,800 if all agents got full stewardship sections.

2. **Structured report block** (ADR-002): `## Knowledge Stewardship` heading with `Queried:`/`Stored:`/`Declined:` bullet prefixes. Validator parses heading + prefix, not free-form prose. Addresses SR-02 brittle parsing risk.

3. **One composite check per gate** (ADR-003): Adds exactly 3 checks total (one per gate). Graduated enforcement: missing block = REWORKABLE FAIL, thin content = WARN. Addresses SR-07 adoption friction.

4. **Separate /store-pattern skill** (ADR-004): what/why/scope required fields. Quality floor: rejects missing "why" or "why" under 10 chars. Decision rule for pattern-vs-lesson ambiguity included. Addresses SR-04.

5. **Bugfix causal linkage** (ADR-005): `caused_by_feature:{id}` tag on outcomes and lessons. Uses existing tag mechanism, no schema changes. Optional -- investigator makes judgment call.

## Risk Mitigations

| Risk | Mitigation |
|------|-----------|
| SR-01 (context bloat) | Three-tier model with per-tier line budgets |
| SR-02 (brittle parsing) | Structured stewardship block with fixed heading + prefixes |
| SR-04 (pattern/lesson ambiguity) | Decision rule in both skill docs |
| SR-06 (feature_cycle tagging) | Skill instructs agent to pass feature_cycle; cannot auto-inject since skills are markdown |
| SR-07 (adoption friction) | Graduated enforcement: FAIL for missing, WARN for thin |

## Knowledge Stewardship

- Queried: Attempted `/knowledge-search` and `/query-patterns` for prior stewardship decisions -- MCP tools not available in spawned agent context
- Stored: ADR files written to filesystem. **MCP storage pending** -- this agent does not have access to `mcp__unimatrix__context_store`. The coordinator must store these 5 ADRs in Unimatrix via `/store-adr` to complete stewardship.
- Declined: No patterns to store -- this is architectural design, not implementation

## Open Questions

1. **Feature_cycle injection**: Skills are markdown instructions, not executable code. The skill tells agents to pass `feature_cycle` but cannot enforce it. Consider whether this is acceptable or if a CLAUDE.md behavioral rule is needed.

2. **CLAUDE.md /store-pattern mention**: Scope says "no CLAUDE.md changes" but `/store-pattern` needs discoverability. Recommend a one-line addition to CLAUDE.md's skill list.

3. **Retro quality pass scope**: Currently scoped to entries from the current feature cycle only. Cross-cycle stale pattern detection deferred to a future iteration.

## Self-Check

- [x] ARCHITECTURE.md contains System Overview, Component Breakdown, Interactions, Integration Surface
- [x] All ADRs follow format: `## ADR-NNN: Title` / `### Context` / `### Decision` / `### Consequences`
- [x] Each ADR is a separate file in `architecture/` with correct naming
- [x] ADR file paths included in return
- [x] Integration Surface table included
- [x] No placeholder or TBD sections -- unknowns flagged as open questions
- [x] All output files within `product/workflow/base-004/architecture/`
- [ ] Searched Unimatrix for existing ADRs -- attempted but MCP tools unavailable in agent context
- [ ] Every ADR stored in Unimatrix via `/store-adr` -- MCP tools unavailable; coordinator must complete
- [x] No superseded ADRs identified (new feature, no prior decisions to supersede)
