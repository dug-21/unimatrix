# col-010 Pseudocode Overview

Feature: Session Lifecycle Persistence & Structured Retrospective
Stage: 3a — Component Design
Date: 2026-03-02

---

## Components Involved

| Component | Priority | Files |
|-----------|----------|-------|
| storage-layer | P0 | `schema.rs`, `sessions.rs` (new), `injection_log.rs` (new), `migration.rs` |
| uds-listener | P0 | `uds_listener.rs` |
| session-gc | P0 | `sessions.rs` (gc_sessions), `tools.rs` (maintain path) |
| auto-outcomes | P0 | `outcome_tags.rs`, `uds_listener.rs` |
| structured-retrospective | P1-adjacent | `structured.rs` (new), `types.rs`, `report.rs`, `tools.rs` |
| tiered-output | P1 | `types.rs`, `wire.rs`, `tools.rs` |
| lesson-learned | P1 | `tools.rs`, `confidence.rs` |

## Build Order (P0 first)

```
1. storage-layer   — foundation; all other components depend on it
2. uds-listener    — writes to SESSIONS and INJECTION_LOG
3. session-gc      — reads SESSIONS to mark/delete; cascade to INJECTION_LOG
4. auto-outcomes   — writes outcome entries from SessionClose; needs VALID_TYPES update
5. structured-retrospective — reads SESSIONS + INJECTION_LOG; new observe entry point
6. tiered-output   — server-side evidence_limit; additive to RetrospectiveReport
7. lesson-learned  — fire-and-forget embed + provenance boost
```

---

## Data Flow Between Components

```
Hook events (UDS)
    │
    ▼
uds_listener.rs
    ├─ SessionRegister ──► insert_session()  ──► SESSIONS table
    ├─ SessionClose  ───► update_session()  ──► SESSIONS table
    │                     auto-outcome  ──────► ENTRIES table (via insert_entry)
    └─ ContextSearch ───► insert_injection_log_batch() ──► INJECTION_LOG table
                                                            COUNTERS["next_log_id"]

SESSIONS + INJECTION_LOG
    │
    ├─► scan_sessions_by_feature() ──► structured-retrospective
    │       └─► from_structured_events() ──► RetrospectiveReport
    │                                         + narratives (Layer 2)
    │
    └─► gc_sessions() (maintain=true) ──► GcStats

context_retrospective tool (tools.rs)
    ├─ structured path: from_structured_events() → apply evidence_limit → return
    ├─ JSONL fallback: build_report() (existing)
    └─ lesson-learned fire-and-forget: tokio::spawn embed + store
```

---

## Shared Types Introduced or Modified

### New Types (storage-layer)

```
SessionRecord { session_id, feature_cycle, agent_role, started_at, ended_at,
                status, compaction_count, outcome, total_injections }

SessionLifecycleStatus { Active, Completed, TimedOut, Abandoned }

InjectionLogRecord { log_id, session_id, entry_id, confidence, timestamp }

GcStats { timed_out_count, deleted_session_count, deleted_injection_log_count }
```

### New Types (structured-retrospective + tiered-output)

```
HotspotNarrative { hotspot_type, summary, clusters, top_files, sequence_pattern }

EvidenceCluster { window_start, event_count, description }

Recommendation { hotspot_type, action, rationale }
```

### Modified Types (structured-retrospective + tiered-output)

```
RetrospectiveReport — adds:
    recommendations: Vec<Recommendation>  (new field, always present)
    narratives: Option<Vec<HotspotNarrative>>  (new, serde skip_serializing_if None)

ObservationRecord — adds (serde default, no migration):
    confidence_at_injection: Option<f64>
    session_outcome: Option<String>
```

### New Constants

```
TIMED_OUT_THRESHOLD_SECS: u64 = 24 * 3600    (sessions.rs)
DELETE_THRESHOLD_SECS: u64 = 30 * 24 * 3600   (sessions.rs)
CLUSTER_WINDOW_SECS: u64 = 30                  (structured.rs)
PROVENANCE_BOOST: f64 = 0.02                   (confidence.rs)
```

---

## Sequencing Constraints

- **storage-layer must be implemented first** — all other components call Store methods.
- **uds-listener depends on storage-layer** — needs `insert_session`, `update_session`, `insert_injection_log_batch`.
- **session-gc depends on storage-layer** — needs `gc_sessions`.
- **auto-outcomes depends on uds-listener** — written in `process_session_close`.
- **structured-retrospective depends on storage-layer** — calls `scan_sessions_by_feature`, `scan_injection_log_by_session`.
- **tiered-output depends on structured-retrospective** — applies `evidence_limit` to whatever path was used.
- **lesson-learned depends on tiered-output** — fires after report is built.

---

## Key Invariants

1. `insert_injection_log_batch` is the ONLY public write API for INJECTION_LOG — no single-record insert.
2. All store writes in `uds_listener.rs` use `spawn_blocking` — never block the async executor.
3. Auto-outcome entries have `embedding_dim = 0` — no ONNX call on SessionClose path.
4. `from_structured_events` excludes `Abandoned` AND `TimedOut` sessions from metrics.
5. `evidence_limit = 0` means "no cap" — full arrays returned (backward compatible).
6. `PROVENANCE_BOOST` is query-time only — never stored in `EntryRecord.confidence`.
7. Migration v4→v5 is idempotent: `next_log_id = 0` only written if key absent.
