## ADR-004: Rank-1/2/3 Durable Aggregate Column Shapes from Durable Streams

### Context
SCOPE Open Q1 (and SR-07) require pinning the exact columns for ass-077 RQ-2 ranks 1–3 so the point-issues (#556, #320) land as fixed shapes, not open-ended over-builds. ass-077 RQ-2 ranks the top decision-value aggregations, ALL sourced from durable, content-opaque streams (never the transcript): rank-1 phase durations/transitions/rework from `cycle_events`; rank-2 rework/failure ratio from `SessionRecord.outcome`; rank-3 knowledge-reuse-all-served from the corpus + injection streams. Rank-4 (curation health) already shipped (crt-047) — the column template. #556 folds "phases declared-but-never-closed" as a rank-1 hotspot; #320 folds "knowledge reuse counts ALL served, not just same-cycle-tagged" into rank-3. Pattern #4178: derived review-time aggregates belong on `cycle_review_index`, not `cycle_events`.

### Decision
Land these `INTEGER NOT NULL DEFAULT 0` columns (crt-047 template), computed in the full-pipeline block, written via the single `store_cycle_review()` (ADR-002):

- **Rank-1 (cycle_events):** `phase_count`, `phase_transition_count`, `phase_rework_count` (phase re-entries / loops), `phase_unclosed_count` (#556 — declared but no matching `cycle_phase_end`/`cycle_stop`), `phase_total_duration_secs` (Σ closed-phase durations).
- **Rank-2 (SessionRecord.outcome):** `rework_session_count` and `total_session_count`. Store the numerator/denominator PAIR, not a pre-divided ratio — the rate is derived at presentation so "0 of 0" → "unavailable" (ADR-003) is distinguishable from a measured 0/N.
- **Rank-3 (#320, knowledge-reuse-all-served):** `knowledge_reuse_served_count` = the union of `query_log` and `injection_log` entries served to the cycle (all served, NOT only same-cycle-tagged). Paired against the corpus-delta store/curate counts already computed for the rate denominator at presentation.

Each column carries a per-metric presence flag (ADR-003). Rank-4 curation-health columns stay exactly as crt-047 shipped (not re-derived). No transcript-content read on the persist path (the leak gate stays structural).

### Consequences
Easier: cross-cycle baselines exist for the highest-value process signals; #556 and #320 land as concrete columns, not prose; storing num/den pairs keeps fail-loud honest; all sources are durable and content-opaque, so the structural leak gate is untouched. Harder: phase-loop and never-closed detection require correct cycle_events timeline reckoning (a closed phase that re-opens is rework, not a new phase); the rank-3 injection_log column/table name must be confirmed at spec (Open Q1, §10). Cross-refs: ass-077 RQ-2 (the ranks), #4178 (derived aggregates on cycle_review_index), crt-047 (rank-4 template), #556 (never-closed phases), #320 (all-served reuse), ADR-002 (single writer), ADR-003 (presence flags).
