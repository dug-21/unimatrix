# Vision Alignment Report: crt-032

## Summary

crt-032 is fully aligned with the product vision. No variances requiring human attention.

---

## Alignment Assessment

### Vision: Intelligence pipeline is a self-improving relevance function, not additive boosts

The product vision explicitly calls out "Intelligence pipeline is additive boosts, not a learned function" as a **High severity gap**, roadmapped for Wave 1A + W3-1 resolution. PPR (crt-030) was a step in that direction — it introduced graph-based score propagation as a learned structural signal, replacing the raw co-access additive term with a graph traversal-derived score.

crt-032 completes the logical consequence of that step: with PPR absorbing co-access signal, the direct additive `w_coac` term is redundant. Zeroing it removes a vestigial additive boost, moving the scoring function closer to a pure graph + ML model. This is **directionally aligned** with the vision gap.

### Vision: Co-access formalized as graph edges

The vision gap "Co-access and contradiction never formalized as graph edges" is marked **Fixed — W1-1**. Co-access pairs are `GRAPH_EDGES.CoAccess` edges that flow through PPR. The direct additive term was a holdover from before that formalization. Zeroing `w_coac` acknowledges this: the signal has moved from the additive formula into the graph layer where it belongs.

### Vision: Configurable, domain-agnostic platform

The `w_coac` field remains configurable — operators may set it above `0.0` in their config file. No capability is removed. This preserves the operator configurability principle established in W0-3 (Config Externalization). **Aligned.**

### Vision: Every intelligence change measured against real query scenarios

Phase 1 measurement (crt-030 follow-up) produced the eval evidence that justifies this change. The decision was data-driven via the evaluation harness (W1-3). **Aligned with the measurement discipline the vision calls for.**

---

## Variance Assessment

| Dimension | Status | Notes |
|-----------|--------|-------|
| Intelligence pipeline direction | Aligned | Removes additive boost in favour of graph-carried signal |
| Configurability / domain-agnostic | Aligned | Field retained; only default changes |
| Measurement discipline | Aligned | Phase 1 eval evidence is the gate |
| Security | Aligned | No new attack surface; validate() range check unchanged |
| Scope boundary | Aligned | Phase 3 deferred cleanly; no scope creep |

**Variances requiring human approval**: None.

---

## Open Questions

None. All open questions from SCOPE.md were resolved by the human before Phase 1b.
