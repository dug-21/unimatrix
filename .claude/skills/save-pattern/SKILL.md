---
name: "save-pattern"
description: "Store APPLICATION patterns (architecture, procedures, conventions) in the pattern store. NOT for swarm/transient memory."
---

# Save Pattern - Store Application Knowledge

## What This Skill Does

Stores **application patterns** to the **pattern store** with semantic embeddings. Patterns are searchable via `get-pattern`.

**Use this AFTER completing work** to share reusable knowledge with future agents.

---

## Pattern Record Structure

Each pattern has:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `taskType` | string | Yes | Category and name (e.g., `architecture:domain-adapter`) |
| `approach` | string | Yes | Full pattern content — what it does, how to use it |
| `successRate` | number | No | Confidence level 0-1 (default: 0.9 for proven patterns) |
| `tags` | array | No | Array of tags for filtering |

---

## Pattern Categories (taskType)

Use consistent `taskType` prefixes for categorization:

| Category | taskType Prefix | Examples |
|----------|-----------------|----------|
| Architecture | `architecture:` | `architecture:domain-adapter`, `architecture:data-layers` |
| Procedures | `procedure:` | `procedure:add-stream`, `procedure:deploy` |
| Implementation | `implementation:` | `implementation:etl-persistence` |
| Configuration | `configuration:` | `configuration:gitops`, `configuration:etcd` |
| Testing | `testing:` | `testing:integration`, `testing:csv-dimension` |
| Deployment | `deployment:` | `deployment:docker`, `deployment:resource-constraints` |
| Troubleshooting | `troubleshoot:` | `troubleshoot:data-issues`, `troubleshoot:write-errors` |
| Conventions | `conventions:` | `conventions:naming`, `conventions:code-style` |
| ETL | `etl:` | `etl:run-lifecycle`, `etl:persistence` |
| Data Quality | `data-quality:` | `data-quality:framework`, `data-quality:csv-patterns` |

---

## Best Practices

### 1. Check First

Always search before creating to avoid duplicates — use `/get-pattern` with the topic.

### 2. Be Specific

Include concrete details:
- **Good**: "Create config/base/streams/{id}/config.yaml with fields array containing name, source_path, unit"
- **Bad**: "Create a config file"

### 3. Include Tags

Add relevant tags for better searchability.

### 4. Reference Files

Mention actual code paths:
```
"Related files: core/src/traits.rs, docs/procedures/HOW_TO_ADD_STREAM.md"
```

### 5. Include Verification

How to confirm the pattern worked:
```
"Verify: Run cargo test, check logs for 'Source initialized'"
```

---

## Update vs. Create New

1. **Update in place** (preferred — no duplicate): Use `/pattern-manage` to update an existing pattern
2. **Deprecate old + create new** (when approach fundamentally changed): Deprecate the old pattern, then save the replacement
3. **Delete obsolete** (pattern references deleted code or is a duplicate): Use `/pattern-manage` to delete

---

## The Pattern Workflow

```
1. BEFORE work:  get-pattern  → Search for existing patterns
2. DURING work:  Note gaps, discover new approaches
3. AFTER work:   save-pattern → Store NEW discoveries (THIS SKILL)
                 reflexion    → Record if existing patterns helped
```

---

## Related Skills

- **`get-pattern`** — Search patterns BEFORE work (always check first)
- **`reflexion`** — Record feedback on pattern effectiveness
- **`pattern-manage`** — Delete, deprecate, update, deduplicate patterns

---

## What NOT to Use This For

| Don't Store | Use Instead |
|-------------|-------------|
| Swarm coordination state | Coordination layer |
| Agent task status | Task tools |
| Temporary working memory | Session coordination |
| Session-specific context | Session coordination |
| Feedback on patterns | `/reflexion` skill |

**Patterns are PERMANENT application knowledge, not transient swarm state.**
