## ADR-003: Weight-0 Guard in record_briefing_usage Precedes filter_access

### Context

`context_briefing` sends entries to the agent as an offer (index of available entries),
not as a selection event. The agent may never read any of the returned entries. Crediting
briefing as a weight-1 access event is an overcounting error: it inflates `access_count`
for entries the agent never deliberately read.

Correcting `context_briefing` access_weight to 0 introduces a new problem: `UsageDedup`
maintains a single `access_counted: HashSet<(String, u64)>` shared across ALL
`AccessSource` variants. `filter_access` is called from both `record_mcp_usage` and
`record_briefing_usage` against this same set.

With the current code (weight=1), a briefing appearance for entry X calls
`filter_access`, which inserts `(agent_id, entry_id_X)` into `access_counted`. A
subsequent `context_get` for entry X — the highest-signal read event — then calls
`filter_access`, finds the dedup entry, and returns an empty `access_ids` list,
producing zero access_count increment.

The `UsageContext` doc comment (line 62–63) states "A value of 0 silently drops the
access increment (EC-04)" but this is a contract declaration. The flat_map repeat
arithmetic (`iter::repeat(id).take(weight as usize)`) produces the `<= 1` branch when
weight is 0, which copies entry_ids unchanged into `multiplied_all_ids`. The EC-04
contract is not enforced by the arithmetic — it must be enforced by an explicit guard.

Source confirmation (usage.rs:313–349): `record_briefing_usage` calls only
`record_usage_with_confidence`. There is no `generate_pairs` call, no
`filter_co_access_pairs` call. Briefing does not produce co-access pairs today.

### Decision

Add an early-return guard at the top of `record_briefing_usage`, before `filter_access`:

```rust
fn record_briefing_usage(&self, entry_ids: &[u64], ctx: UsageContext) {
    if ctx.access_weight == 0 {
        return; // offer-only event; do not register dedup slot or increment access_count
    }
    let agent_id = ctx.agent_id.clone().unwrap_or_default();
    let access_ids = self.usage_dedup.filter_access(&agent_id, entry_ids);
    // ... rest of function unchanged
}
```

This guard must appear before the `filter_access` call so that no dedup slot is
consumed. With `context_briefing` weight=0, the guard fires and returns immediately.
The dedup set is not touched. A subsequent `context_get` for the same entry proceeds
normally, calls `filter_access`, gets a fresh dedup slot, and produces an
`access_count += 2` increment (weight=2 for `context_get`).

The guard is placed in `record_briefing_usage` rather than at the `AccessSource`
dispatch level in `record_access` because the dispatch level would need to inspect
`ctx.access_weight` before routing — creating a coupling between the dispatcher and the
weight concept that is otherwise encapsulated per-path. The current structure is: each
path (`record_mcp_usage`, `record_briefing_usage`, `record_hook_injection`) owns its own
logic. The weight-0 guard belongs to the briefing path's logic.

### Consequences

- The EC-04 contract ("weight 0 drops access increment") is now enforced at the correct
  boundary, not just documented.
- AC-07 (briefing on X then get on X → access_count = 2) directly validates this guard.
- `record_briefing_usage` is now consistent with `record_mcp_usage`, which has its own
  `if access_ids.is_empty() { return; }` short-circuit.
- SR-07 risk acknowledged: if a future refactor routes `AccessSource::Briefing` through
  `record_mcp_usage`, this guard is bypassed. The guard is complete for the current
  routing but not structurally enforced at the dispatch level. This is an acceptable
  trade-off given the explicit `AccessSource::Briefing` arm in `record_access`.
- Briefing continues to produce zero co-access pairs (unchanged from today).

Related: ADR-001 col-028, pattern #3503 (UsageDedup weight-0 gotcha), pattern #3510.
