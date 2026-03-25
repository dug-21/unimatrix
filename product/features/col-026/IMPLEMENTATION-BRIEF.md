# col-026 Implementation Brief — Unimatrix Cycle Review Enhancement

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/col-026/SCOPE.md |
| Architecture | product/features/col-026/architecture/ARCHITECTURE.md |
| Specification | product/features/col-026/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-026/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-026/ALIGNMENT-REPORT.md |

---

## Goal

Enhance `context_cycle_review` to produce a first-class retrospective report by: surfacing the feature
goal, cycle type, and attribution path in the header; adding a Phase Timeline table computed from
`cycle_events` time windows; fixing the GH#320 knowledge reuse undercounting (cross-feature vs.
intra-cycle split); adding a "What Went Well" section; replacing threshold language with baseline
framing; and reformatting evidence as burst notation with phase annotations. No schema migration.
No new MCP tools. Schema remains at v16.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| RetrospectiveReport struct extensions | pseudocode/retrospective-report-extensions.md | test-plan/retrospective-report-extensions.md |
| PhaseStats type + computation (step 10h) | pseudocode/phase-stats.md | test-plan/phase-stats.md |
| FeatureKnowledgeReuse extension + batch lookup | pseudocode/knowledge-reuse-extension.md | test-plan/knowledge-reuse-extension.md |
| Formatter overhaul (retrospective.rs) | pseudocode/formatter-overhaul.md | test-plan/formatter-overhaul.md |
| compile_cycles recommendation fix (report.rs) | pseudocode/recommendation-fix.md | test-plan/recommendation-fix.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component Map lists
expected components from the architecture — actual file paths are filled during delivery.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| `is_in_progress` type | `Option<bool>` — three states: `None` (no cycle_events), `Some(true)` (open), `Some(false)` (confirmed stopped). Plain `bool` with `#[serde(default)]` silently misreports pre-col-024 historical retros as confirmed-complete | SR-03 | architecture/ADR-001-is-in-progress-option-bool.md |
| Timestamp conversion in PhaseStats | `cycle_ts_to_obs_millis()` from `services/observation.rs` is the only permitted conversion. All inline `* 1000` multiplications are prohibited. Make `pub(crate)` if extracted to a separate module | SR-01 | architecture/ADR-002-cycle-ts-to-obs-millis-mandatory.md |
| Cross-feature entry metadata lookup | Second closure `entry_meta_lookup: impl Fn(&[u64]) -> HashMap<u64, EntryMeta>` passed to `compute_knowledge_reuse`; called once per invocation with the full ID set. Chunked to 100 IDs per IN-clause (pattern #883). Single SQL query: `SELECT id, title, category, feature_cycle FROM entries WHERE id IN (...)` | SR-02 | architecture/ADR-003-batch-entry-meta-lookup.md |
| Threshold language replacement | Formatter-only post-processing in `render_findings()` via `format_claim_with_baseline()`. Detection rules are untouched; JSON path retains original claim strings. Nine-site audit is complete and enumerated | SR-05 | architecture/ADR-004-formatter-threshold-language-replacement.md |
| compile_cycles recommendation text | Replace `"Add common build/test commands to settings.json allowlist"` with batching/iterative-compilation framing. `permission_friction_events` recommendation confirmed independent — no cross-contamination | AC-19 | architecture/ADR-005-compile-cycles-recommendation-fix.md |
| `pass_number` + `pass_count` on PhaseStats | Both fields are present: `pass_number: u32` (1-indexed, identifies which pass this row represents) and `pass_count: u32` (total number of passes for this phase name, used for rework detection). This resolves VARIANCE 1 from the Alignment Report | VARIANCE 1 resolved by spawn prompt | architecture/ARCHITECTURE.md |
| `total_served` + `total_stored` on FeatureKnowledgeReuse | Both fields are present and distinct: `total_served: u64` (all distinct entries delivered across sessions) and `total_stored: u64` (entries created during this cycle). This resolves VARIANCE 2 from the Alignment Report | VARIANCE 2 resolved by spawn prompt | architecture/ARCHITECTURE.md |
| GateResult word-boundary matching | `contains()` substring matching is used per spec. The "compass" → false-positive case (R-03 scenario 8) is a documented known fragility, not blocked at design time | R-03 | architecture/ARCHITECTURE.md |
| "No phase information captured" detection | Simple per-cycle check: if `cycle_events` returns zero rows for this `cycle_id`, show the note. No cross-cycle comparison | SR-06 | architecture/ARCHITECTURE.md |
| `category_gaps` field retirement | Suppress in formatter only. Field stays on `FeatureKnowledgeReuse` struct to avoid breaking JSON consumers | AC-12 | product/features/col-026/SCOPE.md |
| What Went Well metric direction table | SPECIFICATION §FR-11 with 16 metrics is the canonical table (superset of ARCHITECTURE.md's 10). R-06 requires all 16 tested | ALIGNMENT-REPORT.md | architecture/ARCHITECTURE.md |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-observe/src/types.rs` | Modify | Add `goal`, `cycle_type`, `attribution_path`, `is_in_progress`, `phase_stats` to `RetrospectiveReport`; add new structs `PhaseStats`, `ToolDistribution`, `GateResult`, `EntryRef`; extend `FeatureKnowledgeReuse` with `total_served`, `total_stored`, `cross_feature_reuse`, `intra_cycle_reuse`, `top_cross_feature_entries` |
| `crates/unimatrix-server/src/mcp/tools.rs` | Modify | Add handler steps 10h (PhaseStats computation) and 10i (`get_cycle_start_goal`, `is_in_progress` derivation, `attribution_path` assignment); extend `compute_knowledge_reuse_for_sessions` call site with `entry_meta_lookup` batch closure; make `cycle_ts_to_obs_millis` `pub(crate)` if needed |
| `crates/unimatrix-server/src/services/observation.rs` | Modify | Change `cycle_ts_to_obs_millis` visibility to `pub(crate)` if `compute_phase_stats` is extracted to a separate module |
| `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` | Modify | Add `entry_meta_lookup` second closure parameter; add `EntryMeta` struct; populate `cross_feature_reuse`, `intra_cycle_reuse`, `total_stored`, `total_served`, `top_cross_feature_entries` fields; update existing construction sites of `FeatureKnowledgeReuse` |
| `crates/unimatrix-server/src/mcp/response/retrospective.rs` | Modify | Reorder sections (10 → new order below); rebrand header; add Phase Timeline, What Went Well; extend Knowledge Reuse rendering; add burst notation for evidence; add phase annotations to findings; add `format_claim_with_baseline()` for threshold language post-processing; extend session table with tool distribution + agents columns; add Top file zones line |
| `crates/unimatrix-observe/src/report.rs` | Modify | Replace `compile_cycles` recommendation text at lines 62 and 88 (ADR-005); update test assertion for `test_recommendation_compile_cycles_above_threshold` |

---

## New Section Order (formatter)

```
1. Header (rebranded + goal / cycle_type / attribution / status)
2. Recommendations (moved from position 9)
3. Phase Timeline (new)
4. What Went Well (new)
5. Sessions (existing, enhanced with tool distribution + agents)
6. Findings (existing, + phase annotation + burst notation)
7. Baseline Outliers (existing)
8. Phase Outliers (existing)
9. Knowledge Reuse (extended)
10. Rework & Context Reload (existing)
11. Phase Narrative (existing)
```

---

## Data Structures

### New structs in `unimatrix-observe/src/types.rs`

```rust
pub struct PhaseStats {
    pub phase: String,
    pub pass_number: u32,          // 1-indexed; which pass this row is
    pub pass_count: u32,           // total passes for this phase name (>1 = rework)
    pub duration_secs: u64,
    pub session_count: usize,
    pub record_count: usize,
    pub agents: Vec<String>,       // deduplicated, first-seen order
    pub tool_distribution: ToolDistribution,
    pub knowledge_served: u64,
    pub knowledge_stored: u64,
    pub gate_result: GateResult,
    pub gate_outcome_text: Option<String>,
    pub hotspot_ids: Vec<String>,  // e.g. ["F-01", "F-02"]; populated by formatter
}

pub struct ToolDistribution {
    pub read: u64,
    pub execute: u64,
    pub write: u64,
    pub search: u64,
}

pub enum GateResult {
    Pass,
    Fail,
    Rework,
    Unknown,
}

pub struct EntryRef {
    pub id: u64,
    pub title: String,
    pub feature_cycle: String,
    pub category: String,
    pub serve_count: u64,
}
```

Derive: `Debug, Clone, Serialize, Deserialize`. `GateResult` and `ToolDistribution` use
`#[serde(default)]` on all fields. `PhaseStats` fields are all required.

### New fields on `RetrospectiveReport`

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub goal: Option<String>,

#[serde(default, skip_serializing_if = "Option::is_none")]
pub cycle_type: Option<String>,

#[serde(default, skip_serializing_if = "Option::is_none")]
pub attribution_path: Option<String>,

#[serde(default, skip_serializing_if = "Option::is_none")]
pub is_in_progress: Option<bool>,

#[serde(default, skip_serializing_if = "Option::is_none")]
pub phase_stats: Option<Vec<PhaseStats>>,
```

### New fields on `FeatureKnowledgeReuse`

```rust
#[serde(default)]
pub total_served: u64,           // all distinct entry IDs served across sessions

#[serde(default)]
pub total_stored: u64,           // entries created during this cycle

#[serde(default)]
pub cross_feature_reuse: u64,    // entries from prior feature cycles

#[serde(default)]
pub intra_cycle_reuse: u64,      // entries stored during this cycle and served

#[serde(default)]
pub top_cross_feature_entries: Vec<EntryRef>,  // top-5 by serve_count
```

### New `EntryMeta` in `knowledge_reuse.rs` (not public API)

```rust
pub struct EntryMeta {
    pub title: String,
    pub feature_cycle: Option<String>,
    pub category: String,
}
```

---

## Function Signatures

### Handler additions (`tools.rs`)

```rust
// Step 10h: pure computation, no DB access
fn compute_phase_stats(
    events: &[CycleEventRecord],
    attributed: &[ObservationRecord],
) -> Vec<PhaseStats>

// Step 10i: async DB read
// get_cycle_start_goal already exists in unimatrix-store/src/db.rs
// is_in_progress derived from events slice in-memory
```

### Extended `compute_knowledge_reuse` (`knowledge_reuse.rs`)

```rust
pub fn compute_knowledge_reuse(
    query_logs: &[QueryLogRecord],
    injection_logs: &[InjectionLogRecord],
    active_categories: &[String],
    current_feature_cycle: &str,
    entry_category_lookup: impl Fn(u64) -> Option<String>,
    entry_meta_lookup: impl Fn(&[u64]) -> HashMap<u64, EntryMeta>,
) -> FeatureKnowledgeReuse
```

The `entry_meta_lookup` closure is called exactly once per invocation after all distinct entry IDs
are collected. When the ID set is empty, the call is skipped. Chunking (100 IDs per IN-clause) is
handled at the call site in `tools.rs`.

### Formatter additions (`retrospective.rs`)

```rust
// Private — post-process claim string to remove threshold language
fn format_claim_with_baseline(
    claim: &str,
    rule_name: &str,
    measured: f64,
    threshold: f64,
    baseline_comparison: &[BaselineComparison],
) -> String
```

---

## Constraints

- **No schema migration**: schema v16 is final for col-026. No new tables or columns.
- **No new MCP tools**: `context_cycle_review` signature unchanged.
- **`cycle_ts_to_obs_millis()` only**: all timestamp conversion from `cycle_events` seconds to
  observation milliseconds must call this function. Inline `* 1000` is prohibited (ADR-002).
- **Batch metadata lookup**: `entry_meta_lookup` closure receives the full ID slice; per-entry
  calls to `store.get()` inside `compute_knowledge_reuse` are prohibited (ADR-003).
- **`is_in_progress: Option<bool>` only**: a plain `bool` field is prohibited (ADR-001).
- **Formatter owns rendering**: threshold language replacement and all new section rendering live
  in `response/retrospective.rs`. Detection rules in `unimatrix-observe/src/detection/` are not
  modified (ADR-004).
- **Fire-and-forget error boundary**: PhaseStats computation (step 10h) and `get_cycle_start_goal`
  (step 10i) follow the same error pattern as steps 10g–17: `tracing::warn!` on failure, fields
  set to `None`, handler continues normally.
- **Test infrastructure**: extend existing fixtures in `retrospective.rs` test module and
  `knowledge_reuse.rs`. Do not create isolated scaffolding. Phase Timeline tests use the `infra-001`
  pattern (cycle_events seeded via UDS-only write path).
- **`FeatureKnowledgeReuse` construction sites**: three sites must be updated when new fields are
  added (`types.rs` test fixtures, `knowledge_reuse.rs` production, `retrospective.rs` test
  fixtures). Compile-time-enforced migration — `cargo build` must pass before commit.
- **Backward compatibility**: all new fields on `RetrospectiveReport` and `FeatureKnowledgeReuse`
  use `#[serde(default)]`. The `category_gaps` field is retained on the struct but not rendered.
- **`events` slice is borrowed, not moved**: both `build_phase_narrative` (step 10g) and
  `compute_phase_stats` (step 10h) borrow `&[CycleEventRecord]`; ownership must not be transferred
  into step 10g.

---

## Dependencies

### Upstream features (hard block — must merge to `main` before col-026 branches)

| Feature | What col-026 consumes |
|---------|-----------------------|
| col-024 | `cycle_ts_to_obs_millis(fn(i64)->i64)` in `services/observation.rs`; `load_cycle_observations` three-path attribution; `CycleEventRecord` type in `unimatrix-observe/src/types.rs` |
| col-025 | `get_cycle_start_goal(&self, cycle_id: &str) -> Result<Option<String>>` in `unimatrix-store/src/db.rs`; `cycle_events.goal` column (schema v16) |

### Assumed API surface pinned from col-024

| Symbol | Location | Signature |
|--------|----------|-----------|
| `cycle_ts_to_obs_millis` | `services/observation.rs` line ~495 | `fn cycle_ts_to_obs_millis(ts_secs: i64) -> i64` |
| `CycleEventRecord` | `unimatrix-observe/src/types.rs` line ~231 | fields: `seq, event_type, phase, outcome, next_phase, timestamp` |

### Assumed API surface pinned from col-025

| Symbol | Location | Signature |
|--------|----------|-----------|
| `get_cycle_start_goal` | `unimatrix-store/src/db.rs` line ~354 | `pub async fn get_cycle_start_goal(&self, cycle_id: &str) -> Result<Option<String>>` |
| `cycle_events.goal` | schema v16 | `TEXT NULL` on `cycle_start` rows only |

### Crate dependencies (no new additions)

- `unimatrix-observe` — types, detection, report
- `unimatrix-store` — DB access (`get_cycle_start_goal`, batch IN-clause)
- `unimatrix-server` — handler, formatter, knowledge_reuse
- `rusqlite` — parameterized batch IN-clause queries
- `serde` — backward-compatible struct extensions
- `tracing` — `warn!` on step failure

---

## NOT in Scope

- Per-CycleType baseline comparison (requires many typed-cycle retrospectives to accumulate)
- Phase velocity trend (same data accumulation requirement)
- Phase knowledge profile anomaly detection (requires per-phase-type expected profiles)
- Rework phase per-pass diff (higher-effort; pass duration/records are covered by Phase Timeline)
- Changing the MCP tool name `context_cycle_review`
- Schema changes (v16 is final)
- Goal-contextualized hotspot severity adjustment (deferred)
- Session-level `entries_analysis` / knowledge health section in markdown
- PreCompact hook content improvements (GH#309)
- `#[non_exhaustive]` on `FeatureKnowledgeReuse` (all construction sites are known and updated)

---

## Alignment Status

**Overall**: PASS with two resolved variances and two minor open notes.

### Resolved variances (per spawn prompt)

**VARIANCE 1** (`PhaseStats` struct field mismatch): Resolved. Both `pass_number: u32`
(1-indexed per-row label) and `pass_count: u32` (total passes for rework detection) are present on
`PhaseStats`. This satisfies both the Phase Timeline row labelling requirement from ARCHITECTURE.md
and the rework annotation requirement from the specification.

**VARIANCE 2** (`total_served` / `total_stored` naming divergence): Resolved. Both fields are
present and distinct: `total_served: u64` (all distinct entries delivered across sessions) and
`total_stored: u64` (entries created during this cycle). Both added to `FeatureKnowledgeReuse` with
`#[serde(default)]`.

### Open notes (non-blocking)

**Gap 1 — Threshold audit scope**: SCOPE goal 6 says "all findings"; SPECIFICATION §FR-14 names 3
files; ARCHITECTURE.md §Component 5 enumerates 9 specific lines across ~4 files. The ARCHITECTURE
enumeration is the operative audit scope. Implementation agents must treat the 9-line list as
exhaustive and apply the formatter's general regex (ADR-004) as a catch-all for any sites missed
by the enumeration (see R-11 for the future-regression test).

**Gap 2 — Golden-output snapshot**: SR-04 recommends a byte-level golden-output snapshot test.
SPECIFICATION AC-11 / NFR-05 implement ordering checks instead. The R-07 section-ordering test
(verify all section headers appear in sequence) is the operative gate artifact. A full byte-level
snapshot is not mandated but may be added by the test agent at their discretion.

**Note — GateResult "compass" edge case**: R-03 scenario 8 documents that `contains("pass")` would
match the word "compass." The spec uses `contains()` matching. This is a known fragility accepted
at design time; word-boundary matching may be added by the implementation agent if they consider
it low-risk.

**Note — Metric direction table**: SPECIFICATION §FR-11 with 16 metrics is canonical. ARCHITECTURE.md
§Component 5 lists 10. Implementation agents must use the spec's 16-entry table for the What Went
Well section and test all 16 directions per R-06.
