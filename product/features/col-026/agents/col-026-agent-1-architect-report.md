# col-026 Architect Report — Agent col-026-agent-1-architect

## Outputs

### ARCHITECTURE.md
`/workspaces/unimatrix/product/features/col-026/architecture/ARCHITECTURE.md`

### ADR Files

| File | Unimatrix ID |
|------|-------------|
| `architecture/ADR-001-is-in-progress-option-bool.md` | #3421 |
| `architecture/ADR-002-cycle-ts-to-obs-millis-mandatory.md` | #3422 |
| `architecture/ADR-003-batch-entry-meta-lookup.md` | #3423 |
| `architecture/ADR-004-formatter-threshold-language-replacement.md` | #3424 |
| `architecture/ADR-005-compile-cycles-recommendation-fix.md` | #3425 |

---

## Key Design Decisions

### 1. is_in_progress is Option<bool> (ADR-001)
Resolves SR-03. Three states required: None (no cycle_events = unknown), Some(false) (confirmed
complete), Some(true) (open cycle). `bool` with `#[serde(default)]` would silently produce
`false` for all pre-col-024 cycles, asserting "confirmed complete" where evidence is absent.

### 2. cycle_ts_to_obs_millis is the only permitted conversion (ADR-002)
Resolves SR-01. PhaseStats computation filters attributed observations by cycle_events time
windows. The unit mismatch (seconds vs millis) must be bridged exclusively via
`cycle_ts_to_obs_millis()` from `services/observation.rs`. Inline `* 1000` is prohibited. The
function needs `pub(crate)` if extracted to a separate module.

### 3. Batch IN-clause for entry metadata (ADR-003)
Resolves SR-02. `compute_knowledge_reuse` gains a second closure `entry_meta_lookup: impl
Fn(&[u64]) -> HashMap<u64, EntryMeta>` called once per invocation with all entry IDs. The
caller issues a single SQL IN-clause query (chunked at 100 IDs per pattern #883). Per-entry
`get()` calls are structurally impossible.

### 4. Threshold language replacement is formatter-side post-processing (ADR-004)
Detection rules are untouched. `format_claim_with_baseline()` in `render_findings()` strips
threshold references and appends either `(baseline: mean ±stddev, +Nσ)` or `(N× typical)`.
JSON consumers see the original unmodified claim strings. Nine claim sites enumerated as the
complete audit scope.

### 5. compile_cycles recommendation fix (ADR-005)
The "add to allowlist" text in `report.rs` is wrong for skip-permissions mode. Replace with
iterative-struct-change framing. The compile_cycles and permission_friction_events paths are
confirmed independent.

### 6. PhaseStats computation inserted as step 10h
Uses already-loaded `events: Vec<CycleEventRecord>` (step 10g) and `attributed:
Vec<ObservationRecord>` (step 3). No new DB queries. Error boundary wraps full computation;
failure leaves `phase_stats = None` with a tracing::warn. Same best-effort pattern as steps
11–17.

### 7. New types: PhaseStats, ToolDistribution, GateResult, EntryRef
All defined in `unimatrix-observe/src/types.rs`. GateResult is an enum inferred from
gate_outcome_text keywords (pass/success → Pass; fail/error → Fail; rework → Rework; else →
Unknown). ToolDistribution is a named struct with four fields (not a HashMap). EntryRef is a
named struct (not a tuple).

### 8. formatter section order
Recommendations move to position 2 (immediately after header). Phase Timeline at 3, What Went
Well at 4, Findings at 5. Sessions table is enhanced (tool distribution + agents columns) but
remains. Phase Narrative stays at position 10.

### 9. What Went Well metric direction table
10-metric table with lower-is-better / higher-is-better classifications. Only `BaselineStatus::Normal` metrics with ≥3 samples qualify. Section omitted when no candidates.

### 10. Burst notation max entries: 10
Maximum 10 burst entries before truncating with `...`. Relative time origin is earliest evidence
timestamp across the finding. Peak line comes from `HotspotNarrative.clusters` (already
computed).

---

## SR-06 Resolution

"No phase information captured" is shown whenever `cycle_events` returns zero rows for the
specific `cycle_id`. No cross-check against other cycles' event presence. Simple, consistent
with `phase_narrative = None` behavior.

## SR-07 Pinned API Surface

col-024: `cycle_ts_to_obs_millis(i64) -> i64` at `services/observation.rs:495` (function
visibility change to `pub(crate)` required); `CycleEventRecord` fields unchanged.

col-025: `get_cycle_start_goal(&self, cycle_id: &str) -> Result<Option<String>>` at `db.rs:354`;
`cycle_events.goal` column as `TEXT NULL` on cycle_start rows.

## SR-08 Resolution

`FeatureKnowledgeReuse {}` is constructed at 3 production sites and 4 test sites. Adding
`#[serde(default)]` new fields is backward-compatible for JSON but requires updating all Rust
construction sites (compile-time enforcement, not a hidden runtime issue). `#[non_exhaustive]`
is not warranted — the construction sites are all internal to the workspace.

---

## Open Questions for Spec Writer

1. **total_stored source**: ARCHITECTURE.md specifies `total_stored` comes from counting
   `feature_entries` rows where `feature_id = current_cycle`. The query is already available in
   step 10g (cross-cycle distribution query). The spec writer should confirm whether to reuse
   that query's count or add a dedicated `COUNT(*)` query. Either works; the spec should pin it.

2. **attribution_path value in cached path**: When `is_cached = true`, no attribution path is
   used. Should `attribution_path` be `None` on cached reports, or a sentinel like
   `"cached (no attribution)"`. This is a minor formatting decision the spec writer can decide.

3. **What Went Well metric descriptions**: The formatter needs hardcoded plain-text descriptions
   for each metric (e.g., "above-average concurrency" for `parallel_call_rate`). These are
   presentational only but need to be specified. The sample review in `SAMPLE-REVIEW.md` shows
   examples for all 7 that fired — the spec writer should use those as the canonical list.

4. **Phase Timeline column ordering for rework phases**: When a phase appears twice (rework),
   should both rows be shown separately in the table (with `pass_number` shown), or should they
   be merged into one row with `pass_count = 2`? The SAMPLE-REVIEW.md shows merged rows. The
   spec should confirm this explicitly for implementation.
