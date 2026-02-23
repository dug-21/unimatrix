# vnc-002 Pseudocode Overview

## Component Interaction Map

```
tools.rs (C5)
  |-- validation.rs (C1)    -- validate params before any operation
  |-- categories.rs (C4)    -- validate category before content scan
  |-- scanning.rs (C2)      -- scan content before embed/store
  |-- response.rs (C3)      -- format results after operation
  |-- error.rs (C7)         -- error types used by all modules
  |-- audit.rs (C6)         -- write_in_txn for combined path
  '-- server.rs (C8)        -- insert_with_audit on UnimatrixServer
```

## Shared Types

All modules import from existing crates:
- `unimatrix_store::{EntryRecord, NewEntry, QueryFilter, Status, Store, COUNTERS, AUDIT_LOG, VECTOR_MAP, ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX}`
- `unimatrix_core::{CoreError, async_wrappers::*}`
- `unimatrix_vector::SearchResult`
- `rmcp::model::{CallToolResult, Content, ErrorCode, ErrorData}`
- `crate::error::ServerError`
- `crate::audit::{AuditEvent, Outcome}`
- `crate::registry::Capability`

## Data Flow Summary

### Read path (context_search, context_lookup, context_get)
1. Identity resolution (existing)
2. Capability check via registry
3. Input validation via validation.rs
4. Format parsing via response.rs
5. Business logic (embed/search/query/get)
6. Response formatting via response.rs
7. Standalone audit via audit.log_event()

### Write path (context_store)
1. Identity resolution (existing)
2. Capability check (Write required)
3. Input validation
4. Format parsing
5. Category validation via categories.rs
6. Content scanning via scanning.rs
7. Embedding via EmbedServiceHandle
8. Near-duplicate detection via vector_store.search
9. Entry build + combined transaction via server.insert_with_audit
10. Vector index insert via vector_store.insert
11. Response formatting

## Implementation Order

```
Layer 1: error-extensions (C7) -- no deps
Layer 2: validation (C1), categories (C4), scanning (C2), response (C3) -- parallel, dep on C7
Layer 3: audit-optimization (C6) -- dep on C7
Layer 4: tools (C5) -- dep on all above
```

## Format Parameter

All 4 tools accept optional `format` param: `"summary"` (default), `"markdown"`, `"json"`.
- SearchParams, LookupParams, StoreParams, GetParams each get `pub format: Option<String>`
- Parsed by `response::parse_format()` before business logic
