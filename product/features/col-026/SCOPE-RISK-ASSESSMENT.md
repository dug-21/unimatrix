# Scope Risk Assessment: col-026

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `ts_millis` vs. `timestamp` (seconds) unit mismatch in PhaseStats computation — `cycle_events` uses epoch seconds; observations use `ts_millis`. col-024 solved this via `cycle_ts_to_obs_millis()`; implementation agents who build PhaseStats from scratch may re-introduce the conversion gap, producing silently wrong phase boundaries with no type-system guard | High | High | Architect must specify that PhaseStats computation MUST call `cycle_ts_to_obs_millis()` from col-024 (#3383); the spec must include a test case where phase boundary is verified against known ms-level timestamps |
| SR-02 | N+1 DB read pattern in knowledge reuse cross-feature split — pre-fetching `feature_cycle` for each served entry requires either N individual `get()` calls or a batch query. Existing pattern #883 (Chunked Batch Scan) exists but is not wired into `compute_knowledge_reuse_for_sessions`. N individual reads on a cycle with 40+ served entries is a measurable latency regression on the `context_cycle_review` hot path | High | Med | Architect must mandate a batch fetch (IN clause or chunked scan per #883) for `feature_cycle` pre-fetch, not a per-entry get. Spec must state max acceptable added latency (suggest ≤50ms for batch ≤100 entries) |
| SR-03 | `is_in_progress: bool` default semantics — all historical retro calls (pre-col-024/025) lack a `cycle_stop` event and will default `is_in_progress = false` via `#[serde(default)]`. This is wrong: absence of `cycle_stop` in older cycles means "events not tracked", not "cycle complete". Callers parsing JSON will misread older retros as confirmed-complete cycles | Med | High | Use `Option<bool>` (`None` = no cycle_events, `true` = open, `false` = confirmed stopped). Architect should decide this before spec is written — it affects the struct definition and all downstream JSON consumers |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Formatter overhaul blast radius — 10 sections, reordering + additions + modifications in one feature. The SCOPE lists 11 goals across all three layers simultaneously. Sections not explicitly targeted (e.g., `render_sessions`, `render_phase_outliers`, `render_rework`) are touched adjacently and at risk of regression. No existing golden-output tests exist to catch formatter regressions | Med | High | Spec must define section ordering as an explicit acceptance criterion with a golden-output fixture (or snapshot test) covering the full rendered output, not just individual sections. Recommend Layer 1 (formatter-only) as a first merge-able unit before Layer 2/3 |
| SR-05 | Threshold language audit completeness — AC-13 requires "all" hotspot claim strings to drop threshold language. Some claim strings are dynamically composed (e.g., format! macros with variable substitution); a text search may miss runtime-composed strings. "Audit complete" has no verifiable exit criterion at gate time | Med | Med | Spec must define a concrete audit scope: enumerate every file containing "threshold" string literals in `detection/` and `report.rs`, and require each one traced in the spec. A Grep-verified list, not a blanket "audit", is the gate artifact |
| SR-06 | "No phase information captured" note proliferation — all cycles predating col-022 (context_cycle launch) will show this note. If this tool is run retrospectively on older cycles, every report shows the note. This may degrade signal-to-noise for users who run retrospectives on historical work | Low | Med | Spec should clarify: show the note only when `cycle_events` table has rows for other cycles but none for this one (indicating the feature was skipped), vs. a simpler "no cycle_events rows at all" check. Architect to decide detection granularity |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | col-024/col-025 in-flight dependency — both features are on `feature/ass-032` and not yet merged. col-026 design is being locked now. If col-024 or col-025 ship breaking changes to `cycle_events` schema, `get_cycle_start_goal` signature, or `CycleEventRecord` type after col-026 spec is written, the spec's integration assumptions are invalidated with no automatic detection | High | Low | Architect must document exact API surface assumed from col-024/025 (function signatures, field names, types). If either feature changes those surfaces before merging, a spec amendment is required. col-026 implementation branch must be cut from `main` only after both merge |
| SR-08 | `FeatureKnowledgeReuse` public API change — new fields added to a public `unimatrix-observe` crate type. Any downstream consumer that pattern-matches or exhaustively destructures this struct will fail to compile. `#[serde(default)]` handles JSON; Rust struct construction does not | Med | Low | Spec must confirm no exhaustive struct construction of `FeatureKnowledgeReuse` exists outside `unimatrix-observe` itself. Architect to add `#[non_exhaustive]` if not already present, or audit all construction sites |

## Assumptions

- **§Constraints/Dependencies**: Assumes col-024 and col-025 are stable at the time col-026 design is locked. If they continue to evolve, interface assumptions encoded in the spec may drift.
- **§Proposed Approach/Layer 3**: Assumes `compute_knowledge_reuse_for_sessions` can be extended to accept a `feature_cycle` lookup closure without restructuring the call site. If the function signature change propagates into multiple callers, the scope of Layer 3 widens unexpectedly.
- **§Background Research/PhaseNarrative**: Assumes the `cycle_events` query at handler line 1590 already returns all rows needed for PhaseStats. If col-024 or col-025 modified this query's projection, the assumption may be stale.
- **§Goals/Goal 8**: Assumes `SessionSummary` already carries tool distribution and agents spawned. If those fields are absent or in a different form post-col-025, Layer 1 formatter work becomes Layer 2 struct work.

## Design Recommendations

- **SR-01 → Architect**: Make `cycle_ts_to_obs_millis()` a named dependency in the implementation brief. Do not leave unit conversion implicit.
- **SR-02 → Architect**: Specify the `feature_cycle` pre-fetch as a batch IN-clause query, not a closure that calls `get()` per entry. Reference pattern #883.
- **SR-03 → Architect**: Decide `is_in_progress` type before spec is written — `Option<bool>` avoids silent semantic corruption on historical retros.
- **SR-04 → Spec writer**: Define a golden-output snapshot test covering the full rendered markdown as an acceptance criterion. Layer the implementation (formatter-only first).
- **SR-05 → Spec writer**: Enumerate the exact files and line ranges that must be audited for threshold language. Make the audit list the gate artifact, not a subjective "complete audit."
- **SR-07 → Architect**: Pin the assumed API surface of col-024/025 explicitly. Cross-reference against their IMPLEMENTATION-BRIEFs at design time.
