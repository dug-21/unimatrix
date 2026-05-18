# Scope Risk Assessment: vnc-016

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `unwrap_or_else` in `tools.rs:2169–2177` swallows SQL errors silently — any regression in the fixed query (column rename, schema drift) will again produce a quiet empty result with only a `tracing::warn!` | High | Med | Rust unit test (AC-09) must assert the non-empty return; architect should confirm no other callers of `query_stale_prerequisite_edges_for_cycle` rely on the silent-empty contract |
| SR-02 | `feature_entries` is populated at `context_store` write time only — back-filling is impossible; if test setup omits `feature_cycle` on entry A, the SQL join silently returns nothing and the test passes as a false negative | High | Med | Harness client extension (AC-05) is the sole write path in tests; implementer must verify `record_feature_entries` is called by the analytics write path, not just the MCP param forwarding |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | `context_cycle_review` result memoization — test using `force=False` (default) would hit stale cache and pass vacuously; scope requires `force=True` but this is easy to omit | Med | Med | AC-07 locks this down; spec writer should make `force=True` a hard constraint in the test body, not a recommendation |
| SR-04 | Negative-path companion test (AC-08) shares the same observation-seeding requirement; if `_seed_observation_sql` is called with a different cycle ID than the detection call, the test is structurally broken and always passes | Med | Low | Spec writer should require that cycle IDs are generated once per test function and reused across all setup steps |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | `client.py` extension adds `feature_cycle` kwarg; if propagation to `args` dict is guarded by `if feature_cycle is not None` but the MCP `StoreParams` treats missing field differently from explicit `null`, the integration may silently no-op | Low | Low | Verify `StoreParams` deserialization: missing key vs. `null` should both map to `Option::None`; check existing `serde(default)` on the field |
| SR-06 | Test extends the vnc-015 section of `test_tools.py`; that file is large and the shared live-server fixture is process-scoped — a new test that leaves corrupt `feature_entries` rows can contaminate sibling tests if the cycle ID is not unique | Med | Low | Use `uuid.uuid4().hex[:8]` suffix per constraint; cycle IDs must be unique per test invocation (already required, worth confirming in spec) |

## Assumptions

- **SCOPE.md §Background Research / Confirmed Wiring Defect**: Assumes `feature_entries.feature_id` is the only correct column name and that this name is stable. If a future schema migration renames it, AC-04 fix would re-introduce the bug silently.
- **SCOPE.md §Observation Data Requirement**: Assumes `_seed_observation_sql` with ≥20 rows is sufficient to trigger the detection path. If the detection threshold changes, the test setup quantity assumption becomes wrong.
- **SCOPE.md §Proposed Approach step 4**: Assumes `context_correct` is the correct deprecation mechanism and that it sets `status = 1` on entry A. This assumption is critical to the SQL `WHERE e.status = 1` clause.

## Design Recommendations

- (SR-01) The Rust unit test (AC-09) is the primary regression guard for the SQL fix — it must assert the returned `Vec` is non-empty, not just that the function does not panic.
- (SR-02) Spec writer should explicitly state that the `context_store` call for entry A must be the call that carries `feature_cycle` — not a subsequent update — because `record_feature_entries` runs at write time only.
- (SR-03, SR-04) Spec writer should express both tests as self-contained functions with a single `cycle_id = f"vnc016-{uuid.uuid4().hex[:8]}"` binding at the top, shared by all setup and assertion steps.
