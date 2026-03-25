# ASS-033: Unimatrix Cycle Review — Enhancement Opportunities

**Research spike**: 2026-03-24
**Status**: Complete

---

## 1. Objective

Analyze what a revised **Unimatrix Cycle Review** (replacing `context_cycle_review`) could deliver given:
- The cycle-events-first observation lookup engine shipped in **col-024**
- The feature goal signal infrastructure shipped in **col-025**
- Documented format gaps from GitHub issues **#203** and **#320**
- The full per-phase timeline now available from `cycle_events` (new, §6)

---

## 2. What col-024 and col-025 Changed

### 2.1 col-024 — Authoritative Attribution Engine

Before col-024, `context_cycle_review` attributed observations by reading `sessions.feature_cycle` — an asynchronously-written field that silently failed for:
- Bugfix cycles run mid-delivery (Pattern A: last writer wins, clobbers bugfix attribution)
- Server-restart scenarios (Pattern B: eager attribution contaminates with agent-ID strings)
- Worktree subagent sessions (never captured at all)

**What col-024 adds:**
- `load_cycle_observations(cycle_id)` as the new primary path — uses `cycle_events` timestamps as authoritative time windows
- Three-step algorithm: (1) extract start/stop windows from cycle_events, (2) find session_ids via `topic_signal` + timestamp overlap, (3) load and Rust-filter to window
- Topic signal write-time enrichment: `enrich_topic_signal()` applied at all four UDS write sites so new observations are tagged correctly
- Full fallback chain: cycle_events-first → legacy sessions.feature_cycle → content-scan unattributed sessions
- Open-ended window support: in-progress cycles use `unix_now_secs()` as implicit stop

**Net effect**: `context_cycle_review` can now reliably collect all observations across sessions — including bugfix cycles, worktree isolation sessions, and multi-cycle interleaved sessions.

### 2.2 col-025 — Feature Goal Signal

Before col-025, the tool had no semantic anchor for *what the cycle was trying to accomplish*. It could report on what happened but not assess it against intent.

**What col-025 adds:**
- `goal TEXT` column in `cycle_events` (schema v16), written on `cycle_start` rows only
- `SessionState.current_goal: Option<String>` — in-memory cache, loaded on resume
- `get_cycle_start_goal(cycle_id)` — DB read for retrospective reconstruction
- `derive_briefing_query` step 2 now returns `state.current_goal` (replaces weak topic-ID synthesis)
- `MAX_GOAL_BYTES = 1024` guard (hard-reject on MCP, truncate+warn on UDS)

**Net effect for cycle_review**: The retrospective tool now has access to a structured, durable statement of intent per cycle. This enables:
- Showing the goal in the report header
- Assessing whether hotspots are expected given the goal type (design vs. implementation vs. bugfix)
- Briefing-quality query derivation for the knowledge reuse section

---

## 3. Current State Gaps (from issues #203, #320, #309)

### 3.1 Issue #203 — Markdown vs JSON Format Gaps

Extensive real-world comparison across two feature cycles (base-004, crt-018b) identified:

| Information | Markdown | JSON |
|---|---|---|
| Tool distribution (read/write/execute/search split) | Missing | Present |
| Agents spawned in session | Missing | Present |
| Top file zones by access frequency | Missing | Present |
| Full baseline table (all 21 metrics) | Outliers only | All |
| Positive metric signals (parallel rate, zero post-completion work) | Hidden | Present |
| Knowledge category breakdown | Count only | Full breakdown |
| entries_analysis (which knowledge entries were active) | Missing | Present |
| Session attribution count | Inconsistent | Authoritative |
| Evidence detail (actual file paths, commands) | Truncated | Full |

**Additional feedback from comment on #203 (bugfix-236 retro):**

The markdown flattens temporal intelligence into counts and raw timestamps. The JSON *story* (burst patterns, peak intensity, file sequences) is the actionable signal. Proposed per-finding format:

```
### F-05 [warning] compile_cycles: 34 compile/check cycles (16 bursts)
Timeline: +0m(3) +12m(5) +28m(4) +45m(6) ... +267m(2)
Peak: 6 compiles in 4m at +45m (background.rs, timeout.rs, tools.rs)
```

Key changes proposed:
- Relative timestamps (`+45m` from session start) instead of raw epochs
- Timeline sketch — compressed burst notation showing temporal shape
- Peak annotation — highest-intensity cluster with duration and files
- Drop raw `ts=` examples that burn tokens without adding signal
- Recommendations retain parenthetical rationale

### 3.2 Issue #320 — Knowledge Reuse Undercounting

Current `feature_knowledge_reuse` only counts entries tagged with the *same* `feature_cycle` as the session being reviewed. Result: reports 9 entries when agents actually served 49.

**Root cause**: The metric conflates "entries stored during this feature" with "all knowledge delivered to agents during this feature." Cross-feature reuse (the high-value signal — prior decisions, patterns, lessons paying dividends) is completely invisible.

**Desired output shape:**
```
## Knowledge Reuse
Total served: 49 (33 S1 + 16 S2) | Stored: 21
By category (served): decision ×12, pattern ×8, outcome ×9, lesson-learned ×3, ...
Cross-feature reuse: 40 entries from prior cycles
Intra-cycle reuse: 9 entries (same feature_cycle)
```

The `category_gaps` field should be dropped or renamed — it implies patterns were never used when agents retrieved 40 cross-feature entries including patterns and decisions.

### 3.3 Issue #309 — Related: Context Compaction Content Review

Tangentially related: the PreCompact hook has similar information gaps (no lessons, no outcomes, no co-access-aware selection). Not directly in scope for context_cycle_review, but the same underlying data availability issue.

---

## 4. What a Revised context_cycle_review Could Deliver

### 4.1 Report Header: Goal + Attribution Provenance

With col-025's goal field now durable in `cycle_events`, the report can open with a structured header:

```
## Feature Cycle: col-025
Goal: Feature Goal Signal — add `goal` to context_cycle, briefing query derivation, subagent injection
Sessions: 3 | Records: 847 | Duration: 8h 12m
Attribution: cycle_events-first (primary) ← authoritative
```

The attribution provenance line tells the reader *how* observations were collected, letting them trust the report. If the legacy fallback fires, it should be visible: `Attribution: sessions.feature_cycle (legacy fallback)`.

### 4.2 Goal-Contextualized Hotspot Assessment

The `goal` string enables classification of the cycle type. A hotspot that would be alarming in a pure implementation cycle is expected in a design-heavy cycle.

**Example logic:**
- If goal contains "research" or "design" → high `context_load_before_first_write_kb` is expected, not a warning
- If goal contains "bugfix" → short cycle duration is expected; a `lifespan` warning firing on an agent is less significant
- If goal contains "implement" or "delivery" → `search_via_bash_count` and `reread_rate` are meaningful warning signals

This doesn't require NLP — simple keyword presence from the goal string contextualizes the severity threshold. The goal can be shown next to each hotspot: `[F-03] context_load (278.6 KB) — possibly expected for research/design cycle`.

### 4.3 Temporal Shape in Findings (from #203 feedback)

Replace raw epoch evidence with relative-time burst notation:

**Before:**
```
### F-05 [warning] compile_cycles: 34 compile/check cycles. 16 cluster(s).
Examples:
- Compile at ts=1773406852000
- Compile at ts=1773406877000
- Compile at ts=1773406930000
```

**After:**
```
### F-05 [warning] compile_cycles: 34 cycles (16 bursts)
Timeline: +0m(3) +12m(5) +28m(4) +45m(6) +55m(4) ... +267m(2)
Peak: 6 compiles in 4min at +45m — background.rs, timeout.rs, tools.rs
Recommendation: Add build/test commands to allowlist (7 permission retries at peak)
```

This is a markdown formatter change — the data already exists in the JSON `hotspots[].evidence[]` and `narratives[]` arrays. The formatter needs to:
1. Compute `relative_ts = (evidence.ts - session.started_at) / 60_000` as minutes
2. Group into clusters and find peak cluster
3. Extract the top files from peak cluster evidence

### 4.4 Full Knowledge Reuse (from #320)

Replace `feature_knowledge_reuse` with a complete served-entries view:

**Current:**
```rust
pub struct FeatureKnowledgeReuse {
    pub delivery_count: u64,        // only same-cycle entries
    pub category_gaps: Vec<String>, // misleading
    pub cross_session_reuse: u64,
}
```

**Revised:**
```rust
pub struct FeatureKnowledgeReuse {
    pub total_served: u64,                          // all entries served across all sessions
    pub total_stored: u64,                          // entries stored during this cycle
    pub cross_feature_reuse: u64,                   // served entries from other feature cycles
    pub intra_cycle_reuse: u64,                     // served entries from this feature cycle
    pub by_category: HashMap<String, u64>,          // served entries by category
    pub top_cross_feature_entries: Vec<EntryRef>,   // highest-reuse entries from prior cycles
}
```

The `total_served` value is already computed from `session_summaries[].knowledge_served` — it just needs to be aggregated and split by source cycle. The `session_summaries` already have per-session entry lists with their feature attribution.

### 4.5 Positive Signals Surface (from #203)

Metrics that are better than average are currently invisible in the markdown report. A "What Went Well" section should surface them:

```
## What Went Well
- parallel_call_rate: 0.49 (mean 0.24) — agents parallelized above average ✓
- bash_for_search_count: 6 (mean 29.3) — tools used correctly ✓
- post_completion_work_pct: 0% — clean stop ✓
- permission_friction_events: 3 (mean 8.8) — low friction ✓
```

Already computable from the `baseline_comparison` array — filter to `is_outlier: false` where `current_value < mean` (for "lower is better" metrics) or `current_value > mean` (for "higher is better" metrics).

### 4.6 Session Profile Section

Currently absent from markdown, available in JSON `session_summaries`:

```
## Session Profile
| Session | Duration | Tools | Agents | Knowledge |
|---------|----------|-------|--------|-----------|
| S1 | 3h 22m | 218R 59E 41W | uni-researcher, uni-architect | 11 served |
| S2 | 1h 45m | 76R 22E 8W | uni-researcher (×3), uni-specification | 16 served |

Top file zones: crates/unimatrix-server/src (47), product/features/col-025 (31)
```

The tool-type abbreviations (R=read, E=execute, W=write) make the distribution scannable without the full table.

### 4.7 Agents Spawned (from #203)

Critical for understanding workflow structure — currently missing from markdown:

```
Agents spawned: uni-researcher (×2), uni-architect, uni-specification,
                uni-risk-strategist, uni-synthesizer, uni-vision-guardian
```

Available from `session_summaries[].subagents_spawned` (or reconstructable from `SubagentStart` observations with tool name = agent type).

### 4.8 entries_analysis as Knowledge Health Signal

Currently JSON-only. The `entries_analysis` array shows which knowledge entries were active during the cycle with injection counts and success rates. This is the signal for "knowledge gaps" — entries with `injection_count: 0` that were stored during this cycle haven't been consumed yet.

**Proposed markdown section:**
```
## Knowledge Health
Stored this cycle: 21 entries | Immediately consumed: 8 | Pending validation: 13
High-reuse from prior cycles: ADR-042 (pattern, ×7), lesson-1560 (lesson-learned, ×4)
```

---

## 5. New Opportunities Enabled by col-024/025 Specifically

### 5.1 Goal-Driven Cycle Type Classification

The `goal` string (now durable in cycle_events) enables a `CycleType` inference that wasn't possible before:

```
enum CycleType {
    Design,       // goal contains design/research/scope/spec keywords
    Delivery,     // goal contains implement/build/deliver keywords
    Bugfix,       // goal contains fix/bug/regression keywords
    Refactor,     // goal contains refactor/cleanup/simplify keywords
    Unknown,      // no goal set or no keyword match
}
```

Each `CycleType` carries different expected hotspot profiles. A design cycle with high `context_load` is normal. A bugfix cycle with `file_breadth > 20` is suspicious. This lets the tool say *"3 of 6 hotspots are expected for a bugfix cycle"* rather than listing all 6 equally.

### 5.2 Reliable Multi-Cycle Comparison

With cycle_events-first attribution, the baseline comparison is now trustworthy across cycle types. Before col-024, bugfix cycles were never properly attributed, so baselines were contaminated with misattributed observations.

The revised tool can provide per-CycleType baselines: "compared to 12 prior bugfix cycles" rather than "compared to all 47 cycles."

### 5.3 In-Progress Cycle Reporting

col-024's open-ended window support (implicit `unix_now_secs()` stop) means `context_cycle_review` can be called on an *active* cycle. The tool could indicate this:

```
Status: IN PROGRESS (cycle_start recorded, no cycle_stop yet)
Observations through: 2026-03-24T14:32:00Z
```

This enables mid-session health checks — call the retro mid-cycle to see if unusual patterns are emerging before they compound.

### 5.4 Goal as Retrospective Anchor for Lesson Extraction

When the cycle completes, the goal provides a natural extraction anchor for lesson-learned entries:

```
Goal: "Fix context_cycle_review for bugfixes run in multi-cycle sessions"
Hotspots: None
What Went Well: cycle_events-first attribution resolved all prior misattribution
Suggested lesson: cycle_events are authoritative for attribution; sessions.feature_cycle is unreliable for interleaved cycles
```

The `goal` field could even be passed to the fire-and-forget lesson extraction that already exists — currently lesson text is synthesized from hotspots alone; adding the goal as context would improve extraction specificity.

---

## 6. Proposed Revised Output Structure

### 6.1 Markdown Format (Revised)

```markdown
## Retrospective: col-025 — Feature Goal Signal
Goal: Feature Goal Signal — add goal to context_cycle, briefing query derivation, subagent injection
Cycle type: Delivery | Attribution: cycle_events-first (primary)
Sessions: 3 | Records: 847 | Duration: 8h 12m | Outcome: SUCCESS

## Session Profile
| Session | Duration | Tools (R/E/W) | Agents | Knowledge Served |
|---------|----------|---------------|--------|-----------------|
| S1 | 3h 22m | 218R 59E 41W | researcher, architect | 11 |
| S2 | 1h 45m | 76R 22E 8W | researcher×3, spec | 16 |
Top zones: crates/unimatrix-server/src (47), product/features/col-025 (31)
Agents spawned: uni-researcher (×2), uni-architect, uni-specification, uni-risk-strategist

## Findings (5 warnings, 1 info)
### F-01 [warning] compile_cycles: 34 cycles (16 bursts)
Timeline: +0m(3) +12m(5) +28m(4) +45m(6) ... +267m(2)
Peak: 6 compiles in 4min at +45m — background.rs, timeout.rs, tools.rs
Recommendation: Add build/test commands to allowlist (7 permission retries at peak)

[... per-finding with relative timestamps + peak annotation ...]

## What Went Well
- parallel_call_rate: 0.49 (mean 0.24) — above-average concurrency ✓
- bash_for_search_count: 6 (mean 29.3) — tools used correctly ✓
- post_completion_work_pct: 0% — clean stop ✓

## Baseline Outliers (2)
- context_load_before_first_write_kb: 278.6 KB — 9.4σ above mean (expected for delivery with design warmup)
- knowledge_entries_stored: 11.0 — 2.2σ above mean

## Knowledge Reuse
Total served: 49 (S1×33 + S2×16) | Stored: 21 | Cross-feature: 40 | Intra-cycle: 9
By category (served): decision×12, pattern×8, outcome×9, lesson-learned×3, convention×5, ...
Top cross-feature: ADR-042 (×7), lesson-1560 (×4), pattern-0891 (×3)
Pending validation: 13 stored entries not yet consumed

## Recommendations
1. [compile_cycles] Add build/test commands to allowlist
2. [reread_rate] Store protocol summary as Unimatrix pattern for fast agent reload
```

### 6.2 JSON Format (Extended)

Add to the existing `RetrospectiveReport` struct:
```rust
pub goal: Option<String>,                           // from cycle_events.goal (col-025)
pub cycle_type: Option<String>,                     // inferred from goal keywords
pub attribution_path: AttributionPath,              // which path was used (new enum)
pub is_in_progress: bool,                           // no cycle_stop in cycle_events
pub positive_signals: Vec<BaselineComparison>,      // non-outlier improvements
pub per_cycle_type_baseline: Option<Vec<BaselineComparison>>, // filtered to same cycle type
```

```rust
pub enum AttributionPath {
    CycleEventsPrimary,     // col-024 primary path succeeded
    SessionsFeatureCycle,   // legacy fallback
    ContentScan,            // unattributed session content inference
    Mixed,                  // multiple paths used across sessions
}
```

---

## 6. Phase-by-Phase Stage Breakdown (New Capability)

### 6.1 What cycle_events Contains Per Stage

The `cycle_events` table records the full lifecycle with precise timestamps:

```
cycle_start     (goal, next_phase="S1-scope", timestamp)
cycle_phase_end (phase="S1-scope",   outcome="SCOPE.md approved",        next_phase="S1-spec",     timestamp)
cycle_phase_end (phase="S1-spec",    outcome="SPECIFICATION.md complete", next_phase="S2-design",   timestamp)
cycle_phase_end (phase="S2-design",  outcome="all artifacts ready",       next_phase="S3a-test",    timestamp)
cycle_phase_end (phase="S3a-test",   outcome="plan complete, gate pass",  next_phase="S3b-impl",    timestamp)
cycle_phase_end (phase="S3b-impl",   outcome="gate 3b pass",              next_phase="S3c-test",    timestamp)
cycle_phase_end (phase="S3c-test",   outcome="all tests passing",         next_phase=None,          timestamp)
cycle_stop      (timestamp)
```

Each adjacent pair `(event[n].timestamp, event[n+1].timestamp)` is a **phase time window**. Combined with col-024's observation lookup by time window, every observation can be placed into the phase it occurred in.

### 6.2 Per-Phase Stats Available

For each phase window, the same observation analysis that currently runs across the full cycle can be computed per phase:

| Metric | Per Phase | Derivation |
|---|---|---|
| Duration | Minutes in phase | `phase_end.timestamp - phase_start.timestamp` |
| Sessions involved | Count and IDs | topic_signal + timestamp overlap per window |
| Observations / tool calls | Count | filter observations to phase window |
| Tool distribution | R/E/W/Search breakdown | event_type + tool name filter |
| Agents spawned | List with types | SubagentStart events in window |
| File zones accessed | Top dirs by touch count | Read/Write tool input paths |
| Knowledge served | Entries delivered | session_summaries scoped to phase |
| Knowledge stored | Entries written | store events in window |
| Hotspot findings | Which rules fired | hotspot detection scoped to phase |
| Gate outcome | pass/fail/rework | from `outcome` text on cycle_phase_end |

**Already partially computed**: `PhaseNarrative.per_phase_categories` gives the knowledge category distribution per phase. The gap is that this doesn't yet include duration, agents, tool distribution, or hotspot scoping.

### 6.3 What This Enables: Phase Intelligence

**Rework detection with evidence**: `rework_phases` already identifies phases that repeated. With per-phase observations, we can show *what was different* the second time:

```
Phase S3b-impl: REWORK (2 passes)
  Pass 1: 3h 12m | 4 agents | 89 records | Gate: FAIL (undefined macro compile error)
  Pass 2: 0h 47m | 2 agents | 31 records | Gate: PASS
  Delta: -2.5h, -2 agents — targeted fix, not full re-implementation
```

**Phase duration comparison against baseline**: How long should S2-design take? Compare to prior cycles:

```
Phase S2-design: 4h 35m (baseline: 2h 12m ±45m, +2.1σ)
  Possible cause: F-03 [warning] reread_rate — 23 files re-read in this phase
```

Connecting an outlier phase duration to the hotspot that fired *within that phase* is the key insight: the retro stops being a post-mortem and starts explaining *why* things took longer.

**Phase knowledge profile**: Each phase has a characteristic knowledge consumption pattern. Design phases consume ADRs and conventions. Implementation phases consume patterns and procedures. A design phase that's consuming procedures is a signal that scope crept into implementation.

```
Phase S1-scope: 3 knowledge queries — decision×2, convention×1 (expected profile)
Phase S2-design: 7 knowledge queries — decision×4, pattern×2, lesson×1 (expected)
Phase S3b-impl: 12 queries — decision×3, pattern×6, procedure×3 (expected)
```

If a phase's knowledge consumption profile doesn't match expectations, flag it:
```
Phase S1-scope: 8 queries — pattern×5, procedure×3 [!] unusual for scope phase — implementation scope?
```

**Phase entry point context**: What was the knowledge state when a phase began? At each `cycle_phase_end`, we know what was stored during the prior phase and what was served. This could be used to seed the briefing query at phase start — but also to show in the retro what "carried forward."

### 6.4 Proposed Stage Breakdown Section in Unimatrix Cycle Review

```markdown
## Phase Timeline

| Phase | Duration | Sessions | Records | Agents | Knowledge | Gate |
|-------|----------|----------|---------|--------|-----------|------|
| S1-scope   | 1h 12m | 1 | 89  | researcher               | 3 served, 0 stored  | PASS |
| S1-spec    | 0h 55m | 1 | 67  | specification            | 5 served, 1 stored  | PASS |
| S2-design  | 4h 35m | 2 | 234 | architect, risk-strat    | 8 served, 4 stored  | PASS ⚠ (+2.1σ) |
| S3a-test   | 0h 48m | 1 | 52  | tester                   | 2 served, 0 stored  | PASS |
| S3b-impl ×2| 4h 01m | 2 | 403 | rust-dev, tester         | 12 served, 6 stored | PASS (rework) |
| S3c-test   | 0h 31m | 1 | 41  | tester                   | 3 served, 0 stored  | PASS |

Rework: S3b-impl (2 passes — gate fail on pass 1: undefined macro compile error)
Design phase duration outlier: 4h 35m vs 2h 12m baseline — reread_rate hotspot fired in this phase
```

The `⚠ (+2.1σ)` and the rework annotation give the table immediate signal without requiring the reader to cross-reference findings.

### 6.5 What Else Becomes Possible With Stage Data

**Phase velocity trend**: Track how each phase's duration changes across successive features. Are design phases getting faster as the knowledge base matures? This is direct evidence of Unimatrix's value.

**Agent efficiency per phase**: Within a phase, what fraction of tool calls were effective (resulted in writes or knowledge queries) vs. exploratory (reads, searches without follow-up action)?

**Phase handoff quality**: The `outcome` text on `cycle_phase_end` is currently free-form. With light parsing or a structured outcome field, we could assess whether the handoff was clean:
- "SCOPE.md approved" → clean
- "partial — resumed next session" → fragmented
- "gate fail, rework" → rework flag

**Cross-cycle phase comparison** (already partially in `PhaseNarrative.cross_cycle_comparison`): This becomes much more powerful with duration + agents + tool distribution per phase, not just category counts.

**Phase-scoped knowledge gaps**: Instead of "no pattern entries served this cycle," we can say "no pattern entries served during S2-design — agents may have missed available patterns during architecture phase."

---

## 7. Proposed Revised Output Structure

### 7.1 Markdown Format (Revised)

```markdown
# Unimatrix Cycle Review — col-025

Goal: Feature Goal Signal — add goal to context_cycle, briefing query derivation, subagent injection
Cycle type: Delivery | Attribution: cycle_events-first (primary)
Sessions: 3 | Records: 847 | Duration: 8h 12m | Outcome: SUCCESS

## Phase Timeline

| Phase | Duration | Sessions | Records | Agents | Knowledge | Gate |
|-------|----------|----------|---------|--------|-----------|------|
| S1-scope   | 1h 12m | 1 | 89  | researcher               | 3 served, 0 stored  | PASS |
| S1-spec    | 0h 55m | 1 | 67  | specification            | 5 served, 1 stored  | PASS |
| S2-design  | 4h 35m | 2 | 234 | architect, risk-strat    | 8 served, 4 stored  | PASS ⚠ (+2.1σ) |
| S3a-test   | 0h 48m | 1 | 52  | tester                   | 2 served, 0 stored  | PASS |
| S3b-impl ×2| 4h 01m | 2 | 403 | rust-dev, tester         | 12 served, 6 stored | PASS (rework) |
| S3c-test   | 0h 31m | 1 | 41  | tester                   | 3 served, 0 stored  | PASS |

Rework: S3b-impl (2 passes — gate fail on pass 1: undefined macro compile error)
Design outlier: 4h 35m vs 2h 12m baseline — reread_rate hotspot originated in this phase

## Session Profile
| S | Duration | Tools (R/E/W) | Agents | Knowledge |
|---|----------|---------------|--------|-----------|
| 1 | 3h 22m | 218R 59E 41W | researcher, architect | 11 served |
| 2 | 1h 45m | 76R 22E 8W | researcher×3, spec | 16 served |
Top zones: crates/unimatrix-server/src (47), product/features/col-025 (31)

## Findings (5 warnings, 1 info)
### F-01 [warning] compile_cycles: 34 cycles (16 bursts) — phase S3b-impl
Timeline: +0m(3) +12m(5) +28m(4) +45m(6) ... +267m(2)
Peak: 6 compiles in 4min at +45m — background.rs, timeout.rs, tools.rs
Recommendation: Add build/test to allowlist (7 permission retries at peak)

[...]

## What Went Well
- parallel_call_rate: 0.49 (mean 0.24) — above-average concurrency ✓
- bash_for_search_count: 6 (mean 29.3) — tools used correctly ✓
- post_completion_work_pct: 0% — clean stop ✓

## Baseline Outliers (2)
- context_load_before_first_write_kb: 278.6 KB — 9.4σ above mean
- knowledge_entries_stored: 11.0 — 2.2σ above mean

## Knowledge Reuse
Total served: 49 (S1×33 + S2×16) | Stored: 21 | Cross-feature: 40 | Intra-cycle: 9
By category (served): decision×12, pattern×8, outcome×9, lesson-learned×3, convention×5
Top cross-feature: ADR-042 (×7), lesson-1560 (×4), pattern-0891 (×3)
Pending validation: 13 stored entries not yet consumed

## Recommendations
1. [compile_cycles] Add build/test to allowlist
2. [reread_rate] Store protocol summary as Unimatrix pattern for fast agent reload
```

### 7.2 JSON Format (Extended)

Add to the existing `RetrospectiveReport` struct:
```rust
pub goal: Option<String>,                               // from cycle_events.goal (col-025)
pub cycle_type: Option<String>,                         // inferred from goal keywords
pub attribution_path: AttributionPath,                  // which lookup path was used
pub is_in_progress: bool,                               // no cycle_stop in cycle_events
pub positive_signals: Vec<BaselineComparison>,          // non-outlier improvements
pub per_cycle_type_baseline: Option<Vec<BaselineComparison>>, // filtered to same cycle type
pub phase_stats: Option<Vec<PhaseStats>>,               // NEW: per-phase breakdown
```

```rust
pub struct PhaseStats {
    pub phase: String,                          // phase name from cycle_phase_end.phase
    pub pass_count: u32,                        // 1 = normal, >1 = rework
    pub duration_secs: u64,                     // phase_end.timestamp - phase_start.timestamp
    pub duration_vs_baseline: Option<f64>,      // z-score vs prior cycles (None if no baseline)
    pub session_count: usize,
    pub record_count: usize,
    pub agents: Vec<String>,                    // agent types that spawned in this phase
    pub tool_distribution: ToolDistribution,   // R/E/W/Search counts
    pub knowledge_served: u64,
    pub knowledge_stored: u64,
    pub knowledge_by_category: HashMap<String, u64>,
    pub outcome: Option<String>,               // from cycle_phase_end.outcome
    pub gate_result: GateResult,               // inferred from outcome text
    pub hotspot_ids: Vec<String>,              // finding IDs that fired in this phase
}

pub enum GateResult {
    Pass,
    Fail,
    Rework,
    Unknown,    // no outcome text or unparseable
}

pub enum AttributionPath {
    CycleEventsPrimary,
    SessionsFeatureCycle,
    ContentScan,
    Mixed,
}
```

---

## 8. Implementation Complexity Assessment

| Enhancement | Complexity | Where | Depends On |
|---|---|---|---|
| Show goal + attribution in header | Trivial | formatter | col-025 (shipped) |
| In-progress cycle indicator | Trivial | tools.rs | col-024 open-ended windows |
| Session profile section (agents/tools/zones) | Low | formatter | data in JSON already |
| What Went Well section | Low | formatter | baseline_comparison array |
| Relative timestamps in findings | Low | formatter | — |
| entries_analysis knowledge health section | Low | formatter | entries_analysis computed |
| Fix knowledge reuse metric (#320) | Medium | services + types | session_summaries |
| Timeline burst sketch in findings | Medium | formatter + narratives | — |
| Goal-contextualized hotspot severity | Medium | observation service | col-025 goal |
| **Phase timeline table** | **Medium** | **new PhaseStats computation** | **col-024 windows + cycle_events** |
| **Per-phase hotspot scoping** | **Medium** | **observation service** | **phase windows** |
| **Rework evidence (per-pass diff)** | **Medium** | **new computation** | **phase windows + rework_phases** |
| CycleType classification | Low | new helper fn | col-025 goal |
| Per-CycleType baselines | High | baseline computation | reliable attribution over time |
| Phase velocity trend | High | new DB aggregation | multiple completed cycles |
| Phase knowledge profile anomaly detection | High | new service logic | expected profiles per phase type |

Most high-value improvements remain **formatter or service-layer changes** — no schema changes needed. Per-phase stats require a new `PhaseStats` computation step in the handler that slices existing observation data by cycle_events time windows.

---

## 9. Priority Ranking (Updated)

**High value, low effort:**
1. Goal + attribution provenance in header (trivial — data exists)
2. Session profile section — agents spawned, tool distribution, file zones
3. What Went Well section — positive baseline signals
4. Relative timestamps in per-finding evidence
5. Fix knowledge reuse metric (#320) — cross-feature count

**High value, medium effort:**
6. **Phase timeline table** — this is the single highest-value new capability: makes the retro scannable at a glance with all temporal context in one place
7. **Per-phase hotspot scoping** — annotates each finding with which phase it fired in (finding F-01 → phase S3b-impl)
8. **Rework phase evidence** — shows diff between pass 1 and pass 2 for reworked phases
9. Timeline burst sketch in findings
10. In-progress cycle indicator

**Medium value, medium effort:**
11. Goal-contextualized hotspot severity labels
12. entries_analysis as knowledge health section
13. CycleType classification

**High value, high effort (longer term):**
14. Per-CycleType baseline comparison
15. Phase velocity trend (requires accumulation of completed cycle phase stats)
16. Phase knowledge profile anomaly detection

---

## 10. Known Content Correctness Issues

Two systematic problems identified during sample review that must be resolved before delivery — not formatting issues, data/language issues.

### 10.1 Permission-Prompt Recommendations Fire in Skip-Permissions Mode

**Symptom**: The `compile_cycles` (and possibly `permission_friction_events`) hotspot generates recommendations like *"add commands to allowlist — N permission prompts fired"* even when the session ran in skip-permissions mode, where no permission prompts exist.

**Impact**: This is consistently wrong feedback. A recommendation that cannot apply to the user's setup erodes trust in all recommendations.

**Likely source**: The hotspot detection rule counts Bash/cargo execution events and infers permission friction from the pattern (e.g., a burst of short-duration tool calls that look like retries). It is not reading actual permission-prompt events from the hook stream — it's inferring them from timing patterns. In skip-permissions mode those patterns still occur (compile retries, short burst intervals) but have nothing to do with permissions.

**Two problems to untangle**:
1. The detection heuristic may be correct (compile burst = something to flag) but the *interpretation* ("permission prompts") is wrong in skip-permissions mode
2. The recommendation template applies a boilerplate action ("add to allowlist") that is meaningless in the user's context

**Resolution needed before delivery**:
- Determine whether skip-permissions mode is detectable from observation data (likely yes — a session-level flag or absence of prompt-related hook events)
- If detectable: suppress the "add to allowlist" recommendation when skip-permissions is active; the compile-cycle count can still be reported as a finding, just without the permission framing
- Separate the `compile_cycles` signal from `permission_friction_events` — they are distinct phenomena with distinct recommendations
- Requires investigation of how `permission_friction_events` is computed in the observation service; this may be a pre-existing bug worth filing as a separate issue

### 10.2 "Threshold" Language Implies User-Configured Rules

**Symptom**: Findings use `(threshold: 45min)` style language — e.g., *"uni-rust-dev lifespan 167min (threshold: 45min)"* — implying the user set a limit of 45 minutes that the agent exceeded.

**Impact**: The user never configured any threshold. The language creates a false impression of a rule violation and invites a question ("where did 45min come from?") that has no good answer. It also misrepresents the nature of the signal, which is statistical deviation, not rule violation.

**Root cause**: Hotspot detection rules use hardcoded comparison values (e.g., `lifespan > 45min → fire`). These are detection heuristics, not user-configured thresholds. The formatter surfaces the raw comparison value as if it were a configured limit.

**Correct framing**: The signal is "this is significantly above what we typically see" — which is a baseline comparison, not a threshold crossing. The right language is:
- `lifespan 167min (baseline: 43min ±18min, +3.4σ)` — for metrics with accumulated baseline data
- `lifespan 167min — 3.7× typical agent lifespan` — simpler version when σ isn't available
- Never: `(threshold: 45min)` or `(limit: 45min)` unless the user actually configured it

**Resolution needed before delivery**:
- Audit all hotspot detection rules for hardcoded comparison values surfaced in finding output
- Replace `threshold: N` language with either baseline comparison (σ) when baseline data exists, or a descriptive ratio ("3.7× typical") when it doesn't
- The hardcoded values can remain as detection heuristics internally; they just should not appear in user-facing output as if they are configured rules
- Consider whether any of these heuristics should eventually become user-configurable (separate, longer-term question)

---

## 11. Branding: Unimatrix Cycle Review

The tool should be rebranded from `context_cycle_review` (internal MCP name) to **Unimatrix Cycle Review** in all user-facing output. The MCP tool name remains `context_cycle_review` for backward compatibility — the branding change is presentation-only.

**Report header change:**
```markdown
# Unimatrix Cycle Review — {feature_cycle}
```
(was: `## Retrospective: {feature_cycle}`)

**Rationale:**
- `context_cycle_review` is a machine name describing the mechanism; "Unimatrix Cycle Review" describes the product
- "Review" is preferable to "Retrospective" — it's shorter and describes what you do with it (review the cycle) not when you do it (post-mortem connotation)
- "Unimatrix" as prefix anchors it as a first-class product deliverable, not just an MCP tool output
- Consistent with the broader direction: Unimatrix is a product, not just a library

**Scope of branding change:**
- Markdown formatter header and section labels
- JSON `tool_name` or `report_type` field (add to RetrospectiveReport)
- No MCP tool rename (backward compat)
- No Rust struct rename

---

## 12. References

| Source | What It Contributes |
|---|---|
| `product/features/col-024/` | Attribution engine: three-path fallback, load_cycle_observations algorithm |
| `product/features/col-025/` | Goal field: cycle_events schema v16, SessionState.current_goal, derive_briefing_query |
| GH #203 | Markdown vs JSON comparison: tool distribution, agents, file zones, positive signals, temporal narrative |
| GH #320 | Knowledge reuse undercounting: cross-feature reuse invisible, category_gaps misleading |
| GH #362 | Root cause analysis that motivated col-024: bugfix attribution failure patterns A and B |
| `crates/unimatrix-store/src/migration.rs` | cycle_events DDL, v16 migration |
| `crates/unimatrix-store/src/db.rs` | insert_cycle_event, get_cycle_start_goal |
| `crates/unimatrix-server/src/infra/validation.rs` | CYCLE_START_EVENT, CYCLE_PHASE_END_EVENT, CYCLE_STOP_EVENT constants |
| `crates/unimatrix-server/src/uds/listener.rs` | handle_cycle_event, phase/outcome/goal extraction |
| `crates/unimatrix-observe/src/types.rs` | PhaseNarrative, RetrospectiveReport |
| `crates/unimatrix-server/src/mcp/tools.rs` | CycleParams, context_cycle_review handler |
