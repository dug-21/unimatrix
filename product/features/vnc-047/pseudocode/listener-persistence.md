# C5 — Listener persistence routing (`handle_cycle_event` step-5)

**File:** `crates/unimatrix-server/src/uds/listener.rs` (~:3035-3080, the step-5 fire-and-forget spawn)
**ADR:** ADR-002 / ADR-003. **Risks:** R-09, R-04, R-06, R-05, R-03. **AC:** AC-02, AC-02a.

## Purpose

The single routing decision that keeps `insert_cycle_event`'s 15 call sites untouched: read
`payload["tags"]`, and when the event is a `Start` carrying tags, route to the new
`insert_cycle_start_with_tags` (C2); otherwise route to the UNCHANGED `insert_cycle_event`. Gate on
the SAME `!feature_cycle.is_empty()` condition already guarding the cycle-event insert (ADR-003) —
NOT on `attribution_result`. The absent/evicted-session durability comes from the EXISTING Step-1b
#519 pre-register (:2894-2908) — no new code there.

## Existing context reused (do not modify)

- `feature_cycle` — sanitized in Step 1 (:2858-2883); empty ⇒ single documented silent drop.
- `lifecycle: CycleLifecycle` — `Start | PhaseEnd | Stop` (fn param).
- `goal_for_event: Option<String>` — computed Step 3b (:2979); rides the start row.
- Step 1b #519 pre-register (:2894-2908) already fires on Start for absent sessions — tags inherit
  it automatically because persistence gates on `feature_cycle`, not registry presence (R-04).

## Pseudocode — modify the Step-5 spawn (:3038-3080)

```
# Step 5: Fire-and-forget cycle-event / cycle-start-with-tags persistence.
if !feature_cycle.is_empty():                       # SAME gate as today (ADR-003); NOT attribution_result

    # existing captures (phase_val, outcome_val, next_phase_for_db, event_type_str,
    #                     cycle_id, timestamp, store_clone, goal_for_db) …

    # NEW: extract tags from payload (array-of-strings; degrade to [] on any shape) — R-03 contract
    let tags_for_db: Vec<String> = event.payload
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter()
                      .filter_map(|v| v.as_str())
                      .map(|s| s.to_string())
                      .collect())
        .unwrap_or_default();

    let is_start = (lifecycle == CycleLifecycle::Start);   # capture for the spawn

    spawn(async move {
        let seq = store_clone.get_next_cycle_seq(&cycle_id).await;

        if is_start && !tags_for_db.is_empty():
            # ---- Start-with-tags arm → NEW primitive (BEGIN IMMEDIATE txn, whole-set-once) ----
            if let Err(e) = store_clone.insert_cycle_start_with_tags(
                    &cycle_id, seq,
                    phase_val.as_deref(), outcome_val.as_deref(), next_phase_for_db.as_deref(),
                    timestamp, goal_for_db.as_deref(), &tags_for_db).await:
                tracing::warn!(error = %e, cycle_id = %cycle_id,
                    "vnc-047: insert_cycle_start_with_tags failed");
            # (C13 wrote-set/frozen-skip trace is emitted INSIDE the primitive — freeze-trace.md)
        else:
            # ---- all other events + start-without-tags → UNCHANGED insert_cycle_event ----
            if let Err(e) = store_clone.insert_cycle_event(
                    &cycle_id, seq, &event_type_str,
                    phase_val.as_deref(), outcome_val.as_deref(), next_phase_for_db.as_deref(),
                    timestamp, goal_for_db.as_deref()).await:
                tracing::warn!(error = %e, cycle_id = %cycle_id,
                    "crt-025: insert_cycle_event failed");
    });
# Step 6 (goal embedding) unchanged.
```

### Routing truth table (R-09)

| lifecycle | payload has non-empty tags | route |
|-----------|----------------------------|-------|
| Start | yes | `insert_cycle_start_with_tags` (C2) |
| Start | no  | `insert_cycle_event` (UNCHANGED) |
| PhaseEnd / Stop | (irrelevant — C4 never sets the key) | `insert_cycle_event` (UNCHANGED) |
| any | `feature_cycle` empty | **no persistence** (single documented drop) |

- Both arms compute `seq` identically and pass `goal_for_db` — so `goal` persists whether or not tags
  are present (R-09 scenario 3). The two arms differ ONLY by which store method runs.
- **Step 6 (goal-embedding UPDATE) is UNCHANGED and still runs on the tags arm.** Step 6 (:3081)
  spawns `update_cycle_start_goal_embedding` for every `Start` regardless of the Step-5 arm; it keys
  on `cycle_id AND event_type='cycle_start'`, so it finds and populates the row inserted by
  `insert_cycle_start_with_tags` exactly as on the plain path. `goal_embedding` is NOT written in the
  Step-5 INSERT on either arm (it starts NULL) — do NOT add it to the new primitive (see C2 HEAD
  verification). The residual INSERT-before-UPDATE ordering race is identical to today (accepted).
  **Verified against HEAD:** `insert_cycle_event` at db.rs:320 is 8-arg WITH `next_phase` and does NOT
  write `goal_embedding` — a Stage 3a tester note to the contrary is incorrect; the else-arm call above
  matches HEAD.
- `is_start && !tags.is_empty()` is the exact ADR-002 §4 condition. A Start with an empty/absent
  `tags` key MUST fall to the else arm (else the tagless start would burn the one-shot / mis-route).

## Error boundaries (ADR-003)

- Both arms are inside the existing `tokio::spawn` — errors only `tracing::warn`, task never panics,
  no caller-visible signal (set-and-forget). Empty `feature_cycle` is the single documented drop.
- No second persistence route (R-06): the `cycle_tags` INSERT is reachable ONLY from
  `insert_cycle_start_with_tags`, reached ONLY from this Start-with-tags arm.

## Key test scenarios (hints)

1. **Assembled Start-with-tags (AC-02, R-03):** drive `context_cycle(start, topic=FC, tags=[…])`
   through hook → RecordEvent → `handle_cycle_event`; assert rows land in `cycle_tags` for FC and the
   cycle_start row exists. (`proven_by` MUST cite this assembled test.)
2. **Non-start with tags (FR-4):** assembled phase-end carrying tags → NO `cycle_tags` rows.
3. **Start-without-tags (R-09):** routed to `insert_cycle_event`; normal start behavior, no
   `cycle_tags` rows; `goal` still persists.
4. **Absent/evicted session (R-04):** start with tags on a session absent from the registry → Step-1b
   pre-register fires; tags still land for the correct FC.
5. **Empty/NULL feature_cycle (R-04.2):** persistence gated off, no orphan rows — the only drop.
6. **DB error in spawn (R-04.3):** `tracing::warn`, no panic, review later still succeeds.
7. **Whole-set-once via assembled re-starts (AC-02a):** exact-equality across changed/subset/
   superset/different + tagless-then-tag-bearing (see C2 scenarios, driven end-to-end here).
