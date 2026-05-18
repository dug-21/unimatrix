# Test Plan: DependencyOnDeprecated Detection Rule

**Component**: `crates/unimatrix-observe/src/detection/scope.rs`,
              `crates/unimatrix-observe/src/detection/mod.rs`
**Architecture ref**: Component 6
**Risk coverage**: R-10 (High)
**AC coverage**: AC-12, AC-13

---

## Key Constraints

- `DetectionRule` trait is synchronous: `detect()` must not perform I/O.
- `DependencyOnDeprecatedRule::new()` is the injection point — store data is pre-queried by
  the `context_cycle_review` handler and passed in as `Vec<(u64, u64)>`.
- `default_rules()` signature change is a breaking API change affecting all callers.
  All callers must be updated atomically — partial updates cause compile errors.
- `test_default_rules_has_22_rules` must be updated to assert 23.

---

## Unit Test Expectations

### Location: `crates/unimatrix-observe/src/detection/mod.rs`

#### test_default_rules_has_23_rules (AC-13 — updated from test_default_rules_has_22_rules)
- Act: `default_rules(None, vec![]).len()`
- Assert: `== 23`
- Note: R-10 Coverage Requirement: this count test must be the first assertion after any
  change to `default_rules()`. If the count is wrong, all other rule registration tests are
  unreliable.

#### test_default_rules_dependency_on_deprecated_is_registered
- Arrange: `let rules = default_rules(None, vec![])`
- Assert: at least one rule has `rule_name() == "dependency_on_deprecated"`
- Note: confirms DependencyOnDeprecatedRule is registered, not just that count increased

#### test_default_rules_signature_accepts_stale_edges
- Act: `default_rules(None, vec![(1u64, 2u64), (3u64, 4u64)]).len()`
- Assert: `== 23` (signature accepts Vec<(u64, u64)> without compile error)
- Note: this is primarily a compile-time check; execution confirms the parameter is forwarded

#### test_default_rules_stale_edges_forwarded_to_rule
- Arrange: `let rules = default_rules(None, vec![(42u64, 99u64)])`
- Find the `DependencyOnDeprecatedRule` in the rules vec
- Create a mock MetricVector record for source_id=42
- Act: `rule.detect(&[mock_record])`
- Assert: at least one finding returned with rule_name `"dependency_on_deprecated"`
- Note: confirms stale_edges parameter is actually forwarded to the rule, not ignored

### Location: `crates/unimatrix-observe/src/detection/scope.rs`

#### test_dependency_on_deprecated_rule_new_constructs
- Act: `DependencyOnDeprecatedRule::new(vec![(1, 2), (3, 4)])`
- Assert: no panic; struct created

#### test_dependency_on_deprecated_rule_detect_fires_on_match (R-10, AC-12)
- Arrange: `rule = DependencyOnDeprecatedRule::new(vec![(42u64, 99u64)])`
- Create a `MetricVector` record for feature cycle that includes source entry with id=42
- Act: `rule.detect(&[record])`
- Assert: returns non-empty findings vec
- Assert: finding has `severity == Warning` (not Error, not Info)
- Assert: finding has `rule_name == "dependency_on_deprecated"`

#### test_dependency_on_deprecated_rule_empty_stale_edges_no_findings
- Arrange: `rule = DependencyOnDeprecatedRule::new(vec![])`
- Act: `rule.detect(&[any_record])`
- Assert: returns empty findings vec (no false-positive when stale_edges is empty)
- Note: edge case from RISK-TEST-STRATEGY.md — empty stale_edges must not fire

#### test_dependency_on_deprecated_rule_no_match_no_findings
- Arrange: `rule = DependencyOnDeprecatedRule::new(vec![(42u64, 99u64)])`
- Create record for entry id=55 (not in stale_edges)
- Act: `rule.detect(&[record])`
- Assert: returns empty findings (no false-positive for entries not in stale_edges)

#### test_dependency_on_deprecated_rule_multiple_stale_pairs
- Arrange: `rule = DependencyOnDeprecatedRule::new(vec![(1, 2), (3, 4), (5, 6)])`
- Create records including entries 1, 3, and 5
- Act: `rule.detect(&records)`
- Assert: findings count equals 3 (one per stale pair match)

#### test_dependency_on_deprecated_rule_detect_is_synchronous
- Note: this is a compile-time property. `detect()` must not be async.
- If implementation uses async accidentally, compilation fails for the trait impl.
- Verify the trait impl does not use `.await` inside `detect()`.

---

## Integration Test Expectations

### Location: infra-001 `test_lifecycle.py`

#### test_cycle_review_finds_dependency_on_deprecated (R-10, AC-12)
- Arrange:
  1. Start a feature cycle via `context_cycle`
  2. Store entry A (within the cycle's scope)
  3. Store entry B (will be deprecated)
  4. Add a Prerequisite edge: A → B
  5. Deprecate B via `context_correct` (now B_deprecated exists; original B has status=Deprecated)
- Act: call `context_cycle_review` for the current cycle
- Assert: response contains a finding with:
  - `rule_name == "dependency_on_deprecated"` (or equivalent key)
  - `severity == "Warning"`
  - Content referencing the affected entry IDs

#### test_cycle_review_no_false_positive_without_stale_edges
- Arrange: Start cycle; store entries; add Prerequisite edge between two Active entries
- Act: call `context_cycle_review`
- Assert: no `dependency_on_deprecated` finding in the response

---

## Breaking-Change Callers Audit (R-10 Coverage Requirement)

Before Stage 3b, Stage 3a establishes the known caller list that must be updated:

| Caller | File | Update Required |
|--------|------|-----------------|
| `context_cycle_review` handler | `tools.rs` | Pass pre-queried stale_edges vec |
| Any unit test calling `default_rules(...)` | `detection/mod.rs` test module | Add `vec![]` as second arg |
| Any integration test invoking `default_rules` directly | test files | Add `vec![]` as second arg |

Stage 3b implementors must grep for all `default_rules(` call sites and update each one.
A CI compile failure after partial update will block the build — this is the desired behavior.

After Stage 3b, Stage 3c must verify:
1. `cargo build --workspace` passes with zero compile errors
2. No `#[allow(unused_variables)]` added to suppress stale_edges warnings
3. Count test asserts 23 (not 22)
