# ASS-021: context_briefing Redesign — Design Exploration

**Date:** 2026-03-16

---

## 1. Problems with the Current Design

### P1 — Role as topic filter is wrong
`role="architect"` maps to `QueryFilter { topic: "architect", category: "convention" }`. But entries aren't tagged with the calling agent's role — they have domain topics like `"vector-search"` or `"store"`. The filter almost never matches anything useful. Role describes the caller; it says nothing about what knowledge domain they need.

### P2 — Hardcoded k=3 with no floors
k=3 + Flexible retrieval mode + no similarity/confidence floor = possible low-quality results. The injection path uses k=5, similarity ≥ 0.5, confidence ≥ 0.3. Briefing has weaker guarantees but is used for more explicit, high-stakes orientation. Should be the other way around.

### P3 — context_cycle keywords not used
col-022 added forced keywords to context_cycle. These are exactly what briefing should use as semantic anchors — they encode what the session is about. Currently completely ignored by briefing.

### P4 — Briefing is invisible to CompactPayload
`context_briefing` (MCP path) doesn't write to the injection log. CompactPayload (prompt construction) only re-injects what ContextSearch previously recorded. An agent that calls `context_briefing` directly gets knowledge at MCP-call time but that knowledge doesn't flow into future prompt injections.

### P5 — Agent definitions can't control what kinds of knowledge they get
An agent def can specify `role=architect`, but it cannot say "give me decisions and patterns, not conventions". The agent has no vocabulary to express type-scoped retrieval. This forces overloading of the role field as a proxy for entry category.

### P6 — Single task string as the sole semantic anchor
One string → one embedding → one HNSW query. If an agent is working on something that spans multiple concepts ("HNSW rebuild on startup with concurrency safety"), those concepts compete in the embedding centroid. Multi-anchor retrieval would get more diverse, relevant results.

---

## 2. What Agents Actually Want from Unimatrix

Different agents arrive at different points in the workflow with different needs:

| Agent Type | When | What they need |
|------------|------|----------------|
| Scrum master | Session start | Procedures for this workflow, conventions for coordination |
| Architect | Design phase | ADRs in this domain, prior patterns, active lessons |
| Spec writer | Design phase | Existing conventions, domain model ADRs, acceptance criteria patterns |
| Implementation agent | Before coding | Patterns for this component, relevant ADRs, lessons from similar bugs |
| Tester | Test planning | Risk patterns, testing procedures, prior failures in this area |
| Security reviewer | Pre-review | Security patterns, prior security lessons |

The common shape is: **"give me [these types] of knowledge relevant to [these semantic anchors] scoped to [this feature domain]".**

Three dimensions:
1. **Type scope** — which entry categories (decision, pattern, convention, procedure, lesson-learned)
2. **Semantic anchors** — what the session is about (keywords from context_cycle, task description)
3. **Feature scope** — boost entries tagged to the current feature/domain

---

## 3. Proposed New Interface

### 3.1 MCP Tool Parameters

```
context_briefing(
    // Type scope (replaces "role")
    categories:       Option<Vec<String>>  — entry categories to retrieve
                                            e.g. ["decision", "pattern", "lesson-learned"]
                                            None = all categories

    // Semantic anchors
    keywords:         Option<Vec<String>>  — semantic anchor terms, max 10
                                            e.g. ["HNSW", "vector search", "compaction"]
                                            if None + feature set → auto-pull from session keywords
    task:             Option<String>       — free-form task description (combined with keywords)

    // Feature scope
    feature:          Option<String>       — feature tag for boost + keyword auto-pull

    // Quality control
    similarity_floor: Option<f64>          — default 0.40 (hard floor, drop if below)
    confidence_floor: Option<f64>          — default 0.20 (hard floor, drop if below)
    max_tokens:       Option<i64>          — default 3000, range [500, 10000]

    // Output
    format:           Option<String>       — summary / markdown / json
    agent_id:         Option<String>
    session_id:       Option<String>
)
```

**Removed:** `role` (deprecated, alias to keywords[0] for one release if needed)

### 3.2 Query Text Construction

Keywords + task are combined into a single embedding query:

```
query_text = [task, keywords...].compact().join(" ")
```

If `query_text` is empty and no injection history → no semantic search. Return only category-filtered store results if any, or empty result if floor-filtered.

**Rationale for concatenation over multi-embed:**
- One embedding call per request (latency constraint)
- Domain term concatenation creates a reasonable centroid in embedding space
- More keywords = broader centroid = more diverse recall (appropriate for briefing)
- Multi-embed adds O(n) embedding calls and result merging complexity

**Keyword auto-pull:**
When `feature` is set and `keywords` is None, attempt to pull keywords from the session record for that feature. For the MCP path this requires a DB read (acceptable: not a hot path). For the UDS path, populate `SessionState.keywords` on `cycle_start` (in-memory, no DB read on query).

### 3.3 Categories as Recognized Keywords

No separate `categories` parameter. Instead, **known category names in the `keywords` list are automatically recognized and partitioned out**:

```rust
const KNOWN_CATEGORIES: &[&str] = &[
    "decision", "pattern", "convention", "procedure", "lesson-learned", "outcome",
];

let (category_terms, semantic_terms): (Vec<_>, Vec<_>) = keywords
    .iter()
    .partition(|k| KNOWN_CATEGORIES.contains(&k.to_lowercase().as_str()));

// semantic_terms → joined for embedding query
// category_terms → category boost signal in re-ranking (NOT embedded)
```

Category terms are deliberately excluded from the embedding — "decision" and "pattern" are common words that would pollute the centroid. Instead they act as a re-rank weight: entries whose `category` matches a declared term get a score premium.

**Example:** `keywords = ["decision", "lesson-learned", "HNSW", "briefing"]`
- Embedding query: `"HNSW briefing"`
- Category boost applied to: `decision`, `lesson-learned` entries in results

If `keywords` is None or contains no category terms → no category boost, broad search (current behavior).

### 3.4 Floor Enforcement — "No Data > Bad Data"

BriefingService applies floors as hard post-filters:

```
for each result in search_results:
    if result.similarity < similarity_floor OR result.confidence < confidence_floor:
        drop it

if all results dropped:
    tracing::debug!("briefing: 0 results passed floors, returning empty")
    return Ok(BriefingResult { relevant_context: vec![], ... })
```

This is a deliberate design choice: returning zero entries is better than returning low-confidence garbage that the agent will act on. An empty briefing is honest; a bad briefing is dangerous.

Default floors are intentionally lower than injection path (Flexible vs Strict mode):
- Briefing: similarity ≥ 0.40, confidence ≥ 0.20
- Injection: similarity ≥ 0.50, confidence ≥ 0.30

Callers can tighten floors (e.g., specialist agents wanting high confidence only).

---

## 4. Unified Interface with Injection Pipeline

### 4.1 The Core Question

ContextSearch (UDS hook) and context_briefing (MCP) both answer "give me relevant knowledge for this query". Should they share the same interface and backend service?

**Current state:**
- Both already call `SearchService::search()` with different params
- CompactPayload already uses BriefingService
- ContextSearch calls SearchService directly (bypasses BriefingService)

**Proposed unification:**

Introduce `KnowledgeQuery` as the common input struct:

```rust
pub struct KnowledgeQuery {
    pub categories:       Option<Vec<String>>,    // type scope
    pub query_text:       Option<String>,          // combined task + keywords
    pub feature:          Option<String>,          // tag boost
    pub similarity_floor: f64,                     // default 0.40
    pub confidence_floor: f64,                     // default 0.30
    pub k:                usize,
    pub retrieval_mode:   RetrievalMode,
    pub max_tokens:       usize,
    pub co_access_anchors: Option<Vec<u64>>,       // for cross-boost
}
```

`BriefingService::query()` takes `KnowledgeQuery` and returns `BriefingResult`.

Callers configure appropriately:

| Caller | retrieval_mode | floors | k | output format | notes |
|--------|---------------|--------|---|---------------|-------|
| context_briefing (MCP) | Flexible | 0.40 / 0.20 | 3–10 (env) | full entries | explicit tool call |
| ContextSearch (UDS hook) | Strict | 0.40 / 0.20 | 15–20 | **semantic index** | auto, hot path |
| CompactPayload (UDS re-inject) | — | — | — | budgeted sections | replays history |

**Injection log remains a ContextSearch-only side effect.** BriefingService doesn't write it (it's a transport-layer concern, not a knowledge-retrieval concern).

### 4.2 ContextSearch Output: Semantic Index

**Current:** ContextSearch injects full entry content (k=5, ~200 tokens/entry, ~1000 token budget).

**Proposed:** ContextSearch returns a **semantic index** — results from the same search pipeline, reformatted as a compact navigational list sorted by category priority then confidence:

```
**Decisions** — #42 BriefingService→SearchService delegation (44%) · #107 EffectivenessStateHandle required (54%)
**Conventions** — #15 Lock ordering: read then drop before mutex (81%)
**Lessons** — #94 No Store reads in spawn_blocking on hot path (89%) · #203 Co-access dedup by session_id (71%)
**Patterns** — #67 Generation-aware caching (68%)
Use context_get(id) for full details.
```

**Why this changes k:** Index format is ~25 tokens/entry vs ~200 for full content. At the same token budget, k increases from 5 to 15–20. Agent sees more relevant results and pulls full content via `context_get` on the ones that matter.

**Category sort order within results:** decisions → conventions → lesson-learned → patterns. Outcomes excluded. Within each category: confidence DESC.

**What agents gain:** Semantically relevant knowledge surfaced in a navigable format. The search responds to the current prompt — what's happening right now drives what appears. The index format means the agent controls depth: scan the list, pull what applies.

**Injection log:** still records all entry IDs surfaced in the index. CompactPayload can re-inject full content for entries the agent subsequently accessed.

### 4.3 What Unification Buys

- **Single place to add features**: keyword auto-pull, multi-category filtering, floor enforcement — one implementation path, both callers benefit
- **Category scope available to injection path**: ContextSearch currently has no category filter. If a session has `categories=["lesson-learned", "pattern"]` set, injection can scope to those entry types automatically
- **Consistent re-ranking**: effectiveness utility delta, provenance boost, co-access boost — same logic everywhere
- **Closing the briefing-invisible-to-compactpayload gap**: if context_briefing writes to the injection log via a session_id param, CompactPayload can incorporate those entries in future compaction. The write path is optional — only when `session_id` is provided.

### 4.4 What Unification Does NOT Mean

- Same k, same floors, same retrieval_mode — NO. Each caller sets its own. The interface is shared; the params are not.
- Same output shape — NO. CompactPayload structures output into token-budgeted sections; ContextSearch returns a flat list; context_briefing returns structured JSON/markdown. Output formatting stays in each transport layer.
- Merging the MCP tool and hook endpoint — NO. These remain separate. `KnowledgeQuery` is a shared Rust struct, not an API contract.

---

## 5. Injection Log Gap Fix (Optional Extension)

When `context_briefing` is called with `session_id`, optionally write to INJECTION_LOG:

```rust
// In context_briefing handler, after BriefingService::assemble():
if let Some(sid) = session_id {
    let records = entry_ids.iter().map(|id| InjectionLogRecord {
        log_id: 0,  // allocated by store
        session_id: sid.clone(),
        entry_id: *id,
        confidence: /* reranked score */,
        timestamp: now(),
    }).collect();
    spawn_blocking_fire_and_forget(move || store.insert_injection_log_batch(&records));
}
```

Also update `SessionRegistry` in-memory injection history so CompactPayload picks it up:
```rust
session_registry.record_injections(session_id, &entry_ids_with_scores);
```

This closes observation O1 (briefing invisible to CompactPayload) without changing the core pipeline.

**Whether to do this:** depends on whether context_briefing calls happen within a session (session_id known) or standalone. For now, treat as a follow-up; document the gap as a known limitation.

---

## 6. Keywords in SessionState

Currently: `SessionRecord.keywords` (DB) but NOT `SessionState.keywords` (in-memory registry).

For keyword use in ContextSearch, keywords need to be in memory:

```rust
pub struct SessionState {
    // ... existing fields ...
    pub keywords: Option<Vec<String>>,  // populated on cycle_start, from context_cycle
}
```

On `cycle_start` event: parse keywords JSON from event payload → set on `SessionState`. No DB read on query path.

On `context_briefing` MCP path: no session state. If `feature` is set and `keywords` is None → query sessions table for that feature_cycle → extract keywords. One DB read, not on hot path.

### 6.1 Timing: When Keywords Are Available

`context_cycle` is called **once per session** by the scrum master, not once per sub-agent. The SM calls it during its first turn after understanding the feature request. Sub-agents are spawned after.

**Sub-agents share the parent `session_id`.** This is the key fact. All hooks — including every sub-agent's `UserPromptSubmit` — fire with the same `session_id` as the SM. So `SessionState.keywords` set by the SM's `context_cycle` call is immediately visible to every sub-agent's ContextSearch.

```
SM turn 1:
  UserPromptSubmit → ContextSearch → no keywords yet → tail-truncated prompt (acceptable:
                                                        SM's prompt IS the feature request)
  SM calls context_cycle(keywords=[...]) → SessionState.keywords set

Sub-agent spawned (same session_id):
  UserPromptSubmit → ContextSearch → SessionState.keywords available ✓
```

**The only gap:** the SM's own first ContextSearch fires before `context_cycle`. Tail-truncated prompt text is the fallback. This is acceptable — the SM's first prompt is the feature request, which is already a meaningful semantic query.

---

## 7. Design Trade-offs

### T1: Role backward compat
Removing `role` breaks existing agent definitions. Mitigation: treat `role` as alias → `keywords: [role]` for one release. Long-term: agent defs updated to use `keywords` (with category terms embedded in the list).

### T2: Keyword concatenation quality
"HNSW vector search embedding" as a query string is not a sentence. Embedding models are trained on natural language. A list of terms is lower quality than `task="I need to rebuild the HNSW index on startup with concurrency safety"`.

Mitigation: if `task` is present, use `task` as the primary embedding target and treat `keywords` as a reranking boost (title/content contains keyword → +small_boost). This keeps a high-quality embedding while still surfacing keyword-relevant entries.

However, this requires keyword-aware re-ranking in SearchService — a new signal. Complexity tradeoff.

**Recommendation for now:** concatenate. If recall quality proves insufficient, add keyword boost as a follow-up.

### T4: Empty result honesty
Returning an empty briefing when nothing passes floors may confuse agents that expect content. Mitigation: include metadata in response explaining why result is empty (`search_available: true, results_below_floor: 3`). Agents can retry with lower floors or no floors.

---

## 8. Implementation Scope Estimate

| Change | Complexity | Impact |
|--------|-----------|--------|
| New `KnowledgeQuery` struct in BriefingService | S | Foundation for everything |
| `categories` param replacing `role` | S | MCP handler + BriefingService |
| Keyword concatenation for query_text | XS | BriefingService::assemble() |
| Default floors in BriefingService | XS | Two constants |
| Extend `QueryFilter` for multi-category | M | Store + read.rs SQL + tests |
| `SessionState.keywords` field | S | session.rs + cycle_start handler |
| Keyword auto-pull from sessions table (MCP path) | S | BriefingService + store read |
| Injection log write from context_briefing | S | MCP handler side effect |
| Update agent definitions to use new params | S | .claude/agents/uni/*.md |

---

## 9. Open Questions

**OQ-1:** Should `categories` be exact string names (`"decision"`, `"pattern"`) or a curated enum? Exact strings allow extension without code changes; enum provides validation. Given that category names are already string-typed throughout, keep as strings.

**OQ-2:** Should `keywords` be caller-provided only, or always auto-augmented from context_cycle session? Auto-pull is convenient but adds a DB read (MCP path) or memory field (UDS path). Caller-provided is simpler and more explicit. **Recommendation:** caller-provided first, auto-pull as opt-in.

**OQ-3:** Should CompactPayload ever perform a fresh semantic search (not just replay history)? Currently it never does. Adding a fallback semantic search on CompactPayload when injection history is empty (or sparse) would improve cold-start quality. But this adds embedding latency to the prompt-construction path. **Recommendation:** document as follow-up, not in scope here.

**OQ-4:** Should `similarity_floor` and `confidence_floor` be fully caller-configurable via MCP params, or just have a server-side default with no caller override? Caller-configurable is more flexible but requires validation. **Recommendation:** caller-configurable with server-side defaults, validated to range [0.1, 0.9].

**OQ-5 — RESOLVED:** Sub-agents share the parent `session_id` (confirmed via Claude Code docs). `SessionState` is shared across the whole session. Keywords set by the SM's `context_cycle` call are immediately available to all sub-agents' ContextSearch via the shared `SessionState`. No FeatureRegistry or per-agent context_cycle needed. Category scoping via keyword partition applies session-wide.

**OQ-6 — OPEN:** SubagentStart injection strategy. See §11.

---

## 10. Recommended Scope for Implementation

**Core:**
1. `keywords` param replaces `role` in context_briefing — category names in list are partitioned out as boost signals; remaining terms embedded as query
2. `task` + semantic keywords concatenated for embedding
3. Default floors: similarity ≥ 0.40, confidence ≥ 0.20, caller-overridable
4. `KnowledgeQuery` struct unifying BriefingService + ContextSearch params
5. `SessionState.keywords` populated from `cycle_start` event (in-memory, same session shared by all agents)
6. ContextSearch output: semantic index format (k=15–20, sorted by category priority then confidence)

**Follow-up:**
- Keyword auto-pull from sessions table on MCP path (when `feature` set, `keywords` absent)
- Injection log write from context_briefing when `session_id` present
- SubagentStart injection strategy (see §11)

---

## 11. SubagentStart Injection — Open Exploration

*Cross-reference: `product/research/ass-019/FINDINGS.md`*

ASS-019 established:
- SubagentStart **can** inject into sub-agent context via `hookSpecificOutput.additionalContext` JSON (output format unverified — plain stdout may also work)
- **`prompt_snippet` does not exist** in the SubagentStart payload — the field in hook.rs is a phantom, always `None` in production
- Payload contains: `agent_type`, `agent_id`, `session_id`, `cwd` — no prompt text of any kind
- Spawn prompt text lives in PreToolUse on the parent session (Agent tool call) with no clean linkage to SubagentStart

### What's available as query signal

At SubagentStart time, the useful signals are:
- `agent_type` (e.g., `"uni-architect"`, `"uni-rust-dev"`) — role dimension
- `SessionState.keywords` (from context_cycle, shared session) — feature domain
- Category terms partitioned from session keywords — type scope

Sub-agents are spawned **after** the SM calls `context_cycle`, so session keywords are always populated by SubagentStart time.

### Still thinking: what should SubagentStart inject?

**Purpose:** proactive orientation before turn 1 — give the agent what it doesn't know to search for. UserPromptSubmit handles reactive retrieval (current prompt drives what surfaces). SubagentStart is the opposite: the agent hasn't said anything yet; the injection must be based on who it is and what the session is about.

Two directions under consideration:

**Direction A — Semantic search index**
Query: `"{agent_type} {semantic_keywords_joined}"` → HNSW → semantic index output.
- Pros: agent_type adds role signal, keywords add feature domain signal, results are semantically relevant
- Cons: agent_type reliability is uncertain; `uni-architect` vs `general-purpose` changes quality significantly. ASS-019 Q2 (does agent_type match subagent_type exactly?) is unverified.

**Direction B — Structured knowledge index (no semantic search)**
Return top entries by category priority + confidence for the active feature, without embedding:
- decisions → conventions → lesson-learned → patterns (outcomes excluded)
- Feature-tagged entries first; otherwise top by confidence
- Compact format: title + ID + confidence%
- Pros: no query quality dependency, always works, surfaces things the agent cannot discover by searching
- Cons: not filtered to the agent's specific role or task

**Direction C — Hybrid**
Structured index as the base (B), enriched with semantic results on top when agent_type is reliable.

### Unresolved prerequisites (from ASS-019)

1. Does plain stdout work for SubagentStart injection, or is `hookSpecificOutput` JSON required? (empirical test needed)
2. Does `agent_type` in the payload match `subagent_type` from the Agent tool call exactly?
3. Token budget for SubagentStart injection — different context semantics than mid-session injection

**Status:** direction not finalized. Not blocking context_briefing/ContextSearch scope.
