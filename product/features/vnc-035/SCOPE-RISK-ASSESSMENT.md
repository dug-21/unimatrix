# Scope Risk Assessment: vnc-035

> Mode: scope-risk. Carry-forward + 5 sub-questions are settled; these are product/scope-level risks for the architect and spec writer. Not reopening any decision.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Warn-and-continue Err-path on edge copy has **no failure signal** — feature works whether or not the failure test exists. vnc-017's mirrored path FAILed Gate 3b for exactly this (AC-07 here = the missed AC-04 there). Lesson #4473. | High | High | Flag AC-07 in the spec as "easy to omit — verify by name." Require an explicit per-edge-copy-failure test that asserts correction + already-copied edges persist. |
| SR-02 | Additive-on-triple upsert rides on `write_graph_edge` returning false (not Err) on UNIQUE conflict (pattern #4041). If the new outgoing query / write path treats a UNIQUE-conflict false as success vs. as a carried edge, `edges_carried` count and idempotency (AC-08) can be wrong. | Med | Med | Spec must define `edges_carried` counting semantics against the rows-affected contract: count actual inserts, not attempted writes. Pin idempotent exact re-pass behavior. |
| SR-03 | No symmetric `query_outgoing_edges(source_id)` exists; a new store query is required, and it must replicate the eligibility filter (exclude derived `Supersedes` + tick-generated `CoAccess`/`Informs`). A filter drift between this query and the incoming-redirect precedent silently carries ineligible edges. | Med | Med | Architect: define the eligibility predicate **once** (shared with/mirroring `query_incoming_edges` precedent) so outgoing and incoming exclusion sets cannot diverge. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | "No ceiling" safety rests entirely on the eligibility filter bounding agent-declared degree. If the filter is wrong (SR-03) or a future edge class is mis-classed as agent-declared, "no ceiling" becomes an unbounded fan-out — the very thing vnc-017's N=50 ceiling (#4463) guards against on the incoming side. | Med | Low | Spec: state the safety invariant explicitly — "no ceiling" is valid **only** while eligibility = agent-declared-only. Any future high-threshold defense is an observability warning that still carries every edge, never a truncating cap (per OQ-02). |
| SR-05 | Doc updates (uni-zero SKILL + agent docs, AC-10) are declared "cleanup, not load-bearing" because the `edges_carried` ack delivers awareness. If the ack is descoped or the docs slip, agents retain stale "manually re-declare" guidance and may double-declare or distrust carry-forward. | Low | Med | Spec: keep AC-10 and AC-11 coupled in one acceptance unit — the ack is what makes the doc change non-load-bearing; neither should ship without the other. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Carried `Contradicts` edges are bidirectional; the carry loop reuses `validate_and_write_edges` bidirectional handling. Carrying an outgoing `Contradicts` must not double-write or orphan the reverse direction the redirect path may also touch (vnc-017 source-validation posture, #4459). | Med | Med | Architect: confirm outgoing-carry and incoming-redirect do not both act on the same `Contradicts` pair during one correction. Spec a test for `Contradicts` carry (AC-06) covering both directions. |
| SR-07 | Carried edges are visible to DB-backed reads immediately but to BFS path-mode only after the next tick (lesson #4526). Integration tests asserting graph-path retrieval will be flaky if they don't drain/tick first (patterns #4517, #4114). | Med | Med | Spec: any path-mode acceptance test must force tick/drain before asserting. Call out tick-window staleness as expected, not a defect, so it isn't mis-filed as a carry-forward bug. |
| SR-08 | The opt-out (`context_edge remove`) requires an **Active** source; only the new entry qualifies (original is Deprecated post-correct). If docs or tests target the Deprecated original they hit the "frozen source" rejection and conclude shed is broken. | Low | Med | Spec + AC-05: shed path must target the **new** entry id explicitly; doc must state the Deprecated original cannot be edited. |

## Assumptions

- **SCOPE §Decision / §Background**: assumes the eligibility set (agent-declared only; exclude `Supersedes` + `CoAccess`/`Informs`) is stable. If the engine taxonomy (`graph.rs:139`) adds an agent-declarable type later, carry-forward inherits it automatically — acceptable, but the invariant must be documented (SR-03/SR-04).
- **SCOPE §Background / OQ-03**: assumes the `edges_carried` count ack is sufficient agent awareness without any DB provenance marker. If downstream tooling ever needs to distinguish carried vs. freshly-declared edges, no marker exists to do so. Accepted per OQ-03; flag as a known one-way door.
- **SCOPE §Constraints (back-compat)**: assumes existing callers that re-pass `edges` are made safe purely by idempotent upsert. This holds **only** if upsert keys on the full triple correctly (SR-02). The whole "no caller changes" claim depends on it.

## Design Recommendations

- **SR-01 is the dominant risk** — the architecture is sound but the failure-path test is the highest-probability gate rejection, with direct precedent (#4473). Spec writer: lift AC-07 to a named, mandatory test and annotate "easy to omit."
- **SR-02/SR-03**: architect should centralize two contracts — (a) the eligibility predicate and (b) the upsert/count semantics against `write_graph_edge`'s rows-affected return (#4041). These are the two places where carry-forward can silently produce wrong edges or wrong `edges_carried` counts.
- **SR-06/SR-07**: give `Contradicts` carry and tick-window staleness explicit acceptance tests; both are integration-boundary risks where most bugs live and where prior features (#4459, #4517, #4114) repeatedly tripped.
