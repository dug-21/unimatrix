# Scope Risk Assessment: col-015

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | ONNX model availability in CI. Server-level integration tests require the all-MiniLM-L6-v2 ONNX model file. If the model is not present in CI environments, tests will fail or need to be skipped, undermining the validation goal. | Med | Med | Architect should design tests to detect model absence and skip gracefully with `#[ignore]` or runtime check. Ensure model download is part of CI setup, or provide a fallback test path using pre-computed embeddings. |
| SR-02 | Kendall tau computation complexity. Formal rank correlation requires O(n^2) pair comparisons. For 20-entry scenarios this is trivial, but if scenario size grows, test execution time could increase. Additionally, Kendall tau interpretation in failure messages may be unclear to developers unfamiliar with the metric. | Low | Low | Implement Kendall tau as a simple pure function (no external dependency). Cap scenario sizes at 20 entries. Include human-readable explanation in assertion failure messages. |
| SR-03 | SearchService constructor complexity. The full SearchService requires Store + VectorStore + EmbedService + AdaptationService + SecurityGateway + AuditContext. Constructing this stack for tests may be fragile and tightly coupled to server internals. | Med | High | Architect should provide a test-only constructor or builder pattern that wires up the service with sensible defaults. Consider a `TestSearchService` helper that encapsulates construction. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Scope creep into weight tuning. Validation tests may reveal that current weights produce poor rankings. The temptation to fix weights within this feature is high, but it violates the "no production code changes" constraint and introduces a moving target for test expectations. | Med | High | SCOPE.md explicitly excludes weight changes. If validation reveals poor rankings, file a follow-up issue. Tests should assert relative ordering properties, not exact score values, to remain stable if weights are later tuned. |
| SR-05 | Ambiguity in "correct ranking." Defining what the "right" ranking is for a given query and knowledge base requires domain judgment. If scenarios are not carefully designed with clear rationale, tests become opinion-encoded-as-code rather than objective validation. | Med | Med | Every scenario fixture must include a doc comment explaining WHY the expected ranking is correct. Use self-evident cases (active human-authored entry beats quarantined auto-extracted entry) rather than subtle preference assertions. |
| SR-06 | Test-only crate vs. integration tests. The scope proposes test code in multiple crates (engine, observe, server). This distributes test infrastructure across three locations, making it harder to maintain a unified view of pipeline validation. | Low | Med | Shared fixtures in unimatrix-engine's test_scenarios module behind `test-support` feature flag provide the single source of truth. Integration tests in other crates consume these fixtures. Document the layering clearly. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Predecessor features incomplete. col-015 depends on 6 predecessor features (crt-011, vnc-010, col-014, crt-012, nxs-009, crt-013). If any are incomplete or contain bugs, pipeline validation tests may fail for reasons unrelated to col-015 code. | Med | Med | Verify predecessor feature completion before implementation begins. If predecessors have known issues, document them as expected failures in test scenarios rather than blocking. |
| SR-08 | Cross-crate feature flag coordination. The `test-support` feature flag must be enabled in unimatrix-engine's Cargo.toml and propagated to dependent crates' `[dev-dependencies]`. Misconfigured feature flags cause compilation failures only in test mode. | Low | Med | Architect should specify exact Cargo.toml changes needed. Verify `cargo test -p unimatrix-engine --features test-support` works before implementation begins. |
| SR-09 | Store schema assumptions. Test scenarios construct EntryRecord structs directly. If the schema evolves (new fields, changed defaults), test fixtures break silently or fail to compile. | Low | Low | Use builder patterns (like existing `make_test_entry`) that fill defaults for new fields. Pin test expectations to current schema version. |

## Assumption Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-10 | Assumption: pure function composition is equivalent to SearchService behavior. The engine-level tests compose `rerank_score()`, `compute_confidence()`, and penalty constants directly. If SearchService applies these differently (e.g., different sort stability, intermediate clamping), engine-level tests may pass while real behavior diverges. | High | Med | Server-level integration tests (now in scope) mitigate this directly. Engine-level tests validate the math; server-level tests validate the application. Both are needed. |
| SR-11 | Assumption: synthetic embeddings are representative. Synthetic embeddings (fixed vectors) may not exercise the same HNSW search paths as real embeddings. Real embeddings cluster differently, and HNSW's greedy search may produce different neighbor orderings. | Med | Med | The blend approach (synthetic + real ONNX) addresses this. At least 3 server-level tests should use real embeddings with natural-language content. |

## Top 3 Risks for Architect Attention

1. **SR-03 (SearchService constructor complexity)**: The biggest implementation friction. The architect must design a clean test construction path.
2. **SR-10 (Pure function vs. real behavior divergence)**: Motivates the server-level tests. Architect should ensure both test levels exercise the same ranking logic.
3. **SR-04 (Scope creep into weight tuning)**: Organizational risk. Tests must assert relative ordering, not exact values, to stay stable across future weight changes.
