# D5c: Design Constraints — Role Genericity, Deterministic Retrieval, and the Generic Query Model

**Addendum to**: D5 Context Injection Playbook + D5b Deep Dive
**Date**: 2026-02-20
**Status**: Complete (Rev 2)
**Source**: Direct product guidance
**Used By**: Track 3 (Interface Spec) — these are hard constraints on the tool design

---

## Constraint 1: No Hardcoded Agent Roles

### The Rule

Unimatrix's code MUST NOT contain any awareness of specific agent roles. No `if role == "architect"`, no `match role { "scrum-master" => ... }`, no role-specific response assembly logic.

Agent roles, their responsibilities, their duties, their workflows — all of this is **DATA** stored in Unimatrix, not logic in Unimatrix's code.

### What This Means

**Unimatrix is a generic context engine.** It stores and retrieves contextual knowledge. The fact that there's an "architect" role that "manages ADRs" is a piece of stored data, not a hardcoded behavior.

```
WRONG (role-specific code):
  fn context_briefing(role: &str) {
      match role {
          "architect" => return_adrs_and_integration_surfaces(),
          "rust-dev" => return_code_patterns_and_conventions(),
          "scrum-master" => return_protocols_and_wave_state(),
          _ => return_generic_search()
      }
  }

RIGHT (data-driven):
  fn context_briefing(role: &str, phase: &str, task: &str) {
      // Step 1: Deterministic lookup — find entries tagged for this role
      let role_context = lookup(role: role, category: "duties");
      let phase_context = lookup(role: role, phase: phase, category: "protocol");

      // Step 2: Semantic search — find relevant knowledge
      let knowledge = search(query: task, filter: { role: role, phase: phase });

      // Step 3: Assemble response from DATA, not from hardcoded logic
      assemble_response(role_context, phase_context, knowledge)
  }
```

The assembly logic is generic: look up role data, look up phase data, search for relevant knowledge, combine. The CONTENT of what comes back is determined by what's stored, not by what's coded.

### Implications

1. **Any agent topology works.** The NDP agents (scrum-master, architect, rust-dev, etc.) are one configuration. A data science team, a DevOps team, a design team — any team structure can use Unimatrix without code changes. You just store different role definitions, duties, and protocols.

2. **Role definitions are entries in the data store.** When someone sets up Unimatrix, they store: "Role: architect. Duties: manage ADRs, design integration surfaces, evaluate technology selections." This is data, queryable like any other entry.

3. **No role enumeration.** Unimatrix doesn't have a list of valid roles. Any string is a valid role. The system filters by whatever role tag is on the stored data.

4. **The NDP agent definitions become seed data.** The current `.claude/agents/ndp/*.md` files are essentially the seed data that would be loaded into Unimatrix. The agent files would shrink to thin shells; the content moves into Unimatrix's knowledge store.

### The Capability vs. Practicality Spectrum

There is a spectrum between "Unimatrix stores and serves everything" and "some things are better as static files":

| Content | In Unimatrix? | Reasoning |
|---------|---------------|-----------|
| Role definitions and duties | Yes | Core use case — dynamic, queryable, updatable |
| Workflow protocols | Yes | Different roles need different protocol excerpts |
| Conventions and patterns | Yes | Core use case — the whole point of memory |
| ADRs and decisions | Yes | Cross-agent routing, searchable, correctable |
| Agent definition files (`.claude/agents/`) | Thin shell + Unimatrix data | Shell provides Claude's persona anchor; details from Unimatrix |
| CLAUDE.md | Stays as file | System prompt must be static, always loaded, never compacted |
| Feature artifacts (SCOPE.md, etc.) | Stay as files | Large documents, version-controlled, read by agents directly |

**The capability must exist.** Whether every team uses it to the maximum is a practical decision per deployment. But Unimatrix's architecture must support storing and serving ANY agent configuration purely from data.

---

## Constraint 2: Deterministic vs. Semantic Retrieval

### The Rule

When an agent asks "what are my duties as a scrum master?", the answer MUST be deterministic — the same query always returns the same data. This is a **lookup**, not a similarity search.

When an agent asks "what patterns are relevant to implementing auth middleware?", the answer is **semantic** — ranked by vector similarity, potentially different results as knowledge grows.

Unimatrix must support BOTH retrieval modes, and Claude must decompose its requests into the appropriate mode.

### Two Retrieval Modes

#### Mode 1: Deterministic Lookup

Exact-match query on structured metadata fields. No vector search. No ranking. The result is the stored data, verbatim.

```
Query:  lookup(role: "scrum-master", category: "duties")
Result: The stored duties document for scrum-master. Always the same.

Query:  lookup(role: "architect", category: "protocol", phase: "planning-wave-1")
Result: The stored protocol excerpt for architects in planning wave 1. Always the same.

Query:  lookup(category: "convention", tags: ["error-handling"])
Result: All stored error handling conventions. Ordered by creation date, not similarity.
```

**Properties:**
- Exact match on metadata fields (role, category, phase, tags)
- No embedding, no vector search
- Results ordered by explicit ordering (creation date, priority, etc.), not similarity
- Same query always returns same results (until data changes)
- This is a database query, not an AI operation

**Use cases:**
- "What are my duties?" → lookup role duties
- "What's the protocol for this phase?" → lookup protocol by phase
- "What conventions apply to error handling?" → lookup by category + tag
- "What are the acceptance criteria for AC-003?" → lookup by ID
- "What ADRs exist for auth?" → lookup by category + tag

#### Mode 2: Semantic Search

Vector similarity search against embedded content. Ranked by relevance. Results may vary as knowledge grows.

```
Query:  search(query: "how to handle authentication in Tower middleware")
Result: Top-k entries ranked by vector similarity to the query embedding.
        Results change as new knowledge is stored.

Query:  search(query: "why did we choose JWT over OAuth2", filter: { category: "decision" })
Result: Decisions ranked by relevance to the query. May surface related
        decisions about session management, token storage, etc.
```

**Properties:**
- Natural language query embedded and compared against stored vectors
- Results ranked by similarity score
- Same query may return different results as knowledge base grows
- Filtered by metadata (role, phase, category) but ranked by similarity
- This IS an AI operation (embedding + ANN search)

**Use cases:**
- "What patterns are relevant to this implementation?" → semantic
- "Has anyone solved a similar problem before?" → semantic
- "What's the rationale behind this design decision?" → semantic
- "Find code examples for this pattern" → semantic

### Forcing Decomposition

Claude should NOT send a single natural-language blob to Unimatrix and hope for the best. The tool interface should guide Claude to decompose its needs:

```
BAD (single ambiguous query):
  context_search("I'm a scrum master starting planning wave 2, what do I need to know?")
  → Is this deterministic? Semantic? Both? The server can't tell.

GOOD (decomposed into appropriate modes):
  1. lookup(role: "scrum-master", category: "duties")           → deterministic
  2. lookup(phase: "planning-wave-2", category: "protocol")     → deterministic
  3. lookup(feature: "nxs-001", category: "wave-state")         → deterministic
  4. search(query: "planning coordination patterns",             → semantic
            filter: { role: "scrum-master" })
```

### How to Force This Decomposition

**Option A: Separate tools**

Two distinct tools with different descriptions that guide Claude to use the right one:

```
Tool: context_lookup
Description: "Retrieve stored definitions, protocols, duties, conventions, or rules
by exact metadata match. Use this when you need a specific, known piece of
information — role duties, workflow protocols, tagged conventions, feature
state. Results are deterministic: same query always returns same data.
DO NOT use this for open-ended questions or similarity-based discovery."

Tool: context_search
Description: "Search stored knowledge by natural language similarity.
Use this when you need to discover relevant patterns, find related
decisions, or explore what the knowledge base contains about a topic.
Results are ranked by relevance and may vary as knowledge grows.
DO NOT use this for looking up specific definitions, duties, or protocols —
use context_lookup for those."
```

**Option B: Single tool with explicit mode**

```
Tool: context_query
Parameters:
  mode: "lookup" | "search"       ← Claude must choose
  ...lookup params when mode=lookup...
  ...search params when mode=search...
```

**Option C: Single tool, server infers mode**

```
Tool: context_query
Parameters:
  // If structured fields provided → deterministic lookup
  role: string?
  category: string?
  tags: [string]?
  phase: string?
  feature: string?

  // If query string provided → semantic search
  query: string?

  // Both can be combined: semantic search with metadata filters
```

**Recommendation: Option A (separate tools).** Reasons:
1. Claude Code's tool descriptions are the prompt engineering surface. Two distinct descriptions with clear "DO / DO NOT" guidance will produce better decomposition than a mode parameter.
2. Separate tools make it obvious in conversation logs which retrieval mode was used.
3. The descriptions can include examples that train Claude's tool selection.
4. Claude naturally calls the right tool when the descriptions are clear — this is how Claude Code's own tools work (Read vs. Grep vs. Glob are three separate tools for what could be one tool with modes).

---

## How These Constraints Interact

### The Data Model

Unimatrix stores entries with rich metadata. The metadata enables BOTH deterministic lookup and semantic search filtering:

```
Entry:
  id: string                    (unique, generated)
  content: string               (the actual knowledge — free text)
  embedding: vector             (for semantic search)
  metadata:
    category: string            (duties, protocol, convention, pattern, decision, correction, state)
    roles: [string]             (which roles this is relevant to — empty means all)
    phases: [string]            (which phases — empty means all)
    tags: [string]              (freeform tags for filtering)
    feature: string?            (scoped to a feature, or null for global)
    priority: float?            (for ordering deterministic results)
    supersedes: string?         (ID of entry this replaces)
    status: string              (active, aging, deprecated)
    created_at: datetime
    updated_at: datetime
```

**Deterministic lookup** queries the metadata fields directly (exact match on category, role, phase, tags). No embedding involved.

**Semantic search** uses the embedding vector for similarity ranking, with metadata fields as pre-filters.

### Neither Mode Knows About "Architect" or "Scrum Master"

The lookup `(role: "architect", category: "duties")` works because someone STORED an entry with `roles: ["architect"]` and `category: "duties"`. Unimatrix's code just does exact-match filtering on string fields. It doesn't know or care what "architect" means.

If someone stores `(roles: ["deployment-engineer"], category: "duties", content: "You manage CI/CD pipelines...")`, the lookup `(role: "deployment-engineer", category: "duties")` returns it. No code changes. No role registration. Just data.

### The `context_briefing` Tool Revisited

In D5b, I proposed `context_briefing` as a compiled orientation tool. With these constraints, it becomes a **composition of lookups and searches**, not server-side role-specific logic:

```
context_briefing(role, phase, task, feature):
  // All deterministic lookups — always return same data for same inputs
  duties     = lookup(role: role, category: "duties")
  protocol   = lookup(role: role, phase: phase, category: "protocol")
  rules      = lookup(role: role, category: "rules")
  wave_state = lookup(feature: feature, category: "wave-state")

  // Semantic search — may vary as knowledge grows
  patterns   = search(query: task, filter: { roles: [role], phases: [phase] })

  // Generic assembly — no role-specific logic
  return format_briefing(duties, protocol, rules, wave_state, patterns)
```

The `format_briefing` function is generic: it concatenates sections with headers. It doesn't know what a scrum master is. It just formats whatever data the lookups returned.

**However** — this raises a question: should `context_briefing` exist as a tool at all, or should Claude compose the lookups itself?

| Approach | Pros | Cons |
|----------|------|------|
| **`context_briefing` as a compound tool** | One tool call instead of 4-5. Less context consumed by tool_use/tool_result overhead. | Hides the decomposition from Claude. Less control over what to query. |
| **Claude composes lookups + searches** | Full transparency. Claude controls exactly what it asks for. | 4-5 tool calls at the start of every agent's work. Context overhead. |
| **Hybrid: `context_briefing` for orientation, individual tools for ad-hoc** | Best of both worlds. Briefing on spawn, specific queries during work. | Two access patterns to maintain. |

**Recommendation: Hybrid.** `context_briefing` exists for the common "I just spawned, orient me" pattern, but it's implemented server-side as a composition of deterministic lookups + one semantic search. No role-specific logic. The individual `context_lookup` and `context_search` tools exist for ad-hoc queries during work.

---

## What This Means for the Data Seeding Story

When Unimatrix is initialized for a project, the data store starts empty. The current NDP agent definitions, protocols, and conventions become **seed data** that gets loaded into Unimatrix:

```
unimatrix init
  → Creates project in Unimatrix
  → Optionally: "Load agent definitions from .claude/agents/ndp/?"
    → Parses each agent definition file
    → Stores as entries:
        Entry: role=scrum-master, category=duties, content="You are the swarm coordinator..."
        Entry: role=scrum-master, category=protocol, phase=planning, content="Read the protocol..."
        Entry: role=architect, category=duties, content="You make design decisions..."
        Entry: role=architect, category=rules, content="ADR format: ## ADR-NNN: Title..."
        ...
  → Optionally: "Load protocols from .claude/protocols/?"
    → Parses each protocol file
    → Stores as entries with phase and role tags
  → Optionally: "Load conventions from CLAUDE.md?"
    → Extracts convention sections
    → Stores as entries with appropriate tags
```

After seeding, the static files become redundant for agents that query Unimatrix. They can shrink to thin shells or be removed entirely.

**But seeding is optional.** A user could start with empty Unimatrix and build up knowledge organically as agents work. The system doesn't require seed data to function — it just returns empty results until knowledge is stored.

---

## Categories: The Deterministic Taxonomy

Since deterministic lookup needs consistent category names, here's a proposed taxonomy. These are NOT hardcoded in Unimatrix — they're conventions in the stored data. But they should be documented so agents and users use consistent terms.

| Category | What It Contains | Retrieval Mode |
|----------|-----------------|----------------|
| `duties` | Role definition, responsibilities, scope | Deterministic |
| `protocol` | Workflow procedures, step-by-step processes | Deterministic |
| `rules` | Hard rules, constraints, non-negotiables | Deterministic |
| `convention` | Coding standards, naming patterns, style guides | Deterministic |
| `decision` | Architecture decisions, technology choices, rationale | Either |
| `pattern` | Reusable code patterns, solution templates | Semantic |
| `correction` | Superseded knowledge, what was wrong, what's right | Deterministic (by supersedes ID) |
| `context` | Shared state, wave outputs, coordination data | Deterministic |
| `knowledge` | General domain knowledge, learned insights | Semantic |

**Deterministic categories** are looked up by exact metadata match. You query `(role: X, category: "duties")` and get back the stored duties.

**Semantic categories** are searched by natural language similarity. You query `"auth middleware pattern"` and get back ranked results.

**Either categories** support both: you can look up `(category: "decision", tags: ["auth"])` deterministically, or search `"why did we choose JWT"` semantically.

---

## Revised Tool Interface (Track 3 Input)

Based on both constraints:

### `context_lookup` — Deterministic retrieval

```
Tool: context_lookup
Description: "Retrieve stored knowledge by exact metadata match. Returns
definitions, protocols, duties, conventions, rules, and state. Same query
always returns same results. Use this for known, structured information —
NOT for open-ended discovery.

Examples:
- Look up role duties: role='architect', category='duties'
- Look up workflow protocol: phase='planning-wave-2', category='protocol'
- Look up conventions: category='convention', tags=['error-handling']
- Look up feature state: feature='nxs-001', category='context'"

Parameters:
  category: string?           // duties, protocol, rules, convention, decision, context...
  role: string?               // Filter to entries tagged for this role
  phase: string?              // Filter to entries tagged for this phase
  feature: string?            // Filter to entries scoped to this feature
  tags: [string]?             // Filter to entries with ALL these tags
  id: string?                 // Look up a specific entry by ID
  status: string?             // active (default), aging, deprecated, all
  limit: int?                 // Max entries (default: 20)

Returns:
  entries: [{ id, content, metadata }]
  count: int
```

### `context_search` — Semantic retrieval

```
Tool: context_search
Description: "Search stored knowledge by natural language similarity. Returns
entries ranked by relevance to your query. Use this for open-ended discovery —
finding relevant patterns, related decisions, or exploring what's known about
a topic. Results may vary as knowledge grows.

Can be combined with metadata filters to narrow scope."

Parameters:
  query: string (required)    // Natural language query
  role: string?               // Filter to entries tagged for this role
  phase: string?              // Filter to entries tagged for this phase
  feature: string?            // Filter to entries scoped to this feature
  tags: [string]?             // Filter to entries with ALL these tags
  category: string?           // Filter to specific category
  k: int?                     // Max results (default: 5)
  max_tokens: int?            // Response budget (default: 2000)

Returns:
  results: [{ id, content, metadata, similarity }]
  total_found: int
```

### `context_store` — Store with metadata

```
Tool: context_store
Description: "Store knowledge with metadata for later retrieval. Tag entries
with roles, phases, and categories so they can be found by both deterministic
lookup and semantic search."

Parameters:
  content: string (required)
  category: string (required)  // duties, protocol, rules, convention, pattern, decision, ...
  roles: [string]?             // Which roles this is relevant to (empty = all)
  phases: [string]?            // Which phases this applies to (empty = all)
  tags: [string]?              // Freeform tags
  feature: string?             // Scope to a feature (null = global)
  supersedes: string?          // ID of entry this replaces
  priority: float?             // Ordering for deterministic results

Returns:
  id: string
  status: "stored"
```

### `context_briefing` — Compiled orientation (compound tool)

```
Tool: context_briefing
Description: "Get a compiled orientation briefing for starting work. Combines
deterministic lookups (duties, protocols, rules) with semantic search
(relevant patterns and knowledge) into a single response. Use this when
starting a new task or when spawned as a new agent.

This is a convenience tool — equivalent to calling context_lookup and
context_search individually, but in one call."

Parameters:
  role: string (required)     // Your agent role
  task: string (required)     // What you're about to do
  phase: string?              // Current workflow phase
  feature: string?            // Feature you're working on

Returns:
  briefing: {
    duties: string?,          // Your role duties (deterministic)
    protocol: string?,        // Current protocol steps (deterministic)
    rules: [string]?,         // Applicable rules (deterministic)
    conventions: [string]?,   // Relevant conventions (deterministic)
    patterns: [{ id, content, similarity }]?,  // Relevant patterns (semantic)
    context: string?,         // Shared context / wave state (deterministic)
  }
```

---

## Constraint 3: Practical Hybrid — Static Anchors + Dynamic Knowledge

### The Reality

Not everything moves into Unimatrix. Some rules SHOULD stay directly in agent definitions. The agent definition file provides the **persona anchor** — the core identity and hard rules that must be in the system prompt (never compacted, always present). Unimatrix provides the **dynamic knowledge layer** — context that varies by task, phase, feature, and accumulated learning.

### Where Things Live

```
Agent Definition (static, system prompt):
  - Core identity ("You are a Rust developer")
  - Hard behavioral rules ("Never use unwrap() in production")
  - Self-check procedures that MUST always run
  - The instruction to query Unimatrix

Unimatrix (dynamic, tool responses):
  - Detailed conventions and patterns
  - Workflow protocols and procedures
  - ADRs and architectural decisions
  - Learned patterns and corrections
  - Cross-agent context and shared state
  - Feature-specific knowledge
```

The line between these isn't rigid. Different teams will draw it differently. Some will keep thick agent definitions with Unimatrix as a supplement. Others will go thin, pulling almost everything from Unimatrix. **Unimatrix should support the full spectrum** — from "I just store patterns" to "I serve the entire agent knowledge base."

### The Key Principle

**Unimatrix is designed so it COULD serve everything. Whether it does is a practical choice per deployment.** The capability must be there. The user decides how much to use.

---

## Constraint 4: The Generic Query Model

### The Problem with the Parameter-Heavy Design

The `context_lookup` and `context_search` tools proposed earlier have too many parameters: role, phase, feature, tags, category, workflow_step. This is over-fitted to the NDP agent topology. A different team using Unimatrix would find half these parameters meaningless and would need different ones.

### The Simpler Model

When you strip away the NDP-specific framing, the query model devolves to something much simpler:

```
{ topic, category, query }
```

That's it. Three fields that can express anything:

| topic | category | query | What You Get |
|-------|----------|-------|-------------|
| `scrum-master` | `duties` | (none) | Deterministic: the scrum master's duties |
| `error-handling` | `convention` | (none) | Deterministic: all error handling conventions |
| `planning-wave-2` | `protocol` | (none) | Deterministic: wave 2 protocol steps |
| `nxs-001` | `context` | (none) | Deterministic: feature state and shared context |
| `auth` | `pattern` | `Tower middleware implementation` | Semantic: patterns related to auth middleware |
| `database` | `decision` | `why redb over SQLite` | Semantic: decisions about database choice |
| `rust` | `convention` | (none) | Deterministic: all Rust conventions |
| `deployment` | `runbook` | `rollback procedure` | Semantic: deployment runbook content |

**`topic`** is freeform — a role name, a technology, a domain area, a feature ID, a phase name. It's whatever the user uses to organize their knowledge.

**`category`** is freeform — a knowledge type. Conventions in naming emerge from usage, not from code enums.

**`query`** is optional — when present, triggers semantic search within the topic+category scope. When absent, returns all entries matching topic+category deterministically.

### How This Maps to the NDP Use Cases

The NDP agents don't need special parameters. They use the generic model with NDP-meaningful topics:

```
Scrum master starting wave 2:
  1. lookup(topic: "scrum-master", category: "duties")        → deterministic
  2. lookup(topic: "planning-wave-2", category: "protocol")   → deterministic
  3. lookup(topic: "nxs-001", category: "wave-state")         → deterministic
  4. search(topic: "planning", category: "pattern",
           query: "coordination patterns for large scope")    → semantic

Architect designing auth:
  1. lookup(topic: "architect", category: "duties")            → deterministic
  2. lookup(topic: "auth", category: "decision")               → deterministic
  3. search(topic: "auth", query: "integration surface patterns") → semantic

Rust developer implementing:
  1. lookup(topic: "error-handling", category: "convention")   → deterministic
  2. lookup(topic: "naming", category: "convention")           → deterministic
  3. search(topic: "auth", category: "pattern",
           query: "Tower middleware with JWT validation")      → semantic
```

No role parameter. No phase parameter. No feature parameter. Just topic + category + optional query. The semantics come from the data, not from the schema.

### How This Maps to Non-NDP Use Cases

The same model works for completely different teams:

```
DevOps engineer:
  lookup(topic: "kubernetes", category: "runbook")
  search(topic: "monitoring", query: "alerting thresholds for memory pressure")

Data scientist:
  lookup(topic: "feature-engineering", category: "convention")
  search(topic: "churn-model", query: "feature importance from last training run")

Product manager:
  lookup(topic: "user-research", category: "findings")
  search(topic: "onboarding", query: "drop-off patterns in signup flow")
```

No code changes. No new parameters. The generic model just works.

### The Retrieval Logic

Server-side, the logic is simple:

```
fn handle_query(topic: Option<&str>, category: Option<&str>, query: Option<&str>) -> Response {
    // Build metadata filter from topic + category
    let filter = build_filter(topic, category);

    if let Some(q) = query {
        // Semantic mode: embed query, search vectors, apply metadata filter
        semantic_search(q, filter)
    } else {
        // Deterministic mode: exact metadata match, ordered by priority/date
        deterministic_lookup(filter)
    }
}
```

No role-awareness. No phase-awareness. No NDP-specific logic. Just filter + optional similarity search.

### What About the Compound `context_briefing`?

With the simpler model, `context_briefing` becomes less necessary. An agent can make 2-3 fast lookups instead:

```
// Instead of one compound briefing call:
context_briefing(role: "rust-dev", task: "implement auth", phase: "implementation")

// The agent makes targeted queries:
lookup(topic: "rust-dev", category: "duties")
lookup(topic: "error-handling", category: "convention")
search(topic: "auth", query: "Tower middleware patterns")
```

Whether `context_briefing` exists as a convenience is a v0.2+ decision. The core model is topic + category + query. Everything builds on that.

### The Storage Side

Entries are stored with the same simplicity:

```
store(
  content: "Use anyhow for application errors, thiserror for library errors...",
  topic: "error-handling",
  category: "convention",
  tags: ["rust", "error", "core"]    // Optional additional tags for cross-cutting queries
)

store(
  content: "You are the swarm coordinator. Your job is to read the protocol...",
  topic: "scrum-master",
  category: "duties",
)

store(
  content: "Wave 2 agents spawn in parallel, ONE message. Each prompt includes...",
  topic: "planning-wave-2",
  category: "protocol",
)
```

`topic` and `category` are the primary organization axes. `tags` provide secondary cross-cutting access (find everything tagged "rust" across all topics and categories).

### The Data Model (Revised)

```
Entry:
  id: string                    (unique, generated)
  content: string               (the knowledge — free text)
  embedding: vector?            (for semantic search — only computed when needed)
  topic: string                 (primary subject — freeform)
  category: string              (knowledge type — freeform)
  tags: [string]                (cross-cutting labels — freeform)
  status: string                (active, aging, deprecated)
  supersedes: string?           (ID of entry this corrects/replaces)
  priority: float?              (ordering for deterministic results)
  created_at: datetime
  updated_at: datetime
```

Gone: `roles`, `phases`, `feature` as separate fields. These are just topics or tags now.

- Role context? `topic: "scrum-master"`, or `tags: ["scrum-master"]` for cross-cutting.
- Phase context? `topic: "planning-wave-2"`, or `tags: ["planning"]`.
- Feature scope? `topic: "nxs-001"`, or `tags: ["nxs-001"]`.

The schema is minimal. The semantics come from data conventions, not from field design.

### One Tool or Two?

The generic model suggests it could be ONE tool:

```
Tool: context
Parameters:
  topic: string?
  category: string?
  query: string?          ← presence triggers semantic mode
  tags: [string]?
  k: int?                 ← max results (semantic mode)
  max_tokens: int?
```

- `topic + category` (no query) → deterministic lookup
- `topic + category + query` → semantic search within scope
- `query` alone → broad semantic search
- `topic` alone → everything under that topic

One tool, two modes, driven by whether `query` is present. The tool description explains both modes.

**Alternatively**, keep two tools if testing shows Claude decomposes better with explicit tool boundaries. The generic model works either way.

---

## Revised Summary

| Constraint | Impact on Design |
|------------|-----------------|
| **No hardcoded roles** | Unimatrix is a generic context engine. Roles are data, not code. |
| **Deterministic vs. semantic** | Driven by presence of `query` parameter. No query = exact match. Query = similarity search. |
| **Practical hybrid** | Agent definitions keep core identity + hard rules. Unimatrix provides dynamic knowledge. Full spectrum supported — users choose how much to pull from Unimatrix. |
| **Generic query model** | `{ topic, category, query }` — three fields that can express any use case. No NDP-specific parameters. |
| **Widely applicable** | Same model works for NDP agents, DevOps teams, data scientists, product managers. No code changes per domain. |
| **Seed data, not hardcoded definitions** | Current agent knowledge becomes seed data. Unimatrix's code is domain-agnostic. |
