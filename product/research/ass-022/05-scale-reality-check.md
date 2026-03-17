# ASS-022/05: Scale Reality Check — Local MCP vs. Larger Deployments

**Date**: 2026-03-16
**Type**: Architectural constraint analysis
**Question**: What actually makes sense at local MCP scale, and how do we handle
              SQLite single-writer as background intelligence grows?

---

## 1. The Deployment Reality

Unimatrix's current deployment model:

- **One binary** per repository, running as an MCP server on a developer's machine
- **One knowledge base** per project, typically 100–5,000 entries at maturity
- **Handful of concurrent agents** — 1-4 at any given time
- **Lightweight background ticks** — confidence recomputation, co-access cleanup, graph compaction
- **No infrastructure** — this is the product's core value proposition

This is very different from "air quality monitoring across 500 sensors" or "regulatory compliance platform for a 50-person team." The innovations in 04 are architecturally correct, but most only pay off at scales that the current deployment model doesn't reach.

---

## 2. What Actually Makes Sense at Local Scale

### Tier the additions by the scale at which they return value

| Addition | Minimum scale to be useful | Makes sense now? |
|----------|---------------------------|-----------------|
| Config externalization (categories, half-life, instructions) | 1 entry | **Yes — highest priority** |
| Typed relationship graph | 20+ entries | **Yes — pays off immediately** |
| NLI contradiction detection | 50+ entries | **Yes — small async model, writes are rare** |
| GNN confidence learning | 100+ helpfulness votes | **No — not enough signal at local scale** |
| Knowledge synthesis | 200+ clustered entries | **No — premature at project scale** |
| Two-database split | 3+ concurrent writers | **Conditionally yes — if adding NLI/graph edge writes** |

**The honest list for local MCP right now**: config externalization + typed graph + NLI. That's it. The GNN and synthesis are right for a hosted or team-shared deployment model — not for a single developer's project knowledge base.

The GNN especially requires a feedback loop: meaningful `helpful_count`/`unhelpful_count` signals accumulate over months of real use. Before that data exists, the GNN just learns noise. The existing Bayesian priors already handle cold start well — the GNN only supersedes them once you have 50+ voted entries, which a typical project knowledge base might reach after 3-6 months of active use.

### What Config Externalization Unlocks at Local Scale

Even without GNN, externalizing the freshness half-life lets different domain deployments configure the right decay rate. An SRE team's runbook knowledge base sets a 30-day half-life. An air quality deployment sets per-category rates. This single change (a constant → a config value) is the most valuable single thing that can be done for domain agnosticism, and it's one hour of work.

---

## 3. The SQLite Single-Writer Problem

### The Current Situation

SQLite WAL mode allows concurrent reads. But there is exactly one writer at a time. Currently competing for writes:

1. **MCP request path** — `context_store`, `context_correct`, `context_deprecate` (user-visible, latency-sensitive)
2. **Fire-and-forget embedding** — runs in `spawn_blocking` after entry creation
3. **Maintenance tick** — confidence recomputation, co-access cleanup, graph compaction (~30s cycle)
4. **Observation hook processing** — session writes, injection log, signal queue
5. **Audit log** — written on every MCP operation (append-only but still a write)

If we add NLI edge writes and graph edge persistence, that's two more writers competing for the lock. For a developer's machine running one agent at a time, this is manageable. For a team shared instance or a real-time system (environmental monitoring triggering writes on sensor events), the single writer becomes a genuine bottleneck.

### The Right Answer: Two-Database Split

The insight: not all writes are equal. There are two fundamentally different classes of writes:

**Hot writes** — integrity-critical, user-visible, latency-sensitive:
- Entry creation/modification (entries, entry_tags, counters)
- Audit log entries
- Agent registry updates
- Vector map updates

**Cool writes** — learned signals, latency-tolerant, batchable:
- Graph edges (relationship graph)
- Co-access pair updates
- Confidence weight updates (GNN output)
- Observation metrics and session data
- Query log
- Injection log

Split these into two SQLite files:

```
~/.unimatrix/{project_hash}/
  knowledge.db    ← hot writes: the integrity chain
  analytics.db    ← cool writes: the learning layer
```

**`knowledge.db` tables** (integrity-critical):
```
entries            ← the knowledge records + hash chain
entry_tags         ← tag index
vector_map         ← HNSW ↔ entry ID mapping
counters           ← monotonic counters
agent_registry     ← trust and capabilities
audit_log          ← immutable event record
```

**`analytics.db` tables** (learning layer):
```
graph_edges        ← typed relationships (new)
co_access          ← co-retrieval pairs
confidence_weights ← GNN output (new; currently stored as constants)
observation_metrics ← feature cycle rollups
observation_phase_metrics
observations       ← raw hook events
sessions           ← session lifecycle
injection_log      ← briefing injection history
signal_queue       ← intra-process signals
query_log          ← search history
shadow_evaluations ← contradiction scan results
feature_entries    ← feature ↔ entry mapping
topic_deliveries   ← topic attribution
outcome_index      ← outcome tracking
```

### Why This Works

MCP request path writes **only** to `knowledge.db`. It gets exclusive access to the writer lock with minimal contention — just the main request and the audit log appender.

Background processes (embedding, maintenance tick, NLI edge detection, observation processing) write **only** to `analytics.db`. They can contend with each other freely without ever blocking a user-visible MCP operation.

The two databases are joined at read time — searches already do application-level result merging (confidence re-ranking, co-access boosting, graph penalties). The data sources being in two files doesn't change the join logic, only which connection each query hits.

**Crash safety**: each database maintains its own WAL. A crash during an analytics write doesn't affect the integrity chain in `knowledge.db`. The worst case is a slightly stale graph edge or confidence weight — which self-heals on the next maintenance tick. A crash during a `knowledge.db` write is handled exactly as it is today (WAL rollback, ACID guarantees, audit log append-only safety).

**Atomic cross-database operations**: currently the server does combined transactions spanning entries + indexes + vector_map + audit_log. With the split, `analytics.db` writes are *never* part of these combined transactions — they are always asynchronous, post-commit side effects. This is already the model for most analytics writes (co-access updates happen after the query completes, not during). The split formalizes this pattern.

### Write Priority Queue

Even within `analytics.db`, multiple background workers may compete. The right model is a single-channel write queue in front of `analytics.db`:

```rust
// Single tokio::mpsc channel; one dedicated writer task drains it
enum AnalyticsWrite {
    CoAccess { id_a: u64, id_b: u64 },
    GraphEdge { from: u64, to: u64, rel: RelationType, weight: f32 },
    ObservationEvent { session_id: String, hook: HookType, ... },
    ConfidenceWeightUpdate { weights: WeightVector },
    QueryLogEntry { ... },
    // etc.
}
```

All background workers send to the channel. One dedicated async task drains it and batches writes into `analytics.db` transactions (commit every 50 events or 500ms, whichever comes first). This:
- Eliminates write contention entirely within `analytics.db`
- Allows batch commits (dramatically faster than individual transactions)
- Provides natural backpressure (bounded channel)
- Keeps the write logic simple and auditable

For `knowledge.db`: writes continue as today (individual transactions per MCP operation), but now with zero competition from background processes.

### What This Means for Real-Time Systems

The two-database split + write queue is what makes Unimatrix viable for real-time data systems (environmental monitoring, IoT, SRE incident response). In these contexts:

- Sensor events or hook events are high-frequency (`analytics.db` writes via queue — no blocking)
- Knowledge lookups during an active incident need to be fast (`knowledge.db` reads — no write contention)
- Knowledge stores during an incident are user-visible (`knowledge.db` write — isolated, fast)
- NLI processing happens asynchronously after the store (`analytics.db` edge write — queued, non-blocking)

The pattern: **the integrity chain is always fast because nothing else competes with it**.

---

## 4. The Coherent Minimum

Putting both concerns together, the coherent minimum set of changes that preserves the deployment model and solves real problems:

### Phase 0: Foundation (do these first, ~2-3 days)

1. **Config externalization** — categories, freshness half-life, server instructions to a TOML config. No schema changes. Domain agnosticism unlocked.
2. **Two-database split** — `knowledge.db` + `analytics.db`. Write queue for analytics. Resolves the write contention problem now and forever. Opens the door for all background intelligence without ever touching MCP latency.

### Phase 1: Intelligence Additions (~1-2 weeks, after Phase 0)

3. **Typed relationship graph** — `RelationEdge` + `GRAPH_EDGES` in `analytics.db`. Persisted, attributed, typed. Supersession + Contradicts + Supports.
4. **NLI contradiction detection** — small ONNX model, runs post-store, writes `Contradicts`/`Supports` edges via the write queue. No hot-path impact.

### Deferred until scale demands it

5. **GNN confidence learning** — only meaningful after 100+ helpfulness votes per deployment. Designed now, shipped when real-world usage generates the training signal. The two-database split and write queue are the prerequisites — with those in place, the GNN training loop drops in cleanly.
6. **Knowledge synthesis** — only at >200 clustered entries. Same story: design now, ship at scale.
7. **Scalability tiers** — read replicas → topic sharding → federation. The two-database split is a prerequisite; once that's in place, each tier is a small incremental step.

---

## 5. What This Looks Like for Different Deployment Models

| Deployment | Phase 0 | Phase 1 | GNN/Synthesis | Scalability |
|-----------|---------|---------|--------------|-------------|
| Local dev (current) | Essential | Nice to have | Not yet | Not needed |
| Team shared instance | Essential | Essential | Maybe (6+ months) | Tier 1 (read replicas) |
| Environmental monitoring | Essential | Essential | Yes (high event volume) | Tier 2 (topic sharding) |
| Multi-org federation | Essential | Essential | Yes | Tier 3 (federation) |

The Phase 0 changes don't change the deployment model at all — same binary, same SQLite, same MCP interface. The two-database split is an internal implementation detail invisible to MCP clients.

Phase 1 adds two ONNX models to the binary (~400MB total with models bundled). This is the only user-visible change — slightly larger binary, slightly richer search results (contradictions caught, support chains visible).

---

## 6. The Bottom Line

You're right: steps 3 (GNN) and 4 (synthesis) from `04-minimum-viable-expansions.md` are premature for the local MCP deployment model. They're architecturally correct and belong in the design, but they need more data than a single-developer project knowledge base generates in a reasonable time.

The actually-minimum-viable set is: **config externalization + two-database split + typed graph + NLI**. That's it.

The two-database split is the unblocking move. It resolves the write contention problem, opens the door for all future background intelligence, and doesn't change anything from the outside. Every new background process — NLI, GNN, synthesis, whatever comes next — just sends to the analytics write queue. The integrity chain never contends with any of it.

The structural insight: **separate "things that affect trust" (knowledge.db) from "things that affect score" (analytics.db)**. Trust is synchronous, user-visible, integrity-critical. Score is asynchronous, latency-tolerant, eventually consistent. These two classes of data have fundamentally different write requirements and should never compete for the same lock.
