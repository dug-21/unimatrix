# Pseudocode Overview: vnc-003 v0.2 Tool Implementations

## Component Interaction Summary

```
C4 (categories)  C5 (vector-index-api)  C2 (validation)  C3 (response)
      |                   |                    |                |
      v                   v                    v                v
      C7 (server-state) --+--> C6 (server-transactions)
                                      |
                                      v
                               C1 (tool-handlers)
```

## Data Flow

All 4 new tools follow the established execution order:
1. Identity resolution (resolve_agent)
2. Capability check (require_capability)
3. Input validation (validate_*_params)
4. Format parsing (parse_format)
5. Business logic (varies per tool)
6. Response formatting (format_*_success)
7. Audit event (standalone or combined txn)

## Shared Types

- `CorrectParams`, `DeprecateParams`, `StatusParams`, `BriefingParams` -- new param structs in tools.rs
- `StatusReport`, `Briefing` -- new response data structs in response.rs
- `VectorIndex::allocate_data_id()`, `VectorIndex::insert_hnsw_only()` -- new VectorIndex API

## Component Dependencies

| Component | Depends On | Depended On By |
|-----------|-----------|----------------|
| C4 (categories) | none | C1 (briefing uses "duties") |
| C5 (vector-index-api) | none | C6, C7 |
| C2 (validation) | tools.rs param structs | C1 |
| C3 (response) | EntryRecord, Status | C1 |
| C7 (server-state) | C5 (VectorIndex type) | C6 |
| C6 (server-transactions) | C5, C7, store schema | C1 |
| C1 (tool-handlers) | C2, C3, C4, C5, C6, C7 | none |

## Implementation Order

Phase 1 (parallel): C4, C5, C2, C3
Phase 2 (sequential): C7, then C6
Phase 3: C1 (integration layer)
