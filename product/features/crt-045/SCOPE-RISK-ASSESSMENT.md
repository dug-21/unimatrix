# Scope Risk Assessment: crt-045

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Post-construction `Arc<RwLock<_>>` write may not propagate if `SearchService` clones the state (value copy) rather than the handle (Arc clone) at construction time | High | Low | Architect must verify `SearchService` holds `Arc::clone()` of the handle, not a cloned `TypedGraphState` value, before committing to the write-after-construction approach |
| SR-02 | `TypedGraphState::rebuild()` is `async fn` with no timeout guard — a snapshot database with a corrupted `GRAPH_EDGES` table could hang `from_profile()` indefinitely | Med | Low | Confirm rebuild has an implicit timeout via `sqlx` query timeout config; if not, add a bounded `tokio::time::timeout` wrapper in the eval path |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | The `typed_graph_handle()` accessor added to `EvalServiceLayer` (C-04) must be `pub(crate)` — if made `pub`, it widens the API surface beyond test use and may conflict with future encapsulation of `ServiceLayer` internals | Med | Med | Scope the accessor strictly to `pub(crate)`; document that it is test-only infrastructure |
| SR-04 | The TOML fix sets `distribution_change = false` but the field name and its required co-fields (`cc_at_k_min`, `icd_min`, `mrr_floor`) are structurally optional only when `distribution_change = false` — if a future profile sets `distribution_change = true` without all three, the same parse failure recurs silently | Low | Low | Add a parse-time comment in `ppr-expander-enabled.toml` explaining why `distribution_change = false` is intentional |

## Integration Risks

| Risk ID | Risk | Likelihood | Recommendation |
|---------|------|------------|----------------|
| SR-05 | Wired-but-unused anti-pattern (entry #1495, crt-019): the handle is written post-construction but `SearchService` reads it at query time under a read lock — the integration test must confirm a live search call observes the rebuilt graph, not just inspect the handle directly | High | Med | The integration test (AC-06) must invoke a search operation against the seeded snapshot, not only assert `use_fallback == false` on the handle |
| SR-06 | `TypedRelationGraph::build_typed_relation_graph()` filters out Quarantined entries (ADR-004 addendum, entry #3768) — the test snapshot must contain non-Quarantined entries with graph edges or the rebuilt graph will appear empty and the test will produce a false `use_fallback = false` with an empty graph | Med | Med | Seed the test snapshot with at least two Active entries and one S1/S2/S8 edge between them |

## Assumptions

- **SCOPE.md §"Proposed Approach" option (b)**: Assumes `ServiceLayer::with_rate_config()` internally constructs `TypedGraphState::new_handle()` and stores it as an `Arc<RwLock<TypedGraphState>>` that is shared by reference (not cloned by value) to `SearchService`. If `SearchService` holds a private snapshot taken at construction time, the post-construction write has no effect.
- **SCOPE.md §"Constraints" C-05**: Assumes the snapshot database is truly read-only during the eval run. If the eval runner writes to the snapshot (e.g., usage tracking), concurrent read/write to `GRAPH_EDGES` could produce a different graph than expected.
- **SCOPE.md §"Decisions" OQ-01**: Assumes `mrr_floor = 0.2651` and `p_at_5_min = 0.1083` are current baseline values. If baseline metrics have shifted since crt-042 shipped, these thresholds may gate-fail a correct implementation on first run.

## Design Recommendations

- **SR-01 / SR-05**: Before implementing option (b), the architect must read the `SearchService` constructor in `services/mod.rs` and confirm the `TypedGraphStateHandle` is stored as `Arc::clone()` — not cloned as a value. This single verification determines whether option (b) is safe or whether option (a) (pre-populated handle constructor parameter) is required despite the signature change.
- **SR-03**: Define the `typed_graph_handle()` accessor as `pub(crate)` in `EvalServiceLayer` and add a `#[cfg(test)]` guard on the delegation body if it is only needed in tests.
- **SR-06**: The test fixture in `layer_tests.rs` must insert at least one graph edge between two Active entries. A snapshot with only entries and no edges will produce an empty `TypedRelationGraph` even after a successful rebuild, making the `non-empty graph` assertion (AC-06) vacuously fail.
