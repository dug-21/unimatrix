# Component 9 — Review pipeline ordering (tools.rs context_cycle_review)

**Crate**: `unimatrix-server`
**Files**: `tools.rs` `context_cycle_review` handler (`:1943`), the full-pipeline block (around the `store_cycle_review` site `:2914` and the purge sites `:2198/:2336/:3036/:3148`)
**ADRs**: ADR-002 (#5037), ADR-006/007/010 | **Risks**: R-01, R-03, R-14 (all ordering-sensitive) | **Wave**: 3 (integrates all)

## Purpose

Wire the six-step ordered pipeline inside the single full-pipeline block, honoring the load-bearing orderings: `auto_close` before rank-1; fold read before purge; one persist via the single writer. This is the integration spine for Components 3–8.

## Constraints honored

- `auto_close` (Component 8) BEFORE rank-1 timeline read (Component 3) — R-14.
- Fold read (Component 6) STRICTLY BEFORE `purge_cycle_transcripts` — Constraint 4 / R-03.
- Single persist via `store_cycle_review` at the full-pipeline return ONLY (Component 2) — Constraint 1 / R-01.
- Per-metric presence flags set from source-non-empty (Component 7) — drives fail-loud.

## 9a. Pipeline order (single full-pipeline block)

```
FULL-PIPELINE BLOCK (RETURN 4 path only — after the memo/purged-retain/force returns are ruled out):

  let mut agg = CycleAggregates::default()
  let mut avail = MetricAvailability::default()

  // STEP 1 — auto_close (#593, Component 8). BEFORE any rank-1 read.
  maybe_auto_close(store, feature_cycle, params.auto_close)

  // STEP 2 — read-before-purge fold landing (Component 6). BEFORE purge_cycle_transcripts.
  let landing = land_activity_fold(registry, feature_cycle, signal_catalog)
  agg.transcript_bytes_total   = landing.transcript_bytes_total
  agg.transcript_delta_count   = landing.transcript_delta_count
  agg.transcript_error_count   = landing.transcript_error_count
  agg.transcript_refusal_count = landing.transcript_refusal_count
  agg.signal_class_counts_json = landing.signal_class_counts_json
  avail.transcript_fold_available = landing.available
  // NOTE: purge_cycle_transcripts(feature_cycle) is called LATER (existing site at the
  // success returns); STEP 2 must precede ALL purge call sites. The ordering is asserted.

  // STEP 3 — aggregate reckoning (Component 3). cycle_events now includes any auto_close stop.
  let p = reckon_phase_aggregates(&cycle_events)              // rank-1 (reads timeline AFTER step 1)
  agg.phase_count = p.phase_count; ...; agg.phase_unclosed_count = p.phase_unclosed
  (agg.rework_session_count, agg.total_session_count) = reckon_rework_ratio(&sessions)   // rank-2
  agg.knowledge_reuse_served_count = reckon_knowledge_reuse_served(store, feature_cycle) // rank-3 (#320)

  // STEP 4 — reload reckoning (Components 4,5). Two columns, two windows, one engine.
  agg.context_reload_pct      = reckon_context_reload_bps(&summaries, &records)          // bps i64
  agg.compaction_count        = reckon_compaction_count(store, &declared_session_ids)
  agg.compaction_reread_count = reckon_compaction_reread(&records, store, &declared_session_ids)

  // STEP 5 — per-metric presence flags (Component 7).
  avail.phase_metrics_available  = !cycle_events.is_empty()
  avail.rework_ratio_available   = agg.total_session_count > 0
  avail.knowledge_reuse_available= knowledge_log_nonempty
  avail.compaction_available     = agg.compaction_count > 0 || any_compaction_boundary
  avail.context_reload_available = summaries.len() >= 2
  // transcript_fold_available already set in step 2.

  // STEP 6 — persist via the SINGLE writer (Component 2). Full-pipeline return ONLY.
  let record = build_cycle_review_record(feature_cycle, &report, curation_snapshot,
                                         first_computed_at, &agg)?
  if let Err(e) = store.store_cycle_review(&record).await:
      warn!("crt-055: store_cycle_review failed for {feature_cycle}: {e} — continuing")

  // render the report with `avail` (Component 7), then RETURN 4.
```

## 9b. The other three returns (NO new-column write — Component 2 §2d)

```
RETURN 1 purged-signals (force + stored record, attributed empty): serve stored; NO write; existing purge side-effect unchanged.
RETURN 2 cached/empty-attributed: serve cached; NO write.
RETURN 3 memo-hit (Staleness::Current): serve verbatim; NO write.
GUARDED-RECOMPUTE: Staleness::Stale + !attributed.is_empty() → clear memo, FALL THROUGH to the full-pipeline block (RETURN 4); Stale + purged → retain + advisory (RETURN-3-like), NO write.
```

The #4750 discipline: any success-only side effect (the new-column write) is factored so it fires at exactly the full-pipeline return, never the other three. `purge_cycle_transcripts` continues to fire at its existing success sites (gated on review success) — STEP 2's fold read must precede every one of them.

## 9c. Ordering invariants (asserted in tests)

```
INVARIANT A (R-14): maybe_auto_close runs before reckon_phase_aggregates reads cycle_events.
INVARIANT B (R-03): land_activity_fold runs before any purge_cycle_transcripts call site.
INVARIANT C (R-01): store_cycle_review(&record) with new columns is reached only on the
                    full-pipeline path; the three other returns reach no such write.
```

## Data flow

- IN: `auto_close`, `feature_cycle`, registry, store, `cycle_events`, sessions/summaries, `ObservationRecord`s.
- OUT: one persisted `cycle_review_index` row + a rendered `RetrospectiveReport` with per-metric availability.

## Error handling

- Any single reckoning failure (rank-3 read, compaction read, fold) degrades that metric to unavailable (Component 7) rather than aborting the pipeline — honest partials, never fabricated zeros.
- `store_cycle_review` Err logged + continue (existing behavior); response still served.

## Key test scenarios

- INVARIANT B inversion: a test that reverses fold-read and purge zeroes the transcript columns (AC-08, R-03) — proves the ordering is load-bearing.
- INVARIANT A: `auto_close=true` closes the final phase before rank-1, so it is not a false never-closed (AC-15, R-14).
- INVARIANT C: the three #5022 assertions hold; exactly one writer reached on full-pipeline; returns 1–3 write nothing (AC-17/18, R-01).
- End-to-end: a seeded cycle (phases + sessions + served knowledge + compaction_events + held fold) runs the full pipeline once and persists all 16 columns with correct values (AC-05/07/11/12/13).
- Byte-identical memoized re-review (force=false, Current) returns the stored report unchanged (no recompute, no write).
