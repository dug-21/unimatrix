# Briefing Evolution: From Agent Tool to Hook Backend

**Date**: 2026-03-03
**Context**: Issue #80 (context_briefing deprecation analysis), col-011 knowledge architecture
**Depends on**: [server-refactoring-architecture.md](server-refactoring-architecture.md)

---

## Situation

`context_briefing` (MCP tool, 223 lines in tools.rs) is effectively dead:

- **Disabled in all specialist agents** since creation — "consumes too much subagent context window"
- **Not called by coordinators** — workflow baked into specialized agent defs
- **Replaced by hook injection** for primary agents — UserPromptSubmit injects 2-3 entries per message automatically
- **Replaced by /query-patterns** for specialists — targeted search by component/domain, not role

Meanwhile, the hook system has evolved to provide the *actual* briefing mechanism through two paths:
1. **UserPromptSubmit → ContextSearch** — per-message semantic injection (~1400 bytes)
2. **PreCompact → CompactPayload** — compaction defense with session injection history (~8000 bytes)

The question from #80: which option (A: deprecate, B: slim to conventions-only, C: repurpose as hook backend)?

---

## Analysis: Option C Is Architecturally Correct

### Why Not Option A (Deprecate entirely)

Option A removes the MCP tool but doesn't address the backend redundancy. The hook system still does its own search orchestration (`handle_context_search`, 228 lines) and its own briefing assembly (`handle_compact_payload`, 266 lines). Deprecating the MCP tool leaves two UDS-only implementations with no shared backend — the duplication shifts from (MCP+UDS) to (UDS search + UDS compact) with no unification.

Additionally, deprecation creates a gap: there's no programmatic way to request briefing content. `/query-patterns` is a skill (prompt template), not an API. If a future integration (IDE plugin, remote agent, API consumer) needs briefing, there's no endpoint.

### Why Not Option B (Slim to conventions-only)

Option B reduces the tool to a metadata query: "give me conventions for role X." This is a specialized `context_lookup` call with `category="convention", topic={role}`. Creating a dedicated tool for a single lookup query is over-engineering. `/query-patterns` already searches conventions. The 400-token output provides minimal value over what the hook already injects.

### Why Option C (Repurpose as hook backend)

Option C aligns with the service extraction proposed in [server-refactoring-architecture.md](server-refactoring-architecture.md). The key insight:

**The hook system already IS the briefing system.** `context_briefing` and `CompactPayload` both assemble knowledge entries into a budget-constrained text payload. They differ only in:
1. Entry source (metadata query vs session injection history)
2. Delivery mechanism (MCP response vs hook stdout)
3. Trigger (agent-initiated vs automatic)

Formalizing this means the "briefing backend" is the **BriefingService** from the service layer proposal, callable by both MCP (for programmatic access) and UDS (for hook delivery).

---

## Concrete Proposal: Briefing Service + Hook-Native Delivery

### BriefingService (transport-agnostic)

```
BriefingService::assemble(params: BriefingParams) -> BriefingResult

struct BriefingParams {
    // What to search for
    query: Option<String>,               // semantic search query (task description)
    feature: Option<String>,             // feature tag for boost

    // Entry source controls
    include_conventions: bool,            // query conventions by role/topic
    role: Option<String>,                 // role filter for conventions
    injection_history: Option<Vec<InjectionRecord>>,  // session entries

    // Budget
    max_tokens: usize,
}

struct BriefingResult {
    sections: Vec<BriefingSection>,
    entry_ids: Vec<u64>,
    total_tokens: u32,
}
```

**Duties section removed** (per col-011 decision — agent defs are sole authority).

### Hook Delivery Paths

| Hook Event | What BriefingService receives | Purpose |
|-----------|------------------------------|---------|
| **UserPromptSubmit** | `query=prompt, include_conventions=false` | Per-message semantic injection (current ContextSearch behavior) |
| **PreCompact** | `injection_history=session.history, include_conventions=true` | Compaction defense (current CompactPayload behavior) |
| **SessionRegister** (new) | `role=agent_role, include_conventions=true, query=None` | One-time role conventions injection at session start |

The `SessionRegister` path is new and fills a real gap: currently, agents get NO conventions on session start. They only get context if/when the user types a prompt (UserPromptSubmit). A lightweight conventions injection on SessionRegister gives agents their project norms upfront.

### MCP Tool Evolution

Two options for the MCP endpoint:

**Option C1: Keep as `context_briefing` with simplified params**
- Remove duties lookup
- Focus on conventions + semantic search
- Primarily for programmatic/API access, not agent workflows
- Keep backward compatible

**Option C2: Merge into `context_search` with a `mode` parameter**
- `context_search(mode="search")` → current search behavior
- `context_search(mode="briefing")` → briefing assembly behavior
- Reduces tool count, single endpoint for knowledge retrieval

*Recommendation*: **C1** — keep separate tools. They serve different intents (find specific knowledge vs assemble orientation). Merging conflates the APIs.

---

## What Changes About context_briefing

### Remove
- **Duties section** — agent defs are sole authority (col-011 decision)
- **Duties lookup** (lines 1721-1732 in tools.rs) — dead code after duties eliminated
- **Duties budget allocation** (lines 1840-1847) — dead code

### Keep
- **Conventions lookup** by role (lines 1709-1719) — still useful for role-based project norms
- **Semantic search** on task (lines 1737-1826) — core value of briefing
- **Feature boost** (lines 1772-1783) — feature-scoped relevance
- **Co-access boost** (lines 1786-1818) — quality signal
- **Token budget allocation** (lines 1828-1856, simplified without duties) — budget control

### Evolve
- **Entry sources become configurable** — not just metadata+semantic, also injection history
- **Backend becomes BriefingService** — both MCP and UDS call same service
- **Response can be text or structured** — hook needs text, MCP needs structured

---

## Overlap Resolution: Hook Architecture + Knowledge Architecture

Issue #80 identifies the overlap between hook-driven context delivery and knowledge architecture. Here's how the service extraction resolves it:

### Before (current)

```
Hook ContextSearch:     embed → vector search → rerank → inject (uds_listener.rs)
Hook CompactPayload:    session history → fetch entries → format (uds_listener.rs)
MCP context_briefing:   metadata query → semantic search → format (tools.rs)
MCP context_search:     embed → vector search → rerank → format (tools.rs)
Skill /query-patterns:  prompt template → context_search (skill file)
```

Five distinct codepaths, three duplicating search logic, two duplicating briefing logic.

### After (proposed)

```
SearchService:          embed → vector search → rerank → return scored entries
BriefingService:        source entries → budget allocation → assembly

Hook ContextSearch:     SearchService → inject + injection log
Hook CompactPayload:    BriefingService(injection_history) → inject
Hook SessionRegister:   BriefingService(conventions) → inject (new)
MCP context_briefing:   identity → BriefingService(conventions+semantic) → format
MCP context_search:     identity → SearchService → format + audit + usage
Skill /query-patterns:  prompt template → context_search (unchanged)
```

Two services, five thin transport wrappers. Each wrapper adds its transport-specific concerns (identity, injection logging, formatting) around the shared service.

### What About /query-patterns?

`/query-patterns` is a skill (prompt template) that calls `context_search` with category/tag filters. It's the right UX for agents — "search for patterns about X before implementing." It's complementary to the briefing service, not competing:

- **Briefing**: "Orient me for this role/task" — proactive, broad, budget-constrained
- **Search / query-patterns**: "Find specific patterns about X" — reactive, targeted, no budget

Both should use SearchService as their backend. No change needed to /query-patterns.

---

## Implementation Sequence (integrated with server refactoring)

This slots into the Wave 1-2 sequence from [server-refactoring-architecture.md](server-refactoring-architecture.md):

### Wave 1: SearchService extraction
1. Extract search/rank/boost from tools.rs and uds_listener.rs into `services/search.rs`
2. Both transports delegate to SearchService
3. **Behavioral change: none** (exact clone of existing logic)

### Wave 2: BriefingService extraction + duties removal
4. Remove duties lookup from context_briefing
5. Extract briefing assembly into `services/briefing.rs`
6. Wire CompactPayload to BriefingService
7. Wire context_briefing to BriefingService
8. **Behavioral change: duties section removed** (per col-011)

### Wave 2b: UDS-native briefing
9. Wire HookRequest::Briefing to BriefingService (conventions + semantic)
10. Hook builds Briefing request from SessionRegister event
11. **New feature: agents get conventions on session start**

### Wave 2c: Clean up
12. Deprecate/quarantine remaining duty entries in Unimatrix
13. Remove duties from category allowlist (or deprecate entire category)
14. Update briefing-related ADRs

---

## Token Budget Considerations

The briefing delivery paths serve different budget contexts:

| Path | Budget | Rationale |
|------|--------|-----------|
| UserPromptSubmit injection | ~350 tokens (1400 bytes) | Fits in per-message margin, repeated every prompt |
| SessionRegister briefing | ~750 tokens (3000 bytes) | One-time cost at session start, more room |
| PreCompact payload | ~2000 tokens (8000 bytes) | Compaction defense, replaces lost context |
| MCP context_briefing | Configurable (default 3000 tokens) | Agent-initiated, can request what they need |

BriefingService accepts `max_tokens` and allocates across sections. The hook paths set fixed budgets per their delivery context. The MCP path passes through the agent's requested budget.

---

## Recommendation Summary

1. **Pursue Option C** from #80 — repurpose briefing as hook backend via BriefingService
2. **Remove duties** immediately (col-011 decision, no debate)
3. **Extract SearchService first** (Wave 1) — enables BriefingService and fixes duplication
4. **Extract BriefingService second** (Wave 2) — unifies context_briefing + CompactPayload
5. **Enable SessionRegister briefing** (Wave 2b) — fills the "no conventions at start" gap
6. **Keep context_briefing MCP tool** with simplified params — serves programmatic access
7. **No changes to /query-patterns** — complementary, not competing
