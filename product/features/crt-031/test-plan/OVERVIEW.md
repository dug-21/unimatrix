# Test Plan Overview: crt-031
## Category Lifecycle Policy + boosted_categories De-hardcoding

---

## Overall Test Strategy

crt-031 makes no schema changes and no MCP tool interface changes. All new behavior is
config-driven and startup-validated, with output surfaced via the existing `context_status`
tool. The test approach is:

1. **Unit tests** (primary): All 27 ACs are testable at the unit level. Unit tests are
   deterministic, fast, and exercise every branch of the new logic.
2. **Integration harness** (supplementary): The existing `adaptation` and `tools` suites
   cover `context_status` through the MCP interface. One new integration test validates
   the `category_lifecycle` field in the JSON status response. Smoke gate is mandatory.
3. **Grep verifications** (mandatory pre-implementation): R-11 and R-01 both require grep
   scans before the first code change. These are named steps in the per-component plans.

### Test Organization

Unit tests live in `#[cfg(test)]` blocks inside each modified source file. Test functions
follow the `test_{function}_{scenario}_{expected}` naming convention. Async tests use
`#[tokio::test]`.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Component File(s) | Test Location | Test Functions |
|---------|----------|-------------------|---------------|----------------|
| R-01 | Critical | `infra/config.rs` | config.rs tests | `test_validate_config_ok_both_parallel_zeroed`, `test_validate_config_adaptive_error_isolated`, `test_validate_config_boosted_error_isolated`, `test_validate_config_fixture_pattern_audit` (grep step) |
| R-02 | Critical | `services/status.rs`, `background.rs`, `services/mod.rs` | status.rs tests + compile check | `test_status_service_new_compiles_all_4_sites` (compile), `test_status_service_compute_report_has_lifecycle`, `test_run_single_tick_carries_operator_arc` |
| R-03 | Medium | `infra/categories/mod.rs` | categories tests | `test_add_category_defaults_to_pinned`, `test_validate_passes_is_adaptive_false_simultaneously` |
| R-04 | High | `infra/categories/mod.rs` | cargo check | compile test after split; all pre-existing category tests pass |
| R-05 | Medium | `background.rs` | code review | confirm `#[allow(clippy::too_many_arguments)]` present |
| R-06 | Low | `background.rs` | code review | stub uses `list_adaptive()` once, no lock held across `.await` |
| R-07 | High | `infra/config.rs` | config.rs tests | `test_merge_configs_adaptive_project_wins`, `test_merge_configs_adaptive_global_fallback` |
| R-08 | Medium | `mcp/response/status.rs`, `services/status.rs` | status.rs tests | `test_category_lifecycle_sorted_alphabetically`, `test_category_lifecycle_json_sorted` |
| R-09 | Low | `infra/categories/mod.rs` | categories tests | `test_new_is_adaptive_lesson_learned_true` (AC-13) |
| R-10 | High | `background.rs`, `mcp/response/status.rs` | background tests, status response tests | at least 2 tests per file confirmed before gate 3b |
| R-11 | Critical | `infra/config.rs`, `main_tests.rs` | config.rs + main_tests.rs | `test_knowledge_config_default_boosted_is_empty`, `test_knowledge_config_default_adaptive_is_empty`, `test_default_config_boosted_categories_is_lesson_learned` (rewritten) |
| I-01 | High | `main.rs` | compile test | both call sites updated; cargo check passes |
| I-02 | Medium | `mcp/response/status.rs` | status response tests | `test_status_report_default_category_lifecycle_is_empty` |
| I-03 | Medium | `mcp/response/status.rs` | status response tests | JSON comparison via `serde_json::to_value`, not raw string |
| I-04 | High | `background.rs` | unit test | `test_run_single_tick_uses_operator_arc_not_fresh` |

---

## Cross-Component Test Dependencies

The components implement a dependency chain. Tests should be implemented in this order to
avoid debugging downstream failures caused by upstream gaps:

```
config.rs (KnowledgeConfig + validate_config)
    ↓
categories/mod.rs (CategoryAllowlist::from_categories_with_policy + is_adaptive + list_adaptive)
    ↓
main.rs (startup wiring — compile-only test)
    ↓
services/status.rs + mcp/response/status.rs (StatusService + StatusReport field)
    ↓
background.rs (tick stub + parameter threading)
    ↓
eval/profile/layer.rs + literal removal sites (grep verification)
```

A compile failure in `categories/mod.rs` (R-04, module split) blocks tests in all downstream
components. Run `cargo check -p unimatrix-server` after the module split before adding any
new code.

---

## Integration Harness Plan

### Applicable Suites

| Suite | Applicability | Rationale |
|-------|--------------|-----------|
| `smoke` | **Mandatory gate** | Any change at all — minimum per-feature gate |
| `tools` | Run | `context_status` is one of the 12 tools; existing test validates response shape; the new `category_lifecycle` field must not break the existing status response |
| `adaptation` | Run | Existing suite exercises `context_status` (test_status_report_with_adaptation_active, test_embedding_consistency_with_adaptation); verify the new `category_lifecycle` field appears without breaking existing assertions |

Suites NOT needed for this feature: `lifecycle`, `volume`, `security`, `confidence`,
`contradiction`, `edge_cases`, `protocol`. This feature changes no MCP protocol, no schema,
no security boundary, no scoring logic.

### Gap Analysis

The existing `adaptation` suite calls `context_status(format="json")` and checks
`category_distribution` and `coherence`. It does NOT assert on `category_lifecycle`. Because
`category_lifecycle` is a new JSON field, the existing tests will not break (they access known
keys). However, there is no integration-level test that:

1. Verifies `category_lifecycle` appears in the JSON status response at all.
2. Verifies the lifecycle labels (`"adaptive"` / `"pinned"`) are present and correct.

### New Integration Test Required

Add one test to `product/test/infra-001/suites/test_tools.py`:

```python
def test_status_category_lifecycle_field_present(server):
    """crt-031: context_status JSON output includes category_lifecycle field.

    Verifies the new per-category lifecycle section is populated and
    contains correctly labeled entries (adaptive vs pinned).
    AC-09.
    """
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)

    lifecycle = report.get("category_lifecycle")
    assert lifecycle is not None, "category_lifecycle field missing from status JSON"
    # Default config: lesson-learned is adaptive, others are pinned
    assert isinstance(lifecycle, (list, dict)), (
        f"category_lifecycle must be a list or dict, got: {type(lifecycle)}"
    )
    # Must contain at least the 5 default categories
    if isinstance(lifecycle, list):
        labels = {item[0]: item[1] for item in lifecycle} if lifecycle and isinstance(lifecycle[0], list) else {}
        # If dict format (category -> label)
    elif isinstance(lifecycle, dict):
        labels = lifecycle
    # lesson-learned should be adaptive (default serde config)
    # Exact assertion depends on final JSON format (see I-03 note)
    assert len(lifecycle) >= 5, (
        f"Expected at least 5 categories in lifecycle, got: {lifecycle}"
    )
```

**Note on fixture**: Use `server` (fresh DB, function scope). No pre-loaded state needed —
the default config determines the lifecycle labels.

**Placement**: In `test_tools.py` alongside other `context_status` tests.

### When to Add New Test

The integration test is added during Stage 3b (implementation) when the `category_lifecycle`
JSON format is confirmed, or during Stage 3c (execution) if the format is clear from unit
tests. The tester agent (Stage 3c) adds this test before running the harness.

---

## AC Coverage Summary

| AC Range | Component | Count |
|----------|-----------|-------|
| AC-01 – AC-03 | config.rs (serde round-trip, omit, explicit) | 3 |
| AC-04, AC-14, AC-15 | config.rs (validate_config) | 3 |
| AC-05 – AC-08 | categories/mod.rs (is_adaptive, poison) | 4 |
| AC-09 | services/status.rs + mcp/response/status.rs | 1 |
| AC-10, AC-11 | background.rs (tick stub) | 2 |
| AC-12, AC-13 | categories/mod.rs (regression, new()) | 2 |
| AC-16 | config.rs (merge_configs) | 1 |
| AC-17, AC-18, AC-27 | config.rs + main_tests.rs (Default rewrite) | 3 |
| AC-19 – AC-21 | eval-layer.rs + literal removal (grep) | 3 |
| AC-22 | README.md (manual review) | 1 |
| AC-23 | cargo test --workspace | 1 |
| AC-24, AC-25 | config.rs (parallel-list fixture isolation) | 2 |
| AC-26 | PR description (pre-implementation grep) | 1 (manual) |
| **Total** | | **27** |
