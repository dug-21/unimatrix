# col-001 Pseudocode Overview

## Component Interaction

```
context_store(category: "outcome", tags: [...], feature_cycle: "col-001")
    |
    v
tools.rs: context_store handler
    |-- (existing) validate_store_params  -- includes feature_cycle validation
    |-- (existing) categories.validate("outcome")
    |-- (NEW) outcome_tags::validate_outcome_tags(&tags)  -- only when category == "outcome"
    |-- (existing) content scanning, embedding, near-dup
    |-- Build NewEntry with feature_cycle from StoreParams
    |
    v
server.rs: insert_with_audit
    |-- (existing) ENTRIES, indexes, VECTOR_MAP, COUNTERS, AUDIT_LOG
    |-- (NEW) if category == "outcome" && feature_cycle non-empty:
    |         OUTCOME_INDEX.insert((feature_cycle, id), ())
    |-- commit()
    |
    v
tools.rs: context_status handler
    |-- (existing) counters, distributions, corrections, security, contradictions, co-access
    |-- (NEW) outcome stats: total_outcomes, by_type, by_result, by_feature_cycle
```

## Shared Types

### OUTCOME_INDEX (store crate)
- `TableDefinition<(&str, u64), ()>` -- same pattern as TOPIC_INDEX, CATEGORY_INDEX
- Defined in schema.rs, opened in db.rs, exported from lib.rs

### StoreParams.feature_cycle (server crate)
- `Option<String>` -- mapped to `NewEntry.feature_cycle`
- Defaults to empty string when None

### StatusReport outcome fields (server crate)
- `total_outcomes: u64`
- `outcomes_by_type: Vec<(String, u64)>`
- `outcomes_by_result: Vec<(String, u64)>`
- `outcomes_by_feature_cycle: Vec<(String, u64)>`

## Data Flow

1. Agent calls context_store with category "outcome" and structured tags
2. validate_outcome_tags checks tag structure (required type, recognized keys, valid values)
3. StoreParams.feature_cycle mapped to NewEntry.feature_cycle
4. insert_with_audit writes ENTRIES + all indexes + OUTCOME_INDEX (if applicable) + AUDIT_LOG atomically
5. context_status reads CATEGORY_INDEX("outcome") entries, extracts tags from records, scans OUTCOME_INDEX for feature_cycle distribution

## Component Boundaries

| Component | Crate | Files | Responsibility |
|-----------|-------|-------|---------------|
| outcome-index | store | schema.rs, db.rs, lib.rs | Table definition + creation |
| outcome-tags | server | outcome_tags.rs, lib.rs | Tag parsing + validation |
| store-pipeline | server | tools.rs, server.rs, validation.rs | StoreParams extension + OUTCOME_INDEX insert in txn |
| status-extension | server | tools.rs, response.rs | Outcome stats computation + formatting |
