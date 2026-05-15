# Component 6: DependencyOnDeprecated Detection Rule

## Purpose

Add `DependencyOnDeprecatedRule` as the 23rd `DetectionRule` implementation. This rule fires a
`Warning`-severity `HotspotFinding` when any Prerequisite edge in the current cycle's entries
points to a source entry with `status = Deprecated`.

Because `DetectionRule.detect()` is synchronous and must not perform blocking I/O, stale edge
data is injected at construction time via the constructor injection pattern (ADR-004), following
the `PhaseDurationOutlierRule` precedent.

## Files Modified

- `crates/unimatrix-observe/src/detection/scope.rs` — add `DependencyOnDeprecatedRule`
- `crates/unimatrix-observe/src/detection/mod.rs` — register rule; update `default_rules()` signature

## detection/scope.rs — New Struct and Implementation

```
// File: crates/unimatrix-observe/src/detection/scope.rs
// DependencyOnDeprecatedRule joins the existing scope-category rules (PhaseDurationOutlierRule)

pub(crate) struct DependencyOnDeprecatedRule {
    stale_edge_pairs: Vec<(u64, u64)>,
    // (source_id, target_id) pairs representing Prerequisite edges
    // where source_id's entry has status = Deprecated
    // Pre-queried by context_cycle_review handler before default_rules() is called
    // Empty vec → detect() returns empty findings (no false positives)
}

impl DependencyOnDeprecatedRule {
    pub fn new(stale_edge_pairs: Vec<(u64, u64)>) -> Self {
        // Constructor injection — no I/O, no async, pure struct initialization
        DependencyOnDeprecatedRule { stale_edge_pairs }
    }
}

impl DetectionRule for DependencyOnDeprecatedRule {

    FUNCTION name(&self) -> &'static str
        RETURN "dependency_on_deprecated"
    END FUNCTION

    FUNCTION category(&self) -> &'static str
        RETURN "scope"
        // Joins the scope category alongside PhaseDurationOutlierRule
        // "scope" aligns with the category of Prerequisite-on-deprecated being a scope health signal
    END FUNCTION

    FUNCTION detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>
        // INVARIANT: Must be synchronous. No await, no I/O, no blocking calls.
        // All data needed was injected at construction time.

        IF self.stale_edge_pairs is empty THEN
            RETURN Vec::new()
            // Fast path: no stale edges → no findings; prevents false positives
        END IF

        LET findings: Vec<HotspotFinding> = empty

        // Cross-reference stale pairs against current cycle's observation records
        // Goal: produce one finding per stale pair (source_id, target_id)
        FOR EACH (source_id, target_id) IN self.stale_edge_pairs
            // Attempt to find the source entry in records for a descriptive message
            // If not found in records, still emit finding with IDs only
            LET source_label = records
                .iter()
                .find(|r| r.entry_id == source_id)
                .map(|r| r.title.as_deref().unwrap_or("(untitled)"))
                .unwrap_or("(unknown)")

            findings.push(HotspotFinding {
                rule:     "dependency_on_deprecated",
                severity: Severity::Warning,
                message:  format!(
                    "Entry {} ('{}') has a Prerequisite edge to entry {} which is Deprecated. \
                     Consider updating the dependency to the successor entry or removing the stale edge.",
                    source_id, source_label, target_id
                ),
                entry_id: Some(source_id),
                // entry_id: source of the stale edge (the deprecated entry)
            })
        END FOR

        RETURN findings
    END FUNCTION
}
```

Note on detect() design: The `stale_edge_pairs` are already the result of a pre-queried,
cycle-scoped DB query (from `query_stale_prerequisite_edges_for_cycle`). The detect() method
does NOT re-query the store — it uses the pre-loaded data only. The `records` parameter
is used only to enrich the message with entry titles. The core finding is driven by
`stale_edge_pairs` alone.

## detection/mod.rs — Signature Change and Rule Registration

### default_rules() signature change (BREAKING API CHANGE — R-10)

```
// BEFORE:
pub fn default_rules(
    history: Option<&[MetricVector]>,
) -> Vec<Box<dyn DetectionRule>>

// AFTER:
pub fn default_rules(
    history:     Option<&[MetricVector]>,
    stale_edges: Vec<(u64, u64)>,
) -> Vec<Box<dyn DetectionRule>>
```

All callers must be updated atomically:
1. `context_cycle_review` handler in `tools.rs` — must pass pre-queried stale pairs
2. All test functions in `detection/mod.rs` that call `default_rules()` — pass `vec![]` for
   the new parameter (no stale edges in unit test context; DependencyOnDeprecated fires nothing)
3. Any other call sites (audit with grep before implementation)

### Rule registration in default_rules()

```
FUNCTION default_rules(
    history:     Option<&[MetricVector]>,
    stale_edges: Vec<(u64, u64)>,
) -> Vec<Box<dyn DetectionRule>>

    LET mut rules: Vec<Box<dyn DetectionRule>> = vec![
        // ── Existing 22 rules (unchanged, in existing order) ─────────────────────
        Box::new(ExistingRule1::new(...)),
        // ... (22 existing rules) ...
        Box::new(PhaseDurationOutlierRule::new(history)),  // existing constructor-injected rule

        // ── Rule 23 (new — vnc-015) ────────────────────────────────────────────
        Box::new(DependencyOnDeprecatedRule::new(stale_edges)),
    ]

    RETURN rules
END FUNCTION
```

### Test update: test_default_rules_has_22_rules → 23

```
// BEFORE:
#[test]
fn test_default_rules_has_22_rules() {
    let rules = default_rules(None);
    assert_eq!(rules.len(), 22);
}

// AFTER:
#[test]
fn test_default_rules_has_23_rules() {
    // Note: function rename optional but recommended
    let rules = default_rules(None, vec![]);
    assert_eq!(rules.len(), 23);
}
```

## context_cycle_review Handler Integration (tools.rs)

```
ASYNC FUNCTION context_cycle_review_handler(params: CycleReviewParams, ...) -> MCP response
    // ... existing steps ...

    // [NEW] Pre-query stale Prerequisite edges for this cycle
    LET stale_edge_pairs: Vec<(u64, u64)> =
        query_stale_prerequisite_edges_for_cycle(store, &params.feature_cycle).await
            .unwrap_or_else(|e| {
                // On DB error: log and continue with empty pairs
                // DependencyOnDeprecated simply fires no findings; does not block review
                log::warn!("stale_dependency_edges query failed: {}; rule will not fire", e);
                vec![]
            })

    // [MODIFIED] Pass stale pairs to default_rules (signature change)
    LET rules = default_rules(history_slice, stale_edge_pairs);

    // ... existing detect_hotspots call (unchanged) ...
    LET findings = detect_hotspots(attributed, &rules);
    ...
END FUNCTION
```

## Error Handling

| Error | Handling |
|-------|---------|
| `detect()` panics | Must not happen; detect() is pure computation on pre-loaded data |
| `stale_edge_pairs` is empty | Fast path — returns empty vec; no finding emitted |
| DB query for stale pairs fails | Handler logs + continues with empty vec; rule fires nothing |
| Caller passes `vec![]` to `default_rules()` | Rule initialized with empty data; detect() returns empty (correct behavior in unit tests) |

## Key Test Scenarios

1. `DependencyOnDeprecatedRule::new(vec![]).detect(&[])` → empty findings (no false positive, R-14-style check)
2. `DependencyOnDeprecatedRule::new(vec![(1, 2)]).detect(&[])` → one Warning finding for (1,2)
3. `DependencyOnDeprecatedRule::new(vec![(1, 2)]).detect(records)` where records contains entry 1
   → finding message includes entry 1's title
4. `default_rules(None, vec![]).len() == 23` (updated count — AC-13)
5. `default_rules(None, vec![(1, 2)]).len() == 23` (rule exists with non-empty stale data)
6. Integration test: write Prerequisite edge; deprecate source; call context_cycle_review;
   assert finding with rule="dependency_on_deprecated" and severity=Warning (AC-12)
7. Integration test: write Prerequisite edge; do NOT deprecate source; call context_cycle_review;
   assert NO dependency_on_deprecated finding (positive baseline)
8. Compile gate: after signature change, `cargo check` must succeed with no caller errors (R-10)
9. `detect()` called with multiple stale pairs → one finding per pair (not batched into one)
