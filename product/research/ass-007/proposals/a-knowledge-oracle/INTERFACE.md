# Proposal A: Knowledge Oracle -- MCP Interface

## Server Instructions Field

```
Unimatrix is this project's knowledge engine. Before starting implementation or design
work, search for relevant conventions and patterns. After making architectural decisions,
discovering patterns, or establishing conventions, store them. When corrected by the user,
record the correction using context_correct. Do not store workflow state or process steps.
```

## v0.1 Tools

### context_search

Semantic similarity search across project knowledge.

```
name: context_search
description: "Search project knowledge by meaning. Use for discovery: finding patterns,
  exploring related decisions, checking what's known about a topic. NOT for looking up
  specific known conventions -- use context_lookup for that."
annotations: { readOnlyHint: true, openWorldHint: false }
params:
  query: string (required)     -- natural language search query
  topic: string (optional)     -- filter to topic (e.g., "error-handling", "auth")
  category: string (optional)  -- filter to category (e.g., "convention", "decision")
  tags: string[] (optional)    -- filter to entries with ALL specified tags
  k: int (optional, default 5) -- max results
  max_tokens: int (optional, default 2000) -- response budget
```

**Response (content -- compact markdown):**
```
Found 7 entries for "error handling patterns" (showing top 3):

1. [e:42] Error Handling Convention (0.94) [convention, active]
   > Use anyhow for application errors, thiserror for library errors.
   > Never use unwrap() in production code.

2. [e:108] CoreError Enum Pattern (0.89) [pattern, active]
   > Define CoreError with variants per domain. Use map_err at boundaries.

3. [e:67] Error Testing Convention (0.81) [convention, active] CORRECTED 2026-02-15
   > Every public Result-returning function needs error path tests.
   > (Supersedes e:31 -- was "only test happy path")

Apply these conventions to your current work. Use context_get for full details.
```

**Response (structuredContent):**
```json
{
  "results": [
    { "id": 42, "title": "Error Handling Convention", "similarity": 0.94,
      "category": "convention", "status": "active", "confidence": 0.92,
      "excerpt": "Use anyhow for application errors...",
      "supersedes": null, "correction_note": null }
  ],
  "total_found": 7, "returned": 3, "truncated": true
}
```

### context_lookup

Deterministic metadata-based retrieval. Same query always returns same results.

```
name: context_lookup
description: "Look up known project knowledge by exact metadata. Use for specific
  conventions, duties, rules, decisions. NOT for discovery -- use context_search for that."
annotations: { readOnlyHint: true, openWorldHint: false }
params:
  topic: string (optional)
  category: string (optional)
  tags: string[] (optional)
  id: int (optional)          -- fetch specific entry
  status: string (optional, default "active") -- "active"|"deprecated"|"all"
  limit: int (optional, default 10)
```

**Response format:** Same structure as context_search but without similarity scores. Ordered by confidence descending, then creation date descending.

### context_store

Store a new knowledge entry.

```
name: context_store
description: "Store a convention, decision, pattern, or lesson for future reference.
  The system checks for near-duplicates automatically. Do NOT store workflow state,
  process steps, or agent instructions."
annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false }
params:
  content: string (required)    -- the knowledge to store (markdown)
  topic: string (required)      -- primary topic
  category: string (required)   -- knowledge type (convention, decision, pattern, lesson-learned, etc.)
  tags: string[] (optional)     -- cross-cutting labels
  title: string (optional)      -- short title (server generates from content if omitted)
  source: string (optional)     -- provenance ("agent:ndp-architect", "user", "retrospective")
```

**Response:**
```
Stored entry e:142 "JWT Auth Decision" [topic: auth, category: decision]
Confidence: 0.70 (new entry, unvalidated)

Near-duplicate check: No similar entries found.
```

**Dedup case response (isError: false but advisory):**
```
Near-duplicate detected. Existing entry e:89 (similarity: 0.94):
  "Use JWT with RS256 for API authentication"

Store anyway? Call context_store again with force: true, or use context_correct
to supersede e:89 if this is an update.
```

### context_get

Retrieve full entry by ID. The drill-down from search results.

```
name: context_get
description: "Get the full content of a specific entry by ID. Use after context_search
  returns an excerpt you need more detail on."
annotations: { readOnlyHint: true, openWorldHint: false }
params:
  id: int (required)
```

**Response:** Full entry content, all metadata, correction chain (if any), usage count, timestamps.

## v0.2 Tools

### context_correct

```
name: context_correct
description: "Supersede an existing entry with a corrected version. The original is
  preserved (deprecated) with a link to the correction. Use when knowledge was wrong."
annotations: { readOnlyHint: false, destructiveHint: false }
params:
  original_id: int (required)
  content: string (required)
  reason: string (optional)
  topic: string (optional)     -- inherit from original if omitted
  category: string (optional)  -- inherit from original if omitted
  tags: string[] (optional)    -- inherit from original if omitted
```

### context_deprecate

```
name: context_deprecate
description: "Mark an entry as deprecated without replacement. Use when knowledge is
  no longer relevant (framework removed, pattern abandoned)."
annotations: { readOnlyHint: false, destructiveHint: false }
params:
  id: int (required)
  reason: string (optional)
```

### context_status

```
name: context_status
description: "View knowledge base health metrics. Shows entry counts, age distribution,
  duplicate candidates, and stale entries."
annotations: { readOnlyHint: true }
params:
  topic: string (optional)   -- filter to topic
  category: string (optional)
```

### context_briefing

```
name: context_briefing
description: "Compile an orientation briefing for an agent about to start work. Returns
  relevant duties, conventions, recent decisions, and patterns in one response. Designed
  for orchestrators building spawn prompts."
annotations: { readOnlyHint: true }
params:
  role: string (required)    -- agent role (e.g., "rust-dev", "architect")
  task: string (required)    -- task description for semantic matching
  feature: string (optional) -- feature ID for scoping
  max_tokens: int (optional, default 3000)
```

**Internally executes:** lookup(topic: role, category: "convention") + lookup(topic: role, category: "duties") + search(query: task, k: 3). Assembles into one response.

## CLI Commands

```
unimatrix init           -- append 5-line config to CLAUDE.md, create data dir
unimatrix status         -- print knowledge base stats (entry count, health)
unimatrix export         -- dump all entries as JSON (backup, migration)
unimatrix import <file>  -- import entries from JSON dump
unimatrix rebuild-index  -- full hnsw_rs rebuild (after many deprecations)
```

No CLI commands for storing or searching -- that's the MCP interface's job.

## Tool-to-Knowledge-Type Mapping

| Knowledge Type | Store Via | Retrieve Via |
|---------------|-----------|-------------|
| Convention | context_store(category: "convention") | context_lookup or context_search |
| ADR / Decision | context_store(category: "decision") | context_lookup(category: "decision") |
| Pattern | context_store(category: "pattern") | context_search(query: ...) |
| Lesson learned | context_store(category: "lesson-learned") | context_lookup(category: "lesson-learned") |
| Correction | context_correct(original_id: ...) | Transparent -- search follows chain |
| Domain fact | context_store(category: "reference") | context_lookup or context_search |
| Agent briefing | N/A (composed from other types) | context_briefing(role: ...) |
