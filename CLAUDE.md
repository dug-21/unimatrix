# NDP — Non-Negotiable Rules

1. **Feature work uses swarms** — spawn `uni-scrum-master` for Unimatrix product work. NDP agents retained as reference in `.claude/agents/ndp/`. 
4. **Anti-stub**: Never leave TODO, `unimplemented!()`, `todo!()`, or placeholder functions. Ask the user if blocked.
5. **Never save files to root.** Use project directory structure.

---

## Project Vision
Unimatrix is a self-learning context engine that serves as the knowledge backbone for multi-agent development orchestration — accumulating conventions, decisions, patterns, and process intelligence across feature cycles, then delivering the right context to the right agent at the right workflow moment. Over time, it evolves from a knowledge store into a workflow-aware system that proposes process improvements from evidence, supports multiple concurrent projects, and provides a real-time interface for human visibility and control.

---

## Project Structure

```
/docs                    - Architecture docs and procedures
/product/features/       - Feature documentation per feature
/product/workflow/        - Workflow evolution (base-{NNN}/ proposals)
/.claude/agents/uni      - Unimatrix product agents (active)
/.claude/agents/ndp      - NDP agent definitions (reference)
/.claude/protocols/uni   - Unimatrix protocols (design, delivery, routing)
/.claude/protocols       - NDP swarm protocols (reference)
/.claude/rules           - Contextual rules (testing, rust workspace)
```

---

## Feature Conventions

Features follow `{phase}-{NNN}` pattern in `product/features/`:

| Phase | Prefix | Focus |
|-------|--------|-------|
| Assimilate | `ass` | Pre-planning/Research features and spikes |
| Nexus | `nxs` | redb, hnsw_rs, storage traits, embedding pipeline, schema |
| Collective | `col` | Orchestration & flow engine |
| Vinculum | `vnc` | MCP server & integration |
| Alcove | `alc` | Agent management & profiles |
| Cortical | `crt` | Learning & drift detection |
| Matrix | `mtx` | UI & dashboards |
| Designation | `dsn` | Project & identity management |
| Nanites | `nan` | Build, deploy, CI, tooling |



### Feature Directory Structure

```
product/features/{phase}-{NNN}/
├── SCOPE.md                    # Phase 1: agent-authored, human-approved
├── SCOPE-RISK-ASSESSMENT.md    # Phase 1b: scope-level risks (SR-XX)
├── specification/              # Phase 2: source document
│   └── SPECIFICATION.md
├── architecture/               # Phase 2: source document
│   ├── ARCHITECTURE.md
│   └── ADR-NNN-{name}.md      # Individual ADR files
├── RISK-TEST-STRATEGY.md       # Phase 2: source document (sacred)
├── ALIGNMENT-REPORT.md         # Phase 2: vision check
├── IMPLEMENTATION-BRIEF.md     # Phase 2: handoff to Session 2
├── ACCEPTANCE-MAP.md           # Phase 2: AC verification map
├── pseudocode/                 # Stage 3a: per-component pseudocode
│   ├── OVERVIEW.md
│   └── {component}.md
├── test-plan/                  # Stage 3a: per-component test plans
│   ├── OVERVIEW.md
│   └── {component}.md
├── testing/                    # Stage 3c: test execution output
│   └── RISK-COVERAGE-REPORT.md
├── reports/                    # Validation gate reports
│   ├── gate-3a-report.md
│   ├── gate-3b-report.md
│   └── gate-3c-report.md
└── agents/                     # Agent reports
    └── {agent-id}-report.md
```

### Implementation Tracking

Features and bugs tracked via **GitHub Issues**, not in-repo STATUS.md files.

- Implementation: `gh issue create --label "implementation,{phase}"`
- Bugs: `gh issue create --label "bug,{phase}"`
- Cross-reference: SCOPE.md `## Tracking` links to GH Issue; commits reference `(#NNN)`

---

## Testing Conventions

- **Test infrastructure is cumulative.** Each feature builds on the testing infrastructure established by prior features — shared fixtures, helpers, database setup patterns, and assertion utilities. Never create isolated test scaffolding when existing infrastructure can be extended.

---

## Behavioral Rules

- Be concise. Prefer short answers. Skip preamble, summaries, and repetition unless asked.
- Do what has been asked; nothing more, nothing less.
- NEVER create files unless absolutely necessary. Prefer editing existing files.
- NEVER proactively create documentation files unless explicitly requested.

---

## Unimatrix Integration

Unimatrix is the project's knowledge engine (MCP server). Agents query it for context and store reusable findings.

- **Briefing**: Agents call `context_briefing(role, task)` at task start for role-specific conventions, patterns, and prior decisions
- **Decisions**: Store ADRs via `context_store(category: "decision")` — architect is the authority
- **Patterns**: Store reusable patterns via `context_store(category: "pattern")` — any agent that discovers cross-feature patterns
- **Outcomes**: Record session outcomes via `context_store(category: "outcome")` — coordinators record at session end
- **Search**: Use `context_search` to find relevant prior decisions and patterns before designing
- **Lookup**: Use `context_lookup` for exact-match retrieval by topic, category, or tags

Do not store workflow choreography or protocol sequences in Unimatrix. Protocols stay as `.md` files in `.claude/protocols/`.
