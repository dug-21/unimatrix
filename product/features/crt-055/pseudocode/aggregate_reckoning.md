# Component 3 — Aggregate reckoning (rank 1/2/3)

**Crate**: `unimatrix-observe` (new module, sibling to `session_metrics.rs`)
**Files**: `unimatrix-observe/src/cycle_aggregates.rs` (new) + `tools.rs` call site
**ADRs**: ADR-004 (#5039) | **Risks**: R-15 (rank-1 timeline), R-16 (rank-3 union), R-17 (num/den) | **Wave**: 2

## Purpose

Derive the rank-1/2/3 durable aggregates from content-opaque durable streams: rank-1 phase aggregates from `cycle_events` (incl. #556 never-closed), rank-2 rework ratio num/den from `SessionRecord.outcome`, rank-3 knowledge-reuse-all-served from `query_log ∪ injection_log` (#320). Output is plain `i64` fields on `CycleAggregates`.

## Constraints honored

- All `i64`, num/den **pairs** stored (never pre-divided — R-17). Ratio derived at presentation.
- Content-opaque sources only (leak gate untouched — no transcript read here).
- Rank-1 reads the timeline AFTER `auto_close` has (optionally) written `cycle_stop` (sequencing — Component 8/9).

## Shared output bundle

```
struct CycleAggregates {                 // grows across Components 3,4,5,6
    // rank-1
    phase_count: i64
    phase_transition_count: i64
    phase_rework_count: i64
    phase_unclosed_count: i64
    phase_total_duration_secs: i64
    // rank-2
    rework_session_count: i64
    total_session_count: i64
    // rank-3
    knowledge_reuse_served_count: i64
    // (transcript_* / signal_class_counts_json — Component 6)
    // (compaction_* / context_reload_pct — Components 4,5)
    ...
}
```

## 3a. Rank-1 phase reckoning (cycle_events)

Input: the cycle's `Vec<CycleEventRecord>` (already loaded by the handler; `event_type`, `phase`, `timestamp`, `cycle_id`). Walk events in `timestamp` order and track per-phase open/close state.

```
fn reckon_phase_aggregates(events: &[CycleEventRecord]) -> PhaseAgg:
    sort events by timestamp asc (stable; seq tiebreak)
    open_phase: map<phase_name, open_start_secs>   // currently-open phases
    seen_phase: set<phase_name>                    // distinct phases ever declared
    transitions = 0
    rework = 0
    total_duration_secs = 0
    for e in events:
        match e.event_type:
          "cycle_phase_start" (or the project's phase-start type):
              if e.phase in open_phase OR e.phase in seen_phase:
                  rework += 1            // re-entry of a phase already started → rework loop (R-15)
              else:
                  // first declaration of this phase
              seen_phase.insert(e.phase)
              open_phase[e.phase] = e.timestamp
          "cycle_phase_end" (phase-end / transition):
              transitions += 1
              if e.phase in open_phase:
                  total_duration_secs += max(0, e.timestamp - open_phase[e.phase])
                  open_phase.remove(e.phase)
          "cycle_stop":
              // closes all still-open phases at stop time (counts their duration,
              // and they are NOT never-closed because the cycle ended)
              for (p, start) in open_phase: total_duration_secs += max(0, e.timestamp - start)
              open_phase.clear()
    phase_count        = seen_phase.len()
    phase_unclosed     = open_phase.len()   // declared, never closed, and NO cycle_stop closed them (#556)
    return PhaseAgg { phase_count, transitions, rework, phase_unclosed, total_duration_secs }
```

Notes:
- **#556**: `phase_unclosed_count` = phases left open after processing all events (no matching end, no `cycle_stop`). When `auto_close=true` wrote a `cycle_stop`, the `cycle_stop` branch clears `open_phase`, so the final phase is NOT a false never-closed (R-14/R-15). When `auto_close=false`, an open final phase correctly surfaces as never-closed (fail-loud, not an error).
- **Rework vs new phase (R-15)**: a phase name that re-opens after being closed counts toward `phase_rework_count`, not a second `phase_count` (distinct-name set + re-entry detection).
- Confirm the exact `event_type` literals for phase start/end against the live `cycle_events` writer at implementation (the handler already uses `"cycle_start"`/`"cycle_stop"`; phase-level literals to verify). Map them; do not invent.

## 3b. Rank-2 rework ratio num/den (SessionRecord.outcome)

Input: the cycle's sessions with `outcome` populated (the handler already joins `SessionRecord.outcome` onto session summaries — `session_metrics.rs:207`).

```
fn reckon_rework_ratio(sessions: &[SessionLike]) -> (rework_session_count, total_session_count):
    total = count of sessions attributed to the cycle (declared sessions)
    rework = count of sessions whose outcome ∈ { rework, failure, ... }   // the project's rework/failure outcome set
    return (rework as i64, total as i64)         // PAIR, never rework/total
```

`"0 of 0"` (total=0) → presentation renders unavailable (Component 7); `"0 of N"` → measured 0/N. Confirm the exact rework/failure outcome enum values at implementation (do not invent; `RetrospectiveReport.rework_session_count: Option<u64>` already exists at `types.rs:420` and encodes the numerator — reuse its source classification).

## 3c. Rank-3 knowledge-reuse-all-served (#320, query_log ∪ injection_log)

Count the UNION of distinct entries served to the cycle across both logs — all served, not same-cycle-tagged only (R-16). Dedup an entry served via both logs (count once).

```
fn reckon_knowledge_reuse_served(store, feature_cycle) -> i64:
    // Two reads (read_pool); union by entry id; dedup.
    served = SELECT DISTINCT entry_id FROM query_log   WHERE <served-to-this-cycle predicate>
           UNION
             SELECT DISTINCT entry_id FROM injection_log WHERE <served-to-this-cycle predicate>
    return count(served) as i64
```

OPEN-Q (Architecture §10 Q1, R-16): confirm the **exact injection_log table/column names** and the "served to this cycle" predicate at implementation. The `query_log` surface is established (`query_log.phase` column exists — `migration.rs:576`); the injection_log surface must be verified against the current schema. A wrong table name yields a silent zero — this is why the AC seeds entries split across both logs and asserts the union size. Implement as a single SQL `UNION` (dedups by id) or as two reads merged into a `HashSet<entry_id>`.

## Data flow

- IN: `Vec<CycleEventRecord>` (rank-1), session outcomes (rank-2), store handle + `feature_cycle` (rank-3).
- OUT: the 8 rank-1/2/3 `i64` fields on `CycleAggregates`.

## Error handling

- A read failure on rank-3 logs → treat as empty (count 0) and let Component 7 mark `knowledge_reuse_available=false` — never abort the pipeline. (Honest "unavailable", not a fabricated 0.)
- Empty `cycle_events` → all phase aggregates 0; Component 7 marks `phase_metrics_available=false`.

## Key test scenarios

- Seeded `cycle_events` with an unclosed phase → `phase_unclosed_count` ≥ 1; that phase surfaces as a hotspot feeding rank-1 (AC-04, #556).
- Closed-then-reopened phase → counted as `phase_rework_count`, not a second `phase_count` (AC-05, R-15).
- Phase with matching close → does NOT increment `phase_unclosed_count`; duration summed into `phase_total_duration_secs` (AC-05).
- With `auto_close=true` closing the cycle, the final phase is NOT counted as never-closed (R-14, R-15).
- Rank-2: num/den pair persisted (`rework_session_count`/`total_session_count`), not a pre-divided ratio (R-17).
- Rank-3: entries split across query_log + injection_log incl. cross-cycle-tagged → count == union size; an entry in both → counted once (AC-06, R-16).
