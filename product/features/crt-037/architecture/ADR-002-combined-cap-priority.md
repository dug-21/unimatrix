## ADR-002: Combined Cap with Informs Second-Priority Ordering

### Context

SCOPE.md OQ-1 resolved that `Informs` candidates share `max_graph_inference_per_tick` as
the sole tick-level throttle, with Informs as second-priority after Supports/Contradicts.

ADR-003 (crt-029, entry #3826) established `max_graph_inference_per_tick` as the sole cap:
no separate `max_source_candidates_per_tick` field. The question for crt-037 is how to
implement second-priority ordering when the two detection passes produce separate candidate
vecs.

Three options were considered:

1. **Flat merge before Phase 5 sort**: combine all candidates into one vec and re-sort with
   a new priority criterion that prefers SupportsContradict origin over Informs origin. Then
   truncate to cap.

2. **Sequential reservation (chosen)**: fully process the Supports/Contradicts candidate
   list first (sort + truncate to cap), compute remaining capacity, then truncate Informs
   candidates to remaining capacity.

3. **Separate fixed sub-caps**: introduce a second config field
   `max_informs_candidates_per_tick` bounded below `max_graph_inference_per_tick`. The
   SCOPE-RISK-ASSESSMENT.md SR-02 suggestion mentioned this option.

Option 1 is ruled out because the sort criteria for Supports (cross-category first,
isolated-endpoint second, similarity desc) are meaningful for that pass but not for Informs
(cross-category is already guaranteed by Phase 4b filter; isolated-endpoint boosting is
not applicable to institutional memory links). A unified sort mixes semantically distinct
priorities into a single comparator, complicating future maintenance.

Option 3 was considered but rejected. A second config field is complexity without
corresponding precision: operators who want to control the Informs budget can adjust
`max_graph_inference_per_tick` directly. More importantly, option 3 could produce a silent
floor guarantee that crowds out Supports/Contradicts in high-signal ticks — the exact
starvation risk the resolved OQ-1 was designed to prevent. The resolved OQ-1 is explicit:
"Supports/Contradicts processed first."

Option 2 is simple, auditable, and matches the resolved OQ-1 language exactly: Informs
candidates "consume remaining capacity after Supports/Contradicts candidates are processed."

SR-03 (SCOPE-RISK-ASSESSMENT.md) requires the cap-drop count to be observable. Option 2
makes this trivial: `informs_candidates_before_cap - informs_candidates_after_cap`.

### Decision

Implement sequential reservation in Phase 5:

```
Step 1: Apply existing Phase 5 sort to supports_pairs (unchanged).
        Truncate supports_pairs to max_graph_inference_per_tick.

Step 2: remaining = max_graph_inference_per_tick - supports_pairs.len()

Step 3: Sort informs_pairs by similarity descending.
        Record informs_total = informs_pairs.len() before truncation.
        Truncate informs_pairs to remaining (may be 0 if Supports fills the cap).

Step 4: merged_pairs = supports_pairs + informs_pairs (append, order preserved).

Step 5: Log at debug level:
        - supports_candidates
        - informs_candidates_total (before truncation)
        - informs_candidates_accepted (after truncation)
        - informs_candidates_dropped (= total - accepted)
```

The combined `merged_pairs.len() <= max_graph_inference_per_tick` invariant holds by
construction (Step 1 + Step 3 both truncate to sub-sums that add to at most the cap).

No new config fields are introduced. `max_graph_inference_per_tick` remains the sole tick-
level throttle (ADR-003, crt-029).

### Consequences

- Supports/Contradicts detection is never starved. In high-churn ticks with a full Supports
  candidate pool, `remaining = 0` and Informs candidates are dropped entirely. This is
  correct behavior: Supports/Contradicts carry higher precision signal (resolved OQ-1).
- Silent starvation is observable via the debug log. SR-03 is satisfied without a new config
  field or metric infrastructure.
- The Phase 7 batch is always a single contiguous `Vec<NliCandidatePair>`. The rayon
  closure's input (`Vec<(&str, &str)>`) is built from this merged vec identically to the
  current approach for Supports-only — the rayon closure body does not change.
- Future features that add a third detection pass (e.g., `Prerequisite` inference) can
  follow this same pattern: compute remaining capacity after Informs, truncate to that.
- The `graph_inference_k` HNSW neighbor count applies to both Phase 4 and Phase 4b scans.
  This is a simplification: Phase 4b could theoretically use a different k, but there is
  no evidence from ASS-034 data that the Informs signal degrades with the same k. A
  separate config field is deferred.
