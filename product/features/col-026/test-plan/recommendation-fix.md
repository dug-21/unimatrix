# Test Plan: recommendation-fix

**Crate**: `unimatrix-observe/src/report.rs`
**Risks covered**: (none from R-01–R-13 directly; this is a correctness fix)
**ACs covered**: AC-19, AC-17

---

## Component Scope

This component replaces the `compile_cycles` recommendation text at two locations in
`report.rs` (lines 62 and 88 per ARCHITECTURE.md §Component 5 audit). It also verifies
that `permission_friction_events` recommendation is independent.

**ADR-005**: The new action text is:
```
"Batch field additions before compiling — repeated compile cycles suggest iterative per-field changes; complete struct definitions before first build"
```

The `rationale` string at line 88 still contains `(threshold: 10)` — this line is in the
`rationale` field, not the `action` field. The threshold language audit (AC-13) applies to
rendered markdown claim strings, not to rationale strings that go into JSON. However, if the
rationale is rendered in recommendations, the formatter must strip threshold language there too.

Confirm: `render_recommendations` in `retrospective.rs` renders `action` only, not `rationale`.
The `rationale` field is included in JSON format but not displayed in markdown. This means the
`(threshold: 10)` in the rationale is not visible in markdown output and does not require
post-processing. This is documented as a known exception in the threshold audit.

---

## Unit Test Expectations

All tests extend the existing `#[cfg(test)] mod tests` block in `report.rs`.

### AC-19: compile_cycles Recommendation Text Replacement

#### Test: `test_recommendation_compile_cycles_above_threshold` (AC-19, updated)

**Current assertion** (must be updated):
```rust
assert!(recs[0].action.contains("incremental"));
```

**Updated assertion**:
```rust
assert!(recs[0].action.contains("batch") || recs[0].action.contains("iterative"),
    "compile_cycles action must reference batching or iterative compilation, got: {}", recs[0].action);
assert!(!recs[0].action.contains("allowlist"),
    "compile_cycles action must not mention allowlist, got: {}", recs[0].action);
assert!(!recs[0].action.contains("settings.json"),
    "compile_cycles action must not reference settings.json, got: {}", recs[0].action);
```

**Setup** (unchanged from current test):
```rust
let hotspot = HotspotFinding {
    rule_name: "compile_cycles".to_string(),
    measured: 15.0,
    threshold: 10.0,
    ...
};
let recs = recommendations_for_hotspots(&[hotspot]);
assert_eq!(recs.len(), 1);
assert_eq!(recs[0].hotspot_type, "compile_cycles");
```

### AC-19: permission_friction_events Independence

#### Test: `test_permission_friction_recommendation_independence` (AC-19)

**Scenario**: Verify that `permission_friction_events` recommendation does not reference
compile cycles or allowlists in the wrong way.

```rust
let hotspot = HotspotFinding {
    rule_name: "permission_retries".to_string(),
    measured: 8.0,
    threshold: 3.0,
    ...
};
let recs = recommendations_for_hotspots(&[hotspot]);
assert_eq!(recs.len(), 1);
// permission_retries CAN reference allowlist (it's the correct recommendation for that type)
assert!(recs[0].action.contains("allowlist"),
    "permission_retries action should reference allowlist (its correct fix)");
// But it must NOT reference compile_cycles concepts
assert!(!recs[0].action.contains("batch"),
    "permission_retries must not reference batch compilation");
assert!(!recs[0].action.contains("iterative"),
    "permission_retries must not reference iterative compilation");
```

**Assert**: The two recommendation paths are independent. `compile_cycles` → iterative
compilation framing. `permission_retries` → allowlist framing. No cross-contamination.

### AC-19: No Allowlist Text in compile_cycles

#### Test: `test_compile_cycles_action_no_allowlist` (AC-19)

**Direct assertion** on the `action` string:
```rust
let action = recommendation_for(&hotspot_with_rule("compile_cycles", 15.0)).unwrap().action;
assert!(!action.contains("allowlist"));
assert!(!action.contains("settings.json"));
```

### AC-17: Existing Tests After Fix

After updating `test_recommendation_compile_cycles_above_threshold`, the following tests
must still pass unchanged:

| Test | Why it must pass |
|------|-----------------|
| `test_recommendation_compile_cycles_below_threshold` | Below-threshold → no recommendation. Not affected by text change. |
| `test_recommendation_permission_retries` | Still asserts `recs[0].action.contains("allowlist")`. Must still pass. |
| `test_recommendation_coordinator_respawns` | Unrelated. Must still pass. |
| `test_recommendation_sleep_workarounds` | Unrelated. Must still pass. |
| `test_recommendation_unknown_type` | Unrelated. Must still pass. |

### Threshold Language in Rationale (Documentation)

The `rationale` for `compile_cycles` at line 88 currently contains:
```
"{:.0} compile cycles detected (threshold: 10) -- consider narrowing test scope"
```

After the fix, the rationale may also need updating. However, the rationale is NOT rendered
in markdown output (`render_recommendations` only renders `action`). The rationale is included
in JSON responses. No threshold language test covers rationale strings in JSON.

Document this as an **open question**: should the rationale at line 88 also be updated to
remove `(threshold: 10)` for JSON consumers? If so, update the rationale to:
```
"{:.0} compile cycles detected — repeated per-field compilation patterns identified"
```

This is a minor improvement not strictly required by AC-19 but consistent with the spirit of
the fix.

---

## Integration Test Expectations

No new infra-001 tests required for this component. The fix is visible in both markdown and
JSON format output. Existing `tools` suite tests that call `context_cycle_review` and verify
the recommendations section will indirectly validate this fix for any cycle that triggers the
compile_cycles hotspot.

---

## Edge Cases

- `measured = 10.5` (just above threshold): recommendation generated. Action text uses new
  iterative framing.
- `measured = 10.0` (exactly at threshold): boundary case. The guard is `> 10.0`, so this
  produces no recommendation. Existing `test_recommendation_compile_cycles_below_threshold`
  uses `measured = 5.0`; a new boundary test at `measured = 10.0` would be thorough.

#### Test: `test_compile_cycles_at_threshold_boundary` (edge case)

```rust
let hotspot = HotspotFinding { rule_name: "compile_cycles".to_string(), measured: 10.0, ... };
let recs = recommendations_for_hotspots(&[hotspot]);
assert!(recs.is_empty(), "measured == 10.0 should not trigger (guard is > 10.0)");
```

---

## Self-Check

- [ ] `test_recommendation_compile_cycles_above_threshold` updated assertion asserts `"batch"` or `"iterative"` present.
- [ ] Same test asserts `"allowlist"` absent.
- [ ] `test_permission_friction_recommendation_independence` verifies allowlist is still correct for `permission_retries`.
- [ ] No other report.rs tests broken by the text change.
- [ ] `cargo test -p unimatrix-observe -- report` passes with all tests.
