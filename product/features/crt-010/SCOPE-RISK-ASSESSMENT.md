# Scope Risk Assessment: crt-010

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Successor similarity computation (cosine against stored embedding) adds per-result vector fetches + math to hot search path, risking latency regression | High | High | Architect should benchmark both options; consider lazy computation only when deprecated entry would otherwise rank in top-k |
| SR-02 | Hardcoded penalty multipliers (0.7x, 0.5x) are arbitrary — wrong values silently degrade retrieval quality with no observable signal | Med | High | Make penalties configurable constants with integration tests that assert ranking invariants (active > deprecated at equal similarity) |
| SR-03 | Vector index pruning permanently removes deprecated embeddings from HNSW; restoring a deprecated entry to Active requires re-embedding (ONNX inference) | Med | Low | Architect should document re-embedding as explicit cost of restore; consider whether restore path even exists today |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | UDS strict filter + superseded exclusion may return zero results when knowledge base is dominated by deprecated entries (current ratio: 123 deprecated vs 53 active) | High | Med | Spec should define fallback behavior — return fewer results, never return wrong results, but handle empty gracefully |
| SR-05 | Single-hop supersession limit means transitive chains (A→B→C) silently drop the correct successor C; no warning surfaces this | Low | Low | Acceptable for v1 per non-goals; architect should ensure the constraint is enforced consistently |
| SR-06 | Scope touches 6 components across 2 crates but declares "no schema changes" — verify that `caller_context` / strict-vs-flexible mode doesn't need new stored state | Med | Low | Confirm all mode signaling is in-memory / per-request only |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Co-access filtering requires entry status to flow from server into engine crate (`coaccess.rs`), creating a new cross-crate data dependency | Med | High | Architect should decide interface — pass status set vs. filter callback — to keep engine crate decoupled from server types |
| SR-08 | Two retrieval modes (strict/flexible) with different filtering, penalty, and injection behavior increase combinatorial test surface significantly | Med | Med | Spec should enumerate exact behavioral matrix (mode × status × superseded × co-access) to bound testing |
| SR-09 | Briefing service, UDS listener, and MCP tools all call into SearchService — changes to SearchService API signature ripple across three callers | Med | High | Architect should design the mode parameter to be backward-compatible (default = current behavior) to limit blast radius |

## Assumptions

- **SCOPE §Non-Goals**: "No schema changes" assumes all required fields (`superseded_by`, `status`) exist with correct types. If any field is Optional and not populated on older entries, filtering may silently skip them.
- **SCOPE §Component 5**: Compaction excludes deprecated from HNSW. Assumes compaction runs frequently enough that deprecated entries don't pollute search for extended periods between compactions.
- **SCOPE §Component 2**: Successor injection assumes `superseded_by` contains a valid, fetchable entry ID. Dangling references (deleted successor) would cause silent failures or panics.

## Design Recommendations

- **SR-01, SR-04**: Architect should treat search latency and empty-result behavior as first-class design constraints, not afterthoughts.
- **SR-07, SR-09**: Cross-crate interface design is the key architectural decision — get the SearchService API and engine-crate boundary right early.
- **SR-02, SR-08**: Spec writer should define ranking invariants as testable acceptance criteria, not just penalty values.
