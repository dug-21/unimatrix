# Agent Report: col-026-agent-1-pseudocode

## Summary

Produced 6 pseudocode files covering all 5 components of col-026. All source documents
read before writing. All interface names traced to architecture output.

## Files Produced

| File | Lines | Component |
|------|-------|-----------|
| `pseudocode/OVERVIEW.md` | 164 | Cross-cutting: data flow, shared types, sequencing |
| `pseudocode/retrospective-report-extensions.md` | 252 | Component 1: types.rs additions |
| `pseudocode/phase-stats.md` | 503 | Component 2: compute_phase_stats, steps 10h/10i |
| `pseudocode/knowledge-reuse-extension.md` | 445 | Component 3: batch lookup, new fields |
| `pseudocode/formatter-overhaul.md` | 877 | Component 4: 12-section formatter overhaul |
| `pseudocode/recommendation-fix.md` | 163 | Component 5: compile_cycles text fix |

## Open Questions / Gaps Found

**GAP-1 — PhaseStats missing start_ms/end_ms** (Medium severity):
The formatter needs to map finding evidence timestamps to phase windows for phase annotations
(FR-09). `PhaseStats` as defined in Component 1 has no `start_ms`/`end_ms` fields. The
formatter cannot reconstruct boundaries from `duration_secs` alone. Recommended fix: add
`start_ms: i64` and `end_ms: Option<i64>` to `PhaseStats` struct (or a server-side shadow
struct). Flag in formatter pseudocode at `build_phase_annotation_map`. Implementation agent
must resolve before implementing the phase annotation logic.

**GAP-2 — CollapsedFinding does not store EvidenceCluster data** (Low severity):
FR-10 burst notation prefers narrative cluster data over raw evidence re-bucketing. The
`CollapsedFinding` struct (private to retrospective.rs) currently stores `cluster_count`
but not `Vec<EvidenceCluster>`. Implementation agent should add this field to get precise
cluster boundaries in burst notation. Fallback (5-min bucketing of raw evidence) is correct
per spec but less precise.

**GAP-3 — EntryRef field name conflict: `feature_cycle` vs `source_cycle`** (Low severity):
ARCHITECTURE.md and IMPLEMENTATION-BRIEF use `feature_cycle: String`.
SPECIFICATION §Domain Models uses `source_cycle: String`. Implementation agent must choose
one name and use it consistently across types.rs, knowledge_reuse.rs, and retrospective.rs.
The architecture document wins per this agent's role boundary.

**GAP-4 — PhaseStats.pass_breakdown field** (Low severity):
SPECIFICATION §Domain Models includes `pass_breakdown: Vec<(u64, u64)>` on `PhaseStats`.
IMPLEMENTATION-BRIEF and ARCHITECTURE.md do not include this field. Not added to pseudocode.
Implementation agent should confirm with the feature architect whether to include it.

**GAP-5 — compute_knowledge_reuse signature migration** (Implementation note):
The existing `entry_category_lookup` closure parameter is superseded by the new
`entry_meta_lookup` closure (which contains category). The pseudocode retains both for
backward compatibility. Implementation agent may simplify by removing the old closure if
all callers can be updated cleanly. The pseudocode notes the synthesis approach.

**GAP-6 — cycle_ts_to_obs_millis return type** (Minor):
SPECIFICATION line 605 says `cycle_ts_to_obs_millis(ts: i64) -> u64` but
ARCHITECTURE.md §Integration Points says `fn cycle_ts_to_obs_millis(ts_secs: i64) -> i64`.
The current implementation (col-024) uses `i64` return. Pseudocode uses `i64`.
Implementation agent must verify the actual type in `services/observation.rs` line ~495.

## Knowledge Stewardship

- Queried: /uni-query-patterns for retrospective formatter — found #3426 (golden-output regression risk), #3420 (Option<bool> pattern), #949 (domain-specific formatter module pattern), #298 (generic formatter pattern)
- Queried: /uni-query-patterns for col-026 architectural decisions — found ADRs #3421-#3425 matching the 5 feature ADRs
- Pattern #3426 applied: section-order regression risk documented in formatter pseudocode (T-FM-01); golden-output note forwarded to implementation
- Pattern #3420 applied: Option<bool> enforced throughout (ADR-001 compliance)
- Pattern #949 applied: all rendering stays in retrospective.rs (ADR-004 from vnc-011 #952)

## Deviations from established patterns

- None. All patterns applied as found.

## ADR Compliance Verification

| ADR | Constraint | Pseudocode Location |
|-----|-----------|---------------------|
| ADR-001 | `is_in_progress: Option<bool>` only | Component 1 (struct), Component 2 (`derive_is_in_progress`) |
| ADR-002 | `cycle_ts_to_obs_millis()` only for ts conversion | Component 2 phase-stats.md — every boundary conversion explicitly calls the function |
| ADR-003 | `entry_meta_lookup: Fn(&[u64]) -> HashMap` called once | Component 3 — `if all_entry_ids.is_empty() { skip }; else { call once }` |
| ADR-004 | Formatter-only threshold replacement | Component 4 — `format_claim_with_baseline` in retrospective.rs only |
| ADR-005 | compile_cycles recommendation text replacement | Component 5 — both sites in report.rs |
