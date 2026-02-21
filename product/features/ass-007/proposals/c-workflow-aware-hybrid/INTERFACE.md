# Proposal C: MCP Interface

## Server Instructions

```
Unimatrix is the project's context engine. It stores expertise, conventions, decisions,
and process knowledge. Before starting implementation or design, call context_briefing
or context_search for relevant context. After completing work, store outcomes using
context_store with category "outcome". When corrected by the user, use context_correct.
```

## v0.1 Tools (Core + Outcome Tracking)

Proposal C ships outcome tracking from v0.1. It's the foundation for retrospectives.

### context_search

Semantic similarity search for discovery and exploration.

```
params:
  query: string (required) -- natural language search
  topic: string? -- filter to topic
  category: string? -- filter to category
  tags: string[]? -- filter by tags
  k: int? (default: 5) -- max results
  max_tokens: int? (default: 2000) -- response budget

annotations: { readOnlyHint: true }

response (content):
  Found 3 entries for "storage trait patterns" (showing top 3):

  1. Domain Adapter Pattern (0.94) [convention, active, confidence: 0.91]
     > All data sources implement core traits. See StoragePort in core/src/traits.rs.

  2. redb Write Pattern (0.87) [pattern, active, confidence: 0.85]
     > Use spawn_blocking with Arc<Database>. Single writer, unlimited readers.

  3. Embedding Storage Decision (0.82) [decision, active, confidence: 0.78]
     > 384d vectors stored in hnsw_rs dump files, metadata in redb.

response (structuredContent):
  { results: [{ id, content, topic, category, tags, status, confidence, similarity }],
    total_found: int, query: string }
```

### context_lookup

Deterministic metadata match. Same input always returns same results.

```
params:
  topic: string? -- exact match
  category: string? -- exact match
  tags: string[]? -- entries must have ALL listed tags
  id: string? -- exact entry by ID
  status: string? (default: "active") -- lifecycle filter
  limit: int? (default: 10)

annotations: { readOnlyHint: true }
```

### context_store

Store knowledge or outcome data. Auto-initializes project on first call. Dedup check on insert.

```
params:
  content: string (required) -- the knowledge to store
  topic: string (required) -- primary classification
  category: string (required) -- type: "convention", "decision", "pattern", "outcome", "process-proposal"
  tags: string[]? -- cross-cutting labels
  source: string? -- agent ID or human

annotations: { readOnlyHint: false }
```

### context_get

Retrieve full entry by ID (drill-down from search results).

```
params:
  id: string (required)

annotations: { readOnlyHint: true }
```

## v0.2 Tools (Lifecycle + Process Loop)

### context_correct

Supersede an entry with a corrected version. Creates audit trail.

```
params:
  original_id: string (required)
  content: string (required) -- corrected content
  reason: string? -- why the correction was needed

annotations: { destructiveHint: false } -- creates new, doesn't delete
```

### context_deprecate

Mark entry as deprecated without replacement.

```
params:
  id: string (required)
  reason: string?

annotations: { destructiveHint: false }
```

### context_briefing

Compound orientation tool. One call replaces 4-5 individual queries.

```
params:
  role: string (required) -- agent role name
  task: string (required) -- current task description
  phase: string? -- workflow phase
  feature: string? -- feature ID for scoping

annotations: { readOnlyHint: true }

response: assembled briefing with sections:
  ## Conventions (from lookup)
  ## Process Knowledge (from lookup, approved entries only)
  ## Relevant Patterns (from search)
  ## Recent Corrections (from lookup, correction chain)
```

### context_retrospective

**Proposal C's differentiator.** Triggers outcome analysis and generates process proposals.

```
params:
  feature: string (required) -- feature ID to analyze
  compare_with: string[]? -- prior feature IDs for trend comparison
  generate_proposals: bool (default: true)

annotations: { readOnlyHint: false } -- creates process-proposal entries

response (content):
  ## Retrospective: nxs-012

  ### Outcomes
  - Duration: 8 days, 3 waves, 5 agents
  - Quality: 2 bugs found post-merge, 1 rework cycle
  - Efficiency: 12 entries retrieved, 9 helpful (75%)

  ### Compared to nxs-010, nxs-011
  - 20% longer duration (wave 2 had 4 agents vs 2-3 in prior features)
  - Merge conflict rate: 3x higher in wave 2

  ### Process Proposals Generated (pending human review)
  1. [PP-001] Limit wave 2 to 3 agents (evidence: 3 features)
  2. [PP-002] Add storage trait pattern to seed data (searched 3x, not found)

  Review proposals: context_lookup(category: "process-proposal", status: "pending-review")
```

### context_status

Knowledge base health metrics.

```
params:
  topic: string? -- scope to topic
  category: string? -- scope to category

annotations: { readOnlyHint: true }

response: entry counts, age distribution, pending proposals count,
          outcome summary, stale entry candidates
```

## v0.3 Tools

- MCP Resources for passive context (conventions as resources)
- MCP Prompts (`/recall`, `/remember`, `/retro`)
- Local embedding model
- Cross-project knowledge sharing

## CLI Commands

```bash
unimatrix init                          # Create DB, append CLAUDE.md section
unimatrix seed --template rust-project  # Load baseline expertise entries
unimatrix status                        # DB health, entry counts, pending proposals
unimatrix retro <feature-id>            # Trigger retrospective from CLI
unimatrix proposals                     # List pending process proposals
unimatrix approve <proposal-id>         # Approve a process proposal
unimatrix reject <proposal-id> <reason> # Reject a process proposal
```

The `approve`/`reject` CLI commands are convenience wrappers around `context_correct`/`context_deprecate` for humans who prefer terminal over MCP.
