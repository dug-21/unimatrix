## ADR-002: Server-Layer Deduplication with Vote Correction

### Context

Without deduplication, an agent calling `context_get(id=42)` in a loop would inflate `access_count` linearly, and passing `helpful: true` on every retrieval would stuff `helpful_count`. Since these counters feed crt-002's confidence formula, this creates a direct manipulation vector.

Deduplication could live at:
- **Store layer**: The store checks whether to increment counters. Requires persisting dedup state and knowing agent_id (which the store doesn't naturally have).
- **Server layer**: The server checks dedup before calling the store. Has access to agent_id from identity resolution. Can use in-memory state scoped to the server session.

Additionally, agents may change their mind about helpfulness within a session. An entry initially judged unhelpful (speculative retrieval) may later prove useful. The system should allow vote correction rather than locking in first impressions.

### Decision

Deduplication lives at the server layer, in an in-memory `UsageDedup` struct.

The store layer provides unconditional update methods (`record_usage`). The server layer filters which entries to update based on dedup state. This separation means:
- The store remains simple and synchronous -- it writes what it's told to write.
- The server makes dedup decisions where agent_id context is available.
- Dedup state is per-session (in-memory, cleared on restart). A new session is a legitimate new access.

Two separate tracking structures:
- `access_counted: HashSet<(String, u64)>` -- tracks (agent_id, entry_id) for access_count. Binary: counted or not.
- `vote_recorded: HashMap<(String, u64), bool>` -- tracks (agent_id, entry_id) -> last vote value. Enables last-vote-wins correction.

**Vote correction (last-vote-wins):** When an agent changes its vote on an entry within a session, the `check_votes` method returns `CorrectedVote` for that entry. The server layer then includes the entry in the appropriate decrement set and the new increment set, both passed to `Store::record_usage` in a single write transaction. This ensures:
- The old counter is decremented (saturating at 0)
- The new counter is incremented
- Both operations are atomic (same transaction)
- The total vote count (helpful + unhelpful) never exceeds the actual number of voting events

Exception: `last_accessed_at` is always updated (no dedup). Recency is a non-gameable signal -- knowing when an entry was last accessed is valuable regardless of whether access_count is deduped.

### Consequences

- **Store stays simple.** No dedup logic, no agent_id awareness. Pure data operations. The store accepts both increment and decrement ID sets without knowing why.
- **Server carries dedup responsibility.** `UsageDedup` must be consulted before every usage write. This is a ~1 microsecond HashMap/HashSet lookup per entry.
- **Vote correction is atomic.** The decrement-old + increment-new happens in the same `record_usage` call (single write transaction). No window for inconsistent state (R-16).
- **Dedup is session-scoped.** Server restart clears dedup state. An agent that accesses the same entry across 100 separate sessions registers 100 access_count increments. Cross-session votes are independent observations -- no correction across sessions (by design).
- **Memory scales with unique (agent, entry) pairs.** At ~72 bytes per pair (HashMap slightly larger than HashSet), 10,000 unique pairs = ~720 KB. Well within budget.
- **Not persisted.** If persistence is needed later (e.g., for long-running server processes), the dedup set can be backed by a redb table without changing the API.
