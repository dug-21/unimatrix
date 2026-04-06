# crt-048 Researcher Agent Report

## Summary

Researched the Lambda freshness dimension problem and authored SCOPE.md for crt-048.
The design decision (Option 1 — drop freshness entirely) was already recorded in GH #520
by the feature owner. SCOPE.md translates that decision into a precise implementation
contract with 12 acceptance criteria.

## Key Findings

### Lambda freshness implementation (infra/coherence.rs)

- `confidence_freshness_score()`: pure function, scans all active entries, returns
  `(score, stale_count)` based on `max(updated_at, last_accessed_at)` vs a 24h staleness
  threshold (`DEFAULT_STALENESS_THRESHOLD_SECS`).
- `oldest_stale_age()`: also scans active entries; used only to produce the staleness
  recommendation string.
- `CoherenceWeights.confidence_freshness` = 0.35 — the single heaviest Lambda weight.
- `DEFAULT_WEIGHTS` sums to 1.0; removal of freshness leaves 0.65 to re-normalize across
  three remaining dimensions.

### Call sites (services/status.rs Phase 5, ~line 695)

Two distinct call sites:
1. Main Lambda computation (~line 695–777)
2. Per-source lambda in `coherence_by_source` loop (~line 793–804)

Both must be updated. The `active_entries` Vec (loaded for freshness scan) is still
needed for the coherence-by-source trust_source grouping — the store read stays.

### Response surface

Three output modes in `mcp/response/status.rs` expose `confidence_freshness_score` and
`stale_confidence_count`: text format, markdown format, and JSON struct. All three require
field removal. This is a breaking JSON API change (field disappears from output).

### crt-036 data model

crt-036 introduced `cycle_review_index` (one row per reviewed cycle, with `computed_at`
and `raw_signals_available`), cycle-linked `sessions.feature_cycle`, and cycle-based
pruning of `observations`, `query_log`, `injection_log`. This data makes a
cycle-relative freshness dimension technically possible in the future — but per GH #520,
it is out of scope for crt-048.

### GH #425 (activity-relative freshness research)

Already closed. Its three candidate options (cycle-anchored freshness, freeze-aware
dampening, two-speed decay) are superseded by the decision to drop time-based freshness
entirely.

### Lesson #3704

`FRESHNESS_HALF_LIFE_HOURS` (confidence scoring pipeline, bugfix-426) is a distinct
constant from `DEFAULT_STALENESS_THRESHOLD_SECS` (Lambda freshness, being removed). Do
not conflate them in implementation.

## Scope Decisions

- Option 1 selected (drop entirely) per GH #520 owner decision — not a researcher choice.
- Re-normalized weight candidates: graph=0.46, contradiction=0.31, embedding=0.23
  (sum=1.00). Owner comment in GH #520 mentioned graph=0.43, contradiction=0.29,
  embedding=0.21 (sum=0.93 — likely a typo). Final values need ADR.
- `coherence_by_source` retained (just remove freshness from per-source lambda).
- `active_entries` store read retained (still needed for coherence-by-source grouping).

## Open Questions Surfaced

1. Exact re-normalized weights need a final ADR decision (owner comment sums to 0.93,
   not 1.00 — needs clarification).
2. JSON field removal is a breaking API change — confirm no external callers depend on
   `confidence_freshness_score` / `stale_confidence_count` fields.
3. Confirm whether `coherence_by_source` is still useful as a pure-structural per-source
   metric, or should it be simplified/removed.

## Files Read

- `crates/unimatrix-server/src/infra/coherence.rs` (full — 600 lines)
- `crates/unimatrix-server/src/services/status.rs` (Phase 5 region, lines 680–810)
- `crates/unimatrix-server/src/services/mod.rs` (full)
- `crates/unimatrix-server/src/mcp/response/status.rs` (grep excerpts)
- `product/features/crt-036/SCOPE.md` (full)
- GH issue #520 (JSON, including owner decision comment)
- GH issue #425 (JSON, closed)
- Unimatrix entries #179 (ADR-003), #3917 (crt-036 ADR-003), #3704 (lesson: half-life
  miscalibration), #3686 (col-031 ADR-002)

## Output

- SCOPE.md: `product/features/crt-048/SCOPE.md`

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` -- returned 18 entries; entries #179
  (ADR-003 Lambda weights), #3917 (crt-036 phase-freq-table ADR), #3704 (freshness
  half-life lesson) were directly relevant and retrieved in full.
- Stored: entry #4189 "Drop time-based Lambda dimensions rather than recalibrate when
  the access-cadence assumption is invalidated by a lifecycle change" via /uni-store-pattern
