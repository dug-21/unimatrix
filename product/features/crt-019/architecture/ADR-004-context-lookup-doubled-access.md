## ADR-004: context_lookup Doubled Access Count Implementation

### Context

Change 6 differentiates deliberate retrieval signals. The SCOPE specifies:
- `context_get`: implicit `helpful: Some(true)` — resolved by setting `UsageContext.helpful`.
- `context_lookup`: doubled `access_count` increment (×2 instead of ×1) — without modifying
  `helpful_count`.

SR-05 raised the question of how `UsageDedup` interacts with the ×2 multiplier: UsageDedup
`filter_access` deduplicates access records per agent per entry. If the dedup suppresses the
access call entirely (same agent, same entry, already seen), the increment is 0 not 2. If dedup
passes the call through, the increment should be 2.

Three implementation approaches:
- **Option A**: Add `access_weight: u32` field to `UsageContext`. After dedup filtering,
  multiply each deduped-out access ID... no. The weight applies to dedup-allowed IDs only:
  if the agent has already seen the entry, access_weight has no effect (dedup filtered it).
  If the entry is new to this agent this session, increment by `access_weight` instead of 1.
- **Option B**: Add a new `AccessSource::McpLookup` variant that routes through a separate
  path in `UsageService` that doubles the increment.
- **Option C**: Call `record_access` twice for lookup entries. Dedup blocks the second call
  for the same agent, so effectively access_count += 1 (not 2) — does not achieve the goal.

### Decision

**Option A: `access_weight: u32` field on `UsageContext`.**

```rust
pub(crate) struct UsageContext {
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub helpful: Option<bool>,
    pub feature_cycle: Option<String>,
    pub trust_level: Option<TrustLevel>,
    pub access_weight: u32,  // NEW: 1 = normal, 2 = deliberate retrieval (lookup)
}
```

All existing `UsageContext` construction sites set `access_weight: 1` (normal behavior,
no regression). `context_lookup` sets `access_weight: 2`.

In `UsageService::record_mcp_usage`, after dedup filtering produces `access_ids`:
```rust
// Weighted access increment for deliberate retrieval signals
let weighted_access_ids: Vec<u64> = access_ids
    .iter()
    .flat_map(|&id| std::iter::repeat(id).take(ctx.access_weight as usize))
    .collect();
```
Then pass `weighted_access_ids` in place of `access_ids` to `record_usage_with_confidence`.

The store's `record_usage_with_confidence` already handles a list of IDs; passing the same ID
twice in the list would double-increment via the SQL `UPDATE ... SET access_count = access_count + 1`
applied once per row. This must be confirmed: if the store increments by `COUNT(*)` of
appearances in the list, the ×2 is achieved. If the store deduplicates IDs internally,
Option A does not work and the multiplier must be explicit in the store API.

**Alternative to verify during implementation:** If the store deduplicates by ID, the multiplier
must be implemented by changing the store's API to accept `(id, increment)` pairs, or by adding
a separate `update_access_count(id, increment: u32)` call in the spawn_blocking task.

**Dedup interaction (SR-05 resolved):**

UsageDedup `filter_access` is called before the weight is applied. The weight only affects
IDs that pass the dedup filter (new accesses for this agent). A repeated lookup by the same
agent for the same entry within one session:
- First lookup: dedup passes → access_count += 2
- Second lookup: dedup suppresses → access_count += 0

This is the intended behavior: the doubled signal rewards deliberate targeted retrieval, but
dedup prevents runaway inflation from repeated lookups.

**Option B was rejected** because a new `AccessSource` variant would require changing the match
in `record_access`, duplicating the `record_mcp_usage` logic, or branching inside it — adding
complexity for a one-field difference.

**Option C was rejected** because UsageDedup's dedup key is `(agent_id, entry_id)` — a second
call with the same agent and entry in the same session is blocked, producing no increment.

### Consequences

**Easier:**
- No new spawn_blocking tasks — the doubled increment is computed inside the existing task.
- Dedup interaction is clean: dedup fires first, weight applies after.
- All existing `UsageContext` construction sites are backwards-compatible (`access_weight: 1`).
- Integration test `test_context_lookup_doubled_access` verifies `access_count += 2` per lookup.

**Harder:**
- All existing `UsageContext` construction sites (tools.rs: 3 sites; usage.rs tests: multiple)
  must add `access_weight: 1`.
- The store's behavior with duplicate IDs in the list must be verified before implementation.
  If deduplication happens at the store layer, the multiplier approach must use explicit
  `(id, count)` pairs instead.
- The `access_weight` concept is specific to the deliberate-retrieval use case; future callers
  adding unusual access patterns must understand this field exists.
