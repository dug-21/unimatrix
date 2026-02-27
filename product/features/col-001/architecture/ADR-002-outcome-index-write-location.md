## ADR-002: OUTCOME_INDEX Write Location

### Context

OUTCOME_INDEX must be populated when an outcome entry is stored. Two options:

1. **Store crate**: Add outcome-aware logic to `Store::insert` — check category, populate OUTCOME_INDEX.
2. **Server crate**: Populate OUTCOME_INDEX within `insert_with_audit`'s write transaction, alongside AUDIT_LOG and VECTOR_MAP writes.

### Decision

OUTCOME_INDEX is populated in the server crate's `insert_with_audit` method, within the existing write transaction. The store crate's `Store::insert` is not modified.

Rationale:
- `insert_with_audit` already extends the write transaction beyond what `Store::insert` does (AUDIT_LOG, VECTOR_MAP). OUTCOME_INDEX follows the same pattern.
- The server crate knows whether the entry is an outcome (it has access to the category and feature_cycle from StoreParams). The store crate would need to inspect category strings to decide whether to populate the index, violating its domain-agnostic constraint.
- This is the same pattern used for FEATURE_ENTRIES — defined in the store crate's schema, populated by the server crate's usage tracking pipeline.

### Consequences

- **Easier**: Store crate stays domain-agnostic. Transaction atomicity is preserved (OUTCOME_INDEX insert is within the same commit as ENTRIES insert).
- **Harder**: Direct `Store::insert` calls (e.g., in tests that bypass the server) do not populate OUTCOME_INDEX. This is acceptable because OUTCOME_INDEX population requires category awareness, which is a server concern.
