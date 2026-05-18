## ADR-004: DependencyOnDeprecated — Constructor Injection, Not Trait Extension

### Context

The `DetectionRule` trait is synchronous: `fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>`.
The `DependencyOnDeprecated` rule must fire when any Prerequisite edge in the current cycle's
entries points to a deprecated source. Detecting this requires knowledge of which GRAPH_EDGES
rows have `relation_type='Prerequisite'` and a deprecated source entry — store data that cannot
be derived from `ObservationRecord` slices alone.

Two design options were evaluated:
- Option A: Extend `DetectionRule` trait with an async initialization method or a store handle.
  This changes the trait interface for all 22 existing rules.
- Option B: Pre-query the stale edge data in the `context_cycle_review` handler before calling
  `default_rules()`, pass the data as a constructor parameter (constructor injection).

SR-05 in the scope risk assessment flags Option A as the risky choice: "if the constructor-injection
interface is not made generic, future rules with the same need will diverge into ad-hoc patterns."
The SCOPE.md Proposed Approach specifies: "constructor injection — matching the
`PhaseDurationOutlierRule` constructor injection pattern. No change to the `DetectionRule` trait."

`PhaseDurationOutlierRule` is the established precedent for this pattern: `default_rules()` accepts
`history: Option<&[MetricVector]>` and passes it to `PhaseDurationOutlierRule::new(history)`.
The trait's `detect()` method is called later with observation records only. Historical data is
pre-loaded and stored on the struct.

### Decision

Use constructor injection for `DependencyOnDeprecatedRule`. The `context_cycle_review` handler
pre-queries stale Prerequisite edge pairs (source_id, target_id) for the current cycle before
calling `default_rules()`. The stale pairs are passed as `Vec<(u64, u64)>` to `default_rules()`,
which passes them to `DependencyOnDeprecatedRule::new(stale_edge_pairs)`.

`default_rules()` signature change:
```rust
pub fn default_rules(
    history: Option<&[MetricVector]>,
    stale_edges: Vec<(u64, u64)>,
) -> Vec<Box<dyn DetectionRule>>
```

`DependencyOnDeprecatedRule` stores the `Vec<(u64, u64)>` at construction time. Its `detect()`
method cross-references the pre-loaded pairs against the observation records to produce findings.
The `DetectionRule` trait is NOT modified.

**On injection interface generality (SR-05)**: The `Vec<(u64, u64)>` type is purposefully
concrete rather than generic. Future rules with different injection needs will add their own
typed parameter to `default_rules()`. This avoids premature abstraction (a generic `RuleContext`
bag) while keeping the pattern explicit and auditable. Each injected data type is visible in the
function signature. If three or more rules require injection, revisiting a context struct becomes
justified.

### Consequences

Easier: `DetectionRule` trait unchanged — all 22 existing rules continue to compile and pass
without modification. The pattern matches `PhaseDurationOutlierRule` exactly. `detect()` remains
synchronous.

Harder: `default_rules()` signature change is a breaking API change. All callers must be updated.
Current callers: `context_cycle_review` handler (tools.rs) and test functions in
`detection/mod.rs`. The `test_default_rules_has_22_rules` test must be updated to assert 23
rules and pass the new `stale_edges` parameter.

Supersedes: none.
Related: ADR-005 (edge_write helper module).
