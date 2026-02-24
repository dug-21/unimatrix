## ADR-006: Vote Correction Atomicity

### Context

SCOPE.md Decision #10 requires last-vote-wins correction: when an agent changes its helpfulness vote on an entry within a session, the old counter is decremented and the new counter is incremented. This prevents early incorrect assessments from permanently degrading entry quality.

The critical question is atomicity: the decrement of the old counter and the increment of the new counter must happen together. If they happen in separate transactions, a crash or error between them could leave inconsistent state where total votes (helpful + unhelpful) exceed the actual number of voting events, corrupting the Wilson score in crt-002.

Options:
1. **Two separate calls**: Decrement old counter in one `record_usage` call, increment new counter in another. Simple but non-atomic.
2. **Single call with decrement support**: Extend `record_usage` to accept both increment and decrement ID sets in one call, executed in one write transaction.
3. **Dedicated correction method**: Add a separate `Store::correct_vote` method for the specific case of vote flips.

### Decision

Use Option 2: extend `record_usage` with `decrement_helpful_ids` and `decrement_unhelpful_ids` parameters. The server layer partitions entries based on `UsageDedup::check_votes` results and passes all six ID sets to a single `record_usage` call.

Rationale:
- **Atomic by construction.** All updates (access_count increment, old vote decrement, new vote increment, last_accessed_at update) happen in one redb write transaction. No crash window for inconsistent state.
- **No new methods.** `record_usage` is already the batch update method. Adding two more slice parameters keeps the API surface minimal.
- **Store stays dumb.** The store doesn't know about vote correction semantics -- it just applies increments and decrements. The correction logic lives in UsageDedup (server layer).

The decrement uses saturating subtraction (`u32::saturating_sub(1)`) to handle edge cases where the counter is already 0 (possible if cross-session state doesn't match in-session dedup state). This prevents underflow without requiring the store to validate counters.

### Consequences

- **`record_usage` gains two parameters.** The signature grows from 4 to 6 slice parameters. This is the maximum complexity for this method -- no further parameters are anticipated.
- **Saturating subtraction prevents underflow.** If an entry's helpful_count is 0 and a decrement is attempted (edge case from cross-session state mismatch), the count stays at 0. No panic, no u32 underflow.
- **Single transaction cost.** The added decrement operations are in the same transaction as the increments. No additional write transaction overhead.
- **StoreAdapter delegates simply.** The trait-level `record_access` method passes empty slices for all vote-related parameters: `record_usage(ids, ids, &[], &[], &[], &[])`. The 6-parameter complexity is internal to the server layer.
