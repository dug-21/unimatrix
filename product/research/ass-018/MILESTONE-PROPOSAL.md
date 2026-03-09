# Milestone: Activity Intelligence

## Position in Roadmap

```
Intelligence Sharpening (6/7 done) → Activity Intelligence → Graph Enablement → Platform Hardening
```

Slots between Intelligence Sharpening and Graph Enablement. Prerequisite: Intelligence Sharpening substantially complete (it is). No dependency on Graph Enablement.

## Why This Milestone

The observation pipeline captures 3,200+ events/day but can't connect them to anything. Sessions have no topic attribution. User prompts are discarded. Query text is never stored. The retrospective pipeline — designed to be the feedback loop that makes Unimatrix learn — has been broken since col-012.

Intelligence Sharpening fixed the *knowledge* pipeline (confidence, retrieval, extraction). Activity Intelligence fixes the *activity* pipeline — connecting sessions to topics, capturing what's missing, and enabling analysis that crosses session boundaries.

## Concrete Capabilities Delivered

1. Sessions automatically attributed to topics on close (not days later during retrospective)
2. Full retrospective works again — better than pre-col-012 (multi-session, cross-session patterns)
3. User prompts stored as observations (richest signal for intent and topic detection)
4. Search query text captured alongside results (search quality evaluation, gap detection)
5. Topic-level view across sessions (how long did this topic take? how many sessions?)
6. Knowledge effectiveness measurable (which entries actually help? which are dead weight?)
7. Query data exportable for embedding tuning

## Features

### Wave 1 — Fix the Data Pipeline (parallel, no dependencies)

#### col-017: Hook-Side Topic Attribution
**Delivers**: Sessions automatically linked to topics. Retrospective fast path works.

The hook binary extracts topic signals from tool inputs (file paths, prompt text) per-event using existing `extract_from_path()` and `extract_feature_id_pattern()` functions. Sends `topic_signal: Option<String>` with each RecordEvent. Server accumulates signals per session. On SessionClose, resolves dominant topic via majority vote → UPDATE sessions SET feature_cycle. Falls back to full content-based attribution if no signals.

Also persists attribution results from `context_retrospective` fallback path (currently runs attribution but discards the result).

Touches: unimatrix-server (hook.rs, listener.rs), unimatrix-observe (attribution.rs).

New column: `observations.topic_signal TEXT` (nullable, ALTER TABLE).

#### col-018: UserPromptSubmit Dual-Route
**Delivers**: User prompts stored as observations. Richest signal for topic detection and intent classification.

Currently UserPromptSubmit → ContextSearch only (prompt text discarded from observation record). Change: store prompt as observation (hook="UserPromptSubmit", tool=NULL, input=prompt JSON) AND dispatch to ContextSearch. New HookRequest variant `PromptAndSearch` or intercept in dispatch.

Touches: unimatrix-server (hook.rs build_request, listener.rs dispatch).

#### col-019: PostToolUse Response Capture
**Delivers**: Response size and snippet data for all PostToolUse events. Unblocks 8+ detection rules and context-load metrics.

Investigate field name mismatch between what Claude Code sends and what `extract_observation_fields()` expects. Fix the mapping. Enables: `total_context_loaded_kb`, `edit_bloat_total_kb`, `context_load_before_first_write_kb`, ContextLoadRule, EditBloatRule.

Touches: unimatrix-server (listener.rs extract_observation_fields).

GH issue: #164.

### Wave 2 — Connect & Capture (depends on Wave 1 for topic attribution)

#### nxs-010: Activity Schema Evolution
**Delivers**: Topic deliveries table, query log table. Schema v10.

New tables:
- `topic_deliveries` — groups sessions by topic, tracks aggregate counters (total_sessions, total_tool_calls, total_duration_secs), lifecycle status. Auto-created on first session attribution.
- `query_log` — captures search query text + result metadata (result_count, entry_ids, similarity_scores, retrieval_mode, source). Written by ContextSearch handler.

Schema migration v9→v10. Backfill: run attribution on all existing unattributed sessions, create topic_deliveries rows.

Touches: unimatrix-store (db.rs, new modules), unimatrix-server (listener.rs search handler).

#### col-020: Multi-Session Retrospective
**Delivers**: Retrospective that spans all sessions for a topic. Cross-session pattern detection.

With topic attribution (col-017) and topic_deliveries (nxs-010), `context_retrospective` can now aggregate across sessions. New metrics:
- `context_reload_pct` — Read calls in session N+1 repeating session N
- `knowledge_reuse_rate` — injected entries reappearing across sessions
- `session_efficiency_trend` — tool_calls/duration improving or degrading
- `rework_session_count` — sessions with outcome="rework" per topic

Updates topic_deliveries aggregate counters after computation.

Touches: unimatrix-observe (metrics.rs, report.rs, source.rs), unimatrix-server (observation.rs, tools.rs).

### Wave 3 — Intelligence & Export (depends on Wave 2)

#### crt-018: Knowledge Effectiveness Analysis
**Delivers**: Measurable knowledge utility. Identifies dead entries, validates confidence calibration.

With query_log + injection_log + session outcomes, compute per-entry effectiveness:
- Entry utility score: helpful_signals / total_injections
- Time-to-first-use: days between creation and first injection
- Confidence calibration: bin entries by confidence → compare to actual helpfulness rate
- Dead knowledge detection: entries never injected, or injected but never helpful

Exposes via `context_status` with new effectiveness section. Feeds into confidence evolution (entries with low utility get confidence decay).

Touches: unimatrix-observe (new module), unimatrix-server (status service).

#### crt-019: Search Quality & Gap Detection
**Delivers**: Identifies what agents search for and can't find. Evaluates retrieval quality.

With query_log data:
- Zero-result queries → knowledge gap candidates
- Query reformulation detection (same session, similar queries) → first search missed
- Result utilization rate (entries returned → actually used in subsequent tool calls)
- Search miss rate by topic/category

Exposes gaps via `context_status`. Can generate candidate entries for gaps (title + suggested topic).

Touches: unimatrix-observe (new module), unimatrix-server (status service).

#### col-021: Query Data Export
**Delivers**: Exportable (query, results, outcome) triples for embedding model tuning.

Export pipeline from query_log + injection_log + signal_queue:
```json
{
  "query": "how does confidence scoring work",
  "positive_entries": [42, 67],
  "negative_entries": [15, 23],
  "similarity_scores": {"42": 0.87, "67": 0.82}
}
```

New MCP tool or CLI subcommand: `context_export_training_data(format, since, limit)`.

Touches: unimatrix-server (new tool or subcommand), unimatrix-observe.

---

## Feature Summary

| Wave | Feature | Phase | Delivers |
|------|---------|-------|----------|
| 1 | col-017 | col | Hook-side topic attribution, session→topic linking |
| 1 | col-018 | col | UserPromptSubmit stored as observation |
| 1 | col-019 | col | PostToolUse response_size + response_snippet capture (#164) |
| 2 | nxs-010 | nxs | topic_deliveries + query_log tables, schema v10 |
| 2 | col-020 | col | Multi-session retrospective with cross-session metrics |
| 3 | crt-018 | crt | Knowledge effectiveness scoring + dead knowledge detection |
| 3 | crt-019 | crt | Search quality analysis + knowledge gap detection |
| 3 | col-021 | col | Training data export for embedding tuning |

## Naming Alignment Note

This milestone introduces `topic` as the universal grouping concept, aligning knowledge-side `entries.topic` with activity-side `sessions.feature_cycle`. New tables use `topic`. Existing `feature_cycle` columns remain for backward compatibility. See DATA-INVENTORY.md §3 for full rationale.

## Size Estimate

~160 MB database at 60 days with all new tables (from real data projection). New tables add ~4 MB to the 157 MB baseline. SQLite handles this trivially.
