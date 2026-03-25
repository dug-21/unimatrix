# col-026: Unimatrix Cycle Review Enhancement — Architecture

## System Overview

`context_cycle_review` is the 12th MCP tool in unimatrix-server. It converts accumulated
observation telemetry into a structured `RetrospectiveReport`, then dispatches to either a
markdown formatter or a JSON serializer. col-026 extends this tool across three layers without
changing its external interface, adding new schema queries, or modifying the DB.

Two upstream features provide the raw material for all new sections:
- **col-024**: authoritative time-window attribution via `cycle_events`; introduces
  `cycle_ts_to_obs_millis()` for timestamp unit conversion
- **col-025**: durable goal text in `cycle_events.goal`; introduces `get_cycle_start_goal()`

col-026 connects those capabilities to the formatter output. No schema migration. No new MCP
tools. Schema remains at v16.

## Component Breakdown

### Component 1: `RetrospectiveReport` struct extensions
**Crate**: `unimatrix-observe/src/types.rs`

New optional fields added to the existing struct:

| Field | Type | Default | Source |
|-------|------|---------|--------|
| `goal` | `Option<String>` | `None` | `get_cycle_start_goal()` in handler |
| `cycle_type` | `Option<String>` | `None` | keyword inference from goal in handler |
| `attribution_path` | `Option<String>` | `None` | set in handler at path-selection point |
| `is_in_progress` | `Option<bool>` | `None` | derived from `cycle_events` (see ADR-001) |
| `phase_stats` | `Option<Vec<PhaseStats>>` | `None` | computed in handler step 10h |

All new fields use `#[serde(default, skip_serializing_if = "Option::is_none")]`.

### Component 2: `PhaseStats` type (new)
**Crate**: `unimatrix-observe/src/types.rs`

New struct representing one phase window's aggregate data:

```
PhaseStats {
    phase: String,
    pass_number: u32,          // 1-indexed; >1 = rework pass
    pass_count: u32,           // total passes for this phase name
    duration_secs: u64,        // end_ts - start_ts for this window
    session_count: usize,      // distinct sessions with observations in window
    record_count: usize,       // observations in window
    agents: Vec<String>,       // deduplicated SubagentStart agent names in window
    tool_distribution: ToolDistribution,
    knowledge_served: u64,
    knowledge_stored: u64,
    gate_result: GateResult,
    gate_outcome_text: Option<String>,
    hotspot_ids: Vec<String>,  // F-01, F-02... refs; populated by formatter
}
```

`ToolDistribution` is a new struct with named counts (not a HashMap) for the four categories:
```
ToolDistribution {
    read: u64,
    execute: u64,
    write: u64,
    search: u64,
}
```

`GateResult` is a new enum:
```
enum GateResult {
    Pass,
    Fail,
    Rework,
    Unknown,
}
```

GateResult inference from `outcome` text (case-insensitive):
- Contains `"pass"` or `"success"` → `Pass`
- Contains `"fail"` or `"error"` → `Fail`
- Contains `"rework"` → `Rework`
- None or no keyword match → `Unknown`

All types derive `Debug, Clone, Serialize, Deserialize`. `GateResult` and `ToolDistribution`
get `#[serde(default)]` on all fields; PhaseStats fields are all required (no Option wrapping).

### Component 3: `PhaseStats` computation (handler step 10h)
**Crate**: `unimatrix-server/src/mcp/tools.rs`

Inserted after step 10g (phase narrative assembly) and before step 11 (audit). Uses the
already-computed `events: Vec<CycleEventRecord>` from step 10g and the already-loaded
`attributed: Vec<ObservationRecord>` from step 3.

**Algorithm:**

1. Extract phase windows from `events`. A window is a `(phase, pass_number, start_ts, Option<end_ts>)` tuple. Windows are derived by walking events in timestamp order:
   - `cycle_phase_end` with `next_phase` = X defines the END of the current phase and the
     transition to X. The window boundary is the `cycle_phase_end.timestamp`.
   - `cycle_start` defines the absolute start of the first window.
   - `cycle_stop` defines the absolute end of the last window.
   - If a phase name appears twice in the sequence, each occurrence is a separate pass:
     `pass_number` increments per occurrence.

2. For each window, convert boundaries to milliseconds using `cycle_ts_to_obs_millis()`
   (imported from `crates/unimatrix-server/src/services/observation.rs`). Filter `attributed`
   by `obs.ts >= window_start_ms && obs.ts < window_end_ms`.

3. From the filtered observations, compute:
   - `record_count`: `filtered.len()`
   - `agents`: collect all `SubagentStart` observations → extract agent name from `obs.tool` or
     `obs.input["tool_name"]` → deduplicate, preserve first-seen order
   - `tool_distribution`: count `obs.event_type` into `(read, execute, write, search)` buckets
     using the same category mapping as `compute_session_summaries`
   - `knowledge_served`: count `PreToolUse` observations where tool is
     `context_search` / `context_lookup` / `context_get`
   - `knowledge_stored`: count `PreToolUse` observations where tool is `context_store`

4. Look up `gate_result` and `gate_outcome_text` from the `cycle_phase_end` event for this
   window (the event whose `phase` field matches this window's phase name at its end boundary).
   `outcome` column on that event → infer `GateResult`.

5. Compute `session_count`: count distinct `session_id` values across filtered observations.

6. `hotspot_ids` is left empty at computation time; the formatter populates it.

**Error boundary**: wrap the entire PhaseStats computation in a `match (|| async { ... })().await`
block. On any error, `tracing::warn!` and leave `report.phase_stats = None`. Same pattern as
steps 11–17 in the existing handler.

**SR-01 compliance**: The only permitted conversion from cycle_events seconds to observation
millis is through `cycle_ts_to_obs_millis()`. No inline `* 1000` multiplication anywhere in
PhaseStats computation code.

### Component 4: `FeatureKnowledgeReuse` extension
**Crate**: `unimatrix-observe/src/types.rs` (struct definition)
**Crate**: `unimatrix-server/src/mcp/knowledge_reuse.rs` (computation)

New fields on existing struct (backward-compatible via `#[serde(default)]`):

```
cross_feature_reuse: u64,                        // entries from prior feature cycles
intra_cycle_reuse: u64,                          // entries stored during this cycle
total_stored: u64,                               // entries created during this cycle
top_cross_feature_entries: Vec<EntryRef>,        // top-N by serve_count, cross-feature only
```

`EntryRef` is a new named struct (not a tuple alias):
```
EntryRef {
    id: u64,
    title: String,
    feature_cycle: String,     // source feature
    category: String,
    serve_count: u64,
}
```

`category_gaps` field stays on the struct (no breaking change). The formatter stops rendering
it. JSON consumers who read it see unchanged data.

**SR-02 compliance — batch query design:**

The `compute_knowledge_reuse` function in `knowledge_reuse.rs` is extended to accept a second
closure: `entry_meta_lookup: impl Fn(&[u64]) -> HashMap<u64, EntryMeta>` where `EntryMeta`
holds `(title, feature_cycle, category)`. This closure is called ONCE per invocation with the
full set of all distinct entry IDs collected in steps 1–4. The caller in `tools.rs` implements
the closure by executing a single SQL IN-clause query:

```sql
SELECT id, title, category, feature_cycle
FROM entries
WHERE id IN (?, ?, ...)
AND status != 'quarantined'
```

Chunking strategy: if the entry ID set exceeds 100 entries, split into chunks of 100 and union
the results (same pattern as Chunked Batch Scan #883). The function signature remains pure
(closure hides the store); tests supply a synthetic HashMap.

`total_stored` is computed by the caller in `tools.rs` (already has access to
`feature_entries` via existing query in step 10g). The count of rows in `feature_entries` where
`feature_id = current_cycle` is `total_stored`. This avoids adding another DB query to
`compute_knowledge_reuse`.

### Component 5: Formatter overhaul
**Crate**: `unimatrix-server/src/mcp/response/retrospective.rs`

#### New Section Order

```
1. Header (rebranded + goal/cycle_type/attribution/status)
2. Recommendations (moved from position 9 to position 2)
3. Phase Timeline (new — from phase_stats)
4. What Went Well (new — from baseline_comparison non-outlier positives)
5. Findings (existing — phase annotation added, burst notation)
6. Baseline Outliers (existing — universal, unchanged)
7. Phase Outliers (existing — unchanged)
8. Knowledge Reuse (extended — new fields, category_gaps suppressed)
9. Rework & Context Reload (existing — retained)
10. Phase Narrative (existing — retained at end)
```

Sessions table is retained but enhanced with tool distribution and agents columns.

#### Header (section 1)

Format:
```
# Unimatrix Cycle Review — {feature_cycle}

**Goal**: {goal_text | "(no goal recorded)"}
**Cycle type**: {Design|Delivery|Bugfix|Refactor|Unknown}  |  **Attribution**: {path_label}  |  **Status**: {COMPLETE|IN PROGRESS}
**Sessions**: N  |  **Records**: N  |  **Duration**: Xh Ym  |  **Outcome**: {top outcome from session_summaries}

---
```

Goal line is omitted entirely (not blank) when `report.goal.is_none()`.
`is_in_progress` = `Some(true)` → Status: `IN PROGRESS`. `Some(false)` → `COMPLETE`. `None` → omit Status line.

Attribution path labels:
- `cycle_events-first (primary)` — Path 1
- `sessions.feature_cycle (legacy)` — Path 2
- `content-scan (fallback)` — Path 3

#### Phase Timeline (section 3)

When `phase_stats` is `None` or empty: emit a single line `No phase information captured.` (no
section header).

When present:
```
## Phase Timeline

| Phase | Duration | Passes | Records | Agents | Knowledge | Gate |
|-------|----------|--------|---------|--------|-----------|------|
| scope | 0h 42m | 1 | 73 | researcher | 3↓ 0↑ | PASS |
```

Knowledge column format: `{served}↓ {stored}↑` (down-arrow = served to agents, up-arrow =
stored by agents).

For reworked phases (`pass_count > 1`), emit a footnote line below the table for each such
phase:
```
**Rework**: {phase} — pass 1 {GateResult}: {gate_outcome_text}. Pass 2: {Xh Ym}, {N} agents, {N} records.
```

Hotspot annotations in Phase Timeline: after computing `phase_stats`, the formatter looks up
each `CollapsedFinding` and determines which phase window its earliest evidence timestamp falls
into. It writes the finding ID (e.g., `F-01`) into `phase_stats[i].hotspot_ids`.

#### What Went Well (section 4)

Metric direction table (determines "favorable"):

| Metric | Direction | Favorable when |
|--------|-----------|----------------|
| `parallel_call_rate` | higher is better | current > mean |
| `bash_for_search_count` | lower is better | current < mean |
| `permission_friction_events` | lower is better | current < mean |
| `post_completion_work_pct` | lower is better | current < mean |
| `coordinator_respawn_count` | lower is better | current < mean |
| `sleep_workaround_count` | lower is better | current < mean |
| `follow_up_issues_created` | higher is better | current > mean |
| `context_reload_pct` | lower is better | current < mean |
| `reread_rate` | lower is better | current < mean |
| `compile_cycles` | lower is better | current < mean |

Only metrics with `status == Normal` (not Outlier/NewSignal/NoVariance) and at least 3 sample
points in the baseline (`stddev > 0` or `sample_count >= 3`) are candidates. Section is omitted
entirely when no candidates qualify.

Format per entry:
```
- **{metric_name}**: {current:.1} vs mean {mean:.1} — {plain description} ✓
```

Plain descriptions are hardcoded in the formatter per metric name (same approach as
`recommendations_for_hotspots`).

#### Findings with burst notation and phase annotation (section 5)

Each finding header gets a `phase: {phase_name}` annotation derived from the phase_stats
hotspot_ids mapping:
```
### F-01 [warning] compile_cycles — phase: implementation/1
```

Evidence rendering replaces the current `ts=` format with burst notation:

```
Timeline: +0m(N) +12m(N) +28m(N) ...  [max 10 entries; trailing "..." when truncated]
Peak: N events in Xmin at +Ym — {top 3 file names from peak cluster evidence}
```

Relative time origin: the earliest evidence timestamp across all evidence in the finding becomes
`+0m`. Each subsequent cluster is shown as `+Nm` from that origin. Cluster grouping reuses
`HotspotNarrative.clusters` (already computed in step 10e). If `narratives` is absent, fall
back to showing up to 3 raw evidence items with relative times only (no peak line).

Maximum burst entries before truncation: **10**. When truncated, append ` ... +Nm(N)` where N
is the last cluster timestamp to preserve the tail.

Threshold language replacement: any `claim` string (already produced by detection rules)
that contains a numeric threshold value is post-processed in the formatter before display.
Replacement logic:
- If `baseline_comparison` contains an entry for the same metric name with `stddev > 0`:
  append `(baseline: {mean:.1} ±{stddev:.1}, +{zscore:.1}σ)` after the existing claim
- If no baseline entry or `stddev == 0`: append `({N}× typical)` where N = measured/threshold
  rounded to 1 decimal
- The `threshold:` substring itself (with numeric value) is stripped from the claim before
  appending the baseline framing

Threshold language audit (enumerated per SR-05):
- `detection/agent.rs` line 71: `context_load_before_first_write_kb` claim — strip threshold, append baseline framing
- `detection/agent.rs` line 136: `lifespan` claim — strip threshold, append baseline framing
- `detection/agent.rs` line 217: `file_breadth` claim — strip threshold, append baseline framing
- `detection/agent.rs` line 282: `reread_rate` claim — strip threshold, append baseline framing
- `detection/agent.rs` line 342: `mutation_spread` claim — strip threshold, append baseline framing
- `detection/agent.rs` line 413: `compile_cycles` claim — strip threshold, append baseline framing
- `detection/agent.rs` line 474: `edit_bloat` claim — strip threshold, append baseline framing
- `detection/scope.rs` line 190: `adr_count` claim — strip threshold, append baseline framing
- `detection/friction.rs` line 68: `permission_retries` claim — strip threshold, append baseline framing
- `report.rs` line 62: `compile_cycles` recommendation text — replace with iterative-compilation language (AC-19)
- `report.rs` line 88: `compile_cycles` recommendation text — same fix (AC-19)
- `extraction/recurring_friction.rs` line 110, 143, 319: lesson-extraction templates — confirm independent from recommendations (read-only audit; no formatter change needed here since these go to lesson-learned entries, not markdown output)

AC-19 fix for `compile_cycles` recommendation: replace `"Add common build/test commands to settings.json allowlist"` with `"Batch field additions before compiling — repeated compile cycles suggest iterative per-field changes; complete struct definitions before first build"`. The `permission_friction_events` recommendation is confirmed separate (handled by `PermissionRetriesRule` path, not `compile_cycles`).

#### Knowledge Reuse (section 8)

```
## Knowledge Reuse

**Total served**: {delivery_count}  |  **Stored this cycle**: {total_stored}

| Bucket | Count |
|--------|-------|
| Cross-feature (prior cycles) | {cross_feature_reuse} |
| Intra-cycle ({feature_cycle} entries) | {intra_cycle_reuse} |

**By category (all {delivery_count} served)**: {category}×{count}, ...

**Top cross-feature entries**:

| Entry | Type | Served | Source |
|-------|------|--------|--------|
| `#{id}` {title} | {category} | {serve_count}× | {feature_cycle} |
```

Top N: show up to 5 entries sorted by `serve_count` descending.
`category_gaps` not rendered.
`cross_session_count` not rendered (superseded by the bucket split).

When `top_cross_feature_entries` is empty (no cross-feature reuse): omit the Top cross-feature
entries table. When `delivery_count` is 0: emit `No knowledge entries served.` in place of the
table.

## Component Interactions

```
context_cycle_review handler (tools.rs)
  │
  ├── Step 3: load_cycle_observations / fallback → attributed: Vec<ObservationRecord>
  │           records attribution_path: Option<String>
  │
  ├── Step 6+: early-exit paths (cached / no-data) — NEW: get_cycle_start_goal called here too
  │
  ├── Step 10c: build_report() → RetrospectiveReport (existing)
  │
  ├── Step 10g: cycle_events SQL query → events: Vec<CycleEventRecord>
  │             build_phase_narrative() → report.phase_narrative (existing)
  │
  ├── Step 10h (NEW): compute_phase_stats(events, attributed, cycle_ts_to_obs_millis)
  │                   → report.phase_stats: Option<Vec<PhaseStats>>
  │
  ├── Step 10i (NEW): get_cycle_start_goal(cycle_id) → report.goal, report.cycle_type
  │                   derive is_in_progress from events → report.is_in_progress
  │                   record attribution_path → report.attribution_path
  │
  ├── Steps 13-14: compute_knowledge_reuse_for_sessions (extended)
  │               batch entry meta lookup (new IN-clause query)
  │               → report.feature_knowledge_reuse (with new fields)
  │
  └── Step 12: format dispatch
        ├── format_retrospective_markdown(&report) — new section order + new sections
        └── format_retrospective_report(&report)   — JSON, gains new struct fields
```

## Technology Decisions

See individual ADR files:
- ADR-001: `is_in_progress` as `Option<bool>` (not `bool`)
- ADR-002: `cycle_ts_to_obs_millis` as named mandatory dependency for PhaseStats
- ADR-003: Batch IN-clause for cross-feature entry metadata lookup
- ADR-004: Formatter-only post-processing for threshold language replacement
- ADR-005: compile_cycles recommendation text correction

## Integration Points

### col-024 surface assumed (SR-07)

| Symbol | Location | Signature |
|--------|----------|-----------|
| `cycle_ts_to_obs_millis` | `crates/unimatrix-server/src/services/observation.rs` line 495 | `fn cycle_ts_to_obs_millis(ts_secs: i64) -> i64` |
| `load_cycle_observations` | `SqlObservationSource` | returns `Ok(Vec<ObservationRecord>)` via `ObservationSource` trait |
| `CycleEventRecord` | `unimatrix-observe/src/types.rs` line 231 | fields: `seq, event_type, phase, outcome, next_phase, timestamp` |

Note: `cycle_ts_to_obs_millis` is currently `fn` (not `pub`). It must be made `pub(crate)` or
re-exported for use by the PhaseStats computation step. This is a one-line visibility change;
no interface change.

### col-025 surface assumed (SR-07)

| Symbol | Location | Signature |
|--------|----------|-----------|
| `get_cycle_start_goal` | `crates/unimatrix-store/src/db.rs` line 354 | `pub async fn get_cycle_start_goal(&self, cycle_id: &str) -> Result<Option<String>>` |
| `cycle_events.goal` column | schema v16 | `TEXT NULL` on `cycle_start` rows only |

### Internal integration surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `compute_knowledge_reuse` | `fn(query_logs, injection_logs, active_cats, entry_category_lookup, entry_meta_lookup) -> FeatureKnowledgeReuse` | `knowledge_reuse.rs` |
| `compute_phase_stats` | `fn(events: &[CycleEventRecord], attributed: &[ObservationRecord]) -> Vec<PhaseStats>` | new, `mcp/tools.rs` or extract to `knowledge_reuse.rs` |
| `format_retrospective_markdown` | `fn(report: &RetrospectiveReport) -> CallToolResult` | `response/retrospective.rs` |
| `EntryRef` | struct `{ id: u64, title: String, feature_cycle: String, category: String, serve_count: u64 }` | `unimatrix-observe/src/types.rs` |
| `PhaseStats` | struct (see Component 2) | `unimatrix-observe/src/types.rs` |
| `GateResult` | enum `{ Pass, Fail, Rework, Unknown }` | `unimatrix-observe/src/types.rs` |
| `ToolDistribution` | struct `{ read, execute, write, search: u64 }` | `unimatrix-observe/src/types.rs` |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `RetrospectiveReport.goal` | `Option<String>` | `unimatrix-observe/src/types.rs` |
| `RetrospectiveReport.cycle_type` | `Option<String>` | `unimatrix-observe/src/types.rs` |
| `RetrospectiveReport.attribution_path` | `Option<String>` | `unimatrix-observe/src/types.rs` |
| `RetrospectiveReport.is_in_progress` | `Option<bool>` | `unimatrix-observe/src/types.rs` |
| `RetrospectiveReport.phase_stats` | `Option<Vec<PhaseStats>>` | `unimatrix-observe/src/types.rs` |
| `FeatureKnowledgeReuse.cross_feature_reuse` | `u64` `#[serde(default)]` | `unimatrix-observe/src/types.rs` |
| `FeatureKnowledgeReuse.intra_cycle_reuse` | `u64` `#[serde(default)]` | `unimatrix-observe/src/types.rs` |
| `FeatureKnowledgeReuse.total_stored` | `u64` `#[serde(default)]` | `unimatrix-observe/src/types.rs` |
| `FeatureKnowledgeReuse.top_cross_feature_entries` | `Vec<EntryRef>` `#[serde(default)]` | `unimatrix-observe/src/types.rs` |
| `cycle_ts_to_obs_millis` | `pub(crate) fn(i64) -> i64` | `services/observation.rs` (visibility change) |
| `compute_knowledge_reuse` (extended) | new second closure param | `knowledge_reuse.rs` |
| `format_retrospective_markdown` | unchanged signature, new rendering | `response/retrospective.rs` |

## FeatureKnowledgeReuse Construction Sites

`FeatureKnowledgeReuse {}` struct literal appears in:
- `unimatrix-observe/src/types.rs` (test fixtures only — update required)
- `unimatrix-server/src/mcp/knowledge_reuse.rs` (production — update required)
- `unimatrix-server/src/mcp/response/retrospective.rs` (test fixtures — update required)

`FeatureKnowledgeReuse` is NOT `#[non_exhaustive]` currently. Adding new `#[serde(default)]`
fields does not break Rust construction at existing sites — Rust struct literal exhaustiveness
is a compile-time error for missing fields. All construction sites must add the new fields.
This is a compile-time-enforced migration, not a hidden runtime issue (SR-08: confirmed safe).

## SR-06: "No Phase Information Captured" Detection Granularity

Decision: use the simple check — if `cycle_events` returns zero rows for this specific
`cycle_id`, show the note. Do NOT cross-check whether other cycles have events. The note is
accurate: no phase information was captured for this particular cycle. This is the same
behavior as `phase_narrative = None`.
