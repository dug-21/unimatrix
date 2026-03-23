# Unimatrix — Non-Negotiable Rules

1. **Feature work uses swarms** — read the protocol for the session type and execute it as Design/Delivery/Bugfix Leader. You ARE the scrum master. Follow the protocol exactly — spawn specialist agents, never generate content yourself.
   | Intent | Session Type | Protocol |
   |--------|-------------|----------|
   | Design, scope, spec | design | `.claude/protocols/uni/uni-design-protocol.md` |
   | Implement, build, code | delivery | `.claude/protocols/uni/uni-delivery-protocol.md` |
   | Bug fix | bugfix | `.claude/protocols/uni/uni-bugfix-protocol.md` |

   **Session type selection rule**: If `product/features/{feature-id}/IMPLEMENTATION-BRIEF.md` does not exist, use **design** regardless of stated intent — delivery cannot proceed without it.

   Read the SM agent definition (`.claude/agents/uni/uni-scrum-master.md`) for role boundaries and behavioral rules. The protocol defines what to do and when; the SM definition defines how you behave.

   For PR review: `/uni-review-pr`. For retrospective: `/uni-retro`.
2. **Anti-stub**: Never leave TODO, `unimplemented!()`, `todo!()`, or placeholder functions. Ask if blocked.
3. **Never save files to root.** Use project directory structure.

---

## Project Layout

```
/crates/unimatrix-{store,vector,embed,core,server}/  - Rust workspace
/product/features/{phase}-{NNN}/                      - Feature docs per feature
/product/research/{ASS}-{NNN}/                        - Research spikes
/.claude/agents/uni/                                  - Agent definitions (active)
/.claude/protocols/uni/                               - Workflow protocols (design, delivery, bugfix)
/.claude/skills/                                      - Skills (/uni-review-pr, /uni-retro, /uni-git, etc.)
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
- **Search tools, not Bash**: Use `Grep` for content search and `Glob` for file discovery — never `grep`, `rg`, `find`, or `ls` via Bash. Reserve Bash for commands with no dedicated tool (cargo, git, etc.).

---

## Unimatrix

Knowledge engine (MCP server). **Use it.**

- `/uni-query-patterns` — before designing or implementing, check what exists
- `/uni-store-adr` — after each architectural decision (Unimatrix-only, no ADR files)
- `/uni-record-outcome` — at the end of every session
- `/uni-store-procedure` — when a reusable technique evolves
- `/uni-store-lesson` — after failures and gate rejections

Do not store workflow choreography in Unimatrix. Protocols live in `.claude/protocols/uni/`.
