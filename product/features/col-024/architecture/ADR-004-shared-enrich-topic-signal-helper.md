## ADR-004: Shared `enrich_topic_signal` Helper for All Write Sites

### Context

Three observation write sites in `listener.rs` need to apply the same fallback
logic: when `extract_topic_signal(input)` returns `None`, read
`session_registry.get_state(session_id)?.feature` and use it as `topic_signal`.
The sites are structurally different:

- **RecordEvent** (line ~684): `extract_observation_fields(&event)` is called
  on the `ImplantEvent`; the `ObservationRow` struct carries `topic_signal:
  event.topic_signal.clone()`.
- **RecordEvents batch** (line ~784): `events.iter().map(extract_observation_fields)`
  builds an `ObservationRow` per event; enrichment must happen per-element inside
  the map.
- **ContextSearch** (line ~842): `topic_signal` is computed via
  `extract_topic_signal(&query)` and inline-assigned to the `ObservationRow`.

SR-05 identifies drift risk if each site implements the fallback independently:
three copies of the same conditional read, three places to update if the registry
API changes, three places where `None`-vs-`Some` handling can diverge.

### Decision

Introduce a module-private free function in `listener.rs`:

```rust
/// Enrich an observation topic_signal using the session registry fallback.
///
/// Returns `extracted` unchanged when it is `Some(_)` — the explicit
/// hook-side signal always wins (AC-08).
///
/// When `extracted` is `None`, reads `session_registry.get_state(session_id)`
/// and returns `state.feature.clone()` if the session has a registered feature.
/// Returns `None` if the session is not registered or has no feature set.
///
/// This is a synchronous Mutex read (~microseconds); no await, no spawn_blocking.
fn enrich_topic_signal(
    extracted: Option<String>,
    session_id: &str,
    session_registry: &SessionRegistry,
) -> Option<String> {
    if extracted.is_some() {
        return extracted;
    }
    session_registry
        .get_state(session_id)
        .and_then(|s| s.feature)
}
```

All three write sites call `enrich_topic_signal` instead of using
`event.topic_signal.clone()` or `topic_signal.clone()` directly.

For the **RecordEvent** and **rework-candidate** paths: call
`extract_observation_fields(&event)` as before to get an `ObservationRow`, then
override `obs.topic_signal` with the enriched value. This avoids mutating the
immutable `ImplantEvent` and avoids adding a session_registry parameter to
`extract_observation_fields` (which would entangle a pure field-extraction function
with registry lookup).

For the **RecordEvents batch** path: inside the `map` closure that constructs each
`ObservationRow`, apply the enrichment to `obs.topic_signal` after calling
`extract_observation_fields`.

For the **ContextSearch** path: replace `topic_signal: topic_signal.clone()` with
`topic_signal: enrich_topic_signal(topic_signal, sid, session_registry)`.

### Consequences

- Easier: the fallback logic has one definition. Changes to the registry API or
  the enrichment contract (e.g., adding a log on fallback) require one edit.
- Easier: AC-08 (explicit signal not overridden) is enforced in one place and
  tested once on the helper.
- Easier: the three write sites remain structurally different but share the same
  enrichment decision; reviewers see a single helper rather than three inline
  conditionals.
- Harder: `extract_observation_fields` returns an `ObservationRow` and the caller
  then patches `topic_signal`; this two-step pattern is slightly more verbose than
  inline construction, but maintains separation of concerns.
- No performance impact: `get_state` acquires the session Mutex for microseconds,
  already on the async handler path before `spawn_blocking_fire_and_forget`.
