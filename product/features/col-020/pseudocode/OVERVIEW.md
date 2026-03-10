# col-020 Pseudocode Overview

## Components

| ID | Component | Crate | New/Modified |
|----|-----------|-------|-------------|
| C1 | session_metrics | unimatrix-observe | New file: `src/session_metrics.rs` |
| C2 | types | unimatrix-observe | Modified: `src/types.rs` |
| C3 | knowledge_reuse | unimatrix-server | Inline in handler (ADR-001) |
| C4 | store_api | unimatrix-store | Modified: `query_log.rs`, `injection_log.rs`, `read.rs`, `topic_deliveries.rs` |
| C5 | report_builder | unimatrix-observe | No code change (post-build mutation pattern) |
| C6 | handler_integration | unimatrix-server | Modified: `mcp/tools.rs` |

## Data Flow

```
ObservationRecord[] (existing load)
        |
        v
C1: compute_session_summaries() --> Vec<SessionSummary>
C1: compute_context_reload_pct() --> f64
        |
        v
C4: store.scan_sessions_by_feature() --> Vec<SessionRecord> (existing API)
C4: store.scan_query_log_by_sessions() --> Vec<QueryLogRecord> (new)
C4: store.scan_injection_log_by_sessions() --> Vec<InjectionLogRecord> (new)
C4: store.count_active_entries_by_category() --> HashMap<String, u64> (new)
        |
        v
C3: knowledge_reuse_computation (inline in handler)
    - join query_log entry IDs + injection_log entry IDs
    - filter: stored in session A, retrieved in session B (A != B)
    - deduplicate by entry ID
    - group by category
    - compute category gaps against active entries
    --> KnowledgeReuse
        |
        v
C6: handler assigns all new fields to report
C4: store.set_topic_delivery_counters() (idempotent absolute-set, ADR-002)
```

## Shared Types (C2, defined in unimatrix-observe/src/types.rs)

```
SessionSummary {
    session_id: String,
    started_at: u64,
    duration_secs: u64,
    tool_distribution: HashMap<String, u64>,
    top_file_zones: Vec<(String, u64)>,
    agents_spawned: Vec<String>,
    knowledge_in: u64,
    knowledge_out: u64,
    outcome: Option<String>,
}

KnowledgeReuse {
    tier1_reuse_count: u64,
    by_category: HashMap<String, u64>,
    category_gaps: Vec<String>,
}

AttributionMetadata {
    attributed_session_count: usize,
    total_session_count: usize,
}
```

## Build Order

1. **C2 (types)** -- all other components depend on these structs
2. **C4 (store_api)** -- batch queries needed by C3 and C6
3. **C1 (session_metrics)** -- pure computation, depends only on C2
4. **C5 (report_builder)** -- no code change, just verify post-build mutation works
5. **C3 (knowledge_reuse)** -- depends on C4 store methods + C2 types
6. **C6 (handler_integration)** -- wires everything together, depends on all above

## Naming Conventions from Architecture

- Spec says `AttributionCoverage`; architecture + implementation brief say `AttributionMetadata`. Use **`AttributionMetadata`** (architecture is authoritative per IMPLEMENTATION-BRIEF.md resolved decisions).
- Rework matching: case-insensitive substring (human override of FR-03.1).
