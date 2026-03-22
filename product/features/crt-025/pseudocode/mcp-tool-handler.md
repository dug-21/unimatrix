# Component 2: MCP Tool Handler
## File: `crates/unimatrix-server/src/mcp/tools.rs`

---

## Purpose

Updates the `CycleParams` wire schema (remove `keywords`, add `phase`/`outcome`/`next_phase`), updates the `context_cycle` handler to call the new `validate_cycle_params` signature, and enriches the `context_cycle_review` handler with three new SQL queries and phase narrative assembly via `build_phase_narrative`.

---

## Modified Struct: `CycleParams`

```
// BEFORE:
pub struct CycleParams {
    pub r#type:   String,
    pub topic:    String,
    pub keywords: Option<Vec<String>>,
    pub agent_id: Option<String>,
    pub format:   Option<String>,
}

// AFTER:
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CycleParams {
    pub r#type:     String,
    pub topic:      String,
    pub phase:      Option<String>,     // NEW
    pub outcome:    Option<String>,     // NEW
    pub next_phase: Option<String>,     // NEW
    pub agent_id:   Option<String>,
    pub format:     Option<String>,
    // keywords REMOVED from struct (unknown fields silently discarded — no deny_unknown_fields)
}
```

Serde behavior: no `#[serde(deny_unknown_fields)]` is in use (existing pattern). Callers passing `keywords` in JSON will have it silently discarded.

---

## Modified Handler: `context_cycle`

### What Changes

Step 3 (validation call): replace `keywords_ref` extraction with new param pass-through.
Steps after validation: remove keywords persistence call; add `PhaseEnd` arm.

### Pseudocode Body (delta from current)

```
FUNCTION context_cycle(params: CycleParams) -> Result<CallToolResult, ErrorData>:

    // 1. Identity resolution (UNCHANGED)
    identity = self.resolve_agent(&params.agent_id).await?

    // 2. Capability check (UNCHANGED)
    self.require_cap(&identity.agent_id, Capability::Write).await?

    // 3. Validation (CHANGED: new signature, no keywords_ref)
    validated = match validate_cycle_params(
        &params.r#type,
        &params.topic,
        params.phase.as_deref(),       // NEW
        params.outcome.as_deref(),     // NEW
        params.next_phase.as_deref(),  // NEW
    ):
        Err(msg) → return Ok(CallToolResult::error([Content::text("Validation error: {msg}")]))
        Ok(v)    → v

    // 4. Action label (CHANGED: add PhaseEnd arm)
    action = match validated.cycle_type:
        CycleType::Start    → "cycle_started"
        CycleType::PhaseEnd → "cycle_phase_transition"    // NEW
        CycleType::Stop     → "cycle_stopped"

    // 4b. Drain pending_entries_analysis on Stop (UNCHANGED)
    IF validated.cycle_type == CycleType::Stop:
        drained = self.pending_entries_analysis.lock().drain_for(&validated.topic)
        IF drained is not empty: log info

    // 5. Compose response text
    response_text = "Acknowledged: {action} for topic '{topic}'. ..."

    // 6. Audit log (UNCHANGED)
    self.audit_fire_and_forget(AuditEvent { operation: "context_cycle", ... })

    // 7. Return acknowledgment (UNCHANGED)
    return Ok(CallToolResult::success([Content::text(response_text)]))

    // REMOVED: keywords persistence call (was: update_session_keywords in listener via hook)
    // keywords persistence is simply removed; sessions.keywords column stays in place (C-04)
```

---

## Modified Handler: `context_cycle_review`

### What Changes

After the existing telemetry pipeline (steps 1–N, unchanged per C-07), add three new SQL queries and phase narrative assembly. The new queries are run against `self.store` via `spawn_blocking_with_timeout` (same pattern as the existing observation load).

### New SQL Queries

All three run against the sqlx `write_pool` (read is fine here; using `store.read_pool()` or any pool access that the existing handler uses).

```sql
-- Query 1: Cycle event log
SELECT seq, event_type, phase, outcome, next_phase, timestamp
  FROM cycle_events
 WHERE cycle_id = :feature_cycle
 ORDER BY timestamp ASC, seq ASC

-- Query 2: Current feature phase/category distribution
SELECT fe.phase, e.category, COUNT(*) AS cnt
  FROM feature_entries fe
  JOIN entries e ON e.id = fe.entry_id
 WHERE fe.feature_id = :feature_cycle
   AND fe.phase IS NOT NULL
 GROUP BY fe.phase, e.category

-- Query 3: Cross-feature baseline (excludes current feature)
SELECT fe.phase, e.category, COUNT(*) AS cnt
  FROM feature_entries fe
  JOIN entries e ON e.id = fe.entry_id
 WHERE fe.feature_id IN (
       SELECT DISTINCT feature_id FROM feature_entries WHERE phase IS NOT NULL
   )
   AND fe.feature_id != :feature_cycle
   AND fe.phase IS NOT NULL
 GROUP BY fe.phase, e.category
```

### Phase Narrative Assembly Pseudocode

```
FUNCTION assemble_phase_narrative(store, feature_cycle) -> Option<PhaseNarrative>:

    // Execute Query 1
    event_rows = store.query_cycle_events(feature_cycle)
        ordered by timestamp ASC, seq ASC
    // Map rows to CycleEventRecord structs

    IF event_rows is empty:
        return None    // backward compatible: no section emitted (AC-12, AC-13, R-08)

    // Execute Query 2
    current_rows = store.query_feature_phase_category_dist(feature_cycle)
    // Build current_dist: HashMap<(phase, category), count>
    // Type alias: PhaseCategoryDist = HashMap<String, HashMap<String, u64>>
    current_dist = build_phase_category_dist(current_rows)

    // Execute Query 3
    cross_rows = store.query_cross_feature_phase_category_dist(feature_cycle)
    cross_dist = build_phase_category_dist_cross(cross_rows)
    // Count distinct feature_ids in cross_rows to get sample_features

    // Call pure function from unimatrix-observe
    narrative = build_phase_narrative(&event_rows, &current_dist, &cross_dist)

    return Some(narrative)
```

### Integration into `context_cycle_review` handler

```
// After existing telemetry pipeline produces `report: RetrospectiveReport`...

// NEW: Phase narrative (runs in same spawn_blocking_with_timeout scope or separately)
phase_narrative = assemble_phase_narrative(&store, &feature_cycle)
report.phase_narrative = phase_narrative

// Return formatted report (UNCHANGED: format_retrospective_markdown or format_retrospective_report)
```

Note: `report` must be mutable at the point of assignment, or `phase_narrative` is set before the report is constructed. The cleanest approach is to run the three queries inside the same blocking closure and pass the assembled `PhaseNarrative` to the `RetrospectiveReport` constructor.

---

## Type `PhaseCategoryDist` (local alias in handler)

Not a public type. Used only within the assembly function.

```
type PhaseCategoryDist = HashMap<String, HashMap<String, u64>>;

FUNCTION build_phase_category_dist(rows: Vec<(phase, category, cnt)>) -> PhaseCategoryDist:
    dist = empty HashMap
    FOR each (phase, category, cnt) in rows:
        dist.entry(phase).or_default().insert(category, cnt)
    return dist
```

---

## Error Handling

- Query 1/2/3 failures: return error to caller (existing pattern — do not suppress DB errors).
- Empty Query 1 result: `phase_narrative = None`, no error, no placeholder.
- Empty Query 2/3 results: `per_phase_categories = {}`, `cross_cycle_comparison = None`. No error.
- `build_phase_narrative` is a pure function (see component 9) — cannot fail.

---

## Key Test Scenarios

1. `CycleParams` with `keywords` in JSON → deserialization succeeds, `keywords` inaccessible on struct.
2. `CycleParams` with `type="phase-end"`, `phase="scope"`, `next_phase="design"` → succeeds.
3. `type="phase-end"` with invalid `phase="scope review"` → Err response (no crash).
4. `context_cycle_review` with no `cycle_events` rows → `phase_narrative` absent from JSON.
5. `context_cycle_review` with `cycle_events` rows but no phase-tagged `feature_entries` → `phase_narrative.per_phase_categories` is empty, no crash.
6. `context_cycle_review` cross-cycle comparison absent when fewer than 2 prior features.
7. `context_cycle_review` cross-cycle comparison present when 2+ prior features, self excluded.
8. Existing telemetry output unchanged (hotspots, metrics, baseline_comparison).
