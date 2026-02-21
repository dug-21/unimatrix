# Proposal B: Dynamic Control Plane -- Interface

## Server Instructions Field

```
Unimatrix is this project's context engine and control plane. It stores knowledge,
conventions, protocols, role definitions, and process rules.

Before starting work: search for relevant conventions and patterns.
After decisions or corrections: store them immediately.
When a process caused friction: store a retrospective entry.
When corrected by the user: use context_correct to fix the original entry.

Categories: knowledge, convention, decision, pattern, retrospective, protocol, role, routing, rule, skill, constitutional.
```

## MCP Tools

### v0.1 Core (4 tools -- same as baseline)

**context_search** -- Semantic similarity search across all entries.
```
params:
  query: string (required)     -- natural language search
  topic: string?               -- filter by topic
  category: string?            -- filter by category
  tags: string[]?              -- filter by tags
  type: string?                -- filter by entry type (knowledge|protocol|role|...)
  k: u32? = 5                  -- max results
  max_tokens: u32? = 2000      -- response budget
annotations: { readOnlyHint: true }
response: compact markdown in content, JSON in structuredContent
```

**context_lookup** -- Deterministic metadata match. No embeddings.
```
params:
  topic: string?
  category: string?
  tags: string[]?
  type: string?
  id: string?
  status: string? = "active"
  limit: u32? = 10
annotations: { readOnlyHint: true }
```

**context_store** -- Store an entry with metadata and embedding.
```
params:
  content: string (required)
  topic: string (required)
  category: string (required)
  tags: string[]?
  type: string? = "knowledge"  -- knowledge|convention|decision|pattern|retrospective
annotations: { readOnlyHint: false }
note: type=protocol|role|routing|rule|skill|constitutional reserved for CLI operations
```

**context_get** -- Retrieve full entry by ID.
```
params:
  id: string (required)
annotations: { readOnlyHint: true }
```

### v0.2 Lifecycle + Process (5 tools)

**context_correct** -- Supersede an entry with a corrected version.
```
params:
  original_id: string (required)
  content: string (required)
  reason: string?
annotations: { destructiveHint: true }
```

**context_deprecate** -- Mark entry as deprecated without replacement.
```
params:
  id: string (required)
  reason: string?
annotations: { destructiveHint: true }
```

**context_status** -- Knowledge base health metrics.
```
params:
  topic: string?
  type: string?
annotations: { readOnlyHint: true }
response: entry counts by status/type, stale entries, duplicate candidates,
          pending corrections awaiting review
```

**context_briefing** -- Compound orientation for orchestrator-passes-context pattern.
```
params:
  role: string (required)      -- agent role name
  task: string (required)      -- task description for semantic matching
  phase: string?               -- workflow phase (filters protocols)
  feature: string?             -- feature ID (filters scope)
annotations: { readOnlyHint: true }
response: assembled from lookup(role duties) + lookup(phase protocol) +
          lookup(role rules) + search(task patterns). Single response, one tool call.
```

**context_export** -- Trigger export from within MCP (for orchestrator agents).
```
params:
  dry_run: bool? = true
  filter_type: string?         -- limit to specific entry type
annotations: { readOnlyHint: false }
response: list of files that would be written/changed, with diffs if dry_run=true
note: actual write requires dry_run=false. Safety: agents default to dry_run.
```

### v0.3 Sophistication

- MCP Resources for passive convention surfacing
- MCP Prompts (`/recall`, `/remember`, `/export`)
- Local embedding model
- Cross-project knowledge sharing
- `context_propose` -- agents propose protocol changes (queued for review)

## CLI Commands

The CLI is a first-class interface, not a convenience wrapper.

### Project Lifecycle
```
unimatrix init                              # create DB, scan CLAUDE.md for constitutional rules
unimatrix seed --from .claude/              # import existing .claude/ files as entries
unimatrix seed --template <name>            # seed from published template pack
unimatrix export [--dry-run] [--filter]     # regenerate .claude/ from DB
unimatrix status                            # pending corrections, checksum mismatches, health
unimatrix diff                              # preview what export would change
```

### Entry Management
```
unimatrix list [--type] [--topic] [--status]
unimatrix show <topic-or-id>                # render entry to terminal
unimatrix edit <topic-or-id>                # open in $EDITOR, save on close
unimatrix log <topic-or-id>                 # version history with diffs
unimatrix search <query> [--type] [--topic]
```

### Correction Management
```
unimatrix correct <topic-or-id> --reason "..." # create correction (opens $EDITOR)
unimatrix revert <topic-or-id> --to <version>
unimatrix review                            # interactive: approve/reject pending auto-corrections
unimatrix review --approve-all              # batch approve
```

### Configuration
```
unimatrix config set correction.auto_threshold 3
unimatrix config set correction.require_review true
unimatrix config set export.auto_on_correct false   # auto-export after corrections
unimatrix config show
```

## Export/Sync Mechanism

Export is explicit, never implicit. The workflow:

1. Entries change (via MCP tools, CLI edits, or auto-corrections)
2. `unimatrix status` shows "3 entries changed since last export"
3. `unimatrix diff` shows exact file-level diffs
4. `unimatrix export` writes files
5. Git commit captures the change

Optional: `unimatrix config set export.auto_on_correct true` triggers export after every correction. Useful in CI, dangerous in interactive sessions.
