# Track Synthesis: What We Know Heading Into Track 3

**Date**: 2026-02-20
**Purpose**: Consolidated synthesis of all D1-D6 deliverables from Tracks 1A/1B/1C and 2A/2B/2C
**Used By**: D7 (MCP Interface Specification)

---

## Backend Layer (Tracks 1A, 1B, 1C)

### hnsw_rs (D1) — Well-Suited, All Questions Answered

- **Filtering**: `search_filter` with `FilterT` closure does pre-filtering during traversal. We implement a closure that checks redb metadata. Single code path, no separate filtered-search flow needed.
- **Search returns**: `Neighbour { d_id, distance, p_id }`. Map directly to entry_id + similarity score (`1.0 - distance`). Always f32, pre-sorted by distance ascending.
- **Batch insert**: `parallel_insert` via rayon, takes `&self` (concurrent-safe). Single insert also safe.
- **Persistence**: Two-file dump (graph + data), NOT atomic. Requires redb for crash-safe metadata. Write-to-temp + rename for safety. Full snapshot every dump (no incremental).
- **Dimension**: Fixed by convention, NOT enforced. We must validate at our layer. Switching embedding models = full index rebuild.
- **Memory**: 384d at 100K = ~183 MB (fine). 1536d at 100K = ~633 MB (pushing it). 384d local model strongly preferred.
- **Distance**: Use `DistDot` (2-3x faster than `DistCosine` for pre-normalized vectors, has SIMD). Fixed at index creation, not user-configurable.
- **No deletion API**: Mark deprecated in redb, filter during search, periodic rebuild.

### redb (D2) — Perfect Complement, All Questions Answered

- **Range queries**: Yes, compound tuple keys `(u64, u64)` with efficient B-tree range scans. Timestamps work natively.
- **Layout**: Single DB file, multiple named tables, atomic cross-table updates in one write transaction.
- **Concurrency**: Single writer + unlimited readers, MVCC, serializable isolation. Readers never block writers. Ideal for MCP server (concurrent search + insert safe).
- **Structured metadata**: Use serde + bincode as `&[u8]` values for flexibility. Tuple keys for indexes. MultimapTable for tags.
- **Size limits**: Not a concern at our scale. Sub-ms reads and writes at 100K entries.
- **Async integration**: `tokio::task::spawn_blocking` with `Arc<Database>` (Iroh-proven pattern).
- **Table layout designed**: ENTRIES, TIME_INDEX, TAG_INDEX, STATUS_INDEX, PHASE_INDEX, VECTOR_MAP, COUNTERS.

### Learning Model (D3) — Metadata Lifecycle Wins Decisively

- **95% of learning value** from metadata: lifecycle state machine (proposed->validated->active->aging->deprecated), Wilson-score confidence formula, correction chains via `supersedes`/`superseded_by`, exponential time decay, dedup via similarity threshold.
- **~930 LoC** vs ~7,500 for sona-style ML. No new dependencies.
- **Interface is forward-compatible**: `confidence` is just a float. Can swap to ML computation later without interface changes.
- **The remaining 5%** (generalization, conflict detection) -> LLM-at-write-time (mem0/Zep pattern), Phase 2+.
- **Sona's code has significant gaps** between claimed and implemented features. The API shape (`store_pattern`/`find_patterns`) is useful as inspiration, but the ML components solve problems that don't exist for a metadata database.

---

## Frontend / Client Behavior Layer (Tracks 2A, 2B, 2C)

### MCP Protocol (D4) — Clear Implementation Path

- **Use `rmcp` 0.16 SDK**: Official, 1.14M downloads/month, Tokio-native, proc macros for tool definitions. Pin exact version.
- **stdio transport** for all initial versions. Streamable HTTP later.
- **Three MCP primitives**: Tools (primary -- model-controlled), Resources (supplementary -- application-driven, user must @-mention), Prompts (user-controlled slash commands).
- **Tool annotations** drive permission behavior: `readOnlyHint: true` for search/get (auto-approved), `destructiveHint: true` for delete (user confirmation). Part of the interface design, not an afterthought.
- **Server `instructions` field**: Direct behavioral injection, zero user config, 70-85% reliability for proactive search behavior. Massively underutilized in the ecosystem.
- **Response size limits**: 10K token warning, 25K truncation default. Keep search responses <2,000 tokens.
- **`structuredContent`**: Dual format (markdown for Claude + structured JSON for programmatic consumers). Design schemas for every tool.
- **Tool errors are guidance**: Use `isError: true` with actionable remediation text. Claude reads errors and self-corrects.

### Context Injection (D5/D5b/D5c) — The Deepest Findings

- **Tool results are user-role messages**, processed through standard transformer attention. No special channel.
- **"Lost in the middle" effect**: Front-load best results, end with guidance footer. Target <2,000 tokens per response.
- **Hybrid response format** works best: Markdown headers + structured metadata + blockquoted content + guidance footer.
- **Tool responses CAN include behavioral guidance** (grounded in returned data), but CANNOT override system prompt or mandate invocation timing.
- **CLAUDE.md + tool responses = complementary model**: CLAUDE.md drives behavior (when to search), tool responses provide context (what to apply).

### Three Hard Design Constraints (D5c)

1. **No hardcoded agent roles**: Roles are DATA, not code. No `match role { "architect" => ... }`. Any agent topology works without code changes.
2. **Deterministic vs. semantic retrieval**: Driven by presence of `query` parameter. No query = exact metadata match. Query present = vector similarity search.
3. **Generic query model**: `{ topic, category, query }` -- three fields that express anything. Not NDP-specific. Topic is freeform (role name, technology, feature ID). Category is freeform (duties, convention, protocol, pattern). Tags for cross-cutting access.

### Multi-Dimensional Context Model

Unimatrix isn't a flat memory search tool. It's a context assembly engine serving different context to different agents (WHO) at different workflow positions (WHERE) for different tasks (WHAT). The generic query model handles this without role-specific code.

### Config Surface (D6a/D6b/D6c) — Three-Tier Design

- **Tier 1 (zero config)**: `claude mcp add --scope user unimatrix`. Server instructions + tool descriptions alone = 70-85% reliability. Most users stop here.
- **Tier 2 (recommended)**: 5-line CLAUDE.md append pushes to ~90% reliability. `unimatrix init` generates this.
- **Tier 3 (multi-agent)**: Agent `mcpServers` field + orchestrator-passes-context pattern for subagent workflows.

### Critical Findings from Track 2C

- MCP server inheritance to custom subagents is BROKEN (5+ GitHub issues). User-scoped servers work; project-scoped don't.
- The orchestrator-passes-context pattern bypasses all inheritance bugs and should be the primary multi-agent integration path.
- Hooks CANNOT trigger MCP tool calls (instruction-driven only).
- Tool descriptions drive selection, NOT invocation timing.
- CLAUDE.md is never compacted (survives context compression).
- Background subagents have NO MCP access (by design).
- Subagent nesting limited to 1 level.

---

## Open Questions for D7

1. **One tool or two for retrieval?** Scenario analysis resolved: two tools (`context_lookup` + `context_search`).
2. **Does `context_briefing` belong in v0.1?** Scenario analysis resolved: v0.2 (optimization for orchestrator workflows).
3. **Tool naming**: Resolved to `context_*` prefix -- domain-neutral.
4. **Version scoping**: Resolved to 3-version model (v0.1 core, v0.2 lifecycle+multi-agent, v0.3 sophistication).
5. **Resources and Prompts**: Deferred to v0.3.
6. **The `instructions` field text**: Must drive both search AND store behavior.
7. **Structured output**: Design schemas for every tool from v0.1 (even if no consumer yet).
