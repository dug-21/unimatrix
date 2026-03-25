# Component 2: PhaseStats Type + Computation (Handler Steps 10h and 10i)

**File**: `crates/unimatrix-server/src/mcp/tools.rs`
**Action**: Modify — insert steps 10h and 10i into the `context_cycle_review` handler,
            immediately after the existing step 10g (phase narrative assembly).
            Add `compute_phase_stats` as a module-level `fn`.

---

## Purpose

Compute per-phase aggregate statistics by slicing the already-loaded observation records
into phase time windows derived from `cycle_events`. Derive `is_in_progress`, `goal`, and
`cycle_type` fields for the report header. Record the attribution path that was used.

---

## Dependency on ADR-002

`cycle_ts_to_obs_millis(ts_secs: i64) -> i64` is the ONLY permitted conversion from
cycle_events seconds to observation milliseconds. It lives in
`crates/unimatrix-server/src/services/observation.rs` (currently module-private).

If `compute_phase_stats` remains in `tools.rs`: no visibility change needed (same file).
If `compute_phase_stats` is extracted to a separate module: make `cycle_ts_to_obs_millis`
`pub(crate)` in `observation.rs`. The extraction is not mandated by the architecture —
keeping it in `tools.rs` is simpler and avoids the visibility change.

This pseudocode keeps `compute_phase_stats` in `tools.rs`.

---

## Handler Integration Point

In the existing handler, after the block ending at line ~1717 (`report.phase_narrative = Some(narrative)`),
before step 11 (audit), insert:

```
// Step 10h: PhaseStats computation (best-effort)
match (|| async {
    let phase_stats = compute_phase_stats(&events, &attributed);
    if phase_stats.is_empty() {
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(None)
    } else {
        Ok(Some(phase_stats))
    }
})().await {
    Ok(result) => report.phase_stats = result,
    Err(e) => {
        tracing::warn!("col-026: phase_stats computation failed: {e}");
        report.phase_stats = None;
    }
}

// Step 10i: goal, cycle_type, is_in_progress, attribution_path (best-effort)
match (|| async {
    let goal = store.get_cycle_start_goal(&feature_cycle).await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(goal)
})().await {
    Ok(goal_opt) => {
        let cycle_type = infer_cycle_type(goal_opt.as_deref());
        report.goal = goal_opt;
        report.cycle_type = Some(cycle_type);
    }
    Err(e) => {
        tracing::warn!("col-026: get_cycle_start_goal failed: {e}");
        // report.goal remains None, report.cycle_type remains None
    }
}

// is_in_progress: derived from events (already loaded, no DB call)
// events is always available here — it is the Vec loaded in step 10g
// but NOTE: events is only loaded if event_rows is Ok and non-empty (line ~1607)
// For pre-col-024 cycles where event_rows was empty, events is NOT in scope.
// Handle both cases: see "is_in_progress derivation" section below.
```

### is_in_progress Derivation

The `events` Vec is only constructed inside the `if !event_rows.is_empty()` block (line ~1608).
To derive `is_in_progress` correctly:

```
// After the if let Ok(event_rows) = event_rows block:
// We need to know whether events was empty or what events contained.
// Approach: hoist the events result to an outer Option<Vec<CycleEventRecord>>.

// Before the existing cycle_events block, declare:
let mut cycle_events_opt: Option<Vec<CycleEventRecord>> = None;

// Inside the existing if !event_rows.is_empty() block, after building events:
cycle_events_opt = Some(events.clone());   // or restructure to avoid clone
// NOTE: if build_phase_narrative takes ownership of events, this is the moment
// to produce the clone. However: build_phase_narrative currently takes &events
// (borrow). Verify this before implementing — if it takes ownership, restructure
// so both 10g and 10h receive borrows from the same owned Vec.

// Then derive is_in_progress from cycle_events_opt:
report.is_in_progress = derive_is_in_progress(cycle_events_opt.as_deref());
```

Alternative: restructure the existing block so `events` is declared in the outer scope.
This avoids the clone. The pseudocode uses the restructured approach for clarity.

### `derive_is_in_progress` function

```
fn derive_is_in_progress(events: Option<&[CycleEventRecord]>) -> Option<bool>
    // ADR-001: three states, no bool
    match events {
        None => None,                       // no cycle_events rows loaded at all
        Some(evts) if evts.is_empty() => None,  // loaded but empty
        Some(evts) =>
            if evts.iter().any(|e| e.event_type == "cycle_stop") {
                Some(false)                 // confirmed complete
            } else {
                Some(true)                  // has cycle_start, no cycle_stop
            }
    }
```

### Attribution path recording

The `attribution_path` label is determined during step 3 (the three-path fallback). Currently
step 3 in the handler sets `attributed` but does not record which path was used. Add a
`let mut attribution_path_label: Option<&'static str> = None;` before step 3, and set it
at the successful path branch:

```
// Path 1: load_cycle_observations returns non-empty
attribution_path_label = Some("cycle_events-first (primary)");

// Path 2: load_feature_observations returns non-empty
attribution_path_label = Some("sessions.feature_cycle (legacy)");

// Path 3: load_unattributed_sessions used
attribution_path_label = Some("content-scan (fallback)");
```

In step 10i, assign:
```
report.attribution_path = attribution_path_label.map(|s| s.to_string());
```

---

## `compute_phase_stats` Algorithm

```
fn compute_phase_stats(
    events: &[CycleEventRecord],
    attributed: &[ObservationRecord],
) -> Vec<PhaseStats>
```

Events slice is borrowed; attributed slice is borrowed.
Both borrows are concurrent — `build_phase_narrative` also borrows `events` before this.

### Phase 1: Extract time windows

```
windows: Vec<PhaseWindow> = []
// PhaseWindow is a local struct: { phase: String, pass_number: u32, start_ms: i64, end_ms: Option<i64>, end_event_outcome: Option<String> }

// Walk events in order (already sorted by timestamp ASC, seq ASC from the SQL query)
let mut current_phase: Option<String> = None
let mut window_start_ms: Option<i64> = None
let mut pass_counters: HashMap<String, u32> = {}  // phase_name → how many passes seen so far

for event in events:
    match event.event_type.as_str():
        "cycle_start" =>
            // Absolute start of the first window
            window_start_ms = Some(cycle_ts_to_obs_millis(event.timestamp))
            // phase from cycle_start may be None; leave current_phase = None until
            // the first cycle_phase_end tells us what phase just ended

        "cycle_phase_end" =>
            // This event ends the current phase window and transitions to next_phase.
            // event.phase = the phase that just ended
            let ending_phase = event.phase.clone().unwrap_or_else(|| String::new())
            let end_ms = cycle_ts_to_obs_millis(event.timestamp)

            if let Some(start_ms) = window_start_ms:
                let pass_number = {
                    let counter = pass_counters.entry(ending_phase.clone()).or_insert(0);
                    *counter += 1;
                    *counter
                }
                windows.push(PhaseWindow {
                    phase: ending_phase.clone(),
                    pass_number,
                    start_ms,
                    end_ms: Some(end_ms),
                    end_event_outcome: event.outcome.clone(),
                })

            // Next window starts at this event's timestamp
            window_start_ms = Some(end_ms)
            current_phase = event.next_phase.clone()

        "cycle_stop" =>
            // Ends the last open window (if any)
            let end_ms = cycle_ts_to_obs_millis(event.timestamp)
            if let Some(start_ms) = window_start_ms:
                // The phase for this final window: current_phase or empty
                let last_phase = current_phase.clone().unwrap_or_else(|| String::new())
                let pass_number = {
                    let counter = pass_counters.entry(last_phase.clone()).or_insert(0);
                    *counter += 1;
                    *counter
                }
                // cycle_stop has no outcome text relevant to a gate
                windows.push(PhaseWindow {
                    phase: last_phase,
                    pass_number,
                    start_ms,
                    end_ms: Some(end_ms),
                    end_event_outcome: None,
                })
            window_start_ms = None

        _ =>
            // Unknown event type — ignore
```

Edge case: if `events` contains no `cycle_stop`, the last window has `end_ms = None`.
For observation slicing, treat open windows as `end_ms = i64::MAX` (all remaining
observations fall in the last window).

After walking all events, compute `pass_count` for each window by looking up
the final counter value from `pass_counters`:

```
for window in windows.iter_mut():
    window.pass_count = pass_counters.get(&window.phase).copied().unwrap_or(1)
```

### Phase 2: Slice observations into windows

For each window in windows:

```
let window_end = window.end_ms.unwrap_or(i64::MAX)
let filtered: Vec<&ObservationRecord> = attributed.iter()
    .filter(|obs| {
        let ts = obs.ts as i64;
        ts >= window.start_ms && ts < window_end
    })
    .collect()
```

Note: `obs.ts` is `u64` epoch millis; cast to `i64` for comparison.
`window.start_ms` and `window_end` are `i64` epoch millis from `cycle_ts_to_obs_millis`.
If `obs.ts` > `i64::MAX as u64`, the cast saturates to `i64::MAX` — still correct behavior.

### Phase 3: Compute per-window aggregates

```
record_count = filtered.len()

// Distinct sessions
session_ids: HashSet<&str> = filtered.iter().map(|obs| obs.session_id.as_str()).collect()
session_count = session_ids.len()

// Agents: SubagentStart observations, deduplicated in first-seen order
agents: Vec<String> = []
seen_agents: HashSet<String> = {}
for obs in filtered.iter().filter(|o| o.event_type == "SubagentStart"):
    // agent name from obs.input["tool_name"] or obs.tool
    let agent_name = extract_agent_name(obs)
    if let Some(name) = agent_name:
        if seen_agents.insert(name.clone()):
            agents.push(name)

// Tool distribution: bucket by obs.event_type using same mapping as compute_session_summaries
tool_distribution = ToolDistribution { read:0, execute:0, write:0, search:0 }
for obs in &filtered:
    match categorize_tool(obs.tool.as_deref(), &obs.event_type):
        "read"    => tool_distribution.read += 1
        "execute" => tool_distribution.execute += 1
        "write"   => tool_distribution.write += 1
        "search"  => tool_distribution.search += 1
        _         => ()   // other/spawn/store not counted

// Knowledge served: PreToolUse where tool is context_search / context_lookup / context_get
knowledge_served = filtered.iter()
    .filter(|o| o.event_type == "PreToolUse")
    .filter(|o| matches!(
        o.tool.as_deref(),
        Some("context_search") | Some("context_lookup") | Some("context_get")
    ))
    .count() as u64

// Knowledge stored: PreToolUse where tool is context_store
knowledge_stored = filtered.iter()
    .filter(|o| o.event_type == "PreToolUse")
    .filter(|o| o.tool.as_deref() == Some("context_store"))
    .count() as u64

// GateResult from window's end_event_outcome
gate_result = infer_gate_result(window.end_event_outcome.as_deref(), window.pass_count)
gate_outcome_text = window.end_event_outcome.clone()
```

### `extract_agent_name` helper

```
fn extract_agent_name(obs: &ObservationRecord) -> Option<String>
    // For SubagentStart events: agent name is in obs.input["tool_name"]
    // or obs.tool field
    if let Some(input) = &obs.input:
        if let Some(name) = input.get("tool_name").and_then(|v| v.as_str()):
            return Some(name.to_string())
    obs.tool.clone()
```

### `categorize_tool` helper

Map tool names to buckets. Matches the logic used in `compute_session_summaries` (check that
function in tools.rs for the authoritative mapping; replicate it here for consistency).

```
fn categorize_tool(tool: Option<&str>, event_type: &str) -> &'static str
    match tool:
        Some("Read") | Some("Glob") => "read"
        Some("Write") | Some("Edit") => "write"
        Some("Bash") => "execute"
        Some("Grep") => "search"
        _ if event_type == "SubagentStart" => "spawn"   // not counted in ToolDistribution
        _ => "other"
```

### `infer_gate_result` function

```
fn infer_gate_result(outcome: Option<&str>, pass_count: u32) -> GateResult
    let outcome_lower = match outcome:
        None => return GateResult::Unknown
        Some(s) if s.is_empty() => return GateResult::Unknown
        Some(s) => s.to_lowercase()

    // Priority order: Rework (multi-pass success) > Fail > Pass > Unknown
    // Check rework FIRST per R-03 (ADR note: multi-keyword "pass after rework")
    if pass_count > 1 && outcome_lower.contains("pass")
        || pass_count > 1 && outcome_lower.contains("success")
        || pass_count > 1 && outcome_lower.contains("approved"):
        return GateResult::Rework

    if outcome_lower.contains("fail") || outcome_lower.contains("error"):
        return GateResult::Fail

    if outcome_lower.contains("pass") || outcome_lower.contains("success")
        || outcome_lower.contains("approved"):
        return GateResult::Pass

    GateResult::Unknown
```

NOTE on R-03 "compass" edge case: `contains("pass")` matches "compass". This is a known
fragility documented in RISK-TEST-STRATEGY.md R-03 scenario 8. The spec uses `contains()`
matching. The implementation agent may add word-boundary matching (e.g., split on whitespace
and check word list) if they consider it low-risk. The pseudocode uses `contains()` per spec.

### Phase 4: Assemble PhaseStats

```
duration_secs = (window.end_ms.unwrap_or(window.start_ms) - window.start_ms)
    .max(0) as u64 / 1000

result.push(PhaseStats {
    phase: window.phase.clone(),
    pass_number: window.pass_number,
    pass_count: window.pass_count,
    duration_secs,
    session_count,
    record_count,
    agents,
    tool_distribution,
    knowledge_served,
    knowledge_stored,
    gate_result,
    gate_outcome_text,
    hotspot_ids: vec![],   // populated by formatter only
})
```

Return `result` (the collected Vec<PhaseStats>).

### Empty and single-event edge cases

- If `events.is_empty()`: return `vec![]` immediately (caller converts to `None`).
- If only `cycle_start` present (no `cycle_phase_end`, no `cycle_stop`): one open window from
  `cycle_start.timestamp` to `i64::MAX`. One `PhaseStats` entry with phase = "" (or the
  phase name from cycle_start's `next_phase` if present).
- Zero-duration window (`start_ms == end_ms`): `duration_secs = 0`. No panic or division.
- Empty phase name in `cycle_phase_end.phase`: use empty string. Formatter renders `—`.

---

## `infer_cycle_type` function

```
fn infer_cycle_type(goal: Option<&str>) -> String
    let goal_lower = match goal:
        None => return "Unknown".to_string()
        Some(s) if s.is_empty() => return "Unknown".to_string()
        Some(s) => s.to_lowercase()

    // Check in priority order (first match wins per FR-03)
    if goal_lower.contains("design") || goal_lower.contains("research")
        || goal_lower.contains("scope") || goal_lower.contains("spec"):
        return "Design".to_string()

    if goal_lower.contains("implement") || goal_lower.contains("deliver")
        || goal_lower.contains("build"):
        return "Delivery".to_string()

    if goal_lower.contains("fix") || goal_lower.contains("bug")
        || goal_lower.contains("regression") || goal_lower.contains("hotfix"):
        return "Bugfix".to_string()

    if goal_lower.contains("refactor") || goal_lower.contains("cleanup")
        || goal_lower.contains("simplify"):
        return "Refactor".to_string()

    "Unknown".to_string()
```

---

## Error Handling

Both steps 10h and 10i use the same error boundary pattern as steps 11-17:

```
match (|| async { ... })().await {
    Ok(result) => report.field = result,
    Err(e) => {
        tracing::warn!("col-026: {step}: {e}");
        report.field = None;   // leave unset; handler continues normally
    }
}
```

`compute_phase_stats` is a pure `fn` (no DB, no async). It cannot fail at the Rust type
level. Wrap it in the async block anyway so panics are caught at the boundary. Return type
of the block: `Result<Option<Vec<PhaseStats>>, Box<dyn std::error::Error + Send + Sync>>`.

`get_cycle_start_goal` is `async` and can fail. It follows the same `match (|| async { ... })().await`
pattern.

`derive_is_in_progress` is a pure `fn`. It runs inline (no error boundary needed).

---

## Key Test Scenarios

**T-PS-01** (R-01): Phase boundary uses `cycle_ts_to_obs_millis`
- Events: `cycle_start` at `ts=1700000000`, `cycle_phase_end` at `ts=1700000100`
- Observation at `ts=1700000100 * 1000 = 1700000100000 ms` (exactly the boundary)
- Assert observation is in the NEXT window (boundary is exclusive on the end side)
- Assert observation at `ts=1700000099999` is in the previous window

**T-PS-02** (R-02): Only cycle_start + cycle_stop, no cycle_phase_end
- One PhaseStats entry spanning full duration, phase = "" or derived from cycle_start.next_phase

**T-PS-03** (R-02): Zero-duration window
- cycle_start.timestamp == cycle_stop.timestamp → duration_secs = 0, no panic

**T-PS-04** (R-02): Empty phase name on cycle_phase_end
- phase = "" → PhaseStats.phase = "", formatter must handle

**T-PS-05** (R-02): No observations in a window
- record_count = 0, knowledge_served = 0, gate_result = Unknown (no panic)

**T-PS-06** (R-03): GateResult inference cases
- "PASS" → Pass; "failed: type errors" → Fail; "REWORK" → Rework
- "compass" → check: contains "pass" substring → Pass (known fragility per R-03 scenario 8)
- "" → Unknown; None → Unknown
- "pass after rework" with pass_count=2 → Rework (rework check fires before pass check)

**T-PS-07** (R-05): is_in_progress three states
- events = [] → None
- events has cycle_start, no cycle_stop → Some(true)
- events has cycle_start + cycle_stop → Some(false)

**T-PS-08** (R-12): Empty result canonicalization
- events = [] → compute_phase_stats returns vec![] → handler sets report.phase_stats = None
- Assert JSON does not contain "phase_stats" key

**T-PS-09**: `infer_cycle_type` keyword matching
- "implement new store layer" → "Delivery"
- "design the embedding pipeline" → "Design"
- None → "Unknown"

**T-PS-10**: Multi-pass phase (rework)
- events: scope → cycle_phase_end(scope, fail) → scope → cycle_phase_end(scope, pass)
- Assert PhaseStats has two entries: both with phase="scope", pass_number=1 and 2, pass_count=2 each

**T-PS-11** (static lint): No `* 1000` in phase_stats computation code
- Grep `compute_phase_stats` and the step 10h block for `* 1000` — assert zero matches
