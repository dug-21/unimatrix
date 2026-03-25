# Component 5: compile_cycles Recommendation Fix

**File**: `crates/unimatrix-observe/src/report.rs`
**Action**: Modify — replace `compile_cycles` recommendation text at two sites (lines 62 and 88).
            Update one test assertion.

---

## Purpose

The current `compile_cycles` recommendation incorrectly tells agents to add commands to
an allowlist. `compile_cycles` counts how many times the Rust compiler was invoked, not how
many permission prompts occurred. High counts come from iterative per-field struct changes,
not permission friction. AC-19 / ADR-005 require correct framing.

---

## Changes to `recommendation_for` function

### Site 1: `action` field for `compile_cycles` match arm (line ~85)

```
// OLD:
"compile_cycles" if hotspot.measured > 10.0 => Some(Recommendation {
    hotspot_type: "compile_cycles".into(),
    action: "Consider incremental compilation or targeted cargo test invocations".into(),
    rationale: format!(
        "{:.0} compile cycles detected (threshold: 10) -- consider narrowing test scope",
        hotspot.measured
    ),
})

// NEW:
"compile_cycles" if hotspot.measured > 10.0 => Some(Recommendation {
    hotspot_type: "compile_cycles".into(),
    action: "Batch field additions before compiling — high compile cycle counts typically \
             indicate iterative per-field struct changes or cascading type errors; \
             complete type definitions and resolve compiler errors in-memory before each build"
             .into(),
    rationale: format!(
        "{:.0} compile cycles detected — each compile-check-fix loop adds 2–6 compile events; \
         batch changes to logical units (complete struct definitions, full impl blocks) \
         before building to reduce total compile count",
        hotspot.measured
    ),
})
```

NOTE on lines: The IMPLEMENTATION-BRIEF cites "lines 62 and 88" as the two sites. The
current code shows the match arm at approximately line 84-92 of report.rs. The "line 62"
reference may be the `action` string and "line 88" the `rationale` string, or there may
be a second occurrence outside the match arm. Implementation agent must verify both sites:

1. The `action: "Add common build/test commands to settings.json allowlist"` string.
2. The rationale that contains `"(threshold: 10) -- consider narrowing test scope"`.
   This second string also satisfies AC-13 (threshold language removal).

Both must be replaced.

### `permission_retries` match arm (line ~60) — CONFIRM UNCHANGED

The existing `permission_retries` recommendation:
```
action: "Add common build/test commands to settings.json allowlist".into(),
```

This is CORRECT for `permission_retries` (AC-19 explicitly allows "allowlist" here only).
Do NOT modify this arm. The "allowlist" text must remain in permission_retries and be absent
from compile_cycles.

---

## Algorithm: No new logic

This component is a pure text replacement. No algorithmic change. The `compile_cycles`
detection rule and threshold constant (`COMPILE_CYCLES_THRESHOLD`) are unchanged.

---

## Changes to `build_report` function

`build_report()` constructs `RetrospectiveReport` directly. When Component 1 adds five new
fields to the struct, `build_report()` must add them too. See Component 1 pseudocode for the
exact additions. This is the shared construction site in `report.rs`.

The recommendation-fix component is responsible for:
1. The text replacements above
2. Adding the new fields to the `RetrospectiveReport` literal in `build_report()` as a
   compile-time migration step (since this file also constructs the struct)

```
// In build_report(), add to the RetrospectiveReport { ... } literal:
goal: None,
cycle_type: None,
attribution_path: None,
is_in_progress: None,
phase_stats: None,
```

---

## Test Updates

### `test_recommendation_compile_cycles_above_threshold`

Current assertion (line ~391 in report.rs):
```
assert!(recs[0].action.contains("incremental"));
```

Replace with:
```
assert!(!recs[0].action.contains("allowlist"),
    "compile_cycles recommendation must not contain allowlist");
assert!(recs[0].action.contains("Batch field additions") || recs[0].action.contains("batch"),
    "compile_cycles recommendation should describe batching approach");
assert!(!recs[0].rationale.contains("threshold"),
    "compile_cycles rationale must not contain threshold language");
```

### `test_recommendation_permission_retries` — CONFIRM STILL PASSES

Current assertion:
```
assert!(recs[0].action.contains("allowlist"));
```

This must still pass after the change (permission_retries is unchanged).

---

## New tests to add

**T-CC-01** (AC-19): compile_cycles action does not contain "allowlist"
- Same as updated `test_recommendation_compile_cycles_above_threshold` above

**T-CC-02** (AC-19): permission_retries action still contains "allowlist"
- `rule_name = "permission_retries"` → assert `action.contains("allowlist")`
- Confirm the two templates share no text (assert compile_cycles action != permission_retries action)

**T-CC-03** (AC-13): compile_cycles rationale does not contain "threshold"
- Compile_cycles hotspot → recommendation → assert rationale does not contain "threshold: N"
- (The ADR-005 rationale no longer references "(threshold: 10)")

**T-CC-04**: compile_cycles below 10.0 produces no recommendation
- `measured = 5.0` → `recommendations_for_hotspots` returns empty vec
- This test already exists (`test_recommendation_compile_cycles_below_threshold`) — must still pass

---

## Error Handling

No error handling — pure text replacement in a synchronous, infallible function.

---

## Key Constraints

- "allowlist" must NOT appear in any `compile_cycles` action or rationale text (AC-19)
- "allowlist" MUST appear in `permission_retries` action text (AC-19 explicitly confirms this)
- The two recommendation templates must be confirmed textually independent (no shared sentences)
- Detection logic in `detection/agent.rs` is NOT modified (ADR-004 and ADR-005)
- `COMPILE_CYCLES_THRESHOLD` constant (10.0) is NOT changed
