# Unimatrix Feature Roadmap: v0.1 → v0.5

**Date**: 2026-02-19
**Goal**: Deliver usable value to a real project as early as possible. Gain experience. Let the system evolve from actual use, not theory.
**Principle**: Every release must be usable. No "infrastructure only" releases.

---

## The Fast Path to Value

The full Unimatrix vision spans multi-project orchestration, agent teams, trust calibration, and cross-project learning. But none of that matters until the foundation works and we've used it enough to know what's actually needed vs. what sounds good on paper.

**Target**: By v0.5.0, Unimatrix is running as an MCP server on a real project, providing context-aware knowledge to Claude Code that measurably improves agent output. Everything before v0.5 builds toward that.

---

## v0.1.0 — "It Remembers"

**Value delivered**: Claude Code can store and retrieve development knowledge across sessions. No more re-explaining project conventions, architecture decisions, or past solutions.

**What ships**:

| Feature | Description |
|---|---|
| MCP server (stdio) | Rust binary, runs as Claude Code MCP server via `claude mcp add` |
| `memory_store` | Store text with metadata (tags, category, source file) |
| `memory_search` | Semantic similarity search — find relevant past knowledge |
| `memory_get` / `memory_list` | Retrieve by ID, list with tag filters |
| `memory_delete` | Remove entries |
| Embedding via API | OpenAI `text-embedding-3-small` for v0.1 (API key required) |
| Persistent storage | hnsw_rs + redb — survives restarts, single file per project |

**What it looks like in practice**:
```
You: "Store this: Our API uses axum with tower middleware. Auth is JWT via jsonwebtoken crate.
      Error responses follow RFC 7807 Problem Details format."

[...later session...]

You: "How do we handle errors in this project?"
Agent: [searches memory, finds RFC 7807 pattern, applies it correctly]
```

**Not included**: Project isolation, phase awareness, learning, corrections. Just store and search.

**Exit criteria**:
- Can store 1,000 entries and search returns relevant results in <50ms
- Knowledge persists across Claude Code sessions
- Binary builds and runs on linux/amd64 and arm64

---

## v0.2.0 — "It Knows Which Project"

**Value delivered**: Run Unimatrix across 2+ projects without cross-contamination. Each project has its own conventions, patterns, and knowledge.

**What ships**:

| Feature | Description |
|---|---|
| Project registry | TOML-based project config. `project_create`, `project_list`, `project_switch` |
| Physical isolation | Separate .redb + .hnsw files per project under `~/.unimatrix/projects/` |
| Auto-detection | Detect active project from cwd / git remote (configurable) |
| `memory_count` | Stats per project — how many entries, by category |
| Local embeddings | `ort` + `all-MiniLM-L6-v2` — no API key required, works offline |

**What it looks like in practice**:
```
# In project-alpha (Rust/Axum API)
Agent uses project-alpha conventions: axum handlers, sqlx queries, RFC 7807 errors

# In project-beta (React/TypeScript frontend)
Agent uses project-beta conventions: React hooks, Zod validation, Tailwind patterns

# Zero leakage between them
```

**Exit criteria**:
- 2 projects running simultaneously with zero cross-contamination
- Auto-detection correctly identifies project from working directory
- Local embeddings work without network access
- Embedding cache hits >60% in typical dev workflow

---

## v0.3.0 — "It Knows What Phase I'm In"

**Value delivered**: Context delivery adapts based on what you're doing. Architecture review gets different knowledge than coding. Testing gets different knowledge than deployment.

**What ships**:

| Feature | Description |
|---|---|
| Phase metadata | Entries tagged with applicable phases: `architecture`, `coding`, `testing`, `deployment`, `debugging` |
| Phase-filtered search | `memory_search` accepts `phase` parameter — results filtered to phase-relevant entries |
| Context policies (YAML) | Per-project policy files defining what knowledge categories apply to each phase |
| Token budget | `memory_search` accepts `max_tokens` — returns entries until budget exhausted, ranked by relevance |
| Bulk import | `memory_import` — load knowledge from markdown files, YAML, or JSONL |

**What it looks like in practice**:
```yaml
# ~/.unimatrix/projects/my-api/context-policy.yaml
phases:
  architecture:
    include: [architecture_decisions, api_contracts, data_models, system_constraints]
    exclude: [test_fixtures, ci_config, deployment_scripts]
    token_budget: 8000
  coding:
    include: [coding_conventions, api_contracts, error_patterns, examples]
    exclude: [architecture_rationale, capacity_planning]
    token_budget: 12000
  testing:
    include: [test_strategy, test_patterns, fixtures, coverage_requirements]
    exclude: [deployment_scripts, architecture_rationale]
    token_budget: 10000
```

**Exit criteria**:
- Phase-filtered search returns demonstrably more relevant results than unfiltered
- Token budget enforcement prevents context overload
- Bulk import works for bootstrapping a project's knowledge from existing docs

---

## v0.4.0 — "It Learns From Corrections"

**Value delivered**: When you correct the agent, that correction becomes permanent knowledge. The same mistake doesn't happen twice.

**What ships**:

| Feature | Description |
|---|---|
| `memory_correct` | Store a correction: what the agent did wrong + what was right. Links to original context. |
| Correction retrieval | Corrections surface automatically when similar context is retrieved — "last time this pattern was used incorrectly, here's what was wrong" |
| `memory_promote` | Manually promote a correction or pattern to long-term knowledge |
| `memory_deprecate` | Mark knowledge as outdated — still retrievable but flagged, excluded from default search |
| Knowledge status | Entries have lifecycle status: `active`, `aging`, `deprecated`. Aging after N days without use. |
| Usage tracking | Track when entries are retrieved and whether the task succeeded or agent was corrected |

**What it looks like in practice**:
```
Session 1:
  Agent: [generates error handler without RFC 7807 format]
  You: "Wrong. We use RFC 7807. Here's the correct pattern: ..."
  → Correction stored automatically

Session 2:
  Agent: [retrieves error handling context, sees correction attached]
  Agent: [generates correct RFC 7807 error handler]
  → No re-correction needed
```

**Exit criteria**:
- Corrections surface when relevant context is retrieved
- Deprecated knowledge excluded from default search results
- Usage tracking captures retrieve→outcome signal
- At least 5 corrections from real use have been captured and prevent repeat mistakes

---

## v0.5.0 — "It's Actually Useful on a Real Project"

**Value delivered**: Full foundational loop running on a real project. Store knowledge → retrieve phase-aware context → learn from corrections → knowledge evolves. The system provides measurable value compared to working without it.

**What ships**:

| Feature | Description |
|---|---|
| AGENTS.md generation | Auto-generate AGENTS.md / CLAUDE.md content from stored knowledge — bridge to existing Claude Code patterns |
| Context compilation | `context_compile` tool — given a task description + phase, assemble the optimal context package within token budget |
| Session summary | On session end, extract key learnings and propose new knowledge entries for review |
| Quality dashboard | CLI command: `unimatrix status` — entries per project, corrections count, staleness, retrieval hit rate |
| Export/Import | JSONL export for git-tracking knowledge alongside code. Import for bootstrapping new projects from existing knowledge. |
| Docker packaging | Single `docker-compose.yml` — Unimatrix server ready to run in any dev container |

**What it looks like in practice**:
```bash
$ unimatrix status
Project: my-api
  Knowledge entries: 247 (198 active, 31 aging, 18 deprecated)
  Corrections: 23 (19 resolved → permanent knowledge)
  Phases configured: architecture, coding, testing, deployment
  Avg retrieval relevance: 0.82
  Token savings vs. full context: ~65%
  Last session: 12 retrievals, 0 corrections needed
```

**Exit criteria**:
- Running on 1 real project for 2+ weeks
- Measurable: fewer repeated corrections over time (tracked)
- Measurable: token usage per request lower than without Unimatrix (tracked)
- Context compilation produces results the user judges as "right context, right time" >80% of sessions
- Knowledge can be exported, committed to git, and imported into a fresh instance

---

## What v0.5 Proves

If v0.5 works:
- The embedded vector store (hnsw_rs + redb) is sufficient for single-project knowledge
- Phase-aware context delivery is better than "load everything"
- Corrections → permanent knowledge is a viable learning loop
- The MCP interface is the right integration point with Claude Code
- We have real usage data to inform multi-project orchestration (v0.6+)

If v0.5 doesn't work:
- We learn exactly what's wrong from actual use, not speculation
- Every component is small enough to replace without rewriting everything
- The `VectorStore` trait lets us swap to Qdrant if embedded storage is insufficient
- We have concrete evidence for what to change, not theoretical concerns

---

## What Comes After (v0.6+, Informed by Experience)

These are NOT committed. They're the vision, gated by what we learn from v0.1–v0.5:

| Version | Theme | Gate |
|---|---|---|
| v0.6 | Multi-project with shared global knowledge | v0.5 running on 2+ projects |
| v0.7 | Agent orchestration — pipeline engine + human gates | v0.5 proving MCP integration model |
| v0.8 | Trust calibration — progressive autonomy | v0.7 proving gate model works |
| v0.9 | Cross-project learning — pattern promotion pipeline | v0.6 showing knowledge isolation works |
| v1.0 | Production-ready — hardened, documented, dockerized | All prior versions stable |

---

## Rough Timeline

Not committing to dates. Committing to sequence and exit criteria.

| Version | Estimated Effort | Depends On |
|---|---|---|
| v0.1.0 | 1-2 weeks | Architecture decisions approved |
| v0.2.0 | 1 week | v0.1 working on real project |
| v0.3.0 | 1-2 weeks | v0.2 running on 2 projects |
| v0.4.0 | 1-2 weeks | v0.3 with real phase policies |
| v0.5.0 | 1 week | v0.4 with real corrections captured |

**Total to v0.5**: ~5-8 weeks of focused work, with real usage between each version.

---

## Technical Foundation (All Versions)

| Component | Technology | Decided |
|---|---|---|
| Language | Rust | Yes |
| Async runtime | Tokio | Yes |
| Vector index | hnsw_rs (direct dep) | Yes |
| Persistent storage | redb (direct dep) | Yes |
| Embeddings (v0.1) | OpenAI API | Yes |
| Embeddings (v0.2+) | ort + all-MiniLM-L6-v2 (local) | Yes |
| Integration | MCP server (stdio transport) | Yes |
| Serialization | serde + bincode (vectors) + serde_json (metadata) | Yes |
| CLI | clap | Yes |
| Logging | tracing | Yes |
