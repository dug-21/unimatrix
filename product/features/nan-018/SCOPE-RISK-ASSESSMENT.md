# Scope Risk Assessment: nan-018

Eval harness strategic upgrade — tunability, trust/cost metrics, durable fixture corpus, drift guard. Mode: scope-risk. Run before architecture.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Retrieval-shape hash (Goal 6/AC-08) is the linchpin for OQ-3, OQ-4, OQ-5 at once. If its enumerated inputs are wrong/incomplete, three things break together: drift goes silent, the protocol trigger keys on a false definition, and embed-model dependence leaks in. Hashing is fragile to ordering/serialization non-determinism. | High | Med | Design the hash inputs as an explicit, ordered, versioned manifest; unit-test hash stability across runs and a deliberate-mismatch case (AC-08). Treat embedding model-id/dim as a first-class input (settles SR-03). |
| SR-02 | Token-proxy cost metric (Goal 3/AC-09) has unstated fidelity: "token-proxy" is not a real tokenizer. A crude proxy (char/4) can mis-rank sets and make cost-of-noise findings misleading downstream (ass-073). | Med | Med | Architect must state the proxy's definition and known error bars in design (OQ-1), not defer. Document it as a proxy in the config-knob/Band-2 reference so downstream reads numbers correctly. |
| SR-08 | AC-01 "bit-for-bit" default reproduction across multi-site config threading is a known trap: config fields added in one wave miss construction/forwarding sites (Unimatrix #4131, #4070, #3779), silently changing behavior. | High | Med | Enumerate every `graph_penalty` call + config construction site up front; add a default-equivalence test asserting penalties at default config == current `const`s. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | "Build the dial / recommendation-only" boundary (AC-09, AC-12/13) is easy to violate under delivery pressure — proxy-narrowing cost into a deferral, or editing a `.claude/protocols/` file. Both are explicit non-goals. | Med | Med | Spec must restate: no protocol-file edits at all; cost narrowing only as justified in-design call, never a deferral. Make AC-13 a hard gate item. |
| SR-05 | Wide feature, two waves (6 capability strands + 3 doc bands). Risk of all-or-nothing delivery or Wave-1/Wave-2 entanglement that blocks the AC-14 proof-by-use exit. | High | Med | Architect must keep Wave-1 (AC-01–09 + AC-14) independently shippable; ensure docs/Band-3 (AC-10–13) have zero code coupling to the instrument core. |
| SR-06 | Trust-metric "class" (AC-02–04) is scoped as reusable beyond crt-053 (quarantine, contradiction). Generality ambition can balloon Wave-1. | Med | Med | Constrain Wave-1 to the two assertions needed (forbidden-absent, rank-below); design extensibly but do not build speculative assertion types. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | OQ-3 embed-model-drift durability trap: embed-at-load makes the durable yardstick silently embed-model-dependent (a 2nd staleness axis beyond schema). Unimatrix #4085 already burned on KB-snapshot drift producing fake MRR deltas. | High | Med | Binding: durable reference must not silently become embed-model-dependent. Architect picks branch (a) frozen vector sidecar OR (b) embed model-id/dim in the hash (SR-01) — and states it in spec. Lean (b) to unify with the drift guard. |
| SR-07 | Property-based `expected` (AC-05, C-04) is the durability bet. If properties are under-specified they regress to literal-ID behavior or to null-`expected` self-consistency — the exact rework that burned ASS-037/ASS-039 (Unimatrix #3997). | High | Med | Spec must define each property type (redirect-to-head, absence, rank-below) operationally; ban null `expected` in the primary set; assert outcomes not constants (crt-013 #703). |

## Assumptions

- **Penalty `const`s cleanly thread into config** (Goals 1, AC-01; SCOPE §"Verified gaps"). If a penalty is read in a hot path or non-config context, additive exposure is not bit-for-bit free. (SR-08)
- **A retrieval-shape hash can be defined deterministically** over enumerated inputs (Goal 6, OQ-4). If shape is not fully capturable by the enumerated set, the guard gives false confidence. (SR-01)
- **A token-proxy is a faithful-enough cost signal** for downstream cost-of-noise findings (Goal 3, OQ-1). If proxy error is large, ass-073's cost conclusions are unsafe. (SR-02)
- **Fixture corpus stays small/durable while still yielding usable signal** (Goals 4–5, AC-06). A corpus too small to exercise trust shapes proves the instrument runs but not that it measures. (SR-06, AC-14)

## Design Recommendations

- **Unify the hash early (SR-01, SR-03).** Make embedding model-id/dimensionality a hash input so OQ-3 branch (b) holds and embed-at-load is safe — collapses two staleness axes into one guard and one OQ-5 trigger definition.
- **Front-load the default-equivalence test (SR-08).** A failing bit-for-bit assertion at default config is the cheapest early signal that a construction site was missed.
- **Specify property assertions operationally (SR-07).** Mirror Unimatrix #3997/#4085: no null/literal `expected` in the primary corpus; carry a snapshot/shape stamp for comparability.
- **Protect Wave-1 independence and AC-14 (SR-05).** Treat the end-to-end correlated sweep as the Wave-1 exit; let docs/Band-3 trail with no code coupling.
- **Hold the boundaries (SR-04).** No `.claude/protocols/` edits; cost narrowing is an in-design justified call, never a deferral.

## Knowledge Stewardship
- Queried: context_search for eval-harness staleness/drift, wide-feature rework, config-exposure patterns — found #4085 (snapshot-drift fake MRR), #3997 (null-expected self-consistency rework), #4131/#4070/#3779 (multi-site config-threading trap). All three directly informed SR-01/03, SR-07, SR-08.
- Stored: nothing novel yet — patterns observed are single-feature confirmations of existing entries; will reassess for a cross-feature "instrument-vs-experiment durability" pattern at retro if a 2nd feature exhibits it.
