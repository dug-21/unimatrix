## ADR-001: Knowledge Reuse Computed Server-Side

### Context

Knowledge reuse measurement requires joining data across three tables: query_log (which entry IDs were returned by search in each session), injection_log (which entries were injected into each session), and entries (which entries were created in which session/topic, and their categories). All three are accessed through `Store` methods in `unimatrix-store`.

The existing architectural split (col-012 ADR-002, Unimatrix #383) places all retrospective computation in `unimatrix-observe` and data loading behind the `ObservationSource` trait, preserving unimatrix-observe independence from unimatrix-store.

Two options exist:

**Option A**: Extend `ObservationSource` with methods for query_log and injection_log access. Compute knowledge reuse in unimatrix-observe.

**Option B**: Compute knowledge reuse in `unimatrix-server` where Store access is direct. Pass the result to the report as a pre-computed struct.

Option A would add 3+ methods to `ObservationSource` (scan_query_log_by_sessions, scan_injection_log_by_sessions, count_active_entries_by_category) used by exactly one consumer (knowledge reuse). This bloats a trait designed for observation data abstraction with query_log/injection_log concerns. It also requires unimatrix-observe to understand `QueryLogRecord` and `InjectionLogRecord` types, creating a dependency on unimatrix-store types even if indirectly.

Option B keeps the trait focused and leverages the fact that the server handler already has Store access and already does post-build mutation of the report (narratives, recommendations).

### Decision

Compute knowledge reuse in the `context_retrospective` handler in unimatrix-server. The handler:

1. Loads query_log and injection_log for the topic's sessions via new batch Store methods
2. Loads entry metadata to determine origin session and category
3. Computes Tier 1 reuse (entries stored in session A, retrieved/injected in session B)
4. Computes category gaps (active categories with zero reuse)
5. Assembles a `KnowledgeReuse` struct and assigns it to `report.knowledge_reuse`

The `KnowledgeReuse` struct is defined in unimatrix-observe (it is a report type), but populated by server-side logic. This parallels how `narratives` and `recommendations` are defined in unimatrix-observe but populated by server-side calls.

This is a scoped exception. Pure observation-derived computation (session summaries, context reload) stays in unimatrix-observe. Only computation requiring multi-table Store joins is server-side.

### Consequences

- **Easier**: ObservationSource trait stays focused on observation data. No new trait methods for a single consumer.
- **Easier**: Server handler already has Store access and the post-build mutation pattern. No new abstractions needed.
- **Harder**: Knowledge reuse logic is in the server crate, not testable in isolation from Store. Integration tests required rather than pure unit tests.
- **Harder**: Creates a precedent for server-side retrospective computation. Must be documented so future features know when to use unimatrix-observe vs server-side. Rule of thumb: if it only needs ObservationRecord data, it goes in unimatrix-observe. If it needs Store joins, it goes in the handler.
