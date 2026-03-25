# col-026 Pseudocode Overview

## Components and Files

| Component | Pseudocode File | Production File |
|-----------|----------------|-----------------|
| RetrospectiveReport struct extensions | retrospective-report-extensions.md | `crates/unimatrix-observe/src/types.rs` |
| PhaseStats type + computation | phase-stats.md | `crates/unimatrix-server/src/mcp/tools.rs` |
| FeatureKnowledgeReuse extension + batch lookup | knowledge-reuse-extension.md | `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` |
| Formatter overhaul | formatter-overhaul.md | `crates/unimatrix-server/src/mcp/response/retrospective.rs` |
| compile_cycles recommendation fix | recommendation-fix.md | `crates/unimatrix-observe/src/report.rs` |

---

## Data Flow

```
context_cycle_review handler (tools.rs)
  │
  ├─ [Step 3] load_cycle_observations / fallback path
  │     → attributed: Vec<ObservationRecord>
  │     → attribution_path_label: &'static str   [COMPONENT 2: recorded here]
  │
  ├─ [Steps 11-15] session summaries, knowledge reuse, reload pct
  │     compute_knowledge_reuse_for_sessions(store, sessions)
  │       → now receives entry_meta_lookup closure    [COMPONENT 3: new closure]
  │       → FeatureKnowledgeReuse (with new fields)
  │
  ├─ [Step 10g] cycle_events SQL → events: Vec<CycleEventRecord>
  │             events is BORROWED from here on — ownership never moved
  │             build_phase_narrative(&events, ...)   [existing]
  │
  ├─ [Step 10h NEW] compute_phase_stats(&events, &attributed)
  │                  → Option<Vec<PhaseStats>>        [COMPONENT 2: algorithm]
  │                  → report.phase_stats = result
  │
  ├─ [Step 10i NEW] get_cycle_start_goal(cycle_id).await
  │                  → report.goal: Option<String>
  │                  → infer_cycle_type(goal) → report.cycle_type: Option<String>
  │                  → derive_is_in_progress(&events) → report.is_in_progress: Option<bool>
  │                  → report.attribution_path = Some(attribution_path_label.to_string())
  │
  └─ [Step 12] format dispatch
        format_retrospective_markdown(&report)   [COMPONENT 4: overhaul]
          outputs new section order (12 sections)
```

---

## Shared Types (new/modified — defined in types.rs)

All new types live in `crates/unimatrix-observe/src/types.rs`.

```
PhaseStats {
    phase: String,
    pass_number: u32,        // 1-indexed; identifies this row's pass in the sequence
    pass_count: u32,         // total passes for this phase name (>1 = rework)
    duration_secs: u64,
    session_count: usize,
    record_count: usize,
    agents: Vec<String>,     // deduplicated, first-seen order
    tool_distribution: ToolDistribution,
    knowledge_served: u64,
    knowledge_stored: u64,
    gate_result: GateResult,
    gate_outcome_text: Option<String>,
    hotspot_ids: Vec<String>,  // populated by formatter, not computation
}

ToolDistribution {
    read: u64,
    execute: u64,
    write: u64,
    search: u64,
}

enum GateResult { Pass, Fail, Rework, Unknown }

EntryRef {
    id: u64,
    title: String,
    feature_cycle: String,   // renamed from source_cycle in spec domain model; use "feature_cycle"
    category: String,
    serve_count: u64,
}
```

NOTE on EntryRef field name: ARCHITECTURE.md and IMPLEMENTATION-BRIEF use `feature_cycle: String`.
SPECIFICATION domain model uses `source_cycle: String`. The architecture wins; use `feature_cycle`.
Flag to implementation agent: reconcile with spec §Domain Models EntryRef before compile.

New fields on `RetrospectiveReport` (all Option<T> with #[serde(default, skip_serializing_if)]):
- `goal: Option<String>`
- `cycle_type: Option<String>`
- `attribution_path: Option<String>`
- `is_in_progress: Option<bool>`
- `phase_stats: Option<Vec<PhaseStats>>`

New fields on `FeatureKnowledgeReuse` (all #[serde(default)]):
- `total_served: u64`
- `total_stored: u64`
- `cross_feature_reuse: u64`
- `intra_cycle_reuse: u64`
- `top_cross_feature_entries: Vec<EntryRef>` (#[serde(default, skip_serializing_if = "Vec::is_empty")])

`EntryMeta` — internal to `knowledge_reuse.rs`, not a public type:
```
EntryMeta {
    title: String,
    feature_cycle: Option<String>,
    category: String,
}
```

---

## Sequencing Constraints

Build order for implementation agents:

1. **FIRST**: Component 1 (types.rs) — all other components depend on the new types.
   - Without `PhaseStats`, `GateResult`, `ToolDistribution`, `EntryRef`, nothing else compiles.
   - Also update `build_report()` in `report.rs` to include new fields (or compilation of the
     `FeatureKnowledgeReuse` construction site breaks immediately).

2. **SECOND**: Component 5 (recommendation-fix.md) — touches `report.rs` only, no dependencies.
   Can be done in parallel with Component 1 once types.rs compiles.

3. **THIRD**: Component 3 (knowledge-reuse-extension.md) — depends on `EntryMeta` from step 1.
   Extends `compute_knowledge_reuse` signature. All call sites (3 construction sites) must update.

4. **FOURTH**: Component 2 (phase-stats.md) — depends on `PhaseStats`, `GateResult`,
   `ToolDistribution` from step 1. Inserts steps 10h/10i into the handler.

5. **FIFTH**: Component 4 (formatter-overhaul.md) — depends on all new types and new fields
   being present on `RetrospectiveReport`. Must compile and pass tests last.

`cargo build` must pass at the end of each step (compile-time migration enforcement).

---

## FeatureKnowledgeReuse Construction Sites

All three must be updated when new fields are added:
1. `crates/unimatrix-observe/src/types.rs` test fixtures (lines ~458, ~585, ~590)
2. `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` — production return sites (lines ~90-95, ~130-135, ~164-169)
3. `crates/unimatrix-server/src/mcp/response/retrospective.rs` test fixtures

Failure to update any construction site produces a compile error (struct literal exhaustiveness).

---

## Key Risk Map

| Risk | Component | Pseudocode Mitigation |
|------|-----------|----------------------|
| R-01: inline * 1000 | Component 2 | Explicitly call cycle_ts_to_obs_millis() by name at every boundary conversion |
| R-02: bad window extraction | Component 2 | Explicit walk algorithm with cycle_start/cycle_stop anchors |
| R-03: GateResult multi-keyword | Component 2 | Priority order: Rework > Fail > Pass > Unknown; check rework condition first |
| R-04: batch query missing rows | Component 3 | Missing entries excluded; arithmetic invariant cross+intra <= delivery_count |
| R-05: is_in_progress None branch | Component 2 | events.is_empty() check is first; None is the explicit return for that branch |
| R-07: section order regression | Component 4 | Numbered comments enforce sequence; section-order test required |
| R-08: threshold regex | Component 4 | format_claim_with_baseline handles three paths; threshold=0 guard |
