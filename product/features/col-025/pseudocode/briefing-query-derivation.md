# Component: briefing-query-derivation

**Crate**: `unimatrix-server`
**File**: `src/services/index_briefing.rs`
         (fn `derive_briefing_query`, fn `synthesize_from_session`)

---

## Purpose

Replace `synthesize_from_session` to return `state.current_goal.clone()`
directly instead of synthesizing from `topic_signals`. Remove
`extract_top_topic_signals` (no longer needed). Extend the `make_session_state`
test helper to a 3-parameter signature `(feature, signals, current_goal)`.
Update all existing call sites to pass `None` as the third argument.
Update tests for step 2 semantics.

---

## Modified Function: `synthesize_from_session`

Current body (topic-signal synthesis — to be REMOVED):
```
fn synthesize_from_session(state: &SessionState) -> Option<String> {
    // Step 2: synthesize from session state when both feature_cycle and signals are present
    let feature_cycle = state.feature.as_deref().unwrap_or("");
    if !feature_cycle.is_empty() {
        let signals = extract_top_topic_signals(&state.topic_signals, 3);
        if !signals.is_empty() {
            return Some(format!("{} {}", feature_cycle, signals.join(" ")));
        }
    }
    None
}
```

New body (ADR-002 — pure O(1) clone of current_goal):
```
/// Return the step-2 briefing query from session state (col-025, ADR-002).
///
/// Returns state.current_goal.clone() — the feature goal set at cycle start
/// or reconstructed on session resume.
///
/// When None (no goal, legacy cycle, or pre-v16 cycle), derive_briefing_query
/// falls through to step 3 (topic-ID) unchanged.
///
/// Contract (NFR-04): pure sync, O(1), no I/O, no locks, no async.
/// Called on both the MCP context_briefing hot path and the UDS
/// handle_compact_payload path.
fn synthesize_from_session(state: &SessionState) -> Option<String> {
    state.current_goal.clone()
}
```

### `extract_top_topic_signals` function

`extract_top_topic_signals` is no longer called by `synthesize_from_session`.
If it has no other callers, it can be removed. Before removing:
- Grep for all callers of `extract_top_topic_signals` in the crate.
- If zero other callers: remove the function and its tests.
- If other callers exist: leave it; just stop calling it from `synthesize_from_session`.

Based on current code review: `extract_top_topic_signals` is only called from
`synthesize_from_session` (current body). It is safe to remove both the function
and its `extract_top_topic_signals_*` tests. However, removal is optional — if
keeping it is safer for the delivery, mark it `#[allow(dead_code)]` and leave
for a follow-up removal. Do not let dead code block delivery.

---

## Function `derive_briefing_query` — no signature change

The `derive_briefing_query` function itself is unchanged in signature. The
step-2 comment must be updated to reflect the new semantics:

Old step-2 comment:
```
// Step 2: synthesize from session state when both feature_cycle and signals are present
if let Some(state) = session_state {
    let feature_cycle = state.feature.as_deref().unwrap_or("");
    if !feature_cycle.is_empty() {
        let signals = extract_top_topic_signals(&state.topic_signals, 3);
        if !signals.is_empty() {
            return format!("{} {}", feature_cycle, signals.join(" "));
        }
    }
    ...
}
```

New step-2 comment + body:
```
// Step 2: goal from session state (col-025, ADR-002).
// Returns current_goal when Some — most semantically precise signal available.
// Falls through to step 3 when None (no goal, legacy cycle, or pre-v16 cycle).
if let Some(state) = session_state {
    if let Some(goal) = synthesize_from_session(state) {
        if !goal.trim().is_empty() {
            return goal;
        }
        // Empty-goal guard: if current_goal is Some("") (edge case),
        // fall through to step 3. Normal path: goal is already non-empty
        // (normalized at MCP handler; UDS filters empty strings in Step 3b).
    }
}
```

Full updated `derive_briefing_query`:
```
pub(crate) fn derive_briefing_query(
    task: Option<&str>,
    session_state: Option<&SessionState>,
    topic: &str,
) -> String {
    // Step 1: explicit task overrides everything
    if let Some(t) = task {
        if !t.trim().is_empty() {
            return t.to_string();
        }
    }

    // Step 2: goal from session state (col-025, ADR-002)
    if let Some(state) = session_state {
        if let Some(goal) = synthesize_from_session(state) {
            if !goal.trim().is_empty() {
                return goal;
            }
        }
    }

    // Step 3: topic fallback (always available)
    topic.to_string()
}
```

---

## Test Helper: `make_session_state`

The `make_session_state` helper in the test module constructs `SessionState`
with a struct literal. It must be updated to include `current_goal: None`
and should accept an optional goal parameter for test cases that need it.

Current helper signature:
```
fn make_session_state(feature: Option<&str>, signals: Vec<(&str, u32)>) -> SessionState
```

Updated helper — extend to 3 parameters (col-025, matching session-state-extension
test plan). All existing call sites gain a trailing `None` argument:
```
fn make_session_state(
    feature: Option<&str>,
    signals: Vec<(&str, u32)>,
    current_goal: Option<&str>,   // NEW — col-025
) -> SessionState {
    // ... existing signal construction ...
    SessionState {
        // ... existing fields ...
        current_phase: None,
        category_counts: HashMap::new(),
        current_goal: current_goal.map(str::to_string),   // ADD: col-025
    }
}
```

All existing call sites within the test module that previously called
`make_session_state(feature, signals)` must be updated to
`make_session_state(feature, signals, None)`.

There is NO separate `make_session_state_with_goal` variant — tests that need
a goal simply pass `Some("...")` as the third argument to `make_session_state`.

---

## Existing Tests That Must Be Updated (R-05)

The following tests assert the OLD step-2 behavior (topic-signal synthesis).
They must be updated or replaced:

1. `derive_briefing_query_session_signals_step_2` — asserts
   `result == "crt-027/spec briefing hook compaction"`. This format no longer
   exists. Update to: when `current_goal = None` and signals present, result
   == topic (step 3), because step 2 now returns `None`.

2. `derive_briefing_query_fewer_than_three_signals` — asserts
   `result == "crt-027/spec briefing"`. Update similarly.

3. `derive_briefing_query_empty_task_falls_through` — the assertion
   `result.contains("crt-027")` is still valid (step 3), but the comment
   "Should NOT return "" — falls to step 2" must be updated to reflect that
   step 2 now uses `current_goal` (None in this test), so it falls to step 3.

The following tests remain VALID (no change needed):
- `derive_briefing_query_task_param_takes_priority` — step 1 unchanged
- `derive_briefing_query_whitespace_task_falls_through` — step 1 + step 3
- `derive_briefing_query_no_feature_cycle_falls_to_topic` — was testing
  step 2 requiring feature_cycle; now step 2 uses `current_goal`. With
  `current_goal = None`, result should still be topic (step 3). Valid.
- `derive_briefing_query_empty_signals_fallback_to_topic` — same reasoning
- `derive_briefing_query_no_session_fallback_to_topic` — step 3, unchanged

---

## Data Flow

Input to `derive_briefing_query`:
- `task: Option<&str>` — caller-provided explicit query
- `session_state: Option<&SessionState>` — current session state with
  `current_goal` populated or `None`
- `topic: &str` — always non-empty topic-ID fallback

Output: `String` — the best available query for `IndexBriefingService`

Step 2 specifically:
- `synthesize_from_session(state)` returns `state.current_goal.clone()`
- `None` → fall through to step 3 (topic-ID)
- `Some(g)` where `g.trim().is_empty()` → fall through to step 3 (edge case guard)
- `Some(g)` where `g` is non-empty → return `g` directly as the query

---

## Error Handling

`synthesize_from_session` is pure sync, O(1). It cannot fail. No error
handling needed within this component. The callers (`derive_briefing_query`,
`handle_compact_payload`, `context_briefing`) already handle errors at their
own level.

---

## Key Test Scenarios

### T-BQD-01: Step 2 returns goal when current_goal is Some (AC-04)
```
setup: state = make_session_state(Some("col-025"), vec![], Some("test goal"))
act:   derive_briefing_query(None, Some(&state), "col-025")
assert: result == "test goal"
```

### T-BQD-02: Step 1 wins over goal (AC-05)
```
setup: state = make_session_state(Some("col-025"), vec![], Some("goal"))
act:   derive_briefing_query(Some("explicit task"), Some(&state), "col-025")
assert: result == "explicit task"
```

### T-BQD-03: Step 3 fallback when current_goal is None (AC-06)
```
setup: state = make_session_state(Some("col-025"), vec![("signal", 5)], None)
       -- current_goal is None
act:   derive_briefing_query(None, Some(&state), "col-025")
assert: result == "col-025"  -- topic-ID fallback, NOT signal synthesis
```

### T-BQD-04: Step 3 fallback when no session state
```
act:   derive_briefing_query(None, None, "col-025")
assert: result == "col-025"
```

### T-BQD-05: Whitespace task falls through to goal (not step 3)
```
setup: state = make_session_state(None, vec![], Some("feature goal"))
act:   derive_briefing_query(Some("   "), Some(&state), "col-025")
assert: result == "feature goal"  -- step 2 wins over whitespace task
```

### T-BQD-06: Goal present, topic_signals also present — goal wins (R-05)
```
setup: state = make_session_state(
           Some("col-025"),
           vec![("briefing", 5), ("hook", 3), ("compaction", 2)],
           Some("feature goal")
       )
act:   derive_briefing_query(None, Some(&state), "col-025")
assert: result == "feature goal"  -- goal, NOT "col-025 briefing hook compaction"
```

### T-BQD-07: CompactPayload path benefits automatically (AC-07)
```
// No additional wiring needed — handle_compact_payload calls
// derive_briefing_query with the session state. AC-07 is satisfied
// by the shared function (ADR-002).
// Test: call handle_compact_payload with session where current_goal = Some("goal"),
//       assert the briefing query used is "goal".
```
