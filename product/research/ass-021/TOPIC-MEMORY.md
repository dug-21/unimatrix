# ASS-021: Topic-Level Memory — Design Exploration

**Date:** 2026-03-16
**Status:** Exploratory — distinct scope from context_briefing rework

---

## Core Observation

The feature_cycle is currently a **label** scattered across tables. Everything needed to reconstruct a topic memory already exists:

| What | Where |
|------|-------|
| Keywords | `sessions.keywords` WHERE `feature_cycle = X` |
| Injected entries | `injection_log` JOIN `sessions` WHERE `feature_cycle = X` |
| Created entries | `FEATURE_ENTRIES` WHERE `feature_cycle = X` |
| Category distribution | above JOIN `entries` GROUP BY `category` |
| Outcomes | `OUTCOME_INDEX` WHERE `feature_cycle = X` |
| Session count | `sessions` GROUP BY `feature_cycle` |

**This is largely a join.** No new storage is required to have a complete topic picture — it can be assembled at read time.

The open question is not *whether* to store it, but **when and how to use it in real time**.

---

## Two Distinct Use Cases

### Use Case A: Retrospective / Post-hoc Analysis

Latency-insensitive. Join at query time. Already partly served by col-002 retrospective pipeline.

A topic view assembled after `cycle_stop` gives the retrospective:
- Full set of entries used during the feature
- Which entries were created (new knowledge produced)
- Session outcomes correlated with injection patterns
- Category distribution: what kinds of knowledge the feature relied on

This works today with a moderately complex SQL query. No new infrastructure needed.

### Use Case B: Real-Time — Seeding a New Session

When a new session starts on a related feature, pre-warming from a prior topic is high value. The access pattern:

1. Session starts with `feature_cycle = "crt-020"`
2. Query: "what topics are similar to crt-020?" → semantic search or keyword match against prior topics
3. Load the matched topic's `injected_ids` as co-access anchors
4. ContextSearch and CompactPayload start with a warm prior

This requires a prior topic to be **queryable** — either as a materialized Unimatrix entry (embeddable, searchable) or as a join executed once at session start and cached in memory.

**The join executed once at session start** is the lower-friction path:
- On `cycle_start`, fire a background query: "load prior sessions for this feature_cycle"
- Cache result in `SessionState.topic_prior` (or a new `FeatureTopicState`)
- ContextSearch and CompactPayload use it as co-access anchors or query enrichment

No new Unimatrix entry type needed for this. It's a cache of a join.

### Use Case C: Real-Time — Accumulating During Session

As the session runs, the topic state grows. Each ContextSearch call adds to `injected_ids`. Each context_store adds to `created_ids`. This is the in-memory accumulator pattern.

The accumulator enables:
- **Richer co-access**: co-access is currently per-session. A topic accumulator enables cross-session co-access for the same feature.
- **Injection dedup**: avoid re-injecting entries already well-covered in this feature's history
- **Adaptive query enrichment**: as more entries are injected, their keywords could enrich the session's semantic query

**But**: the injection log already captures what was injected. The co-access table already captures cross-session pairs. The accumulator may duplicate what's already persisted, just in memory for lower latency.

---

## The "Materialize to Unimatrix" Path

The more ambitious option: at `cycle_stop`, flush the topic state as a first-class Unimatrix entry.

```
Entry {
    category:  "topic",
    title:     "crt-019: context_briefing redesign",
    content:   structured summary of: keywords, entry refs, outcomes, category distribution,
    tags:      ["crt-019", "cortical"],
    embedding: embed(keywords + title)
}
```

This makes topics **semantically searchable** — "find features similar to this one" becomes a standard HNSW query. Cross-feature learning becomes possible without joins.

**New capability unlocked**: topic entry → co-access entries → "features that used these entries also used these other entries" — a higher-order knowledge graph.

**Cost**: new `"topic"` category in the schema. The category list is currently a loose string convention, so this is additive. The entry would need good embedding content to be useful in semantic search.

---

## Key Open Questions

**OQ-A: Join at session-start vs materialized entry**

Join at session-start is zero new schema, workable for known feature_cycles. Materialized entry enables similarity search across unknown/related features. Both can coexist: join for known features, materialized entry for semantic similarity to unknown ones.

**OQ-B: What is the real-time access pattern?**

- Does anything need topic history on the *hot path* (every prompt)?
- Or is it sufficient to load once at `cycle_start` and cache?

If load-once-and-cache is sufficient, the join approach works. If per-prompt enrichment is needed (accumulating injections affect the next prompt's search), then in-memory state is required.

**OQ-C: Cross-session co-access**

Currently co-access pairs are written per session (deduped by session_id). Topic-level co-access would aggregate across all sessions for a feature. This could be computed by the retrospective pipeline and written back as topic-level co-access weights. Does this buy enough signal to be worth the complexity?

**OQ-D: Is "topic" a first-class Unimatrix category, or a view?**

Making topic a category means it participates in search, confidence scoring, co-access — all the machinery. A view/join is lower friction but doesn't participate in the knowledge graph. This is the central architectural question for a future scope.

---

## Scope Boundary

This is **not** part of the context_briefing rework. The rework is about:
- categories/keywords as query parameters
- quality floors
- KnowledgeQuery unification

Topic-level memory is a separate feature that builds on top of the rework's foundations. Natural sequence: rework context_briefing first → topic memory as a follow-on that uses the improved query interface.

Candidate scope: `col-023` or `crt-020` depending on whether this is classified as orchestration learning or retrieval improvement.
