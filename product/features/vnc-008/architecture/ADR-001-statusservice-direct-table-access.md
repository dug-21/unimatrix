## ADR-001: StatusService Inherits Direct-Table Access

### Context

The `context_status` handler (628 lines in tools.rs) computes a comprehensive status report by scanning multiple redb tables directly: ENTRIES, COUNTERS, CATEGORY_INDEX, TOPIC_INDEX. This is the largest single MCP handler and needs extraction into `services/status.rs` as a StatusService.

The product vision states that the service layer "must not introduce new direct-storage coupling" and that "StoreService and `Store::insert_in_txn` should be the only paths to the database from the service layer." However, the Store public API does not expose a `compute_report()` method, and adding one would require significant API expansion in `unimatrix-store` — scope creep for a pure restructuring wave.

SR-05 in the Scope Risk Assessment flagged this as a trade-off: StatusService either inherits the direct-table pattern or goes through Store public API.

### Decision

StatusService inherits the existing direct-table access pattern from `context_status`. The `compute_report()` method opens a read transaction and scans ENTRIES, COUNTERS, CATEGORY_INDEX, and TOPIC_INDEX directly, exactly as the current code does. This is a **code move, not a redesign**.

The direct-table access is documented as a known exception to the "services go through Store public API" principle. The exception is acceptable because:
1. StatusService is read-only (no writes bypass StoreService)
2. The pattern already exists — we are not introducing new coupling, we are relocating existing coupling
3. Store API expansion to support `compute_report()` is significant work orthogonal to module reorganization
4. The exception is trackable — a single module with documented imports

### Consequences

- StatusService has direct `use unimatrix_store::{ENTRIES, COUNTERS, CATEGORY_INDEX, TOPIC_INDEX, deserialize_entry}` imports
- This is the only service (besides StoreService's `insert_in_txn`) that accesses Store internals
- Future work can introduce a `Store::compute_report()` method to eliminate this exception
- The `mcp/tools.rs` exception for `context_status` direct-table imports is resolved: those imports move to `services/status.rs`
