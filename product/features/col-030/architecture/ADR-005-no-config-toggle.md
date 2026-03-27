## ADR-005: No Config Toggle for Suppression

### Context

col-030 introduces always-on Contradicts suppression in `SearchService::search`. A
`Contradicts` edge written by a false-positive NLI detection would silently drop a
legitimate result from search output. Before `#412` (audit visibility for suppression
events) ships, there is no operator-facing signal for when suppression fires — beyond the
DEBUG log line mandated by SR-04.

A config toggle (`suppress_contradicts_enabled: bool` in `SearchConfig`) would provide
an escape hatch if NLI false positives are widespread. However:
1. SCOPE.md explicitly states: "This feature does NOT introduce a new config toggle for
   enabling/disabling suppression; suppression is applied whenever the TypedRelationGraph
   is available (`use_fallback = false`) and Contradicts edges exist in the current result set."
2. The existing NLI threshold (`nli_contradiction_threshold`, default 0.6) is already
   calibrated to reduce false positives. Contradicts edges are rare in practice.
3. A config toggle adds complexity and a new untested code path (the disabled branch).
4. The cold-start guard (`use_fallback = true`) already provides a natural bypass that
   covers the server startup window.

### Decision

No config toggle. Suppression is active whenever `use_fallback = false` (graph is built
and valid). The sole gating condition is the `if !use_fallback` guard already in scope from
Step 6 of the search pipeline.

SR-04 observability is addressed by a `debug!()` log line emitted when at least one entry
is suppressed, including the suppressed entry ID and the contradicting entry ID. This
provides the minimum audit trail until #412 ships.

```rust
// In the mask-application loop:
if !keep_mask[i] {
    // SR-04: DEBUG log when suppression fires
    let suppressed_id = results_with_scores[i].0.id;
    // contradicting_id is the highest-ranked entry that caused the suppression.
    // The implementation brief must specify how this ID is captured in suppress_contradicts
    // or derived in the call site.
    debug!(
        suppressed_entry_id = suppressed_id,
        "contradicts collision suppression: entry suppressed"
    );
}
```

The exact logging signature (whether `suppress_contradicts` returns the contradicting pair
or the call site derives it) is left to the pseudocode agent. The constraint is: at least
the suppressed entry ID must appear at DEBUG level.

### Consequences

- No new `SearchConfig` fields — config struct is unchanged.
- Operators who encounter unexpected missing results can enable DEBUG logging to see
  suppression events while #412 is pending.
- If NLI produces a flood of false-positive Contradicts edges in a future scenario, the
  only escape is to lower or disable NLI detection via existing `nli_contradiction_threshold`
  — not a suppression toggle. This is acceptable given the feature scope.
- PPR (#398) will need to decide separately whether it requires a config toggle for its
  score modifications; col-030's no-toggle choice does not constrain that decision.
