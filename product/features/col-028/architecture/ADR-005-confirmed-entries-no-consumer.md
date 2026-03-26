## ADR-005: confirmed_entries Ships Without Consumer; Semantic Contract Is Explicit-Fetch-Only

### Context

`SessionState.confirmed_entries` is being added in this feature with no downstream
consumer. The intended consumer (Thompson Sampling per-(phase, entry) arms) is a
separate future feature.

Two alternatives were considered:

1. **Defer the field**: Add `confirmed_entries` when Thompson Sampling is implemented.
2. **Add the field now with no consumer**: Sessions that occur between now and the
   Thompson Sampling feature will have populated `confirmed_entries` from day one.

Sessions are ephemeral and non-backfillable. If the field is deferred, the Thompson
Sampling feature cold-starts with zero confirmed_entries data for all past sessions.
Since sessions are never replayed, this data gap is permanent.

### Decision

Add `confirmed_entries: HashSet<u64>` to `SessionState` in this feature. No consumer
is wired in this feature. The field accumulates data in production immediately so that
when Thompson Sampling lands, it inherits populated session state.

**Semantic contract (must not be silently reinterpreted by future consumers)**:

An entry appears in `confirmed_entries` if and only if:
- `context_get` was called with that entry's ID in this session, OR
- `context_lookup` was called with exactly one target ID equal to that entry's ID in
  this session (request-side cardinality, ADR-004).

This is the "explicit-fetch" contract. It does NOT include:
- Entries returned by `context_search` (many results, not individually confirmed)
- Entries returned by `context_briefing` (offer event)
- Entries returned by a multi-target `context_lookup` (batch retrieval)

The field is in-memory only. It is not persisted. It is reset to `HashSet::new()` on
`register_session`. It is consistent with all other `SessionState` fields in this regard.

Future consumers must consume this field under these semantics. If the contract needs to
change (e.g., multi-target lookup should also confirm), a new ADR is required and the
impact of in-flight session data must be assessed.

### Consequences

- Thompson Sampling inherits populated `confirmed_entries` from the first session after
  this feature ships. No cold-start gap.
- The semantic contract is locked here. SR-04 risk (silent reinterpretation by future
  consumer) is mitigated by explicit documentation in this ADR.
- `make_state_with_rework` and all related test helpers must be updated (pattern #3180).
  Failing to do so produces compile errors in existing tests — a hard gate.
- AC-08 (field initialised empty on register), AC-09 (context_get populates), AC-10
  (single-target lookup populates, multi-target does not) validate the contract.

Related: ADR-004 (cardinality trigger), SR-04 (risk assessment).
