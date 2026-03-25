# Agent Report: col-026-agent-2-spec

**Agent**: col-026-agent-2-spec
**Task**: Write SPECIFICATION.md for col-026 Unimatrix Cycle Review Enhancement
**Artifact**: `product/features/col-026/specification/SPECIFICATION.md`

---

## Summary

Specification written covering all 19 acceptance criteria from SCOPE.md. The specification translates the three implementation layers (formatter-only, struct extensions, PhaseStats + knowledge reuse fix) into numbered functional requirements with testable assertions, defines the complete domain model including all new types, and maps every AC-ID to a concrete verification method.

---

## Key Decisions Made

### 1. `Option<bool>` for `is_in_progress` (SR-03)

Used `Option<bool>` per the risk assessment recommendation. Three-state semantics: `None` (no cycle_events — historical pre-col-024), `Some(true)` (in-progress), `Some(false)` (confirmed complete). Plain `bool` with `#[serde(default)]` would silently report all historical retros as confirmed-complete. This aligns with Unimatrix entry #3420.

### 2. CycleType and AttributionPath as `Option<String>` (not enums)

Represented as `Option<String>` rather than Rust enums to avoid future serde breakage when new values are added. Valid string constants are enumerated in the spec. This sidesteps the `#[non_exhaustive]` complexity while keeping the values well-defined.

### 3. Threshold Audit Scope Made Explicit (SR-05)

Rather than a blanket "complete threshold audit" requirement, the spec enumerates exactly two user-facing files with threshold strings that must change: `report.rs` line 88 (compile_cycles rationale) and `detection/agent.rs` line 474 (edit_bloat claim). Detection constant names in Rust source are scoped as internal-only and excluded from AC-13. This gives gate reviewers a verifiable exit criterion.

### 4. GateResult Derivation from Outcome String

`GateResult` is derived by case-insensitive substring matching against the `cycle_phase_end.outcome` text field. String-based matching is appropriate because `outcome` is free-text. Strings containing "pass", "complete", or "approved" → `Pass`; "fail" → `Fail`; multi-pass with final success → `Rework`; otherwise `Unknown`.

### 5. Burst Cluster Window Fixed at 5 Minutes

The Timeline burst grouping window is fixed at 5 minutes. This matches the evidence in SAMPLE-REVIEW.md and provides a deterministic, testable grouping without requiring dynamic window sizing.

### 6. "No Phase Information Captured" Scope

The note is shown whenever there are no `cycle_events` rows for the cycle, regardless of whether other cycles have rows. The SCOPE.md resolved open question (§Open Questions item 3) specified a simpler check, not a "was this cycle expected to have phase data" check. This avoids introducing additional DB queries.

### 7. Section Order Anchored

The new section order is explicitly numbered in FR-12 with the Recommendations section moved to position 2 (immediately after header). This was implicit in SCOPE.md goal 10 but needed explicit ordering to prevent formatter regression.

### 8. EntryRef as Struct, Not Tuple

`top_cross_feature_entries` uses a named `EntryRef` struct rather than `Vec<(u64, String, u64)>`. This produces readable JSON, avoids positional confusion between fields, and is extensible.

---

## Open Questions for Architect

1. **`PhaseStats` module location**: Should `PhaseStats`, `GateResult`, and `EntryRef` live in `unimatrix-observe/src/types.rs` (alongside existing types) or in a new `unimatrix-observe/src/phase_stats.rs` module? The file is already large (~780 lines). Recommendation: new module, re-exported from `lib.rs`.

2. **`FeatureKnowledgeReuse` exhaustive construction audit**: The spec flags this as a required check. `category_gaps` field stays (no struct breaking change), but the new fields (`total_served`, `cross_feature_reuse`, `intra_cycle_reuse`, `top_cross_feature_entries`) mean any exhaustive construction site outside `unimatrix-observe` must be updated. Architect should confirm no such sites exist (or add `#[non_exhaustive]`).

3. **Phase window derivation for rework passes**: When a phase is entered twice (e.g., `implementation` appears twice in `phase_sequence`), the Phase Timeline needs two time windows for the same phase name. The `cycle_events` query returns events ordered by `(timestamp ASC, seq ASC)`. The architect should specify exactly how to split same-named phase windows into pass 1 and pass 2 — specifically whether to use `seq` order or timestamp order when phase names repeat. The SCOPE.md states `seq` is advisory (#2999); timestamps are authoritative.

4. **`pass_breakdown` field on `PhaseStats`**: The rework annotation (FR-07) needs per-pass duration and record count. `pass_breakdown: Vec<(u64, u64)>` (indexed tuples) is compact but may be confusing. An alternative: `Vec<PhasePass { duration_secs, record_count }>`. Architect should decide which form to implement.

---

## Knowledge Stewardship

Queried: /uni-query-patterns for retrospective formatter acceptance criteria, cycle_events PhaseStats time window observation lookup, serde backward compatibility new fields — findings:
- #3383 (×2): cycle_events-first observation lookup algorithm with time windows — directly applicable to Phase Timeline computation
- #952: ADR-003 retrospective formatter module boundary — all rendering stays in `response/retrospective.rs`
- #3420: `Option<bool>` for event-derived status fields on `RetrospectiveReport` — directly incorporated as `is_in_progress` type decision
- #3255: `serde(default)` + `skip_serializing_if` wire-optional field pattern — applied to all new `RetrospectiveReport` fields
- #3373, #3375: col-024 fallback logging and open-ended window ADRs — context for attribution path detection and in-progress derivation
- #646, #923: serde backward-compat patterns — confirmed approach for `FeatureKnowledgeReuse` new fields
