# crt-033 Pseudocode Overview: CYCLE_REVIEW_INDEX

## Components Involved

| Component | File | Role |
|-----------|------|------|
| cycle_review_index (new) | `crates/unimatrix-store/src/cycle_review_index.rs` | All CRUD for `cycle_review_index` table |
| migration v17→v18 | `crates/unimatrix-store/src/migration.rs` + `db.rs` | Schema cascade — 7 touchpoints |
| tools.rs handler | `crates/unimatrix-server/src/mcp/tools.rs` | Steps 2.5, 8a, force paths |
| status.rs response | `crates/unimatrix-server/src/mcp/response/status.rs` | `pending_cycle_reviews` field + formatters |
| services/status.rs | `crates/unimatrix-server/src/services/status.rs` | Phase 7b in `compute_report()` |

## Data Flow

```
context_cycle_review call
    │
    ├─ step 3: three-path observation load → attributed: Vec<ObservationRecord>
    │
    ├─ step 2.5 (NEW — INSERT AFTER step 3, BEFORE step 4):
    │       if force is NOT true:
    │           get_cycle_review(feature_cycle)         [read_pool]
    │               → Some(record):
    │                   deserialize summary_json → RetrospectiveReport
    │                   [on deser error: fall through to full pipeline — ADR-003]
    │                   apply evidence_limit at render step only
    │                   return immediately (skip steps 4–8a)
    │               → None: proceed to step 4
    │       if force IS true AND attributed.is_empty():
    │           get_cycle_review(feature_cycle)         [read_pool]
    │               → Some(record): return stored + purged-signals note
    │               → None: return ERROR_NO_OBSERVATION_DATA
    │
    ├─ step 4–8: full pipeline (unchanged)
    │
    ├─ step 8a (NEW — INSERT AFTER step 8, BEFORE step 9/audit):
    │       build_cycle_review_record(feature_cycle, &report)
    │           → serde_json::to_string(&report)
    │       store_cycle_review(&record)                 [write_pool_server — sync, NOT spawn_blocking]
    │       [on store error: propagate as tool error; do NOT panic]
    │
    └─ step 9: audit + format dispatch (unchanged)
               evidence_limit truncation applied HERE, not before step 8a

context_status call
    │
    └─ services/status.rs compute_report()
           Phase 7b (NEW — after Phase 7 retrospected_feature_count):
               let cutoff = now_unix_secs() - PENDING_REVIEWS_K_WINDOW_SECS
               pending_cycle_reviews(cutoff)            [read_pool]
               report.pending_cycle_reviews = result or []
```

## Shared Types (new, defined in cycle_review_index.rs)

```
CycleReviewRecord {
    feature_cycle:         String    -- PK; matches cycle_events.cycle_id
    schema_version:        u32       -- SUMMARY_SCHEMA_VERSION at compute time
    computed_at:           i64       -- unix timestamp seconds
    raw_signals_available: i32       -- sqlx INTEGER binding: 1=live signals, 0=purged
    summary_json:          String    -- full RetrospectiveReport JSON, no evidence_limit
}

SUMMARY_SCHEMA_VERSION: u32 = 1     -- single definition, no other location
```

Note on `raw_signals_available` type: the spec domain model shows `bool` but
`i32` is used in the Rust struct to match sqlx's SQLite INTEGER→i32 mapping
(RISK-TEST-STRATEGY edge case). The value 1 maps to true (live), 0 maps to false
(purged). Confirm consistent binding before merge (AC-16 round-trip test surfaces
any mismatch).

## SQLite Table DDL

```sql
CREATE TABLE IF NOT EXISTS cycle_review_index (
    feature_cycle         TEXT    PRIMARY KEY,
    schema_version        INTEGER NOT NULL,
    computed_at           INTEGER NOT NULL,
    raw_signals_available INTEGER NOT NULL DEFAULT 1,
    summary_json          TEXT    NOT NULL
)
```

No FOREIGN KEY clause — consistent with all other tables (C-09).

## Sequencing Constraints (build order)

1. `cycle_review_index.rs` + migration changes (store crate) — must compile first
2. `status.rs` response changes — no store dependency, can parallel with (1)
3. `services/status.rs` Phase 7b — depends on `pending_cycle_reviews` from (1)
4. `tools.rs` handler — depends on all of (1), imports `SUMMARY_SCHEMA_VERSION`
   from cycle_review_index; imports `CycleReviewRecord` from store crate

## Key Constraints Encoded in All Component Files

- `store_cycle_review` uses `write_pool_server()` directly in async context.
  MUST NOT be wrapped in `spawn_blocking` (ADR-001, entries #2266, #2249).
- `get_cycle_review` and `pending_cycle_reviews` use `read_pool()` (entry #3619).
- `evidence_limit` truncation is applied at format dispatch (step 9) only.
  MUST NOT be applied before `serde_json::to_string` in step 8a (C-03).
- `SUMMARY_SCHEMA_VERSION` defined ONLY in `cycle_review_index.rs` (C-04, FR-12).
- 4MB ceiling in `store_cycle_review`: return `Err`, not panic (NFR-03).
- On `get_cycle_review` read error: treat as cache miss, fall through to full
  computation (ADR-003, RISK-TEST-STRATEGY failure mode table).
- On `serde_json::from_str` deserialization error: treat as cache miss, fall
  through to full computation with tracing warning (ADR-003).
- `force=true` + empty `attributed`: sole discriminator is `get_cycle_review()`
  return value — no `cycle_events` COUNT query (OQ-01 closed).
- Step 2.5 executes AFTER three-path observation load, BEFORE step 4
  (is_empty check on attributed).
