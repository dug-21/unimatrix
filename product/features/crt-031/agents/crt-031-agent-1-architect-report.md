# Agent Report: crt-031-agent-1-architect

## Status: Complete

## Artifacts Produced

- `product/features/crt-031/architecture/ARCHITECTURE.md`
- `product/features/crt-031/architecture/ADR-001-lifecycle-policy-config-model.md`

## ADR Unimatrix Entry IDs

| ADR File | Title | Unimatrix ID |
|---|---|---|
| ADR-001-lifecycle-policy-config-model.md | CategoryAllowlist lifecycle policy constructor and config model | #3772 |

## Key Design Decisions

1. **Constructor API**: `from_categories_with_policy(cats, adaptive)` is the canonical constructor. `from_categories` and `new()` delegate to it — zero call site breakage.

2. **Config model**: `adaptive_categories: Vec<String>` on `KnowledgeConfig` with `#[serde(default)]`, mirroring the `boosted_categories` structural pattern exactly. Validation follows the same cross-check approach with `ConfigError::AdaptiveCategoryNotInAllowlist`.

3. **Internal struct layout**: Two independent `RwLock<HashSet<String>>` fields (`categories` + `adaptive`) to avoid adding contention to the hot `categories` read path.

4. **Module split (SR-01)**: `categories.rs` (454 lines) will split into `infra/categories/mod.rs` + `infra/categories/lifecycle.rs`. Public path `crate::infra::categories::CategoryAllowlist` unchanged.

5. **Status format asymmetry (locked)**: Summary text shows only adaptive categories; JSON includes all per-category lifecycle data. Intentional per locked design decision — spec must include a golden-output test.

6. **Maintenance tick (SR-02)**: `Arc<CategoryAllowlist>` threaded through `spawn_background_tick`, `background_tick_loop`, and `maintenance_tick`. `BackgroundTickConfig` composite deferred out of scope.

7. **Test construction invariant (SR-03)**: All `validate_config` tests with custom `categories` must zero out both `boosted_categories` and `adaptive_categories`.

8. **Wiring test (SR-05)**: Compile-level test verifies `from_categories_with_policy` is used at `main.rs` call sites and `is_adaptive` returns expected values.

## Open Questions for Spec/Implementation

1. Does `StatusService` currently hold `Arc<CategoryAllowlist>`? If not, confirm whether it is added as a field or passed as a parameter to `compute_report`. Spec must check `StatusService::new()`.

2. Confirm the exact name of the `default_adaptive_categories` fn against the existing `boosted_categories` default fn name in `config.rs` before speccing.

3. New test count estimate: 22–30 unit tests. Confirm whether the gate requires an exact count or accepts a range in the implementation brief.

4. `BackgroundTickConfig` composite (SR-02) is deferred — flag as a follow-up issue after crt-031 ships.
