# Unimatrix — Non-Negotiable Rules

1. **Feature work uses swarms** — spawn `uni-scrum-master` with the session type. The SM reads the protocol and executes it.
   | Intent | Session Type | Protocol |
   |--------|-------------|----------|
   | Design, scope, spec | design | `.claude/protocols/uni/uni-design-protocol.md` |
   | Implement, build, code | delivery | `.claude/protocols/uni/uni-delivery-protocol.md` |
   | Bug fix | bugfix | `.claude/protocols/uni/uni-bugfix-protocol.md` |

   For PR review: `/review-pr`. For retrospective: `/retro`.
2. **Anti-stub**: Never leave TODO, `unimplemented!()`, `todo!()`, or placeholder functions. Ask if blocked.
3. **Never save files to root.** Use project directory structure.

---

## Project Layout

```
/crates/unimatrix-{store,vector,embed,core,server}/  - Rust workspace
/product/features/{phase}-{NNN}/                      - Feature docs per feature
/.claude/agents/uni/                                  - Agent definitions (active)
/.claude/protocols/uni/                               - Workflow protocols (design, delivery, bugfix)
/.claude/skills/                                      - Skills (/review-pr, /retro, /uni-git, etc.)
/.claude/rules/                                       - Contextual rules
```

Features use `{phase}-{NNN}` naming. Track via **GitHub Issues**; commits reference `(#NNN)`.

| Phase | Prefix | Focus |
|-------|--------|-------|
| Assimilate | `ass` | Research spikes |
| Nexus | `nxs` | Storage, vectors, embedding, schema |
| Collective | `col` | Orchestration & flow |
| Vinculum | `vnc` | MCP server |
| Alcove | `alc` | Agent management |
| Cortical | `crt` | Learning & drift |
| Matrix | `mtx` | UI & dashboards |
| Designation | `dsn` | Project identity |
| Nanoprobes | `nan` | Build, deploy, CI |

---

## Behavioral Rules

- Be concise. Skip preamble, summaries, repetition.
- Do what was asked; nothing more, nothing less.
- NEVER create files unless necessary. Prefer editing existing files.
- NEVER proactively create documentation unless explicitly requested.
- NEVER store keys/secrets in code. Always in .env
- **Test infrastructure is cumulative** — extend existing fixtures and helpers, never create isolated scaffolding.

---

## Unimatrix

Knowledge engine (MCP server). **Use it.**

- `/query-patterns` — before designing or implementing, check what exists
- `/store-adr` — after each architectural decision (Unimatrix-only, no ADR files)
- `/record-outcome` — at the end of every session
- `/store-procedure` — when a reusable technique evolves
- `/store-lesson` — after failures and gate rejections

Do not store workflow choreography in Unimatrix. Protocols live in `.claude/protocols/uni/`.
