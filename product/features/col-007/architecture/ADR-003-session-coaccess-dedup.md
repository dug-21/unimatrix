## ADR-003: Session-Scoped Co-Access Dedup via In-Memory Set

### Context

The MCP `context_search` tool generates co-access pairs on every call. This is acceptable because MCP calls are infrequent (agents call tools explicitly). Hook injection fires on every prompt -- a 50-prompt session could inject the same top 3 entries 50 times, generating 150 redundant co-access pair writes to redb. This inflates co-access counts and creates unnecessary I/O.

SR-06 flagged this as a medium-severity risk. The specification requires session-scoped dedup: at most one co-access recording per unique entry set per session.

### Decision

Maintain an in-memory `CoAccessDedup` struct in the UDS listener:

```rust
struct CoAccessDedup {
    sessions: Mutex<HashMap<String, HashSet<Vec<u64>>>>,
}
```

On each ContextSearch response:
1. Sort the injected entry IDs into a canonical `Vec<u64>`.
2. Check `sessions[session_id]` for this exact vector.
3. If not present: generate co-access pairs, record them, add the vector to the set.
4. If present: skip co-access recording.

On `SessionClose`: remove the session's entry from the map.

### Consequences

**Easier:**
- Co-access data reflects unique entry co-occurrences, not prompt frequency. The existing co-access boost formula (log-transformed count) remains meaningful.
- No redb writes for redundant pair recordings. Reduces I/O per prompt.
- Cleanup is automatic via SessionClose handler.

**Harder:**
- Server restart loses the dedup state. This is acceptable: the consequence is some redundant co-access pairs after restart, which the log-transform formula absorbs gracefully (co-access counts are already capped at `MAX_MEANINGFUL_CO_ACCESS = 20.0`).
- Memory usage: worst case is ~50 sessions x ~100 unique entry sets x ~5 IDs x 8 bytes = ~200KB. Negligible.
- Sessions without explicit SessionClose (crash, timeout) leak entries in the map. This is bounded by session count and the entries are small. A periodic cleanup (e.g., remove sessions older than 24h) could be added in col-010 when session lifecycle tracking is available.
