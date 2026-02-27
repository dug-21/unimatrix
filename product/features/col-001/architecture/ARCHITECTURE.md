# Architecture: col-001 Outcome Tracking

## System Overview

col-001 adds structured outcome tracking to Unimatrix's knowledge engine. Outcomes are entries with `category: "outcome"` that follow a tag schema for structured queryability. The feature introduces one new table (OUTCOME_INDEX), one new server-side validation module (outcome_tags), extensions to two existing tool handlers (context_store, context_status), and a new MCP parameter (feature_cycle on StoreParams).

This feature is the first in the Collective (M5) phase, bridging the Learning & Drift infrastructure (M4) to process intelligence. It provides the structured data layer that col-002 (Retrospective Pipeline), col-003 (Process Proposals), and col-004 (Feature Lifecycle) consume.

## Component Breakdown

### C1: OUTCOME_INDEX Table (store crate)

**Responsibility**: Secondary index linking feature cycles to outcome entry IDs.

- Table definition: `TableDefinition<(&str, u64), ()>` — consistent with TOPIC_INDEX, CATEGORY_INDEX pattern
- Added to `schema.rs` as the 13th table constant
- Created during `Store::open` in the table initialization block
- Exported from `unimatrix-store` for use by the server crate
- No schema version change (this is a new table, not an EntryRecord field change)

**Files modified**: `crates/unimatrix-store/src/schema.rs`, `crates/unimatrix-store/src/db.rs`, `crates/unimatrix-store/src/lib.rs`

### C2: Outcome Tag Validation (server crate)

**Responsibility**: Parse and validate structured `key:value` tags for outcome entries.

- New module: `crates/unimatrix-server/src/outcome_tags.rs`
- Defines tag key enum, workflow type enum, outcome result enum
- `validate_outcome_tags(tags: &[String]) -> Result<(), ServerError>` — entry point
- Validates: recognized keys only for `key:value` tags, required `type` tag, enum values for `type` and `result`
- Plain tags (no `:`) pass through unvalidated
- Called from context_store ONLY when `category == "outcome"`

**Files created**: `crates/unimatrix-server/src/outcome_tags.rs`
**Files modified**: `crates/unimatrix-server/src/lib.rs` (module declaration)

### C3: StoreParams Extension (server crate)

**Responsibility**: Expose `feature_cycle` as an MCP parameter on context_store.

- Add `feature_cycle: Option<String>` to `StoreParams`
- Map to `NewEntry.feature_cycle` in the context_store handler
- Default to empty string when absent (backward compatible)
- No validation beyond standard input validation (max length, no control chars)

**Files modified**: `crates/unimatrix-server/src/tools.rs`

### C4: context_store Outcome Pipeline (server crate)

**Responsibility**: When storing an outcome entry, validate tags and populate OUTCOME_INDEX.

- After category validation (step 5), if category == "outcome", call `validate_outcome_tags`
- After entry insertion (step 10), if category == "outcome" and feature_cycle is non-empty, insert into OUTCOME_INDEX within the write transaction
- OUTCOME_INDEX insert is inline (part of the transaction), not fire-and-forget
- Non-outcome entries are completely unaffected — no new code paths execute for them

**Files modified**: `crates/unimatrix-server/src/server.rs` (insert_with_audit), `crates/unimatrix-server/src/tools.rs` (context_store handler)

### C5: context_status Outcome Statistics (server crate)

**Responsibility**: Add outcome aggregation to the status report.

- Extend `StatusReport` with 4 new fields: `total_outcomes`, `outcomes_by_type`, `outcomes_by_result`, `outcomes_by_feature_cycle`
- Compute during the status report build (step 5): scan OUTCOME_INDEX for feature cycle counts, intersect CATEGORY_INDEX("outcome") with TAG_INDEX for type/result breakdowns
- Extend `format_status_report` to render outcome statistics in all three formats
- Outcome stats always computed when category is not filtered away

**Files modified**: `crates/unimatrix-server/src/response.rs`, `crates/unimatrix-server/src/tools.rs` (context_status handler)

## Component Interactions

```
Agent (MCP caller)
  │
  ▼
context_store(category: "outcome", tags: ["type:feature", "gate:3a", "result:pass"], feature_cycle: "col-001")
  │
  ├─ (1) validate_store_params [existing]
  ├─ (2) category validation [existing]
  ├─ (3) validate_outcome_tags [NEW — C2, only when category == "outcome"]
  ├─ (4) content scanning, embedding, near-dup detection [existing]
  ├─ (5) build NewEntry with feature_cycle from StoreParams [C3]
  ├─ (6) insert_with_audit [existing transaction]
  │       └─ (6a) if category == "outcome" && feature_cycle non-empty:
  │              insert (feature_cycle, entry_id) into OUTCOME_INDEX [C4]
  └─ (7) seed confidence [existing]

context_lookup(category: "outcome", tags: ["type:feature", "gate:3a"])
  │
  └─ Uses existing tag intersection on TAG_INDEX — no changes needed

context_status()
  │
  ├─ [existing status computation]
  └─ (new) scan OUTCOME_INDEX + TAG_INDEX for outcome stats [C5]
```

## Technology Decisions

### ADR-001: Tag Validation Boundary

Tag validation for outcome entries lives in the server crate (`outcome_tags.rs`), not in the store crate. The store crate treats tags as opaque strings. See `architecture/ADR-001-tag-validation-boundary.md`.

### ADR-002: OUTCOME_INDEX Write Location

OUTCOME_INDEX is populated in the server crate's `insert_with_audit` transaction, not in the store crate's `Store::insert`. See `architecture/ADR-002-outcome-index-write-location.md`.

### ADR-003: Extensible Per-Category Validation

The outcome tag validation pattern is designed to be extensible for future per-category rules without modifying outcome-specific code. See `architecture/ADR-003-extensible-category-validation.md`.

## Integration Points

### Store Crate (unimatrix-store)

- OUTCOME_INDEX table definition added alongside existing 12 tables
- Store::open creates the 13th table in the same initialization transaction
- No changes to `Store::insert` or `Store::query` — OUTCOME_INDEX is managed by the server crate
- `OUTCOME_INDEX` exported from `unimatrix-store` for server crate use

### Server Crate (unimatrix-server)

- `outcome_tags` module provides validation functions
- `context_store` handler gains: outcome tag validation call, feature_cycle parameter mapping, OUTCOME_INDEX insert in transaction
- `context_status` handler gains: outcome statistics computation
- `StatusReport` gains 4 new fields
- `format_status_report` gains outcome statistics rendering

### Existing Tool Behavior (Unchanged)

- `context_lookup` — uses existing TAG_INDEX intersection. Tags like `type:feature` and `gate:3a` are stored as regular tag strings. No changes.
- `context_get` — returns entry as-is. No changes.
- `context_search` — semantic search. No changes.
- `context_correct` — correction chain. No changes.
- `context_deprecate` — deprecation. No changes.
- `context_briefing` — orientation. No changes.
- `context_quarantine` — quarantine. No changes.

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `OUTCOME_INDEX` | `TableDefinition<(&str, u64), ()>` | `crates/unimatrix-store/src/schema.rs` |
| `Store::open` | Opens 13 tables (was 12) | `crates/unimatrix-store/src/db.rs` |
| `StoreParams.feature_cycle` | `Option<String>` (new field) | `crates/unimatrix-server/src/tools.rs` |
| `validate_outcome_tags` | `fn(tags: &[String]) -> Result<(), ServerError>` | `crates/unimatrix-server/src/outcome_tags.rs` (new) |
| `NewEntry.feature_cycle` | `String` (existing field, now populated from StoreParams) | `crates/unimatrix-store/src/schema.rs` |
| `StatusReport.total_outcomes` | `u64` (new field) | `crates/unimatrix-server/src/response.rs` |
| `StatusReport.outcomes_by_type` | `Vec<(String, u64)>` (new field) | `crates/unimatrix-server/src/response.rs` |
| `StatusReport.outcomes_by_result` | `Vec<(String, u64)>` (new field) | `crates/unimatrix-server/src/response.rs` |
| `StatusReport.outcomes_by_feature_cycle` | `Vec<(String, u64)>` (new field) | `crates/unimatrix-server/src/response.rs` |
| `insert_with_audit` | Gains OUTCOME_INDEX insert within existing txn | `crates/unimatrix-server/src/server.rs` |
| `format_status_report` | Gains outcome statistics section | `crates/unimatrix-server/src/response.rs` |

## Data Flow

### Outcome Storage

```
StoreParams { category: "outcome", tags, feature_cycle, ... }
  → validate_outcome_tags(tags)        // reject if type tag missing or keys invalid
  → NewEntry { feature_cycle, tags, ... }
  → insert_with_audit:
      begin_write()
        → ENTRIES.insert(id, record_bytes)
        → TOPIC_INDEX.insert((topic, id), ())
        → CATEGORY_INDEX.insert(("outcome", id), ())
        → TAG_INDEX.insert("type:feature", id)
        → TAG_INDEX.insert("gate:3a", id)
        → TAG_INDEX.insert("result:pass", id)
        → TIME_INDEX, STATUS_INDEX, VECTOR_MAP, COUNTERS
        → OUTCOME_INDEX.insert(("col-001", id), ())  // NEW
        → AUDIT_LOG
      commit()
```

### Outcome Querying (via existing tools)

```
context_lookup(category: "outcome", tags: ["type:feature", "gate:3a"])
  → CATEGORY_INDEX scan for "outcome" → {entry_ids}
  → TAG_INDEX get "type:feature" → {entry_ids}
  → TAG_INDEX get "gate:3a" → {entry_ids}
  → intersect all three → matching outcome entries
```

### Outcome Statistics (in context_status)

```
context_status()
  → read_txn
  → scan CATEGORY_INDEX("outcome") → total_outcomes count
  → for each outcome entry: read tags from EntryRecord
    → aggregate by type: tag prefix, result: tag prefix
  → scan OUTCOME_INDEX → aggregate by feature_cycle string
  → populate StatusReport outcome fields
```

## Error Boundaries

| Error | Origin | Handling |
|-------|--------|----------|
| Unknown structured tag key (e.g., `foo:bar` on outcome) | `validate_outcome_tags` | Return `ServerError::InvalidInput` — stops before entry creation |
| Missing required `type` tag on outcome | `validate_outcome_tags` | Return `ServerError::InvalidInput` — stops before entry creation |
| Invalid `type` value (e.g., `type:unknown`) | `validate_outcome_tags` | Return `ServerError::InvalidInput` — stops before entry creation |
| Invalid `result` value (e.g., `result:maybe`) | `validate_outcome_tags` | Return `ServerError::InvalidInput` — stops before entry creation |
| OUTCOME_INDEX insert failure | `insert_with_audit` | Transaction rollback — entire entry creation fails atomically |
| OUTCOME_INDEX read failure in status | `context_status` | Return `ServerError` — status report fails entirely |
| Empty `feature_cycle` on outcome | context_store | Entry stored successfully; OUTCOME_INDEX not populated; warning in response |
