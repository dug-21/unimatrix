# Unimatrix Cycle Review — First-Class Report Enhancement

## Problem Statement

`context_cycle_review` produces a technically correct retrospective but it fails as a product. The
markdown output buries its most valuable signal (temporal shape of findings, phase-level performance),
omits data the JSON path already carries (agents spawned, tool distribution, file zones, positive
baseline signals), systematically undercounts knowledge reuse by ignoring cross-feature entries
(GH#320), and uses internal heuristic threshold values in user-facing language that implies
user-configured rules where none exist (GH#203 / §10.2).

Two upstream features have now shipped new capabilities that the retrospective tool cannot yet
express:

- **col-024** — cycle_events-first attribution engine with authoritative time windows and three-path
  fallback. The attribution path used is currently invisible in the report.
- **col-025** — feature goal stored durably in `cycle_events.goal`. The goal is never shown in the
  report, though it is the most important single fact about any cycle.

The result is a tool that agents and humans use on every completed feature but that consistently
under-reports what actually happened, mis-labels healthy work as "threshold violations," and hides
the structured evidence that would make retros actionable.

## Goals

1. Surface the feature goal, cycle type classification, and attribution path in the report header.
2. Produce a Phase Timeline table — the single highest-value new section — using `cycle_events`
   timestamps to break observations into per-phase windows with duration, agents, records, knowledge
   throughput, and gate outcome per phase.
3. Annotate each finding with the phase it fired in (per-phase hotspot scoping).
4. Fix the knowledge reuse metric (GH#320): split all served entries into cross-feature vs.
   intra-cycle buckets, show by-category breakdown and top cross-feature entries.
5. Add a "What Went Well" section from the existing `baseline_comparison` data (positive, non-outlier
   metrics are currently hidden).
6. Replace "threshold" language in all findings with baseline framing (`+3.4σ above mean`) or a
   descriptive ratio when no baseline exists.
7. Reformat per-finding evidence as relative-time burst notation with peak annotation (replacing
   raw `ts=` values).
8. Enhance the session profile section with tool distribution (R/E/W/Search abbreviations), agents
   spawned list, and top file zones.
9. Surface the in-progress indicator when no `cycle_stop` event exists (col-024 open-ended window).
10. Rebrand the report header from `# Retrospective:` to `# Unimatrix Cycle Review —`.
11. Investigate and resolve the permission-friction false-positive recommendation in skip-permissions
    mode (§10.1 of FINDINGS.md) — fix it here if it is a recommendation-framing bug, or file a
    separate bug issue if it is a detection-logic bug.

## Non-Goals

The following are explicitly deferred from col-026:

- **Per-CycleType baseline comparison** — requires accumulation of many typed-cycle retrospectives
  to produce statistically meaningful per-type baselines; that data does not yet exist.
- **Phase velocity trend** — same data accumulation requirement; this is a reporting view on top
  of data that must be gathered across many future cycles.
- **Phase knowledge profile anomaly detection** — identifying when a phase's knowledge consumption
  pattern deviates from expected type-specific patterns requires defining expected profiles per phase
  type, which is a separate design problem.
- **Rework phase per-pass diff** — showing the before/after difference between pass 1 and pass 2 of
  a reworked phase. Rework evidence (duration, agents, records per pass) is covered by the Phase
  Timeline table. The full diff is a higher-effort addition for a follow-on feature.
- **Changing the MCP tool name** — `context_cycle_review` remains unchanged for backward
  compatibility. Branding change is header text only.
- **Schema changes** — no new database tables or columns. All new data derives from existing
  `cycle_events`, `feature_entries`, `query_log`, `injection_log`, and `entries` tables.
- **Goal-contextualized hotspot severity adjustment** — inferring CycleType from the goal string and
  suppressing expected hotspots based on cycle type. This is goal classification logic deferred to a
  follow-on; in col-026 the goal is shown in the header without affecting hotspot severity.
- **Session-level entries_analysis / knowledge health section** — the `entries_analysis` drain
  (pending injection/flag counts per entry) is already rendered in JSON. Adding it to markdown in a
  non-trivial way requires design work on the presentation model; deferred.
- **PreCompact hook content improvements** (GH#309) — related but a separate surface.

## Background Research

### Codebase State

**Handler** (`crates/unimatrix-server/src/mcp/tools.rs`, `context_cycle_review` at line 1194):
- Three-path attribution logic already shipped (col-024): primary `load_cycle_observations` → legacy
  `load_feature_observations` → content-scan `load_unattributed_sessions`. Attribution path taken is
  logged at `debug` level but never surfaced in `RetrospectiveReport` or the markdown output.
- `get_cycle_start_goal(cycle_id)` DB method exists and is called in the UDS listener (line 553) but
  is NOT called anywhere in the `context_cycle_review` handler. Adding it is a single async call.
- `phase_narrative` (crt-025) already fetches and processes `cycle_events` rows via a raw SQL query
  in the handler (line 1590). The same query result contains all timestamps needed to construct
  phase time windows — no new DB query is required for the Phase Timeline.
- `is_in_progress` is implicit: if `cycle_events` has a `cycle_start` row but no `cycle_stop` row,
  the cycle is open. This is derivable from the already-loaded `events: Vec<CycleEventRecord>`.

**Formatter** (`crates/unimatrix-server/src/mcp/response/retrospective.rs`):
- 10-section rendering pipeline. Current section order: header → sessions → attribution note →
  baseline outliers → findings → phase outliers → knowledge reuse → rework/reload →
  recommendations → phase narrative.
- `render_header`: hardcoded `"# Retrospective: {}"` format (line 129). Single-line change.
- `render_findings`: renders raw `ts=` timestamps in `Examples:` blocks (line 238). This is the
  evidence rendering that needs burst-notation replacement.
- `render_knowledge_reuse`: renders only `delivery_count` and `cross_session_count` (lines 363–379).
  The `FeatureKnowledgeReuse` struct lacks `total_stored`, `cross_feature_reuse`, `intra_cycle_reuse`,
  and `top_cross_feature_entries` fields — these must be added.
- No "What Went Well" section exists. The `baseline_comparison` array is filtered to outliers only
  (lines 54–68, 79–96). Non-outlier positive signals are unused.
- `render_sessions`: renders sessions table (line 134) but does not include agents spawned or
  tool distribution abbreviations. The `SessionSummary` struct already carries both.

**`FeatureKnowledgeReuse` type** (`crates/unimatrix-observe/src/types.rs`, line 200):
- Current fields: `delivery_count`, `cross_session_count`, `by_category`, `category_gaps`.
- `delivery_count` counts ALL distinct entries delivered (not just same-cycle). The bug in GH#320
  is that there is no split between cross-feature entries and intra-cycle entries — `delivery_count`
  includes both but is labeled ambiguously. The by-cycle split requires knowing each entry's
  `feature_cycle` field, which is not currently passed to `compute_knowledge_reuse`.
- Fix path: add `cross_feature_reuse: u64`, `intra_cycle_reuse: u64`, and
  `top_cross_feature_entries: Vec<EntryRef>` to the struct. In `compute_knowledge_reuse_for_sessions`,
  pre-fetch each entry's `feature_cycle` alongside its `category`, then pass as a second lookup
  closure. The category_gaps field can be retired (see Non-Goals framing in FINDINGS §3.2).

**`HotspotFinding` type** (`crates/unimatrix-observe/src/types.rs`, line 47):
- Has `threshold: f64` field. This value is surfaced in some claim strings (e.g.,
  `CONTEXT_LOAD_THRESHOLD_KB`, `LIFESPAN_THRESHOLD_MINS` at 45.0 in `detection/agent.rs` line 84).
- Audit needed: every claim string and finding formatter that prints `threshold` must be identified
  and replaced. The `threshold` field can remain on the struct (used internally for detection) but
  must not appear in rendered output as a configured limit.

**Permission-friction detection** (`crates/unimatrix-observe/src/metrics.rs`, line 77):
- `permission_friction_events` metric = sum of `(PreToolUse - PostToolUse)` per tool, positive only.
  This is a tool-cancellation/abort count, not a literal permission-prompt count.
- The `PermissionRetriesRule` in `detection/friction.rs` (line 13) fires when `(pre - post) > 2`
  per tool. The claim string is: "Tool '{tool}' had {retries} permission retries (Pre-Post
  differential)" — which correctly says "retries" not "permission prompts".
- However, the `Recommendation` generated by `recommendations_for_hotspots` in `report.rs` (line 70)
  says "Review coordinator agent lifespan and handoff patterns" for the `lifespan` rule, not a
  permission recommendation. The permission-retries recommendation needs direct inspection.
- The `permission_friction_events` metric appears in "What Went Well" in the SAMPLE-REVIEW.md as
  `"3 vs mean 8.8 — low friction outside compile bursts"`. The false-positive issue from §10.1 may
  be in how a recommendation template conflates `compile_cycles` with `permission_retries` — this
  needs targeted investigation during design/implementation.

**cycle_events schema** (confirmed via `db.rs` line 534): columns are `id, cycle_id, seq,
event_type, phase, outcome, next_phase, timestamp, goal`. The `goal` column was added in v16
(col-025). No schema change needed for PhaseStats computation — all required data is already present.

**PhaseNarrative** (crt-025): already computes `phase_sequence`, `rework_phases`, and
`per_phase_categories` from `cycle_events`. The per-phase duration, observation records per window,
agents, and knowledge throughput are NOT currently computed. The Phase Timeline requires a new
`PhaseStats` type and a new computation step that slices the already-loaded `attributed` observation
records by the `cycle_events` time windows (the same windows used by col-024's primary path).

### Patterns Found in Unimatrix

- **#3383** — cycle_events-first observation lookup algorithm (topic_signal + time windows) is the
  same algorithm used in col-024's primary path; Phase Timeline computation can reuse this window
  extraction logic without duplicating it.
- **#952 / #949** — Retrospective formatter lives in a dedicated module (`response/retrospective.rs`)
  per ADR-003; all rendering logic must stay there, not in the handler.
- **#2999** — `seq` in `cycle_events` is advisory; timestamps are authoritative for ordering phase
  windows. Phase Timeline implementation must use `timestamp ASC` ordering (already in the existing
  query at handler line 1590).
- **#3396** — Goal is on `cycle_start` row only; `get_cycle_start_goal(cycle_id)` is the correct
  read path.

## Proposed Approach

The feature splits into three layers in ascending scope complexity:

**Layer 1 — Formatter-only changes** (no struct changes, no handler changes):
- Rebrand header to `# Unimatrix Cycle Review —`
- Add in-progress indicator from `phase_narrative` presence and `rework_phases` check
- Replace `ts=` evidence with relative burst notation in `render_findings`
- Add "What Went Well" section from `baseline_comparison` (filter: not outlier, metric direction
  favorable)
- Enhance session rows to include tool distribution abbreviations and agents list
- Recommendations moved to top of report (section order change)

**Layer 2 — RetrospectiveReport struct extensions** (new fields, backward-compatible as
`skip_serializing_if = "Option::is_none"` or default):
- `goal: Option<String>` — loaded via `get_cycle_start_goal` in handler
- `cycle_type: Option<String>` — inferred from goal keywords (`"design"/"research"` → Design,
  `"implement"/"deliver"` → Delivery, `"fix"/"bug"` → Bugfix, `"refactor"` → Refactor)
- `attribution_path: Option<String>` — set in handler based on which path returned non-empty
- `is_in_progress: bool` — derived from `cycle_events` presence of `cycle_stop` row
- `phase_stats: Option<Vec<PhaseStats>>` — new type (see below), computed in handler

**Layer 3 — New PhaseStats type + knowledge reuse fix**:
- New `PhaseStats` struct in `unimatrix-observe/src/types.rs` (or a new `phase_stats.rs` module)
  with fields: `phase`, `pass_count`, `duration_secs`, `session_count`, `record_count`, `agents`,
  `tool_distribution`, `knowledge_served`, `knowledge_stored`, `gate_result`, `hotspot_ids`
- Extend `FeatureKnowledgeReuse` with `cross_feature_reuse: u64`, `intra_cycle_reuse: u64`,
  `total_stored: u64`, `top_cross_feature_entries: Vec<(u64, String, u64)>` (id, title, served count)
- In handler: pre-fetch each entry's `feature_cycle` in `compute_knowledge_reuse_for_sessions`,
  pass as lookup closure to split delivery counts by source cycle

Backward compatibility: JSON consumers only see new Optional fields. Markdown output is the primary
change surface for agents. The `format="json"` path gains new fields transparently.

## Acceptance Criteria

- AC-01: The markdown report header reads `# Unimatrix Cycle Review — {feature_cycle}` (not `# Retrospective:`).
- AC-02: The header line shows `Goal: {goal_text}` when a goal is stored in `cycle_events`; omits the Goal line (not a blank line) when no goal is recorded.
- AC-03: The header line shows `Cycle type: {type}` inferred from goal keywords when a goal is present; shows `Cycle type: Unknown` when goal is absent.
- AC-04: The header line shows `Attribution: cycle_events-first (primary)` when the primary path was used, `Attribution: sessions.feature_cycle (legacy)` when Path 2 was used, or `Attribution: content-scan (fallback)` when Path 3 was used.
- AC-05: When no `cycle_stop` event exists in `cycle_events` for the cycle, the report header includes `Status: IN PROGRESS`.
- AC-06: When `cycle_events` records exist, the report contains a Phase Timeline table with one row per phase showing: phase name, duration, pass count (>1 = rework), record count, agents spawned, knowledge served + stored, and gate outcome.
- AC-07: Phase Timeline table includes a rework annotation below the table for any phase with `pass_count > 1`, showing pass-level duration and record count (e.g., `Rework: implementation — pass 1 gate fail: {outcome}`).
- AC-08: Each finding in the Findings section includes a `phase: {phase_name}` annotation identifying which phase the hotspot fired in (when `cycle_events` data is available for that cycle).
- AC-09: Per-finding evidence is rendered as relative-time burst notation: `Timeline: +0m(N) +12m(N) ...` with a `Peak: N events in Xmin at +Ym — {top files}` line. Raw `ts=` epoch values are not shown in the rendered output.
- AC-10: A "What Went Well" section appears in the report when at least one metric shows a non-outlier favorable signal (value better than mean). It lists metric name, value, mean, and a plain-text description. The section is omitted entirely when no favorable signals exist.
- AC-11: The Recommendations section appears immediately after the header block, before Findings.
- AC-12: The Knowledge Reuse section shows: total entries served, total stored, cross-feature count (entries from prior cycles), intra-cycle count (entries stored during this cycle), by-category breakdown of all served entries, and top 3–5 cross-feature entries by serve count. The `category_gaps` field is retired from the rendered output.
- AC-13: No finding or claim string in the markdown output contains the word "threshold" paired with a numeric value (e.g., `threshold: 45min`). All numeric comparisons must be expressed as baseline framing (`+N.Nσ above mean of X`) or ratio framing (`N× typical`). The word "allowlist" must not appear in any `compile_cycles` finding or recommendation text.
- AC-14: The session profile table includes a tool distribution abbreviation column (`NR NE NW` for read/execute/write counts) and an Agents column listing agent types spawned in the session.
- AC-15: The session profile section includes a "Top file zones:" line showing the top 3–5 directory paths by touch count across all sessions.
- AC-16: The JSON format response (`format="json"`) includes the new `goal`, `cycle_type`, `attribution_path`, `is_in_progress`, and `phase_stats` fields on `RetrospectiveReport`.
- AC-17: All existing `context_cycle_review` tests pass without modification. New behavior is tested via new test cases. No test file is deleted or renamed.
- AC-18: The `FeatureKnowledgeReuse` struct change is backward-compatible: existing JSON deserializers using `#[serde(default)]` on new fields continue to work.
- AC-19: The `compile_cycles` recommendation text describes the likely cause as repeated compilation errors (e.g., iterative per-field changes, unresolved type errors triggering recompile loops) with no mention of allowlists or permission prompts. The `compile_cycles` and `permission_friction_events` recommendation templates are confirmed to be independent with no cross-contamination.

## Constraints

**Dependencies (hard)**:
- col-024 and col-025 must be merged before col-026 ships. AC-02 through AC-09 all depend on data
  that only exists post-col-024/025. These are branches on the `main` branch; col-026 should branch
  from `main` after both are merged.
- Schema is at v16 (col-025). No schema migration is required or permitted in col-026.

**Struct compatibility**:
- `RetrospectiveReport` is serialized to JSON and returned via the `format="json"` path. New fields
  must use `#[serde(default, skip_serializing_if = "Option::is_none")]` or `#[serde(default,
  skip_serializing_if = "Vec::is_empty")]` to preserve backward compatibility for consumers using
  the JSON path.
- `FeatureKnowledgeReuse` is exposed in the public `unimatrix-observe` crate API. Adding new fields
  requires `#[serde(default)]` on new fields; removing `category_gaps` from rendered output is a
  formatter-side change only (the field stays on the struct for now to avoid a breaking change).

**Fire-and-forget constraint**:
- The Phase Timeline computation must not be in a hot-path blocking call. Per #3000 (col-025
  ADR-003), `cycle_events` uses the direct write pool. Phase stats computation should follow the
  same pattern as existing steps 10g (phase narrative): raw SQL query on `write_pool_server()`,
  error-logged-and-skipped on failure (best-effort, same as all col-020 steps).

**Test infrastructure**:
- Extend existing fixtures in `crates/unimatrix-server/src/mcp/response/retrospective.rs` test
  module and `crates/unimatrix-server/src/mcp/knowledge_reuse.rs`. Do not create isolated test
  scaffolding.
- Phase Timeline computation involves observation slicing by time window — tests must use the
  existing `infra-001` test infrastructure pattern identified in #3040 (cycle_events seeded via
  UDS-only write path).

**Performance**:
- Phase Timeline observation slicing is an in-memory filter over the already-loaded `attributed`
  observation slice (already in RAM). No additional DB query is needed for the observation-side of
  phase computation. The only new DB calls are `get_cycle_start_goal` (one read) and the
  `cycle_events` query already present in the handler.

## Open Questions — RESOLVED

1. **Permission-friction / compile_cycles recommendation** *(resolved)*: `compile_cycles` has
   nothing to do with allowlists. The session runs in skip-permissions mode — there are essentially
   no scenarios where an allowlist is the right recommendation. Compile cycles means repeated
   compilation errors, not permission friction. **Decision**: AC-19 is a recommendation-text fix.
   The `compile_cycles` finding recommendation must describe the likely cause (repeated compilation
   errors, iterative per-field changes, etc.) without any allowlist or permission framing. The
   `compile_cycles` and `permission_friction_events` signals are entirely separate; their
   recommendation templates must not cross-contaminate.

2. **`category_gaps` retirement** *(resolved)*: Suppress in the formatter only. The struct field
   stays on `FeatureKnowledgeReuse` to avoid a breaking change for JSON consumers. The formatter
   simply does not render it.

3. **Phase Timeline when `cycle_events` are absent** *(resolved)*: Show an explicit note in the
   report: `No phase information captured` (single line, no section header). Silent omission would
   leave the reader wondering whether phase data exists.

4. **Top cross-feature entry titles** *(resolved)*: Safe to include. No PII risk in this context.
   Entry titles are authored knowledge labels. The `get(entry_id)` reads are already running for
   the `entries_analysis` step; the fetch does not materially increase query count.

## Tracking

https://github.com/dug-21/unimatrix/issues/376

## Dependencies

- **col-024** (cycle_events-first observation lookup) — must be merged. AC-04, AC-05, AC-06, AC-07,
  AC-08 all require its shipped data infrastructure.
- **col-025** (feature goal signal) — must be merged. AC-02, AC-03 require `cycle_events.goal`
  column (schema v16) and `get_cycle_start_goal` DB method.
- **GH#320** (knowledge reuse undercounting) — this feature resolves it. No dependency on any other
  open issue.
- **GH#203** (markdown format gaps) — this feature resolves the cited gaps except per-CycleType
  baseline comparison. No external dependency.
