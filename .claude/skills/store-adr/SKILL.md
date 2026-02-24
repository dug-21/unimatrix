---
name: "store-adr"
description: "Store an architectural decision record in Unimatrix. Use after producing ADRs to make them queryable by all agents."
---

# Store ADR — Write Architectural Decisions to Unimatrix

## What This Skill Does

Stores a full architectural decision record in Unimatrix using the `context_store` MCP tool. ADRs become semantically searchable and deterministically queryable by any agent via `/knowledge-search` or `/knowledge-lookup`.

**Use this AFTER writing each ADR file** — a file-only ADR that isn't in Unimatrix is incomplete work.

---

## How to Store a New ADR

Call the `context_store` MCP tool with these parameters:

| Parameter | Value | Notes |
|-----------|-------|-------|
| `title` | `"ADR-NNN: {decision title}"` | Match the ADR file title exactly |
| `content` | Full ADR text | All sections: Context, Decision, Consequences |
| `topic` | `"{feature-id}"` | e.g., `"nxs-001"`, `"vnc-002"` |
| `category` | `"decision"` | Always `"decision"` for ADRs |
| `tags` | `["adr", "{phase}", ...]` | Include phase prefix + domain tags |
| `source` | `"architect"` | Identifies the producing agent role |
| `agent_id` | Your agent ID | From your spawn prompt |

### Example

```
context_store(
  title: "ADR-003: bincode v2 serde-compatible path",
  content: "## Context\nThe storage engine needs serialization...\n## Decision\nUse bincode v2 with serde...\n## Consequences\n...",
  topic: "nxs-001",
  category: "decision",
  tags: ["adr", "nexus", "serialization", "bincode"],
  source: "architect",
  agent_id: "nxs-001-agent-2-architect"
)
```

---

## How to Deprecate an Existing ADR

When a new decision supersedes a prior ADR, you need two actions:

### Step 1: Find the old ADR

Use `/knowledge-search` or `/knowledge-lookup` to find the existing ADR:

```
context_search(query: "bincode serialization decision", category: "decision")
```
or
```
context_lookup(topic: "nxs-001", category: "decision", tags: ["adr"])
```

Note the old entry's ID from the results.

### Step 2: Store a deprecation notice

Call `context_store` with a deprecation entry:

| Parameter | Value |
|-----------|-------|
| `title` | `"DEPRECATED: ADR-NNN ({old-feature-id}) — {short reason}"` |
| `content` | Why it was deprecated, what supersedes it, the new ADR reference |
| `topic` | `"{old-feature-id}"` |
| `category` | `"decision"` |
| `tags` | `["adr", "deprecated", "superseded-by:{new-feature-id}"]` |
| `source` | `"architect"` |

### Step 3: Store the new ADR

Use the normal "How to Store a New ADR" flow above. Include a `supersedes` note in the content:

```
## Context
Previously, ADR-003 (nxs-001) chose bincode v2 serde path.
With the new serialization layer in nxs-005, this is superseded.
...
```

### Deprecation Limitation (v0.1)

The current MCP tools cannot change an existing entry's status directly. The deprecation notice is a NEW entry that documents the supersession. When agents search for ADRs in that domain, they will find both the original and the deprecation notice, giving them full context.

True status-change deprecation will arrive with v0.2 tools (`context_deprecate`).

---

## Tagging Conventions

Use consistent tags for discoverability:

| Tag Type | Examples |
|----------|----------|
| Phase prefix | `nexus`, `vinculum`, `collective`, `cortical` |
| Domain | `storage`, `serialization`, `mcp`, `embedding`, `security` |
| Cross-cutting | `error-handling`, `async`, `thread-safety`, `api-design` |
| Lifecycle | `deprecated`, `superseded-by:{feature-id}` |

Always include `adr` as a tag. Include the phase prefix that matches the feature's phase.

---

## Self-Verification

After calling `context_store`, verify the response:
- Confirms entry was stored (returns entry ID and summary)
- If you get a **near-duplicate warning**, review the existing entry — you may need to update rather than create
- Record the Unimatrix entry ID in your agent report for traceability

---

## What NOT to Store

| Don't Store | Why |
|-------------|-----|
| Draft decisions still under discussion | Store only after the ADR is finalized |
| Implementation details | ADRs capture the "why", not the "how" — code does that |
| Decisions made by other agents | You are the ADR authority; don't store others' decisions |
