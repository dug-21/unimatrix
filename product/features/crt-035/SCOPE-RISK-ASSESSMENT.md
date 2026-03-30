# Scope Risk Assessment: crt-035

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Weight divergence between forward and reverse edges: Step B/C updates the forward edge independently of the reverse edge; a partial tick failure can leave the two directions with different weights, causing asymmetric PPR scores | Med | Med | Architect should specify whether both-direction updates must be atomic per pair (same transaction) or whether eventual convergence on the next tick is acceptable |
| SR-02 | `co_access_promotion_tick.rs` 500-line ceiling: adding two INSERTs + two UPDATE paths per pair without a helper refactor pushes the file over limit | Low | High | Scope explicitly requires a `promote_one_direction` helper; architect must enforce this constraint in the implementation brief |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | `inserted_count` / `updated_count` semantics shift: the tick now counts edge writes, not pair promotions; callers or dashboards that parse the tracing log assuming 1 insert = 1 pair will misinterpret doubled counts | Low | Med | Spec writer must document the new log format (D2 in SCOPE.md) and flag that `edges_inserted` can be up to 2× `promoted_pairs` |
| SR-04 | Back-fill SQL (D4 NOT EXISTS guard) scans `GRAPH_EDGES` twice per CoAccess row; on production DBs with large bootstrap edge sets this could cause a slow migration open | Low | Low | Architect should confirm an index on `(source_id, target_id, relation_type)` exists to cover the NOT EXISTS self-join; if not, recommend adding one or accepting the one-time cost |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | Test `test_inserted_edge_is_one_directional` inverted but sibling tests `test_basic_promotion_new_qualifying_pair` and `test_double_tick_idempotent` have implicit `count == 1` assertions that also break — SCOPE.md lists them but they could be missed | Med | Med | Spec writer should enumerate every test that asserts edge count and require each to be updated; a missed test will produce a passing suite with a stale unidirectional assertion (see lesson #3579: absent test modules pass gate silently) |
| SR-06 | AC-12 (PPR traversal regression) depends on `TypedGraphState` tests; if those tests use a mock graph that bypasses `GRAPH_EDGES`, the back-fill may be correct but the PPR integration benefit goes unverified | Med | Low | Architect must confirm AC-12 test hits the real SQLite-backed graph path, not a synthetic in-memory fixture |
| SR-07 | Near-threshold oscillation risk on bidirectional pairs: pattern #3822 documents that oscillating pairs (weight flips above/below threshold between ticks) are unspecified — with two edge rows per pair, an oscillating pair can write one direction but not update the other, leaving the graph transiently asymmetric | Low | Low | Spec writer should reference pattern #3822 and define whether both-direction weight update must succeed atomically or is eventually consistent |

## Assumptions

- **SCOPE.md §GRAPH_EDGES Schema**: Assumes `UNIQUE(source_id, target_id, relation_type)` is the sole idempotency mechanism. If a future migration drops or weakens this constraint, INSERT OR IGNORE loses its safety guarantee.
- **SCOPE.md §Migration Framework**: Assumes schema version is exactly 18 at crt-035 start. Any concurrent branch that also bumps schema version (e.g., a hotfix) invalidates the `current_version < 19` guard.
- **SCOPE.md §Existing Forward-Only Promotion Tick**: Assumes every CoAccess edge in `GRAPH_EDGES` was written with `source = 'co_access'`. If any CoAccess edge was written via an ad-hoc path without setting `source`, the back-fill filter `WHERE source = 'co_access'` silently skips it.
- **SCOPE.md §Non-Goals**: Assumes cycle detection exclusion of CoAccess is stable (Pattern #2429, ADR-006 #3830). If a future feature re-includes CoAccess in cycle detection, bidirectional edges would then introduce real cycle risk.

## Design Recommendations

- **SR-01** — Architect should make explicit whether forward+reverse weight updates are wrapped in a single transaction per pair or remain independent SQL calls; the infallible-tick constraint (SCOPE.md §Constraints) already requires per-operation error handling, so atomicity here is a deliberate choice, not a default.
- **SR-05** — Spec writer should produce a complete list of all test assertions that reference edge direction or edge count in `co_access_promotion_tick_tests.rs` and require each to be explicitly updated; relying on "invert the one test" underspecifies the blast radius (lesson #3579).
- **SR-06** — Architect should confirm the AC-12 test path and document it in the implementation brief so the delivery agent does not default to a synthetic graph fixture.
