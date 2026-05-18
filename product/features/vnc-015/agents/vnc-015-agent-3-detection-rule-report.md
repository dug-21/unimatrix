# Agent Report: vnc-015-agent-3-detection-rule

**Component**: Component 6 — DependencyOnDeprecated Detection Rule
**Feature**: vnc-015
**Date**: 2026-05-15

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-observe/src/detection/scope.rs`
- `/workspaces/unimatrix/crates/unimatrix-observe/src/detection/mod.rs`
- `/workspaces/unimatrix/crates/unimatrix-observe/src/extraction/recurring_friction.rs`
- `/workspaces/unimatrix/crates/unimatrix-observe/src/report.rs`
- `/workspaces/unimatrix/crates/unimatrix-observe/tests/detection_isolation.rs`

## Signature of default_rules() Before and After

**Before:**
```rust
pub fn default_rules(
    history: Option<&[MetricVector]>,
) -> Vec<Box<dyn DetectionRule>>
```

**After:**
```rust
pub fn default_rules(
    history: Option<&[MetricVector]>,
    stale_edges: Vec<(u64, u64)>,
) -> Vec<Box<dyn DetectionRule>>
```

## Implementation Summary

### scope.rs
Added `DependencyOnDeprecatedRule` struct with constructor injection (ADR-004). `detect()` is fully synchronous — no I/O, no async. Logic: fast-path empty check, then one `HotspotFinding` per `(source_id, target_id)` stale pair with `Severity::Warning`.

### mod.rs
- Updated `default_rules()` signature with `stale_edges: Vec<(u64, u64)>` second parameter
- Registered `DependencyOnDeprecatedRule::new(stale_edges)` as rule 23 (Scope category)
- Renamed `test_default_rules_has_22_rules` to `test_default_rules_has_23_rules`, assert updated to 23
- Updated `test_default_rules_names` to include `"dependency_on_deprecated"`
- Added 4 new tests: `test_default_rules_dependency_on_deprecated_is_registered`, `test_default_rules_signature_accepts_stale_edges`, `test_default_rules_stale_edges_forwarded_to_rule`
- Updated `test_default_rules_with_history` assert from 22 → 23

### recurring_friction.rs
Added `"dependency_on_deprecated"` match arm in `remediation_for_rule()`. Required by `test_all_default_rules_have_non_fallback_recommendation_and_remediation` — which enforces every registered rule has a specific remediation string.

### report.rs + detection_isolation.rs
Updated `default_rules(None)` → `default_rules(None, vec![])` at all internal call sites. The `tools.rs` caller in `unimatrix-server` was intentionally left for Wave 2 per task specification.

## Caller Comment

Added to `mod.rs` doc comment: `// CALLERS: tools.rs context_cycle_review handler must pass stale_edges`

## Tests: Pass/Fail Count

**unimatrix-observe total: 512 passed, 0 failed**
- Lib tests: 440 passed
- detection_isolation integration: 22 passed
- DSL isolation: 44 passed
- extraction_pipeline: 6 passed

New tests added (6 unit tests in scope.rs, 4 in mod.rs):
- `test_dependency_on_deprecated_rule_new_constructs`
- `test_dependency_on_deprecated_rule_detect_fires_on_match`
- `test_dependency_on_deprecated_rule_empty_stale_edges_no_findings`
- `test_dependency_on_deprecated_rule_no_records_still_fires`
- `test_dependency_on_deprecated_rule_multiple_stale_pairs`
- `test_dependency_on_deprecated_rule_name_and_category`
- `test_default_rules_has_23_rules`
- `test_default_rules_dependency_on_deprecated_is_registered`
- `test_default_rules_signature_accepts_stale_edges`
- `test_default_rules_stale_edges_forwarded_to_rule`

## Workspace-Wide Compile

Expected compile error exists at `crates/unimatrix-server/src/mcp/tools.rs:1988`:
```
error[E0061]: this function takes 2 arguments but 1 argument was supplied
let rules = unimatrix_observe::default_rules(history_slice);
```
This is the Wave 2 tools.rs caller update — intentionally not touched in this wave.

## Issues / Blockers

None. Implementation follows pseudocode exactly.

One non-obvious dependency discovered: `recurring_friction.rs::remediation_for_rule()` enforces a test invariant that all registered rules have non-fallback remediation text. This is enforced at unit-test time, not compile time. Fixed by adding the `"dependency_on_deprecated"` match arm.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entry #2929 (critical: unimatrix-observe has zero logging dependencies, no `tracing::warn!` allowed) and entry #4421 (ADR-004 constructor injection decision, confirmed approach).
- Stored: entry #4433 "Every new DetectionRule must have a matching arm in remediation_for_rule() or the registration test fails" via /uni-store-pattern — this gotcha is invisible in source code and would re-hit any future agent adding a detection rule.
