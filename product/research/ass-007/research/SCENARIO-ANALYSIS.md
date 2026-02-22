# Scenario-Driven Design Analysis

**Date**: 2026-02-20
**Purpose**: Explore how different starting assumptions drive different interface and backend designs. Pressure-test assumptions before committing to D7.
**Used By**: D7 (MCP Interface Specification)

---

## Scenario 1: Multiple Query Types, Different Timing, Mixed Determinism

**Assumption**: Agents need fundamentally different retrieval modes at different moments. A rust-dev checking "what's the error handling convention?" needs a deterministic, instant answer -- same every time. That same dev asking "what patterns exist for auth middleware?" needs semantic similarity search. A scrum-master checking "what's the wave 2 protocol?" needs deterministic. An architect exploring "what prior decisions constrain this design?" needs semantic.

**The timing dimension is key**: Deterministic queries happen at task-start (orient me), at decision-points (what's the rule?), and at validation (did I comply?). Semantic queries happen during creative work (what's been tried before?), during problem-solving (has anyone solved this?), and during exploration (what do we know about X?).

### Interface Implications

**Two distinct tools, not one**. The presence/absence of `query` as a mode switch is elegant but creates ambiguity. When Claude calls `context(topic: "error-handling", category: "convention")`, is it a deterministic lookup or did Claude just forget to include a query? Two tools make the intent unambiguous in conversation logs and give Claude two distinct description-driven decision paths.

```
context_lookup -- exact metadata match. Deterministic. Same query -> same results.
  params: topic?, category?, tags?, id?, status?, limit?

context_search -- semantic similarity. Ranked results. May vary over time.
  params: query (required), topic?, category?, tags?, k?, max_tokens?
```

The tool descriptions become the prompt engineering surface. `context_lookup` description says "Use for known information -- conventions, duties, protocols, rules. NOT for discovery." `context_search` description says "Use for discovery -- finding patterns, exploring related decisions, checking what's known. NOT for looking up specific definitions."

**But**: The `context_store` tool is unified. You don't store "deterministically" vs. "semantically" -- you store once, with metadata, and the entry is retrievable both ways.

### Backend Implications

**Two retrieval paths, shared storage**:

```
context_lookup:
  1. Build redb filter from topic + category + tags
  2. Range scan on appropriate index table(s)
  3. Return entries ordered by priority/date
  4. No embedding, no hnsw_rs involved
  5. O(log n) per index scan -- sub-ms at any reasonable scale

context_search:
  1. Embed the query via embedding model
  2. Optionally build redb filter from topic/category/tags
  3. If filter: collect matching entry IDs -> build FilterT Vec<usize>
  4. hnsw_rs search_filter(embedding, k, ef, filter)
  5. For each result: fetch metadata from redb by d_id
  6. Assemble response with similarity scores
  7. O(log n) for hnsw search + O(m) for filter build where m = filtered set size
```

**Table design reinforced**: The existing table layout (ENTRIES, TIME_INDEX, TAG_INDEX, STATUS_INDEX, PHASE_INDEX) already supports this. But the generic model suggests simplifying:

```
ENTRIES:        entry_id -> serialized metadata + content (bincode)
TOPIC_INDEX:    (topic_hash, entry_id) -> ()        // fast topic lookup
CATEGORY_INDEX: (category_hash, entry_id) -> ()     // fast category lookup
TAG_INDEX:      tag_string -> entry_id (multimap)   // cross-cutting
TIME_INDEX:     (created_at, entry_id) -> ()        // temporal ordering
STATUS_INDEX:   (status_u8, entry_id) -> ()         // lifecycle filtering
VECTOR_MAP:     entry_id -> hnsw_data_id            // bridge to vector index
COUNTERS:       "next_entry_id" -> u64
```

Topic and category get their own index tables because they're the primary axes for deterministic lookup. The original design had PHASE_INDEX -- but with the generic model, "phase" is just a topic or tag, not a special field.

### What This Scenario Settles

- Two retrieval tools, not one
- `context_store` is a single tool (unified storage, dual retrieval)
- Backend has two code paths sharing one data store
- hnsw_rs is only invoked for semantic search; deterministic lookup skips it entirely
- Index table design driven by the generic `{ topic, category, tags }` model

---

## Scenario 2: The Solo Developer, Day One

**Assumption**: A single developer discovers Unimatrix, runs `claude mcp add`, and starts working. They have no seed data, no agent definitions, no multi-agent workflow. They just want Claude to remember things across sessions and get smarter about their project over time.

This is the **acquisition scenario** -- the first experience that determines whether Unimatrix gets adopted or abandoned.

### Interface Implications

**The cold-start problem is acute**. Every query returns nothing. If Claude calls `context_search("auth patterns")` and gets "No results found", the user sees Unimatrix failing, not an empty database.

**Solution**: The empty-result response becomes onboarding guidance:

```
No entries found matching "auth patterns".

This project's Unimatrix memory is empty. As you work, store conventions,
decisions, and patterns using the context_store tool. Future searches
will return relevant knowledge.

Tip: After making an architectural decision, store it with
category "decision" and relevant tags.
```

**But there's a deeper problem**: Who stores the first entries? If the user must explicitly say "remember this", adoption fails. The server `instructions` field needs to drive proactive storage, not just proactive search:

```
Unimatrix is the project's context engine. Before starting implementation
or design work, search for relevant patterns. After making decisions,
discovering patterns, or establishing conventions, store them for future
reference. When corrected by the user, record the correction.
```

**The tool set for Day One must be minimal**:
- `context_search` -- semantic search (returns empty with guidance initially)
- `context_store` -- store knowledge (the workhorse for building up the knowledge base)
- `context_lookup` -- deterministic lookup (useful once there's data)

Everything else (`context_briefing`, lifecycle tools, workflow state) is noise for a solo dev.

### Backend Implications

**Embedding on first store creates a latency spike**. The first `context_store` call triggers the embedding pipeline. If using OpenAI API, that's a network round-trip. If using local model (`all-MiniLM-L6-v2` via `ort`), that requires model loading.

**Cold-start optimization path**:
- First store: lazy-load embedding model, embed, insert into hnsw_rs (creates index)
- Subsequent stores: model already loaded, fast
- If using OpenAI initially (v0.1): accept the latency; local model in v0.2 eliminates it

**The hnsw_rs index starts empty**. First `search` after first `insert` requires `set_searching_mode(true)` toggle. This is a one-time `&mut self` call -- needs the `RwLock<Hnsw>` wrapper.

**Implication for persistence**: With few entries, dump/reload is trivial. But the first session creates the database file, the hnsw_rs dump files, and establishes the project. `context_store` on an uninitialized project should auto-initialize -- don't require a separate `project_create` or `init` tool call.

### What This Scenario Settles

- Auto-initialization: first `context_store` creates the project transparently
- Empty-result responses must be onboarding, not errors
- Server `instructions` must drive both search AND store behavior
- The core tool set for v0.1 is just 3 tools: `context_search`, `context_store`, `context_lookup`
- Embedding model lazy-loading on first store
- No mandatory `init` step in the tool interface (the CLI `unimatrix init` is for CLAUDE.md config, not project creation)

---

## Scenario 3: The Multi-Agent Swarm (NDP-Scale Orchestration)

**Assumption**: 17 agent types, multi-wave workflows, orchestrator spawning subagents with different context needs. The scrum-master needs protocols, the architect needs ADRs, the rust-dev needs conventions, the tester needs acceptance criteria. Each agent is a fresh context window with limited turns.

### Interface Implications

**Subagents are context-constrained and turn-constrained**. A subagent spawned with `max_turns: 50` can't afford to spend 5 turns on Unimatrix queries before starting work. The `context_briefing` compound tool becomes essential here -- one call instead of 4-5.

**But D5c's Constraint 1 (no hardcoded roles) means `context_briefing` can't have role-specific logic**. It's a generic composition:

```
context_briefing(role, task, phase?, feature?):
  duties     = lookup(topic: role, category: "duties")
  protocol   = lookup(topic: phase, category: "protocol")  [if phase given]
  conventions = lookup(topic: role, category: "rules")
  patterns   = search(query: task, topic?: role, k: 3)

  return assembled_response(duties, protocol, conventions, patterns)
```

The assembly is generic: concatenate non-empty sections with headers. The content comes from data, not code.

**However**: This scenario reveals that `context_briefing` is really an optimization for the **orchestrator-passes-context pattern**. If the scrum-master calls `context_briefing(role: "rust-dev", task: "implement auth middleware", phase: "implementation")`, it gets a compiled briefing to paste into the spawn prompt. The subagent never calls Unimatrix at all.

**This changes who the primary Unimatrix consumer is**:
- Solo dev: Claude directly (Scenario 2)
- Multi-agent: The orchestrator, not the leaf agents

### Backend Implications

**Burst read patterns**: When the scrum-master spawns 4 agents in parallel, it makes ~4 `context_briefing` calls in quick succession (or 12-20 individual lookups). All are reads. redb's MVCC handles this perfectly -- unlimited concurrent readers, all see a consistent snapshot.

**Cross-agent write coordination**: When the architect stores an ADR, and minutes later the rust-dev needs that ADR, the write must be committed and visible. redb's `Durability::Immediate` (fsync on commit) guarantees this. No eventual consistency concerns.

**Knowledge routing emerges naturally**: The architect stores `(topic: "auth", category: "decision", content: "JWT with RS256...")`. The rust-dev's briefing searches `(topic: "auth", category: "pattern")` and gets the ADR in results (because semantic search over "auth" surfaces it). No explicit routing code -- just data + search.

**The `workflow_state` tool question**: D5b proposed a `workflow_state` tool for swarm coordination (wave progress, agent status). This is a separate concern from knowledge management. Including it in Unimatrix blurs the product boundary. Alternative: leave workflow coordination in files/protocols, keep Unimatrix focused on knowledge.

### What This Scenario Settles

- `context_briefing` belongs in the interface (v0.2+, not v0.1) as an optimization for orchestrator workflows
- The primary consumer in multi-agent is the orchestrator, not leaf agents
- `workflow_state` is OUT of scope -- Unimatrix is a knowledge engine, not a workflow coordinator
- Cross-agent knowledge routing works naturally through topic/category/search, no special routing code
- Burst read patterns are well-handled by redb's MVCC

---

## Scenario 4: Knowledge Accumulates, Quality Degrades

**Assumption**: After 3 months of active use, a project has 5,000 entries. Some are stale (the framework was upgraded), some are contradictory (early decisions reversed later), some are duplicates (different agents stored the same pattern slightly differently). The signal-to-noise ratio is deteriorating. Search results are increasingly polluted.

### Interface Implications

**Lifecycle management tools become essential**:

```
context_correct -- supersede an entry with a corrected version
  params: original_id, content, reason?
  Effect: creates new entry with supersedes: original_id,
          marks original as deprecated

context_deprecate -- mark an entry as deprecated without replacement
  params: id, reason?

context_status -- view knowledge base health metrics
  params: (none, or filter by topic/category)
  Returns: entry counts by status, age distribution,
           duplicate candidates, stale entries
```

**Search must respect lifecycle**: `context_search` by default excludes deprecated entries. If a deprecated entry matches the query vector, follow the correction chain to the current version. This is server-side logic, transparent to Claude.

**Deduplication must happen at store-time**: When `context_store` is called, the server checks for existing entries above a similarity threshold (e.g., 0.92). If a near-duplicate exists, the server can either merge, reject with guidance, or store with a "duplicate_of" link.

**Confidence scoring becomes visible**: Search results include confidence scores. Entries that haven't been used or validated in 90+ days have decayed confidence. Claude can factor this into its decisions.

### Backend Implications

**Correction chains in redb**: Each entry has `supersedes: Option<u64>` and `superseded_by: Option<u64>`. When searching, if a result has `superseded_by`, follow the chain. Chain traversal is O(chain_length), typically O(1-2).

**Status transitions as atomic operations**: `context_correct` in one write transaction: create new entry, update old entry's `superseded_by` and `status`, update all index tables for both entries.

**Dedup on insert**:
```
1. Embed new content
2. search_filter(embedding, k=1, ef=32, filter=active_entries)
3. If top result similarity > 0.92: return "Near-duplicate detected" with existing entry
4. Else: proceed with insert
```

**Periodic maintenance**: hnsw_rs has no deletion. Over time, deprecated entries still exist in the vector index, just filtered out during search. This means filter sets grow and search becomes slightly slower. Mitigation: periodic index rebuild (dump all active entries, create new index, swap). This could be triggered by `context_status` showing >30% deprecated entries.

**The confidence formula from D3 runs on every search result**:
```
confidence = base_confidence
    * usage_factor(usage_count)         // Wilson score
    * freshness_factor(days_since_use)  // exponential decay, 90-day half-life
    * correction_penalty(correction_count)
```

Computation is microseconds per entry. No ML, no model loading.

### What This Scenario Settles

- Lifecycle tools (`context_correct`, `context_deprecate`, `context_status`) belong in the interface, version-gated to v0.2 or v0.3
- Dedup check runs at store-time, server-side, transparent to caller
- Correction chains are a core data model feature, not an add-on
- Confidence scores are included in search results from v0.1 (even if the formula starts simple)
- Periodic index rebuild needed as a maintenance operation (not exposed as a tool -- server-side concern)

---

## Scenario 5: The Non-NDP User (Generic Teams)

**Assumption**: A DevOps team uses Unimatrix for runbooks and incident patterns. A data science team uses it for experiment tracking and feature engineering conventions. A design team uses it for design system tokens and component patterns. None of these teams use NDP agents or SPARC workflows.

### Interface Implications

**Validates Constraint 1 (no hardcoded roles) and Constraint 4 (generic query model) absolutely**.

DevOps engineer:
```
context_store(content: "Rollback procedure: kubectl rollout undo deployment/api...",
              topic: "kubernetes", category: "runbook", tags: ["deployment", "rollback"])

context_lookup(topic: "kubernetes", category: "runbook")
  -> Returns all Kubernetes runbooks

context_search(query: "how to handle memory pressure alerts",
               topic: "monitoring")
  -> Returns semantically relevant monitoring patterns
```

Data scientist:
```
context_store(content: "Random forest with 500 trees, max_depth=12 gave 0.87 AUC...",
              topic: "churn-model-v3", category: "experiment", tags: ["random-forest", "churn"])

context_search(query: "feature importance for customer churn prediction",
               category: "experiment")
```

**The tool names must be domain-neutral**. `context_search` works. `memory_search` works. `knowledge_query` works. What doesn't work: anything suggesting "code" or "development" specifically.

**The category taxonomy is entirely user-defined**: No hardcoded enum. DevOps uses "runbook", "incident", "postmortem". Data science uses "experiment", "hypothesis", "dataset". The system just filters strings.

### Backend Implications

**No backend changes whatsoever**. The generic model handles all these cases with the same index tables, same retrieval paths, same embedding. This is the payoff of Constraints 1 and 4 -- the backend is truly domain-agnostic.

**Embedding model choice matters across domains**: `text-embedding-3-small` (OpenAI) or `all-MiniLM-L6-v2` (local) are both trained on general English text. They handle code, prose, and technical writing reasonably well. No domain-specific embedding needed for these use cases.

**One concern**: Embedding quality for highly specialized domains (bioinformatics sequences, legal citations, mathematical notation) may be poor with general-purpose models. This is a future concern, not a v0.1 concern.

### What This Scenario Settles

- The generic `{ topic, category, query, tags }` model is correct -- no domain-specific parameters
- Tool names should be generic (not "code memory" or "dev patterns")
- Categories are freeform strings, never an enum
- Backend is domain-agnostic -- proven by the fact that zero changes needed for completely different use cases
- The NDP agent topology is a validation case, not the design target (confirmed)

---

## Scenario 6: Context Budget Scarcity

**Assumption**: Claude's context window is 200K tokens. A complex session with multiple file reads, tool calls, and conversation history consumes 150K. An agent spawned with `max_turns: 30` has a much smaller effective budget. Every token from Unimatrix competes with actual work context.

### Interface Implications

**Response size control becomes a first-class concern**:

- `max_tokens` parameter on search (default: 2000) -- Claude controls its own budget
- Content excerpts, not full entries, in search results
- `context_lookup` returns full entries (deterministic, expected small)
- `context_search` returns excerpts + metadata (semantic, potentially many results)
- `context_get(id)` exists for "I need the full entry for this specific result" -- the search->get chain from D5

**The `context_briefing` compound tool is specifically an optimization for this scenario**: 4-5 individual tool calls consume ~4,000 tokens of tool_use/tool_result overhead (message framing, IDs, etc.). One compound call saves ~3,000 tokens.

**Response format matters enormously**:

```
BAD (wastes tokens on verbose JSON):
{
  "results": [
    {
      "id": "abc123",
      "content": "Use anyhow for application errors...",
      "metadata": {
        "topic": "error-handling",
        "category": "convention",
        "tags": ["rust", "error"],
        "status": "active",
        "confidence": 0.92,
        "created_at": "2026-01-15T10:30:00Z",
        "updated_at": "2026-02-20T14:22:00Z"
      },
      "similarity": 0.94
    }
  ],
  "total_found": 7,
  "query": "error handling patterns"
}

BETTER (compact, front-loaded, Claude-optimized):
Found 7 entries for "error handling patterns" (showing top 3):

1. Error Handling Convention (0.94) [convention, active]
   > Use anyhow for application errors, thiserror for library errors.
   > Never use unwrap() in production code.

2. CoreError Enum Pattern (0.89) [pattern, active]
   > Define CoreError with variants per domain. Use map_err at boundaries.

3. Error Testing Convention (0.81) [convention, active]
   > Every public Result-returning function needs error path tests.

Apply these conventions to your work.
```

The compact format uses ~200 tokens for 3 results. The verbose JSON uses ~400. Over hundreds of queries, this compounds.

### Backend Implications

**Token counting at the server level**: The server needs approximate token counting to respect `max_tokens`. Simple heuristic: ~4 characters per token for English text. Count content length, stop adding results when budget exhausted.

**Content excerpting**: Store full content in redb, but return excerpts in search results. Excerpt strategy: first N characters (front-loading the most important info, per "lost in the middle" findings).

**The `structuredContent` field solves the format tension**: Return compact markdown in `content` (for Claude) and full typed JSON in `structuredContent` (for programmatic consumers). Two formats, one response, different consumers.

### What This Scenario Settles

- `max_tokens` parameter on search tools (default: 2000)
- Compact markdown is the primary response format (not verbose JSON)
- `structuredContent` carries the full typed data alongside
- `context_get(id)` exists for drill-down from search results
- `context_briefing` justified as a token-saving optimization for multi-agent
- Server-side token budgeting (approximate, heuristic)
- Content excerpting in search results (full content via `context_get`)

---

## Scenario 7: The Wrong Thing Was Stored

**Assumption**: An agent confidently stores `(topic: "database", category: "convention", content: "Always use raw SQL, never use an ORM")`. This is wrong for the project. A week later, 3 agents have retrieved this convention and followed it. The user discovers the damage and needs to correct it.

### Interface Implications

**Correction must be explicit and traceable**:

```
context_correct(
  original_id: "abc123",
  content: "Use SQLx for type-checked queries. Raw SQL only for complex joins.",
  reason: "Original entry was incorrect. Project uses SQLx."
)
```

This creates a new entry that supersedes the old one. The old entry is deprecated but preserved (audit trail). Any future search that would have matched the old entry now returns the new one via correction chain traversal.

**But what about the damage already done?** The 3 agents that followed the wrong convention already produced code. Unimatrix can't undo that. However, it can:
- Return the correction prominently when anyone searches "database convention"
- Include correction history: "This supersedes an earlier convention (corrected 2026-02-20). Previous version recommended raw SQL."
- Surface corrections proactively in briefings: "Recent corrections in your topic area: ..."

**This argues for a `corrections` section in search results**:

```
Found 3 entries for "database query patterns":

1. SQLx Query Convention (0.96) [convention, active] CORRECTED 2026-02-20
   > Use SQLx for type-checked queries. Raw SQL only for complex joins.
   > (Supersedes: "Always use raw SQL" -- corrected because project uses SQLx)

2. Connection Pool Pattern (0.88) [pattern, active]
   > ...
```

### Backend Implications

**Correction chain data model**:
```
Entry {
  ...
  supersedes: Option<u64>,     // ID of entry this replaces
  superseded_by: Option<u64>,  // ID of entry that replaced this
  correction_count: u32,       // times this entry has been corrected
}
```

**Search-time chain traversal**:
```
for each search result:
  if result.status == Deprecated && result.superseded_by.is_some():
    follow chain to current version
    annotate result with correction history
  if result.correction_count > 0:
    reduce confidence (correction_penalty)
```

**Embedding the correction**: The new entry gets its own embedding. If the correction significantly changes the content (raw SQL -> SQLx), the new embedding will match different queries. The old entry's embedding still exists in hnsw_rs but is filtered out (deprecated status). This is correct behavior -- semantic search now surfaces the correction for relevant queries.

### What This Scenario Settles

- `context_correct` is a distinct tool (not just store + deprecate separately)
- Correction chain traversal is a core search behavior
- Correction history is surfaced in search results (not hidden)
- Old entries are preserved (audit trail), not deleted
- Correction count feeds into confidence scoring

---

## Scenario Synthesis: Decisions Driven by Scenarios

| Decision | Driven By Scenario | Resolution |
|----------|-------------------|------------|
| Two retrieval tools vs one | #1 (mixed determinism) | **Two tools**: `context_lookup` + `context_search` |
| Core v0.1 tool set | #2 (solo dev, day one) | **4 tools**: `context_search`, `context_lookup`, `context_store`, `context_get` |
| `context_briefing` timing | #3 (multi-agent) + #6 (budget) | **v0.2** -- optimization for orchestrator workflows |
| Lifecycle tools timing | #4 (quality degrades) | **v0.2**: `context_correct`, `context_deprecate`, `context_status` |
| `workflow_state` in/out | #3 (multi-agent) | **OUT** -- Unimatrix is knowledge, not workflow coordination |
| Generic query model | #5 (non-NDP users) | **Confirmed**: `{ topic, category, query, tags }` -- no domain params |
| Response format | #6 (context budget) | **Compact markdown** in `content` + full JSON in `structuredContent` |
| Auto-initialization | #2 (day one) | **Yes** -- first `context_store` creates project transparently |
| Correction as first-class | #7 (wrong thing stored) | **Yes** -- correction chains, traversal, surfacing in results |
| Tool naming | #5 (generic teams) | **`context_*`** prefix -- domain-neutral, not `memory_*` |
| Dedup at store-time | #4 (quality) | **Yes** -- similarity check on insert, server-side |
| Confidence in results | #4 (quality) + #6 (budget) | **Yes from v0.1** -- simple formula, visible in results |
| `max_tokens` param | #6 (budget) | **Yes** -- on search and briefing tools |
| `instructions` field | #2 (day one) | **Critical** -- must drive both search AND store behavior |

---

## The Version Map That Emerges

### v0.1 (Core -- Solo Dev Viable)

Tools:
- `context_search` -- semantic retrieval
- `context_lookup` -- deterministic retrieval
- `context_store` -- store with metadata
- `context_get` -- full entry by ID

Features:
- Server `instructions` driving behavior
- Basic confidence (source-based only)
- Auto-initialization on first store
- Compact markdown responses + `structuredContent`
- Tool annotations for permission behavior

### v0.2 (Lifecycle + Multi-Agent)

Tools:
- `context_correct` -- supersede with correction
- `context_deprecate` -- deprecate without replacement
- `context_status` -- knowledge base health
- `context_briefing` -- compound orientation tool

Features:
- Full confidence formula (usage + freshness + correction)
- Dedup on insert (near-duplicate detection)
- Correction chain traversal in search
- Store-time validation

### v0.3 (Sophistication)

Features:
- MCP Resources (passive context for conventions)
- MCP Prompts (user slash commands: `/recall`, `/remember`)
- Local embedding model (drop OpenAI dependency)
- Index rebuild maintenance operation
- Cross-project knowledge sharing
