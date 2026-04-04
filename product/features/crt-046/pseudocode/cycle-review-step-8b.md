# crt-046 — Component: cycle-review-step-8b

## Purpose

Insertion point in `context_cycle_review` handler (`crates/unimatrix-server/src/mcp/tools.rs`).

Step 8b extracts co-access pairs from cycle observations, emits bidirectional Informs
graph edges, and writes a `goal_clusters` row. It runs on EVERY `context_cycle_review`
call — cache-hit (`force=false`) or cache-miss — because all writes are idempotent
(INSERT OR IGNORE throughout).

Also adds `parse_failure_count: u32` as a top-level field in the JSON response (Resolution 1).

Wave: 3 (depends on store-v22 + behavioral_signals).

---

## CRITICAL: Memoisation Early-Return Placement (Resolution 2, FR-09, AC-15)

The architecture §Component 3 prose stating that `force=false` early-return "returns
before step 8b" is WRONG. Do not follow it.

FR-09 is authoritative: step 8b runs on every call.

The existing memoisation early-return block at step 2.5 (the `if !force { match
store.get_cycle_review(&feature_cycle) ... }` block) MUST remain where it is — it
is NOT moved. Step 8b is inserted AFTER step 8a (store_cycle_review) but BEFORE
step 11 (audit). The existing memoisation block still exists and still returns early
in the `force=false` cache-hit case — but only AFTER step 8b has run.

Wait. Let me be precise. The existing code flow is:

```
[step 2.5]  if !force → get_cycle_review → if found, RETURN (cache hit)
[step 4-7]  full pipeline ...
[step 8a]   store_cycle_review()
[step 11]   audit
[step 12]   format and return
```

Resolution 2 requires step 8b to run on cache-hit. The ONLY correct implementation is
to MOVE the memoisation early-return to AFTER step 8b. The new flow becomes:

```
[step 4-7]  full pipeline ... (unchanged)
[step 8a]   store_cycle_review()             // existing
[step 8b]   behavioral_signals::run_step_8b()  // NEW — always runs
[step 11]   audit                            // existing
[step 2.5*] if memoised: format and return  // MOVED — was before step 4, now here
[step 12]   format and return (non-memoised)
```

No. That doesn't make sense either — the full pipeline (steps 4-7) only runs on a
cache-miss. The correct reading of Resolution 2 is:

On a cache-hit (`force=false`, review already stored), the handler currently returns
before running any computation. Resolution 2 says step 8b must run on that path too.

The implementation approach is:

1. At the existing memoisation check location (step 2.5, after `force` is read):
   - Do NOT return early yet on cache-hit.
   - Instead, record that a cache-hit occurred: `let memo_hit = step_2_5_check(...)`.
2. Continue to step 8b (which must run regardless of memo_hit).
3. After step 8b, if `memo_hit` is true: format the cached record and return.
4. If `memo_hit` is false: continue with the full pipeline.

This is the ONLY placement that satisfies both:
- "step 8b always runs" (FR-09, AC-15)
- "full pipeline only runs on cache-miss" (existing behavior)

---

## Revised context_cycle_review Control Flow

```
[step 1]    identity + capability check
[step 2]    validation (feature_cycle, force)
[step 2.5]  read force param
            check memoisation:
                memo_result = store.get_cycle_review(&feature_cycle).await
                (do NOT return early here — only record memo_result)

[step 3]    load observations (three-path attribution)

[step 4-7]  IF memo_result.is_none() OR force == true:
                run full pipeline (existing steps 4-7)
                run step 8a: store_cycle_review()

[step 8b]   behavioral_signals::run_step_8b(    // ALWAYS RUNS
                &store, &feature_cycle, &report.outcome
            )
            → returns parse_failure_count: u32

[step 11]   audit (fire-and-forget)

[step 12]   IF memo_result.is_some() AND force == false:
                format memo_result and return   // cache-hit path returns here
            ELSE:
                format report and return        // full pipeline result
```

Note: `report` in step 8b refers to whatever outcome context is available:
- On cache-hit: the `CycleReviewRecord` already stored contains `summary_json` which
  includes `outcome`. Step 8b reads `report.outcome` from the retrieved record.
- On cache-miss: `report` is the freshly computed RetrospectiveReport.

The step 8b function signature abstracts this: it accepts `outcome: Option<&str>` directly.

---

## Step 8b Sequence (run_step_8b)

This logic lives in `services/behavioral_signals.rs` as a free function (or as a
standalone async function called from the handler). The handler calls it as:

```
let parse_failure_count: u32 = behavioral_signals::run_step_8b(
    &store,
    &feature_cycle,
    outcome_opt,   // Option<&str> — from report or from memo record
).await;
```

All errors are non-fatal. `run_step_8b` returns `parse_failure_count: u32` even if
all internal steps fail. The handler never propagates step 8b errors to the caller.

### run_step_8b Algorithm

```
async fn run_step_8b(
    store: &SqlxStore,
    feature_cycle: &str,
    outcome: Option<&str>,
) -> u32
```

Returns: `parse_failure_count` (number of unparseable `context_get` observation rows).

Step 1: Load session IDs.
```
let session_ids = match store.load_sessions_for_feature(feature_cycle).await {
    Ok(ids) => ids,
    Err(e) => {
        warn!("step 8b: load_sessions_for_feature failed for {feature_cycle}: {e}");
        return 0;
    }
};
```

Step 2: Load observations.
```
let observations = match store.load_observations_for_sessions(&session_ids).await {
    Ok(obs) => obs,
    Err(e) => {
        warn!("step 8b: load_observations_for_sessions failed for {feature_cycle}: {e}");
        return 0;
    }
};
```

Step 3: Collect co-access entry IDs.
```
let (by_session, parse_failures) =
    behavioral_signals::collect_coaccess_entry_ids(&observations);
```
`parse_failures` is the count of unparseable rows. Returned at the end.

Step 4: Build co-access pairs.
```
let (pairs, cap_hit) = behavioral_signals::build_coaccess_pairs(by_session);
```

Step 5: Log if cap was hit.
```
if cap_hit {
    warn!(
        "step 8b: pair cap reached ({PAIR_CAP}) for {feature_cycle} — some pairs not emitted"
    );
}
```

Step 6: Determine edge weight.
```
let weight = behavioral_signals::outcome_to_weight(outcome);
```

Step 7: Emit behavioral edges (skip if no pairs — AC-04).
```
if pairs.is_empty() {
    debug!("step 8b: no co-access pairs for {feature_cycle} — skipping edge emission");
} else {
    let (enqueued, skipped) =
        behavioral_signals::emit_behavioral_edges(store, &pairs, weight).await;
    debug!(
        "step 8b: {enqueued} edges enqueued, {skipped} pairs skipped on conflict for {feature_cycle}"
    );
}
```

Step 8: Get goal embedding.
```
let embedding_opt = match store.get_cycle_start_goal_embedding(feature_cycle).await {
    Ok(opt) => opt,
    Err(e) => {
        warn!("step 8b: get_cycle_start_goal_embedding failed for {feature_cycle}: {e}");
        None
    }
};
```

Step 9: Populate goal cluster if embedding available.
```
if let Some(embedding) = embedding_opt {
    // Collect all entry IDs accessed (union across all sessions).
    let all_entry_ids: Vec<u64> = {
        let (by_session_2, _) = behavioral_signals::collect_coaccess_entry_ids(&observations);
        let mut ids: Vec<u64> = by_session_2
            .values()
            .flat_map(|v| v.iter().map(|(id, _)| *id))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };

    // Determine phase from latest cycle_events row with non-NULL phase.
    let phase_opt: Option<String> = get_latest_cycle_phase(store, feature_cycle).await;
    // get_latest_cycle_phase: SELECT phase FROM cycle_events
    //     WHERE cycle_id = ? AND phase IS NOT NULL
    //     ORDER BY timestamp DESC LIMIT 1
    // Returns Ok(None) if no phase row found; errors logged and treated as None.

    match behavioral_signals::populate_goal_cluster(
        store,
        feature_cycle,
        embedding,
        &all_entry_ids,
        phase_opt.as_deref(),
        outcome,
    ).await {
        Ok(true)  => debug!("step 8b: goal_cluster written for {feature_cycle}"),
        Ok(false) => debug!("step 8b: goal_cluster UNIQUE conflict for {feature_cycle} — no-op"),
        Err(e)    => warn!("step 8b: populate_goal_cluster failed for {feature_cycle}: {e}"),
    }
}
```

Step 10: Return parse failures.
```
parse_failures as u32
```

### Note on entry_ids for populate_goal_cluster

The call to `collect_coaccess_entry_ids` at step 3 produces `by_session` which is
consumed by `build_coaccess_pairs` at step 4. Step 9 needs the flat union of all entry
IDs. Two approaches:

Option A: Re-call `collect_coaccess_entry_ids(&observations)` for step 9 (observations
still available). Slightly redundant CPU work but simple and correct.

Option B: Preserve `by_session` before consuming and reconstruct from it.

Pseudocode uses Option A (simpler). Implementation agent should choose whichever avoids
borrow conflicts in Rust.

### Module-Private Helper: get_latest_cycle_phase

```
async fn get_latest_cycle_phase(store: &SqlxStore, cycle_id: &str) -> Option<String>
```

Algorithm:
1. Query `read_pool()`:
   ```sql
   SELECT phase FROM cycle_events
   WHERE cycle_id = ?1 AND phase IS NOT NULL
   ORDER BY timestamp DESC
   LIMIT 1
   ```
2. On row: return `Some(phase_string)`.
3. On no row or SQL error: return `None` (log warn! on SQL error).

---

## parse_failure_count in JSON Response (Resolution 1)

`parse_failure_count: u32` is a TOP-LEVEL field in the `context_cycle_review` JSON
response, OUTSIDE the serialized `CycleReviewRecord`.

`CycleReviewRecord` is NOT modified. No `SUMMARY_SCHEMA_VERSION` bump is required.

The handler currently returns `format_retrospective_report(&report)` (for format=json)
or `format_retrospective_markdown(&report)` (for format=markdown).

For the JSON format path, the response must be wrapped:

```
// Current (format=json):
Ok(format_retrospective_report(&report))

// New (format=json):
// Build a JSON object that wraps the report and adds parse_failure_count at the top level.
let report_json = serde_json::to_value(existing_report_serialization)?;
// OR: if format_retrospective_report returns a string, deserialize and re-wrap:
let mut wrapper = serde_json::json!({
    "parse_failure_count": parse_failure_count,
});
// Merge all existing report fields at top level (or under a "review" key — see below).
// wrapper["review"] = report_json;  // if nested under a key
// OR inject parse_failure_count into the flat report object.

// Per Resolution 1 intent: parse_failure_count is a top-level field alongside other
// report fields, not nested under a "review" key. Implementation agent should verify
// how format_retrospective_report currently serializes the report and whether the
// tool output is a JSON string or a structured CallToolResult.
```

For the markdown format path: append a line to the formatted output:
```
// At the end of the markdown output:
// "Parse failures: {parse_failure_count}" (only if parse_failure_count > 0, or always)
```

Implementation agent must verify the exact response format mechanism. The invariant is:
`parse_failure_count` appears in the MCP tool response (AC-13 assertion: caller can read
it from the returned payload without server log access).

The field must always be present (not omitted when zero). AC-13 test inspects the
returned payload and asserts count >= 1 for the malformed-row scenario.

---

## Changes to mcp/tools.rs

1. Import `behavioral_signals` at the top of the handler body:
   `use crate::services::behavioral_signals;`
   Or reference via module path `crate::services::behavioral_signals::run_step_8b(...)`.

2. After step 8a (`store_cycle_review` call), before step 11 (audit), add:
   ```
   // Step 8b — always runs (FR-09, Resolution 2, AC-15)
   let parse_failure_count = behavioral_signals::run_step_8b(
       &store,
       &feature_cycle,
       outcome_from_report_or_cache,  // see below
   ).await;
   ```

3. `outcome_from_report_or_cache` extraction:
   - On cache-hit path (memoised): extract outcome from `CycleReviewRecord.summary_json`
     by deserializing the stored JSON and reading the `outcome` field. If parsing fails,
     pass `None`.
   - On full pipeline path: `report.outcome` or the equivalent field on `RetrospectiveReport`.
     The implementation agent should check the exact field name on `RetrospectiveReport`.

4. Move the memoisation early-return to AFTER the step 8b block.

5. In the JSON format branch, wrap the response to include `parse_failure_count: u32`.

---

## Key Test Scenarios (cycle-review-step-8b)

| Test | Risk | Assertion |
|------|------|-----------|
| AC-01 | R-10 | After review with 2+ context_get in same session, graph_edges has Informs row with source='behavioral' for BOTH (A→B) and (B→A) |
| AC-02 | NFR-01 | Second call same cycle: graph_edges count unchanged (idempotent) |
| AC-03 | FR-06 | outcome="success" → weight=1.0; outcome=None → weight=0.5 |
| AC-04 | FR-08 | Zero context_get obs → zero behavioral edges |
| AC-05 | FR-13 | Review with goal embedding → goal_clusters row written with correct entry_ids_json |
| AC-06 | FR-11 | Review with no goal embedding → goal_clusters has zero rows |
| AC-13 | R-04 | Malformed input JSON → parse_failure_count >= 1 in response payload |
| AC-14 | R-09 | 21 distinct context_get obs → edge count <= 400, server log warn! present |
| AC-15 | R-01 | force=false call after prime → graph_edges count unchanged (step 8b ran) |
| F-01 | F-01 | emit_behavioral_edges error → handler returns success with review record |
| I-01 | I-01 | store_cycle_review fails → step 8b does NOT run |

### I-01 clarification

Step 8b runs AFTER step 8a. If step 8a (store_cycle_review) fails and returns early
(with a warn! log), step 8b also does not run because the code flows to the next step
only on success. The test for F-01 injects an error into `emit_behavioral_edges` (not
into store_cycle_review) and confirms the handler still returns a successful response.

### Note on drain flush (I-02)

`emit_behavioral_edges` uses `write_graph_edge` (direct `write_pool_server()` call),
NOT `enqueue_analytics`. Therefore, integration tests querying `graph_edges` after step
8b do NOT need an analytics drain flush. The rows are present immediately after the call.

If a future change routes step 8b edges through the analytics drain, I-02 flush logic
must be added to all affected tests at that time.
