# Agent Report — nan-018-agent-2-spec (Specification Writer)

## Deliverable
`product/features/nan-018/specification/SPECIFICATION.md`

## Summary
Authored SPECIFICATION.md from SCOPE.md (AC-01…AC-14, all OQs resolved) and the SR-01…SR-08 risk assessment, grounded in the live eval module (`crates/unimatrix-server/src/eval/`), `docs/testing/eval-harness.md`, and the penalty constants in `crates/unimatrix-engine/src/graph.rs:41–59` (`graph_penalty` at `:478`). Confirmed against current main: penalty values are module `const`s, no penalty/orphan/hop_decay fields exist in config, metric set lacks absence/rank-below, cost is latency-only.

## Coverage
- 31 FRs (tunability, trust class, token-weighted cost, fixture corpus, two-corpus model, drift guard, docs, Band-3, proof-by-use), each testable, Wave-tagged.
- 8 NFRs incl. AC-01 bit-for-bit equivalence (NFR-01), additive-only (NFR-02), hash determinism (NFR-03), Wave independence (NFR-04).
- AC-01…AC-14 mapped 1:1 with verification methods, incl. default-equivalence test, deliberate-hash-mismatch test, and the AC-14 end-to-end correlated sweep.
- Domain models: five fixture shapes, three property assertion types (operationally defined), retrieval-shape hash manifest (4 enumerated input categories), two-corpus roles, token-weighted cost.
- C-01…C-06 + C-07 dissolved + Band-3 recommendation-only boundary.

## Key decisions / interpretations
- **Durability branch (b) taken** (OQ-3): embedding model-id/dimensionality is a hash input, so embed-at-load is safe and no frozen vector sidecar is required. Stated explicitly per SR-03; flagged that removing it reverts to branch (a).
- **Trust failures counted as regressions** with the existing body-only / exit-0 semantics, evaluated in the same run as P@5/MRR/cost (C-03/AC-04).
- **Cost token-proxy** mandated as primary; precision+k allowed only as a justified, non-deferral in-design call.

## Open questions handed to architect
1. Exact per-result token-proxy formula + error bars (OQ-1/SR-02).
2. Precise entry-column / confidence-dimension list feeding the hash manifest + manifest version (OQ-4/SR-01).
3. Fail-loud vs warn on hash mismatch (FR-22) — scope allows either; existing eval convention (exit 0, body-only) vs corpus-validity protection argue opposite ways.
4. On-disk schema for trust assertions vs property-based `expected` (whether they extend `ScenarioRecord.expected` or add a sibling field) — pseudocode/tester need this aligned.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #4888 (eval positive-relevance baseline), #3610 (nan-010 7-component harness-extension layer order), #2806 (A/B profile-TOML pattern), #4148/#4333 (spec-vs-struct field-type mismatch lessons → informs FR-03/AC-01 site enumeration). No prior trust/cost/fixture-corpus pattern exists; nan-018 establishes it. No storage (read-only tier).
