## ADR-004: confirmed_entries Trigger Uses Request-Side Cardinality for context_lookup

### Context

`context_lookup` fetches entries by exact filters (topic, category, tags, IDs). When
the caller passes a single target ID, the intent is unambiguous: the agent deliberately
retrieved one specific entry and confirms its existence. This is a strong signal of
agent awareness.

When the caller passes multiple target IDs (batch lookup), the semantics are different:
the agent is retrieving a group of entries, possibly to compare them or get an overview.
It is not confirmed that every returned entry was individually deliberated over.

Two possible triggers for adding to `confirmed_entries` were considered:

1. **Request-side cardinality**: `target_ids.len() == 1` in the incoming request.
2. **Response-side cardinality**: only one entry was returned by the query.

Response-side cardinality is ambiguous. A multi-ID request that happens to return one
result (because some IDs were not found) would produce a confirmation signal that the
agent did not intend. "Single entry returned" is a filtering artifact, not agent intent.

### Decision

`context_lookup` adds to `confirmed_entries` when and only when `target_ids.len() == 1`
at the time the request parameters are parsed — before any database call. This is
request-side cardinality.

```rust
// In context_lookup handler, after confirmed_entries:
if target_ids.len() == 1 {
    self.session_registry
        .record_confirmed_entry(&session_id, target_ids[0]);
}
```

`target_ids` here refers to the ID list in the lookup request parameters, not the
entries returned from the database.

`context_get` always adds to `confirmed_entries` — it retrieves exactly one entry by
ID and always constitutes a deliberate fetch. There is no cardinality ambiguity.

### Consequences

- The confirmed_entries signal faithfully reflects agent intent at request time,
  not an artifact of query results.
- AC-10 (single-target lookup confirms; multi-target does not) directly validates this.
- Future Thompson Sampling consumers must not reinterpret this contract. The semantic
  is: an entry in `confirmed_entries` was explicitly and individually requested by the
  agent. See ADR-005 for the full semantic contract.
- `context_lookup` access_weight remains 2 (deliberate retrieval signal, unchanged).

Related: ADR-005 (confirmed_entries contract), ADR-001 col-028.
