# Test Plan — Aggregate reckoning (rank 1/2/3)

**Component**: `unimatrix-observe` aggregate module (sibling to `session_metrics.rs`); sources `cycle_events`, `SessionRecord.outcome`, `query_log ∪ injection_log`
**Risks**: R-15 (rank-1 timeline mis-reckon — High), R-16 (rank-3 union under/over-count — High), R-17 (pre-divided ratio)
**ACs**: AC-04 (#556), AC-05, AC-06 (#320)

## Unit tests

### Rank-1 phase aggregates from cycle_events (R-15, AC-04, AC-05)
- `test_rank1_declared_phases_counted` — seed N distinct declared phase-start events → `phase_count == N`.
- `test_rank1_phase_transitions_counted` — seed phase-end transitions → `phase_transition_count` matches.
- `test_rank1_closed_then_reopened_is_rework_not_new_phase` (R-15) — seed a phase that closes then re-opens → `phase_rework_count` increments, `phase_count` does NOT double. This is the marquee rank-1 mis-reckon guard.
- `test_rank1_unclosed_phase_increments_unclosed_count` (#556, AC-04) — phase with declared start and NO close → `phase_unclosed_count` increments (declared-but-never-closed hotspot).
- `test_rank1_matching_close_not_unclosed` (#556 false-positive guard) — phase WITH a matching close → does NOT increment `phase_unclosed_count`.
- `test_rank1_total_duration_sums_closed_phases_only` — `phase_total_duration_secs == Σ` closed-phase durations; an unclosed phase contributes 0 (no fabricated end time).

### Rank-2 rework ratio from SessionRecord.outcome (R-17, AC-05)
- `test_rank2_rework_session_count_and_total_stored_as_pair` (R-17) — assert `rework_session_count` and `total_session_count` are stored as a num/den PAIR, never a pre-divided ratio. Seed M rework/failure sessions out of T total → counts match.
- `test_rank2_ratio_zero_of_zero_vs_zero_of_n` (R-17) — see fail_loud_guard.md: "0 of 0" → unavailable; "0 of N" → measured rate. The pair makes them distinguishable.

### Rank-3 knowledge-reuse all-served #320 (R-16, AC-06)
- `test_rank3_counts_union_of_query_and_injection_log` (R-16) — seed served entries split across `query_log` and `injection_log`, including cross-cycle-tagged; assert `knowledge_reuse_served_count == size of the UNION`, NOT same-cycle-tagged only.
- `test_rank3_entry_served_via_both_logs_counted_once` (R-16) — an entry present in BOTH logs → counted ONCE (union dedup), not twice.
- `test_rank3_wrong_table_name_yields_silent_zero_guard` — confirm against the ACTUAL `injection_log` table/column names (Open Q1); a wrong name silently returns zero. Assert non-zero for a seeded non-empty union (negative-control against the silent-zero failure).

## Integration tests

- `test_cycle_review_phase_aggregates_from_seeded_timeline` (AC-04, AC-05) — extend `test_phase_tag_store_cycle_review_flow`: seed `cycle_events` (declared / transition / unclosed / close-reopen) → full review → assert persisted `phase_*` columns match expected aggregates and the unclosed phase surfaces as a hotspot.
- `test_cycle_review_knowledge_reuse_union_dedup` (harness, AC-06) — extend `test_cycle_review_knowledge_reuse_cross_feature_split`: seed an entry served via BOTH query_log and injection_log → `knowledge_reuse_served_count` counts it once; total == union size.

## Edge cases
- Zero cycle_events → all phase metrics "unavailable" (see fail_loud_guard), not 0.
- Cycle with only unclosed phases → `phase_unclosed_count > 0`, `phase_total_duration_secs == 0` (genuine, distinct from unavailable since cycle_events present).
- `auto_close=true` closing the final phase → it is NOT counted as never-closed (coupling with auto_close.md / AC-15).
- Knowledge entry served multiple times in the SAME log → union semantics (count distinct entries, per #320 intent — confirm dedup key at spec time).

## Expected behaviors / assertions summary
- Close-then-reopen = rework, never a new phase; unclosed detection has no false positive/negative.
- Rank-3 == size of `query_log ∪ injection_log` union, deduped across logs.
- Num/den pairs persisted, never a pre-divided ratio.
