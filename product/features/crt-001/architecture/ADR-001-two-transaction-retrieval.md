## ADR-001: Two-Transaction Retrieval Pattern

### Context

crt-001 adds write side-effects (usage counter updates) to read operations. The existing retrieval tools use redb read transactions. Usage recording requires a write transaction. We need to decide how to combine these.

Option A: Use a single write transaction for both the query and the usage recording. This serializes all retrievals (redb write transactions are exclusive), but guarantees atomicity.

Option B: Use a read transaction for the query, then a separate write transaction for usage recording. This allows concurrent reads but means a crash between the two transactions loses usage events.

### Decision

Use Option B: two-transaction retrieval. The read executes in a read transaction (non-blocking), and usage recording executes in a follow-up write transaction.

Rationale:
- **Usage data is analytics, not critical.** Lost usage events from a crash between transactions cause no data corruption or inconsistency. The worst case is an entry's `access_count` being slightly undercounted.
- **Read performance preserved.** Read transactions in redb are concurrent. Serializing all reads through write transactions would bottleneck the server, especially if future multi-agent concurrency is added.
- **Crash window is tiny.** The usage write follows immediately after the read. The crash window is milliseconds.
- **Consistent with AUDIT_LOG pattern.** Retrieval tools already log audit events in a separate transaction after the read (see tools.rs). Usage recording follows the same pattern.

### Consequences

- Usage counts may be slightly undercounted if the server crashes between the read and write transactions. This is acceptable for analytics data.
- The write transaction for usage recording serializes with other write transactions (entry inserts, corrections, etc.). At current scale (single-agent stdio), this is not a bottleneck.
- If multi-agent concurrency makes write serialization a problem, usage writes can be batched asynchronously (write to a channel, drain in background) without schema changes.
