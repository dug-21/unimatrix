---
name: "get-pattern"
description: "Retrieve APPLICATION patterns (architecture, procedures, conventions) from the pattern store. Use BEFORE implementing to ensure consistency."
---

# Get Pattern - Retrieve Application Knowledge

## What This Skill Does

Retrieves established **application patterns** (architecture, procedures, conventions) for Unimatrix using multiple retrieval signals:

1. **Pattern Search** (primary) — Semantic similarity against the patterns table
2. **Recall with Certificate** (enriched) — Blends similarity + causal uplift + recency
3. **Learning Predict** (optional) — RL-based action recommendations from past episodes

**Use this BEFORE implementing anything** to ensure you follow project standards.

---

## Retrieval Methods

### Primary: Pattern Search

Search patterns by task description using semantic similarity.

**Parameters:**

| Parameter | Description | Default |
|-----------|-------------|---------|
| `task` | What you're looking for (semantic search) | required |
| `k` | Number of results | 10 |
| `threshold` | Minimum similarity (0-1) | 0 |
| `filters.taskType` | Filter by category | optional |
| `filters.minSuccessRate` | Minimum success rate | optional |
| `filters.tags` | Filter by tags | optional |

### Enhanced: Recall with Certificate

Blends three signals for richer retrieval: **similarity** (how well it matches), **causal uplift** (did using this lead to success?), and **recency** (how recent is the knowledge?).

| Parameter | Description | Default |
|-----------|-------------|---------|
| `query` | What you're looking for | required |
| `k` | Number of results | 12 |
| `alpha` | Weight for similarity (0-1) | 0.7 |
| `beta` | Weight for causal uplift (0-1) | 0.2 |
| `gamma` | Weight for recency (0-1) | 0.1 |

**Tuning Weights:**

| Scenario | alpha | beta | gamma |
|----------|-------|------|-------|
| Default (balanced) | 0.7 | 0.2 | 0.1 |
| Proven patterns only | 0.4 | 0.5 | 0.1 |
| Recent changes matter | 0.5 | 0.1 | 0.4 |

### Optional: Learning Predict

RL-based action recommendations based on what worked in past episodes. Requires a persistent learning session.

### Fallback: Reflexion Retrieve

If no patterns exist, search past experience episodes for similar work.

---

## Pattern Categories

| Category | Example Queries |
|----------|-----------------|
| Architecture | "domain adapter pattern", "hexagonal architecture" |
| Data Flow | "ingestion pipeline", "bronze silver gold" |
| Development | "add new stream", "implement source trait" |
| Deployment | "docker deployment", "container setup" |
| Troubleshooting | "data not appearing", "write errors" |
| Conventions | "naming conventions", "code organization" |

---

## Interpreting Results

| Field | Meaning |
|-------|---------|
| `ID` | Pattern identifier |
| `taskType` | Category (e.g., `architecture:domain-adapter`) |
| `Similarity` | How well it matches your query (0-1) |
| `Success Rate` | How often this pattern succeeded (0-100%) |
| `Approach` | The pattern content/description |
| `Uses` | Number of times used |

**High-value patterns**: Success Rate > 80% AND Similarity > 0.3

**Deprecated patterns**: Check reflexion episodes — patterns with reward=0.0 and success=false may be obsolete.

---

## Typical Workflow

```
1. Search patterns by task description (primary — always do this)
2. Enriched recall with causal scoring (enhanced — for important decisions)
3. RL prediction (optional — only if learning session exists)
4. Combine results — if conflicts, prefer patterns with high causal uplift
5. If nothing found — check reflexion episodes for past experiences
6. After work — record feedback via /reflexion
7. If new discovery — store via /save-pattern
```

**Minimum viable workflow**: Steps 1 + 6 (pattern search + reflexion).

---

## CRITICAL: Record Pattern Usage

After using a pattern, **always use the `/reflexion` skill** to record whether it helped. Without feedback, the system can't learn which patterns work.

---

## If No Patterns Found

1. Check pattern statistics
2. Search reflexion episodes for past experiences
3. Check file-based documentation in `docs/`
4. After implementing, store the new pattern via `/save-pattern`

---

## The Pattern Workflow

```
1. BEFORE work:  get-pattern  → Search for relevant patterns (THIS SKILL)
2. DURING work:  Apply the pattern, note what works/gaps
3. AFTER work:   reflexion    → Record if pattern helped (required)
                 save-pattern → Store NEW discoveries (if any)
```

---

## Related Skills

- **`save-pattern`** — Store NEW patterns after discovering reusable approaches
- **`reflexion`** — Record feedback on pattern effectiveness (REQUIRED after using patterns)
- **`pattern-manage`** — Delete, deprecate, update, deduplicate patterns (lifecycle management)

---

## What NOT to Use This For

| Don't Search For | Use Instead |
|------------------|-------------|
| Current swarm status | Coordination layer |
| Agent task state | Task tools |
| Temporary working memory | Session coordination |
| Session-specific context | Session coordination |

**Patterns are PERMANENT application knowledge, not transient swarm state.**
