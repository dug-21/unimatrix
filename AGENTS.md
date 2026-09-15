# Unimatrix - Non-Negotiable Rules

1. **Feature work uses delegated protocol leaders** - spawn a dedicated `uni-scrum-master` subagent to lead the session. The primary agent does not assume the SM role: it kicks off the SM, relays human checkpoints, resumes the same SM thread with the human's decision, and presents the final result. The SM reads the protocol and its agent definition, spawns specialists, manages gates, and does not generate worker artifacts itself.

   | Intent | Session Type | Protocol |
   |--------|--------------|----------|
   | Design, scope, spec | design | `.claude/protocols/uni/uni-design-protocol.md` |
   | Implement, build, code | delivery | `.claude/protocols/uni/uni-delivery-protocol.md` |
   | Bug fix | bugfix | `.claude/protocols/uni/uni-bugfix-protocol.md` |
   | Execute research spike(s) | research | `.claude/protocols/uni/uni-research-protocol.md` |

   **Session type selection rule**: If `product/features/{feature-id}/IMPLEMENTATION-BRIEF.md` does not exist, use **design** regardless of stated intent - delivery cannot proceed without it.

   **Research session rule**: Phase 1 (scope completion) happens interactively with the human (uni-zero session), not in a research session. A research session begins only when `SCOPE.md` is complete. Single spike: invoke `uni-spike-researcher` where available. Multiple dependent spikes: invoke `uni-research-sm` where available.

   Every delegated role must read its canonical definition at `.claude/agents/uni/{role}.md`. For Codex, map Claude `Task`/`Agent` calls to subagent spawning and `SendMessage` to follow-up on the same subagent thread. Read `.claude/agents/uni/uni-scrum-master.md` for the SM's role boundaries and escalation handshake. The protocol defines what to do and when; the agent definition defines how the role behaves.

   For PR review: `/uni-review-pr`. For retrospective: `/uni-retro`.
2. **Anti-stub**: Never leave TODO, `unimplemented!()`, `todo!()`, or placeholder functions. Ask if blocked.
3. **Never save files to root** unless the user explicitly requests a root-level project control file such as this one.

---

## Project Layout

```text
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
- Never create files unless necessary. Prefer editing existing files.
- Never proactively create documentation unless explicitly requested.
- Never store keys or secrets in code. Use `.env`.
- Always follow predefined protocols for work unless specifically directed not to.
- For Rust or Cargo work, read and follow `.claude/rules/rust-workspace.md` before acting; Claude applies this contextually, while Codex must load it explicitly.
- **Test infrastructure is cumulative** - extend existing fixtures and helpers, never create isolated scaffolding.
- **Search efficiently**: use `rg` for content search and `rg --files` for file discovery. Use shell commands for cargo, git, and other project tooling as needed.

---

## Unimatrix

Knowledge engine (MCP server). **Use it.**

- `/uni-store-adr` - after each architectural decision (Unimatrix-only, no ADR files)
- `/uni-record-outcome` - at the end of every session
- `/uni-store-procedure` - when a reusable technique evolves
- `/uni-store-lesson` - after failures and gate rejections

**Single item lookup** - use `context_get` to retrieve full details of any Unimatrix entry by ID. The `id` field is an **integer** - never quote it.

Format options:

```text
{null}              // Summarized content in text
{markdown}          // Full text in markdown
{json}              // Full text in json format
```

Example:

```json
mcp__unimatrix__context_get({
  "id": 3267,
  "feature": "{feature id}",
  "format": "markdown"
})
```

**MCP Parameter Format Rules** - applies to all Unimatrix tool calls:

| Parameter | Correct | Wrong |
|-----------|---------|-------|
| `id`, `original_id` | `3267` (integer) | `"3267"` (string) |
| `tags` | `["adr", "col-031"]` (JSON array) | `"adr, col-031"` or `"[\"adr\"]"` |
| `agent_id` | `"uni-rust-dev"` (bare agent type) | `"col-031-rust-dev"`, feature-suffixed values, or omitted |
| String content | `"content here"` | `"content \"quoted\" here"` - avoid escaped quotes inside strings |

**`agent_id` is always the bare agent type name** (`uni-scrum-master`, `uni-rust-dev`, `uni-architect`, etc.), never feature-suffixed and never omitted. Feature identity belongs in `topic`, `feature_cycle`, or `tags`.

If Unimatrix MCP tools are unavailable in the current Codex host, report that capability gap immediately. Do not fabricate calls or silently substitute file-based ADRs or outcomes.

Do not store workflow choreography in Unimatrix. Protocols live in `.claude/protocols/uni/`.
