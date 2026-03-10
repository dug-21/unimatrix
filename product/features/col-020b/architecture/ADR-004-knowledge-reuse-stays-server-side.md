## ADR-004: FeatureKnowledgeReuse Computation Stays in unimatrix-server

### Context

col-020 ADR-001 (Unimatrix #864) decided that knowledge reuse computation lives in `unimatrix-server/src/mcp/knowledge_reuse.rs` rather than `unimatrix-observe` because it requires multi-table Store joins (query_log + injection_log + entry category lookup) that would bloat the `ObservationSource` trait for a single consumer.

col-020b revises the semantics of `compute_knowledge_reuse` (changing primary count from cross-session to all-delivery) but does not change the data requirements. The function still needs:
- `QueryLogRecord` slices (from `scan_query_log_by_sessions`)
- `InjectionLogRecord` slices (from `scan_injection_log_by_sessions`)
- Active category counts (from `count_active_entries_by_category`)
- Entry category lookup closure (from `Store::get`)

Moving the computation to `unimatrix-observe` would require either:
1. Adding these Store types as dependencies to `unimatrix-observe` (creates a circular dependency since observe is meant to be Store-agnostic), or
2. Abstracting the data through traits/generic parameters (over-engineering for a single consumer)

### Decision

Keep `compute_knowledge_reuse` in `unimatrix-server/src/mcp/knowledge_reuse.rs`. This upholds col-020 ADR-001. The function remains a pure computation that takes slices and a closure -- the Store coupling is in the caller (`compute_knowledge_reuse_for_sessions` in `tools.rs`), not in the computation itself.

The type `FeatureKnowledgeReuse` (the return type) stays in `unimatrix-observe/src/types.rs` because it is part of `RetrospectiveReport`, which is an observe type.

### Consequences

- **Easier:** No new crate dependencies. No ObservationSource trait changes. Preserves the existing architecture boundary.
- **Easier:** The pure function signature (`&[QueryLogRecord], &[InjectionLogRecord], ...) -> FeatureKnowledgeReuse`) remains testable with synthetic data in unit tests.
- **Harder:** The computation module lives in a different crate than its return type. This is the same tradeoff col-020 accepted and is manageable because `unimatrix-server` depends on both `unimatrix-observe` and `unimatrix-store`.
