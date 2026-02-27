# Pseudocode: outcome-index (store crate)

## Purpose

Add OUTCOME_INDEX table definition to the store crate. This is a structural secondary index -- the store crate treats it as an opaque (string, u64) -> () table. No domain-specific logic.

## Changes

### schema.rs

Add after CO_ACCESS definition (line 48):

```
// Update comment from "12 total" to "13 total" on line 6

pub const OUTCOME_INDEX: TableDefinition<(&str, u64), ()> =
    TableDefinition::new("outcome_index");
```

### db.rs

Add OUTCOME_INDEX to the import list from schema (line 7-8 area):

```
use crate::schema::{
    AGENT_REGISTRY, AUDIT_LOG, CATEGORY_INDEX, CO_ACCESS, COUNTERS, DatabaseConfig, ENTRIES,
    FEATURE_ENTRIES, OUTCOME_INDEX, STATUS_INDEX, TAG_INDEX, TIME_INDEX, TOPIC_INDEX, VECTOR_MAP,
};
```

Add to the table initialization block in open_with_config (after CO_ACCESS, line 53):

```
txn.open_table(OUTCOME_INDEX).map_err(StoreError::Table)?;
```

Update comments referencing "12 tables" to "13 tables".

### lib.rs

Add OUTCOME_INDEX to the exports (line 18 area):

```
pub use schema::{ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP, FEATURE_ENTRIES, OUTCOME_INDEX};
```

## Invariants

- OUTCOME_INDEX uses the same `TableDefinition<(&str, u64), ()>` pattern as TOPIC_INDEX and CATEGORY_INDEX
- The table is created during Store::open alongside all other tables
- Schema version remains 2 (no EntryRecord field change)
- No domain logic in the store crate -- OUTCOME_INDEX is just a structural table
