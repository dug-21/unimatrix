# NDP — Non-Negotiable Rules

1. **Feature work uses swarms** — spawn `ndp-scrum-master` 
4. **Anti-stub**: Never leave TODO, `unimplemented!()`, `todo!()`, or placeholder functions. Ask the user if blocked.
5. **Never save files to root.** Use project directory structure.

---

## Project Context

**Unimatrix** — Self learning e2e development orchstrator for multi workstream coordination of agentic development teams


---

## Project Structure

```
/docs                    - Architecture docs and procedures
/product/features/       - SPARC documentation per feature
/.claude/agents/ndp      - NDP agent definitions
/.claude/protocols       - Swarm protocols (planning, implementation, routing)
/.claude/rules           - Contextual rules (testing, rust workspace)
```

---

## Feature Conventions

Features follow `{phase}-{NNN}` pattern in `product/features/`:

| Phase | Prefix | Focus |
|-------|--------|-------|
| Nexus | `nxs` | Fredb, hnsw_rs, storage traits, embedding pipeline, schema|
│ `col` │ Collective  │ Orchestration & flow engine │ Control injection, flow steps, wave management, agent spawning, gates │        
│ `vnc`    │ Vinculum    │ MCP server & integration        │ stdio transport, tool definitions, Claude Code integration, context compilation      │        
│ `alc`    │ Alcove │ Agent management & profiles     │ Agent registry, prompt assembly, context budgets, role definitions                   │
│ `crt`    │ Cortical    │ Learning & drift detection      │ Corrections, reflexion loop, pattern effectiveness, drift events, suggested controls │
│ `mtx`    │ Matrix      │ UI & dashboards │ Flow builder, control manager, retrospective dashboard, prompt debugger │
│ `dsn` │ Designation │ Project & identity management   │ Project registry, isolation, auto-detection, config, export/import │
│ `nan`    │ Nanites  │ Build, deploy, CI, tooling      │ Docker packaging, CLI, dev containers, release automation  │



### Feature Directory Structure (SPARC)

```
product/features/{phase}-{NNN}/
├── SCOPE.md                    # Human writes, agents never modify
├── IMPLEMENTATION-BRIEF.md     # Synthesizer output, implementation input
├── ALIGNMENT-REPORT.md         # Vision guardian output
├── ACCEPTANCE-MAP.md           # AC verification map
├── LAUNCH-PROMPT.md            # Implementation launch prompt
├── specification/              # SPARC S
├── pseudocode/                 # SPARC P
├── architecture/               # SPARC A
├── test-plan/                  # Test strategy + per-component plans
├── refinement/                 # SPARC R
├── completion/                 # SPARC C
├── agents/                     # Agent Reports
└── reports/
```

### Implementation Tracking

Features and bugs tracked via **GitHub Issues**, not in-repo STATUS.md files.

- Implementation: `gh issue create --label "implementation,{phase}"`
- Bugs: `gh issue create --label "bug,{phase}"`
- Cross-reference: SCOPE.md `## Tracking` links to GH Issue; commits reference `(#NNN)`

---

## Behavioral Rules

- Be concise. Prefer short answers. Skip preamble, summaries, and repetition unless asked.
- Do what has been asked; nothing more, nothing less.
- NEVER create files unless absolutely necessary. Prefer editing existing files.
- NEVER proactively create documentation files unless explicitly requested.
