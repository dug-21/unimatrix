## ADR-005: Open-Ended Window Capped at `unix_now_secs()`, No Additional Max-Age Limit

### Context

`cycle_events` records `cycle_start` and `cycle_stop` events for each cycle_id.
When `load_cycle_observations` pairs events to form `(start, stop)` windows, a
`cycle_start` row with no subsequent `cycle_stop` row produces an open-ended
window. This happens for:

1. In-progress features: `context_cycle_review` called during active work.
2. Abandoned features: the feature was stopped without a `cycle_stop` event (e.g.,
   server crash, explicit abandonment via non-cycle path).

SCOPE.md §Resolved Design Decisions #2 specifies: use `unix_now_secs()` as the
implicit stop boundary for open-ended windows. This allows in-progress features to
work correctly.

SR-03 asks whether a maximum window cap (e.g., 24 hours, 30 days) should apply to
prevent abandoned-cycle over-inclusion. Abandoned cycles without a `cycle_stop` would
otherwise include all observations from the time of `cycle_start` to the present.

### Decision

The implicit stop for open-ended windows is `unix_now_secs()` at the time
`load_cycle_observations` is called. No additional maximum window duration is
applied.

Rationale:
- The call is bounded by the `MCP_HANDLER_TIMEOUT` already wrapping
  `context_cycle_review`. If the scan is too large, the timeout fires.
- Applying an arbitrary max-age cap (e.g., 30 days) would silently truncate
  legitimate long-running features without any user signal.
- The over-inclusion risk for abandoned features is mitigated by the three-step
  algorithm: Step 2 only selects sessions whose `topic_signal` equals the
  `cycle_id`. An abandoned cycle that never had enriched observations has zero
  matching session IDs, producing an empty primary-path result. The legacy fallback
  then applies, which is the correct pre-col-024 behaviour.
- Adding the cap is a forward-compatible enhancement: it can be introduced later as
  a configurable parameter if operational evidence shows over-inclusion.

The implementation must document this as a known limitation in the function's
doc comment:

> Open-ended windows (cycle_start with no cycle_stop) use unix_now_secs() as the
> implicit stop. Features that were force-abandoned without a cycle_stop event
> will include all observations with matching topic_signal up to the present.

### Consequences

- Easier: the implementation stays simple; no cap constant to configure or
  explain.
- Easier: in-progress features work correctly with no special-casing.
- Harder: a feature that received `cycle_start` but no `cycle_stop` months ago
  will pull in all subsequent matching observations when reviewed. The per-AC-08
  enrichment rule (explicit signal wins) limits cross-contamination to sessions
  whose `topic_signal` was heuristically set to the abandoned cycle_id.
- This decision is revisable: adding a max_window_secs parameter to
  `load_cycle_observations` is a backward-compatible trait change.
