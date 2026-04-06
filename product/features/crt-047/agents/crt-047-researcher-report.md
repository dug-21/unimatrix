# crt-047 Researcher Report

## Summary

SCOPE.md written to `product/features/crt-047/SCOPE.md`.

---

## Key Findings

### 1. Schema placement decision: cycle_review_index, not cycle_events

The issue brief suggested adding curation columns to `cycle_events`. After examining
both tables, `cycle_review_index` is the correct target:

- `cycle_events` is a structural audit trail for in-flight hook events (per-row: seq,
  event_type, phase, outcome, timestamp). Adding derived aggregates here would pollute
  the event stream with review-time computed data.
- `cycle_review_index` is written atomically once per cycle at `context_cycle_review`
  call time via `INSERT OR REPLACE` in `store_cycle_review()`. This is the right place
  for per-cycle aggregates computed at review time.
- Consequence: the five columns (`corrections_total`, `corrections_agent`,
  `corrections_human`, `deprecations_total`, `orphan_deprecations`) belong on
  `cycle_review_index`, not `cycle_events`.

### 2. trust_source values in production

`context_correct` hard-codes `trust_source: "agent"` for all agent-called corrections
(tools.rs ~L637, ~L830). The correcting entry (the new one that supersedes the old)
carries this value. Known values: `"agent"`, `"human"`, `"system"`, `"direct"`. The
agent/human bucketing rule is an open question — `"system"` writes (cortical implant
lesson-learned) should not count as agent corrections. ADR-gated.

### 3. Correction and orphan deprecation queries

Corrections: `SELECT ... FROM entries WHERE feature_cycle = ? AND supersedes IS NOT NULL`
grouped by `trust_source`. The correcting entry's `feature_cycle` is the cycle in which
the correction was made — unambiguous.

Orphan deprecations: `SELECT COUNT(*) FROM entries WHERE feature_cycle = ? AND status = 1
AND superseded_by IS NULL`. Uses the deprecated entry's original `feature_cycle` — no
audit log join needed.

### 4. Existing σ baseline infrastructure

`unimatrix_observe::baseline` already implements the exact statistical machinery needed:
`compute_entry()` (population stddev), `BaselineEntry { mean, stddev, sample_count }`,
`BaselineStatus` enum with four modes, 1.5σ threshold. The new curation baseline is a
parallel function in `unimatrix-server/services/` (server→store is established direction)
— it reads snapshot columns from `cycle_review_index` rows rather than `MetricVector`
history. MIN_HISTORY = 3 is the established minimum.

### 5. SUMMARY_SCHEMA_VERSION bump required

Currently `1` in `cycle_review_index.rs`. Must bump to `2` when curation columns are
added. This triggers the existing advisory mechanism on stale memoized records. The bump
affects all existing `cycle_review_index` rows — all historical cycles will show advisory
until force-recomputed. The blast radius is limited to advisory text, not data loss.

### 6. Current schema version: 23

`CURRENT_SCHEMA_VERSION = 23` (bugfix-509, compound index). This feature introduces v24.
The five-column migration uses the established `pragma_table_info` pre-check pattern
(crt-043 ADR-003, entry #4088) with `DEFAULT 0` on all new INTEGER columns.

### 7. context_status phase structure

`compute_report()` has 8+ numbered phases. The curation health block is a new phase
(Phase 7c by convention, after Phase 7b pending cycle reviews). It reads the last N
`cycle_review_index` rows by `computed_at DESC` using `read_pool()`.

### 8. #520 relationship confirmed

GH #520 (drop Lambda freshness dimension) is a parallel design concern. crt-047 is
explicitly complementary and independent — no blocking dependency.

---

## Open Questions Surfaced

- OQ-01: trust_source bucketing: does `"system"` count as agent, human, or excluded?
- OQ-02: Deprecation attribution: confirmed using deprecated entry's `feature_cycle`
  (no audit log join), but needs explicit human confirmation.
- OQ-03: σ threshold: confirm 1.5σ (same as unimatrix_observe::baseline) or different.
- OQ-04: Trend direction lookback window (last-5 vs. prior-5, or fixed 20-cycle lookback).
- OQ-05: Cold-start threshold: 3 cycles (same as MIN_HISTORY) or higher (5+) for stability.
- OQ-06: force=true recomputation: confirm snapshot recomputed on force-recompute (expected yes).

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned 18 entries; entries #3793, #3794,
  #3802, #3420, #4047, #4088 were directly relevant (crt-033 ADR-001/002/004, col-026
  Option<bool> pattern, crt-043 migration atomicity, audit schema extension pattern).
- Stored: entry #4178 "Derived aggregate columns for cycle_review_index belong there, not
  on cycle_events" via /uni-store-pattern — generalizable to any future per-cycle aggregate.
