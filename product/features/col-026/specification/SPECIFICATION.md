# SPECIFICATION: col-026 — Unimatrix Cycle Review Enhancement

## Objective

`context_cycle_review` is the primary retrospective tool used after every feature cycle, but it consistently under-reports what actually happened: the feature goal and attribution path are invisible, the phase timeline is absent, knowledge reuse is undercounted by ignoring cross-feature entries (GH#320), threshold language implies configured rules that do not exist (GH#203), and positive signals from `baseline_comparison` are silently discarded. This feature enhances the tool across three implementation layers — formatter-only changes, `RetrospectiveReport` struct extensions, and new `PhaseStats` type with knowledge reuse fix — to produce a report that is both more accurate and more actionable.

---

## Functional Requirements

### FR-01 — Report Header Branding

The markdown report header line must read:

```
# Unimatrix Cycle Review — {feature_cycle}
```

The string `# Retrospective:` must not appear in any rendered markdown output from `format_retrospective_markdown`. The MCP tool name `context_cycle_review` is unchanged.

### FR-02 — Goal Line in Header

When `RetrospectiveReport.goal` is `Some(text)`, the header block must include a line:

```
Goal: {text}
```

When `goal` is `None`, the Goal line is entirely absent — no blank line, no placeholder. The `goal` field is populated by calling `get_cycle_start_goal(cycle_id)` in the `context_cycle_review` handler (a single async DB read added to the existing step sequence). The field is stored on `RetrospectiveReport` as `goal: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`.

### FR-03 — Cycle Type Classification

When `goal` is `Some`, `cycle_type` is inferred from the goal string using keyword matching (case-insensitive substring match against the full goal text):

| Keyword(s) present in goal | Cycle type string |
|---|---|
| `design`, `research`, `scope`, `spec` | `"Design"` |
| `implement`, `deliver`, `build` | `"Delivery"` |
| `fix`, `bug`, `regression`, `hotfix` | `"Bugfix"` |
| `refactor`, `cleanup`, `simplify` | `"Refactor"` |
| None of the above match | `"Unknown"` |

When goal is `None`, `cycle_type` is `"Unknown"`.

Keyword evaluation is first-match on the list order above (Design checked before Delivery, etc.). The cycle type is stored on `RetrospectiveReport` as `cycle_type: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`.

The header block must include:

```
Cycle type: {cycle_type}
```

### FR-04 — Attribution Path in Header

The handler tracks which attribution path produced non-empty observations for the current invocation:

| Path taken | `attribution_path` string |
|---|---|
| `load_cycle_observations` returned non-empty | `"cycle_events-first (primary)"` |
| `load_feature_observations` returned non-empty | `"sessions.feature_cycle (legacy)"` |
| `load_unattributed_sessions` used | `"content-scan (fallback)"` |

The value is stored on `RetrospectiveReport` as `attribution_path: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`.

The header block must include:

```
Attribution: {attribution_path}
```

When `attribution_path` is `None` (pre-col-024 data or cached report), the Attribution line is omitted.

### FR-05 — In-Progress Status Indicator

`RetrospectiveReport` gains the field:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub is_in_progress: Option<bool>,
```

Semantics:
- `None` — no `cycle_events` rows exist for this cycle (pre-col-024 data; open/closed status unknown)
- `Some(true)` — `cycle_events` contains a `cycle_start` row but no `cycle_stop` row for this cycle
- `Some(false)` — a `cycle_stop` row is confirmed present

This is derived from the already-loaded `CycleEventRecord` slice in the handler. `is_in_progress` must never be a plain `bool` with `#[serde(default)]` — the three-state semantics are load-bearing for historical retros (pre-col-024 cycles have no `cycle_events` rows and must not be reported as confirmed-complete).

When `is_in_progress == Some(true)`, the header block must include:

```
Status: IN PROGRESS
```

When `Some(false)` or `None`, no Status line appears.

### FR-06 — Phase Timeline Section

When `phase_stats` is `Some` and non-empty, the report contains a Phase Timeline section immediately after the header block (before Sessions), rendering as:

```
## Phase Timeline

| Phase | Duration | Passes | Records | Agents | Knowledge | Gate |
|---|---|---|---|---|---|---|
| {name} | {Xh Ym} | {n} | {n} | {agent_list} | {n} served, {n} stored | {gate} |
```

Column definitions:
- **Phase**: the `phase` string from `PhaseStats.phase`
- **Duration**: formatted as `Xh Ym` (omit hours if zero, omit minutes if zero) from `PhaseStats.duration_secs`
- **Passes**: `PhaseStats.pass_count`; when `pass_count > 1` the value is rendered bold (`**2**`)
- **Records**: `PhaseStats.record_count`
- **Agents**: comma-separated distinct agent names from `PhaseStats.agents`; if empty, `—`
- **Knowledge**: `{knowledge_served} served, {knowledge_stored} stored`
- **Gate**: `PhaseStats.gate_result` rendered as one of: `PASS`, `FAIL`, `PASS (rework)`, `UNKNOWN`

When `phase_stats` is `None` or empty, the Phase Timeline section is replaced by a single line (no section header):

```
No phase information captured.
```

Phase Timeline computation is best-effort: on any DB or processing error, the handler logs at `warn!` level and sets `phase_stats = None`. The report continues normally.

### FR-07 — Rework Annotation Below Phase Timeline Table

For each phase in `phase_stats` where `pass_count > 1`, append a line below the Phase Timeline table (not a new section header):

```
**Rework**: {phase_name} — pass 1 gate fail: {gate_outcome_text}. Pass 2: {Xh Ym}, {n} records.
```

Where `gate_outcome_text` is the `outcome` string from the `cycle_phase_end` event that ended pass 1 of the reworked phase. Each reworked phase gets one annotation line.

### FR-08 — Top File Zones Below Phase Timeline

Below the Phase Timeline table (and any rework annotations), include:

```
Top file zones: {dir1} ({n}), {dir2} ({n}), {dir3} ({n})
```

This is the top 3–5 directory paths by total touch count aggregated across all `SessionSummary.top_file_zones` entries. "Touch count" is the integer count value from each `(path, count)` pair. Directories are sorted descending by total count; truncate to 5 entries.

This line also appears in the Sessions section (AC-15) — both locations are required.

### FR-09 — Per-Finding Phase Annotation

Each finding header in the Findings section includes a phase annotation when `phase_stats` is available and the finding's `rule_name` can be mapped to a phase:

```
### F-01 [warning] compile_cycles — phase: implementation/1
```

The annotation format is `— phase: {phase_name}/{pass_number}`. When the finding fired across multiple phases, use the phase with the highest event count for the annotation. When `phase_stats` is unavailable or no phase mapping exists for a finding, the annotation is omitted and the header reverts to the existing format:

```
### F-01 [warning] compile_cycles
```

Phase mapping: `PhaseStats.hotspot_ids` contains the IDs (F-01, F-02, etc.) of findings associated with that phase; the formatter uses this inverse map.

### FR-10 — Relative Burst Notation for Finding Evidence

The current `Examples:\n- {description} at ts={ts}` block is replaced with:

```
Timeline: +0m(N) +18m(N) +45m(N▲) ...
Peak: N events in Xmin at +Ym — file1.rs, file2.rs
```

Computation rules:
- `relative_ts_min = (evidence.ts - session_started_at) / 60_000` rounded to nearest minute
- Group evidence records into 5-minute buckets (i.e., records within the same 5-minute window are one cluster entry)
- Render up to 10 cluster entries in the Timeline line; if more than 10 clusters exist, truncate with `...` after entry 10
- The peak cluster (highest `event_count`) is marked with `▲` appended to its count in parentheses: `+45m(6▲)`
- The `Peak:` line shows: event count, window duration in minutes (fixed at 5min unless the cluster spans a boundary), relative start offset, and top 2–3 files from evidence within that cluster (by occurrence count)
- `session_started_at` is taken from the earliest `started_at` across all `SessionSummary` entries (cycle start epoch)

Raw `ts=` epoch values must not appear in any rendered finding output.

### FR-11 — What Went Well Section

A "What Went Well" section appears in the report when at least one metric in `baseline_comparison` satisfies:
- `is_outlier == false`
- `status != BaselineStatus::Outlier` and `status != BaselineStatus::NewSignal`
- The metric value is favorable per the metric direction table below

**Metric direction table** (lower value is better unless marked "higher"):

| Metric name | Favorable direction |
|---|---|
| `compile_cycles` | lower |
| `permission_friction_events` | lower |
| `bash_for_search_count` | lower |
| `reread_rate` | lower |
| `coordinator_respawn_count` | lower |
| `sleep_workaround_count` | lower |
| `post_completion_work_pct` | lower |
| `context_load_before_first_write_kb` | lower |
| `file_breadth` | lower |
| `mutation_spread` | lower |
| `cold_restart_count` | lower |
| `task_rework_count` | lower |
| `edit_bloat_kb` | lower |
| `parallel_call_rate` | higher |
| `knowledge_entries_stored` | higher |
| `follow_up_issues_created` | higher |

A metric is "favorable" when:
- Direction is "lower" and `current_value < mean`
- Direction is "higher" and `current_value > mean`

Metrics not in this table are excluded from "What Went Well" consideration.

Render format:

```
## What Went Well
- **{metric_name}**: {current_value} vs mean {mean} — {plain-text label}
```

Plain-text labels are hardcoded per metric (not generated). Examples from SAMPLE-REVIEW.md: "above-average concurrency across all sessions", "Grep/Glob used correctly throughout", "low friction outside compile bursts", "clean stop after gate 3c", "no SM context loss", "no polling hacks".

When no favorable signals exist, the section is omitted entirely. The section appears between Phase Timeline and Findings in the section order.

### FR-12 — Recommendations Section Position

The Recommendations section must appear immediately after the header block and before the Phase Timeline / Sessions section. This is a section order change from the current position (position 9 of 10).

New section order:
1. Header (FR-01 through FR-05)
2. Recommendations (moved from position 9)
3. Phase Timeline (FR-06/07/08) or "No phase information captured." line
4. What Went Well (FR-11)
5. Sessions table (existing, enhanced per FR-15)
6. Attribution note (existing, when partial)
7. Baseline Outliers (existing)
8. Findings (enhanced per FR-09/FR-10)
9. Phase Outliers (existing)
10. Knowledge Reuse (enhanced per FR-13)
11. Rework & context reload (existing)
12. Phase Narrative (existing, crt-025)

### FR-13 — Knowledge Reuse Section Enhancement

The `FeatureKnowledgeReuse` struct gains the following fields (all additive, backward-compatible):

```rust
/// All distinct entry IDs served across all sessions for this cycle.
#[serde(default)]
pub total_served: u64,

/// Total entries stored (created) in Unimatrix during this cycle.
/// Derived from feature_entries WHERE feature_id = current_cycle.
#[serde(default)]
pub total_stored: u64,

/// Entries whose stored `feature_cycle` != current cycle ID (from prior cycles).
#[serde(default)]
pub cross_feature_reuse: u64,

/// Entries whose stored `feature_cycle` == current cycle ID (stored in this cycle).
#[serde(default)]
pub intra_cycle_reuse: u64,

/// Top 3–5 cross-feature entries by serve count.
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub top_cross_feature_entries: Vec<EntryRef>,
```

Existing fields `delivery_count`, `cross_session_count`, `by_category`, and `category_gaps` are retained on the struct unchanged (no breaking change). `category_gaps` is no longer rendered in markdown output.

`by_category` semantics: count of ALL served entries by category (not restricted to intra-cycle).

`top_cross_feature_entries` contains top 3–5 entries by serve count where the entry's stored `feature_cycle` differs from the current cycle ID. Minimum 3, maximum 5.

Cross-feature `feature_cycle` lookup uses a single batch IN-clause query against the `entries` table (not N individual `get()` calls). Maximum added latency for a batch of ≤100 served entries: 50ms. Reference pattern #883 (Chunked Batch Scan) if entry count exceeds 100.

Rendered Knowledge Reuse section format:

```
## Knowledge Reuse

**Total served**: {N}  |  **Stored this cycle**: {M}

| Bucket | Count |
|---|---|
| Cross-feature (prior cycles) | {cross_feature_reuse} |
| Intra-cycle ({feature_cycle} entries) | {intra_cycle_reuse} |

**By category (all {N} served)**: {cat}×{n}, {cat}×{n}, ...

**Top cross-feature entries**:

| Entry | Type | Served | Source |
|---|---|---|---|
| `#{id}` {title} | {category} | {n}× | {source_cycle} |
```

The `category_gaps` field is not rendered. "Top cross-feature entries" table is omitted when `top_cross_feature_entries` is empty.

### FR-14 — Threshold Language Elimination

All user-facing text rendered by the formatter must not contain the word "threshold" paired with a numeric value. The `threshold` field may remain on `HotspotFinding` for internal detection use but must not appear in rendered output.

**Replacement rules for rendered claim strings and recommendation rationale:**

When `baseline_comparison` data is present for the metric:
```
(threshold: N unit) → (baseline: mean {mean} ±{stddev}, +{Z}σ)
```

When no baseline data is available for the metric:
```
(threshold: N unit) → ({N}× typical)
```

**compile_cycles recommendation text (AC-19):** The `compile_cycles` recommendation action and rationale must describe repeated compilation errors — specifically that iterative per-field changes or unresolved type errors trigger recompile loops. Neither "allowlist" nor any reference to permission prompts may appear. The `permission_retries` and `compile_cycles` recommendation templates must be confirmed independent with no text shared between them.

**allowlist language:** The word "allowlist" must not appear in any `compile_cycles` finding claim, recommendation action, or recommendation rationale. It may only appear in `permission_retries` context.

**Threshold audit scope — files that must be modified to meet AC-13:**

The following files contain "threshold" in user-facing rendered strings and are in scope for AC-13:

| File | Current user-facing strings with "threshold" |
|---|---|
| `crates/unimatrix-observe/src/report.rs` line 88 | `"(threshold: 10) -- consider narrowing test scope"` in compile_cycles rationale |
| `crates/unimatrix-observe/src/detection/agent.rs` line 474 | `"(threshold {EDIT_BLOAT_THRESHOLD_KB} KB)"` in claim string for `edit_bloat` |
| `crates/unimatrix-server/src/mcp/response/retrospective.rs` | Any `ts=` epoch values in examples (replaced by FR-10) |

Detection constant names (`CONTEXT_LOAD_THRESHOLD_KB`, `LIFESPAN_THRESHOLD_MINS`, etc.) in Rust source are internal and not user-facing; they are not in scope for AC-13.

### FR-15 — Session Profile Table Enhancement

The sessions table gains two new columns: **Tools** and **Agents**.

Updated table header:

```
| # | Window | Calls | Tools | Agents | Knowledge | Outcome |
```

**Tools column**: formatted as `{N}R {N}E {N}W {N}S` where R=read, E=execute, W=write, S=search. Values drawn from `SessionSummary.tool_distribution` using these key mappings: `read`→R, `execute`→E, `write`→W, `search`→S. Omit a letter if the count is zero. Example: `218R 59E 41W`.

**Agents column**: comma-separated list from `SessionSummary.agents_spawned`. If empty, render `—`. Truncate to first 3 agents with `+N more` if more than 3 are present.

### FR-16 — Top File Zones in Sessions Section

The Sessions section (after the sessions table) includes a Top file zones line:

```
Top file zones: {dir1} ({n}), {dir2} ({n}), ...
```

Aggregation: sum touch counts across all `SessionSummary.top_file_zones` entries, group by directory path, sort descending by total count, take top 3–5. This is the same computation as FR-08; the line appears in both locations (Phase Timeline section and Sessions section).

### FR-17 — JSON Backward Compatibility for New RetrospectiveReport Fields

All new fields on `RetrospectiveReport` use:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
```

for `Option<T>` fields, or:

```rust
#[serde(default, skip_serializing_if = "Vec::is_empty")]
```

for `Vec<T>` fields.

All new fields on `FeatureKnowledgeReuse` use `#[serde(default)]`.

A pre-col-026 JSON payload (lacking `goal`, `cycle_type`, `attribution_path`, `is_in_progress`, `phase_stats`) must deserialize into `RetrospectiveReport` without error, with all new fields defaulting to `None` or empty.

### FR-18 — Test Preservation

All existing tests in `crates/unimatrix-server/src/mcp/response/retrospective.rs` test module and `crates/unimatrix-observe/src/types.rs` test module pass without modification. No test file is deleted or renamed. New behavior is covered by new test cases added to the existing test modules.

### FR-19 — compile_cycles Recommendation Independence

The `compile_cycles` recommendation template in `crates/unimatrix-observe/src/report.rs` is confirmed independent from `permission_retries`. Specifically:
- `compile_cycles` action text: describes iterative compilation due to per-field type errors or unresolved imports, recommends batching struct definition changes before first build
- `compile_cycles` rationale: references the measured compile count and session duration context; does not mention allowlists, permission prompts, or settings.json
- `permission_retries` action text: unchanged — references settings.json allowlist (appropriate for that rule only)
- Test `test_recommendation_compile_cycles_above_threshold` must assert `action` does not contain "allowlist"

---

## Non-Functional Requirements

### NFR-01 — Phase Timeline Computation: Best-Effort

Phase Timeline computation (the new handler step producing `PhaseStats`) must be treated as best-effort. Any DB error, observation-slice error, or type conversion failure must:
1. Log at `warn!` level with the error reason
2. Set `phase_stats = None` on the report
3. Allow the `context_cycle_review` response to complete normally

Phase Timeline computation must not be in a `spawn_blocking` hot-path call. Follow the same pattern as existing step 10g (phase narrative): raw SQL query on `write_pool_server()`, error-logged-and-skipped on failure.

### NFR-02 — Timestamp Unit Safety

All Phase Timeline observation slicing must use `cycle_ts_to_obs_millis()` from col-024 for converting `cycle_events.timestamp` (epoch seconds) to observation-compatible epoch milliseconds. Direct multiplication by 1000 without using this function is prohibited; the architect must reference this function by name in the implementation brief.

### NFR-03 — Knowledge Reuse Batch Query

The cross-feature `feature_cycle` lookup for all served entries must use a single batch IN-clause SQL query (or chunked batches per pattern #883 for counts >100), not N individual `get()` calls. The added latency for the knowledge reuse computation must not exceed 50ms for a cycle with ≤100 served entries.

### NFR-04 — `is_in_progress` Type

`RetrospectiveReport.is_in_progress` must be `Option<bool>`, not `bool`. The three-state semantics are required: `None` (no cycle_events), `Some(true)` (in-progress), `Some(false)` (confirmed complete). A plain `bool` with `#[serde(default)]` incorrectly reports all pre-col-024 historical retros as confirmed-complete.

### NFR-05 — Section Order Stability

The new section order defined in FR-12 must be enforced by the formatter's `format_retrospective_markdown` function. The order must be documented as a numbered comment in the function body (following the existing `// N.` comment pattern).

### NFR-06 — No Schema Changes

No new database tables or columns are introduced in col-026. Schema remains at v16 (col-025). All new data is derived from existing `cycle_events`, `feature_entries`, `query_log`, `injection_log`, and `entries` tables.

### NFR-07 — Performance: In-Memory Phase Slicing

Phase Timeline observation slicing is an in-memory filter over the already-loaded `attributed` observation slice (already in RAM). No additional DB query is needed for the observation-side of phase computation. The only new DB calls in the handler are:
1. `get_cycle_start_goal(cycle_id)` — one read
2. Cross-feature batch IN-clause query for `feature_cycle` lookup (FR-13/NFR-03)

The `cycle_events` query is already present in the handler (step 10g).

---

## Acceptance Criteria

| AC-ID | Statement | Verification Method |
|---|---|---|
| AC-01 | The markdown report header reads `# Unimatrix Cycle Review — {feature_cycle}` (not `# Retrospective:`) | Formatter test: assert output starts with `# Unimatrix Cycle Review —`; assert `# Retrospective:` does not appear |
| AC-02 | When `cycle_events.goal` is non-null, header includes `Goal: {text}`; when null, the Goal line is entirely absent (not blank) | Two formatter tests: (a) `goal = Some("text")` → assert "Goal: text" present; (b) `goal = None` → assert "Goal" not in output |
| AC-03 | Cycle type inferred from goal keywords; `Unknown` when goal absent | Unit test of keyword inference function; integration test asserting header shows `Cycle type: Delivery` for delivery-keyword goal |
| AC-04 | Header shows correct Attribution string for each of three paths | Three formatter tests, one per path string value |
| AC-05 | `Status: IN PROGRESS` appears when `is_in_progress = Some(true)`; absent when `None` or `Some(false)` | Three formatter tests: one per `Option<bool>` value |
| AC-06 | Phase Timeline table present with correct columns when `phase_stats` is non-empty | Formatter test with synthetic `PhaseStats` slice; assert table header and one data row present |
| AC-07 | Rework annotation below table for any phase with `pass_count > 1` | Formatter test with one rework phase; assert "**Rework**:" annotation line present below table |
| AC-08 | Each finding header includes `— phase: {name}/{pass}` when phase data is available | Formatter test: set `phase_stats` with `hotspot_ids` mapping; assert finding header contains "— phase:" |
| AC-09 | Per-finding evidence rendered as `Timeline: +Xm(N)` with `Peak:` line; no `ts=` epoch values in output | Formatter test: finding with 5 evidence records; assert output contains "Timeline:" and "Peak:"; assert "ts=" absent |
| AC-10 | "What Went Well" appears with at least one favorable signal; absent when no favorable signals | Two formatter tests: (a) one favorable metric → section present; (b) all metrics unfavorable → section absent |
| AC-11 | Recommendations section appears before Phase Timeline in output | Formatter test: assert "## Recommendations" appears before "## Phase Timeline" in string |
| AC-12 | Knowledge Reuse shows total served, stored, cross-feature, intra-cycle, by-category, top cross-feature entries; category_gaps not rendered | Formatter test with populated `FeatureKnowledgeReuse`; assert all new fields rendered; assert "Gaps:" not in output |
| AC-13 | No "threshold: N" pattern in rendered output; "allowlist" absent from compile_cycles text | String-search test on formatter output for multiple finding types; dedicated test for compile_cycles recommendation |
| AC-14 | Sessions table includes Tools column (`NR NE NW NS`) and Agents column | Formatter test: session with tool_distribution and agents_spawned; assert table header and row contain tool abbreviations and agent names |
| AC-15 | "Top file zones:" line appears in Sessions section | Formatter test with sessions having `top_file_zones`; assert "Top file zones:" line present |
| AC-16 | JSON format response includes `goal`, `cycle_type`, `attribution_path`, `is_in_progress`, `phase_stats` on `RetrospectiveReport` | Serde roundtrip test; assert all five fields serialize/deserialize correctly; handler integration test for JSON path |
| AC-17 | All existing `context_cycle_review` tests pass without modification; no test file deleted | CI: cargo test passes; no test names disappear from output |
| AC-18 | Pre-col-026 JSON (missing new fields) deserializes into `RetrospectiveReport` without error; new FeatureKnowledgeReuse fields default correctly | Backward-compat deserialize test in types.rs test module |
| AC-19 | `compile_cycles` recommendation text describes compilation error patterns; "allowlist" absent; `compile_cycles` and `permission_retries` templates are independent | Unit test asserts compile_cycles action does not contain "allowlist"; second test asserts permission_retries action contains "allowlist"; both templates verified to share no text |

---

## Domain Models

### `PhaseStats` (new type in `unimatrix-observe/src/types.rs` or `phase_stats.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseStats {
    /// Phase name token (e.g., "implementation", "design").
    pub phase: String,
    /// 1-indexed pass number for this specific window (1 = first pass, 2 = rework pass).
    /// Used to render the finding annotation `— phase: implementation/1`.
    pub pass_number: u32,
    /// Total passes for this phase name across the whole cycle (>1 = rework occurred).
    pub pass_count: u32,
    /// Total duration across all passes of this phase, in seconds.
    pub duration_secs: u64,
    /// Total observation records in this phase's time windows.
    pub record_count: u64,
    /// Distinct agents spawned within this phase's time windows.
    pub agents: Vec<String>,
    /// Knowledge entries served within this phase's time windows.
    pub knowledge_served: u64,
    /// Knowledge entries stored within this phase's time windows.
    pub knowledge_stored: u64,
    /// Gate result for this phase (from cycle_phase_end.outcome).
    pub gate_result: GateResult,
    /// Finding IDs (e.g., "F-01") associated with this phase.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hotspot_ids: Vec<String>,
    /// Per-pass breakdown: (duration_secs, record_count) for each pass (index 0 = pass 1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pass_breakdown: Vec<(u64, u64)>,
}
```

### `GateResult` (new enum in same module)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateResult {
    Pass,
    Fail,
    Rework,  // pass_count > 1 and last pass is Pass
    Unknown,
}
```

Derivation: for a phase with `pass_count == 1`, check the `outcome` string from the `cycle_phase_end` event. If it parses as a success indicator (case-insensitive contains "pass" or "complete" or "approved"), result is `Pass`. If it contains "fail", result is `Fail`. Otherwise `Unknown`. For `pass_count > 1` where the final pass succeeded, result is `Rework`.

Rendered strings:
- `Pass` → `"PASS"`
- `Fail` → `"FAIL"`
- `Rework` → `"PASS (rework)"`
- `Unknown` → `"UNKNOWN"`

### `EntryRef` (new type for `top_cross_feature_entries`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryRef {
    /// Unimatrix entry ID.
    pub id: u64,
    /// Entry title.
    pub title: String,
    /// Entry category.
    pub category: String,
    /// Number of times this entry was served during the cycle.
    pub serve_count: u64,
    /// The feature_cycle string recorded on the entry (source cycle).
    pub source_cycle: String,
}
```

### `CycleType` (string constants, not a Rust enum — to avoid serde breakage on future additions)

Represented as `Option<String>` on `RetrospectiveReport`. Valid string values: `"Design"`, `"Delivery"`, `"Bugfix"`, `"Refactor"`, `"Unknown"`.

### `AttributionPath` (string constants, represented as `Option<String>`)

Valid string values: `"cycle_events-first (primary)"`, `"sessions.feature_cycle (legacy)"`, `"content-scan (fallback)"`.

### Ubiquitous Language

| Term | Definition |
|---|---|
| **Phase window** | Time interval `[event[n].timestamp, event[n+1].timestamp)` derived from adjacent `cycle_events` rows; authoritative time boundary for slicing observations into phases |
| **Pass** | One sequential entry into a named phase. A phase with `pass_count > 1` was entered again after a gate failure (rework). Pass 1 is index 0. |
| **Rework phase** | A phase where `pass_count > 1`; the phase was re-entered after a gate fail |
| **Cross-feature entry** | A knowledge entry whose `feature_cycle` column value differs from the current cycle being reviewed |
| **Intra-cycle entry** | A knowledge entry whose `feature_cycle` matches the current cycle being reviewed |
| **Burst cluster** | A group of evidence records whose timestamps fall within the same 5-minute window; the unit of aggregation for Timeline notation |
| **Favorable signal** | A metric in `baseline_comparison` where `is_outlier == false` and the value direction (lower or higher) indicates better-than-average performance |
| **Attribution path** | Which of three fallback paths (`cycle_events-first`, `sessions.feature_cycle`, `content-scan`) was used to collect observations for the cycle |
| **Total served** | The count of distinct entry IDs delivered to agents across all sessions of the cycle (union of all query_log / injection_log entry references) |

---

## User Workflows

### Workflow 1 — Agent runs post-cycle retrospective

1. Agent calls `context_cycle_review` with `cycle_id = "col-026"` after cycle completion
2. Handler loads `cycle_events` → derives `is_in_progress = Some(false)`, extracts phase windows
3. Handler calls `get_cycle_start_goal("col-026")` → `goal = Some("Enhance context_cycle_review...")`
4. Handler infers `cycle_type = "Delivery"` from goal keyword match
5. Handler uses `load_cycle_observations` (primary path) → `attribution_path = "cycle_events-first (primary)"`
6. Handler computes `PhaseStats` by slicing attributed observations into phase windows
7. Handler batch-queries `entries` table for `feature_cycle` of all served entry IDs → splits into `cross_feature_reuse` / `intra_cycle_reuse`
8. Formatter renders: Recommendations → Phase Timeline → What Went Well → Sessions → Findings with burst notation → Knowledge Reuse
9. Agent receives actionable markdown with goal, phase breakdown, favorable signals, and relative-time finding evidence

### Workflow 2 — Agent runs mid-cycle health check

1. Agent calls `context_cycle_review` with active `cycle_id` (no `cycle_stop` yet)
2. Handler derives `is_in_progress = Some(true)` (no `cycle_stop` row in `cycle_events`)
3. Report header includes `Status: IN PROGRESS`
4. Open-ended window capped at `unix_now_secs()` (col-024 ADR-005) collects all observations to date
5. Agent receives partial report usable for mid-cycle intervention

### Workflow 3 — Legacy cycle (pre-col-024)

1. Agent calls `context_cycle_review` on a historical cycle predating col-024
2. No `cycle_events` rows → `is_in_progress = None`, `phase_stats = None`, `goal = None`
3. Report shows "No phase information captured." (single line, no section header)
4. Report omits Goal, Cycle type, Attribution, and Status lines from header
5. Report renders all non-phase-dependent sections normally

---

## Constraints

### Hard Dependencies

- **col-024** (cycle_events-first observation lookup) must be merged before col-026 ships. AC-04, AC-05, AC-06, AC-07, AC-08 require its data infrastructure and `cycle_ts_to_obs_millis()` function.
- **col-025** (feature goal signal) must be merged before col-026 ships. AC-02, AC-03 require `cycle_events.goal` column (schema v16) and `get_cycle_start_goal()` DB method.
- col-026 implementation branch must be cut from `main` only after both col-024 and col-025 are merged.

### API Surface Assumed from col-024/025

The following function signatures and types are assumed stable at col-026 design time:

| Symbol | Source | Usage |
|---|---|---|
| `get_cycle_start_goal(cycle_id: &str) -> Result<Option<String>>` | col-025 DB method | Load goal in handler |
| `cycle_ts_to_obs_millis(ts: i64) -> u64` | col-024 utility | Phase window timestamp conversion |
| `CycleEventRecord { seq, event_type, phase, outcome, next_phase, timestamp }` | col-025 shipped | Phase window derivation |
| `load_cycle_observations(cycle_id)` | col-024 primary path | Attribution path detection |

If any of these surfaces change before col-024/025 merge, a spec amendment is required.

### Struct Compatibility

- `FeatureKnowledgeReuse` is a public `unimatrix-observe` crate type. New fields require `#[serde(default)]`. The `category_gaps` field stays on the struct; only its rendering is suppressed.
- No exhaustive construction of `FeatureKnowledgeReuse` should exist outside `unimatrix-observe` itself. If found, add `#[non_exhaustive]` or update all construction sites.
- `RetrospectiveReport` new fields: all `Option<T>` with `skip_serializing_if = "Option::is_none"`.

### Test Infrastructure

- Extend existing fixtures in `crates/unimatrix-server/src/mcp/response/retrospective.rs` test module and `crates/unimatrix-observe/src/` test modules.
- Do not create isolated test scaffolding. Do not create new test files.
- Phase Timeline computation tests must use the existing `infra-001` test infrastructure pattern (cycle_events seeded via UDS-only write path per #3040).

### "No phase information captured" Note

The note is shown when no `cycle_events` rows exist for the requested cycle. It is not conditioned on whether other cycles have `cycle_events` rows. The note is a single line (no section header). This matches the resolved open question in SCOPE.md §Open Questions item 3.

---

## Dependencies

### Crates and Internal Components

| Dependency | Type | Reason |
|---|---|---|
| `unimatrix-observe` | Internal crate | `PhaseStats`, `EntryRef`, `GateResult` types defined here |
| `unimatrix-store` | Internal crate | Batch IN-clause query for `feature_cycle` lookup; `write_pool_server()` for Phase Timeline query |
| `unimatrix-server/src/mcp/response/retrospective.rs` | Primary change surface | All formatter changes (FR-01 through FR-16) |
| `unimatrix-server/src/mcp/tools.rs` | Handler | New fields set in handler (FR-02, FR-03, FR-04, FR-05, FR-06) |
| `unimatrix-observe/src/report.rs` | Recommendation engine | compile_cycles and permission_retries template fix (FR-19/AC-19) |
| `unimatrix-observe/src/detection/agent.rs` | Detection | Threshold string in `edit_bloat` claim (AC-13 audit) |

### Unimatrix Knowledge Entries Referenced

| Entry | Usage |
|---|---|
| #3383 | cycle_events-first lookup algorithm reused for phase window extraction |
| #952 | ADR-003: all rendering logic stays in `response/retrospective.rs` |
| #3420 | `Option<bool>` pattern for `is_in_progress` |
| #3255 | `serde(default)` + `skip_serializing_if` pattern for wire-optional fields |
| #883 | Chunked Batch Scan for knowledge reuse batch query |

---

## NOT in Scope

The following are explicitly excluded to prevent scope creep:

- **Per-CycleType baseline comparison** — requires accumulation of typed-cycle retrospective history not yet available
- **Phase velocity trend** — same data accumulation requirement
- **Phase knowledge profile anomaly detection** — requires defining expected profiles per phase type
- **Rework phase per-pass diff** — showing before/after difference between pass 1 and pass 2; per-pass duration/records/agents are shown in the Phase Timeline table, full diff is deferred
- **Changing the MCP tool name** — `context_cycle_review` is unchanged; branding is header text only
- **Schema changes** — no new tables or columns; schema stays at v16
- **Goal-contextualized hotspot severity adjustment** — inferring CycleType and suppressing expected hotspots; goal is shown in header only, hotspot severity is unaffected
- **Session-level entries_analysis in markdown** — `entries_analysis` remains JSON-only
- **PreCompact hook content improvements** (GH#309) — separate surface
- **Markdown rendering of `category_gaps`** — field stays on struct for JSON compat; rendering removed only

---

## Knowledge Stewardship

Queried: /uni-query-patterns for retrospective formatter, cycle_events phase window, serde backward compat — found:
- #3383: cycle_events-first observation lookup algorithm (time windows) — reused for Phase Timeline
- #952: ADR-003 retrospective formatter module boundary — all rendering stays in `response/retrospective.rs`
- #3420: `Option<bool>` pattern for event-derived status fields — directly applicable to `is_in_progress`
- #3255: `serde(default)` + `skip_serializing_if` pairing — wire-optional field pattern
- #883: Chunked Batch Scan — mandated for cross-feature `feature_cycle` pre-fetch
