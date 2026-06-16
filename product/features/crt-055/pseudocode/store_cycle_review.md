# Component 2 — store_cycle_review() extension (single writer, four returns, no zero-clobber)

**Crate**: `unimatrix-store` (the writer) + `unimatrix-server` (the four-return discipline, `build_cycle_review_record`)
**Files**: `cycle_review_index.rs:209` (INSERT/UPDATE binds), `tools.rs:3774` (`build_cycle_review_record`), `tools.rs:1943` (handler returns)
**ADRs**: ADR-002 (#5037), ADR-001 (#5051) | **Patterns**: #4750 (four returns), #5022 (three assertions) | **Risks**: R-01, R-02 (both Critical) | **Wave**: 2

## Purpose

Thread the 16 new columns through the ONE `store_cycle_review()` writer and ensure they are written ONLY at the full-pipeline return — never at memo-hit, purged-retain, or force+purged. This closes the #750 empty-clobber class by construction.

## Constraints honored

- One write site (Constraint 1). The three non-full-pipeline returns DO NOT write the new columns.
- Coexist with #758 guarded-recompute: stale + source-present recomputes via clear-memo-and-fall-through, routing through this same writer (Constraint 2).
- `first_computed_at` stays excluded from the UPDATE SET clause (existing ADR-001 rule, unchanged).

## 2a. `store_cycle_review()` INSERT binds (`cycle_review_index.rs:246`, step 2a)

```
EXTEND the INSERT (column list, VALUES, binds) with the 16 new columns appended after
first_computed_at (?12). New placeholders ?13..?28; binds in this exact order (must match
the SELECT mapping in cycle_review_index_schema.md §1c):

  .bind(record.phase_count)                  // ?13
  .bind(record.phase_transition_count)       // ?14
  .bind(record.phase_rework_count)           // ?15
  .bind(record.phase_unclosed_count)         // ?16
  .bind(record.phase_total_duration_secs)    // ?17
  .bind(record.rework_session_count)         // ?18
  .bind(record.total_session_count)          // ?19
  .bind(record.knowledge_reuse_served_count) // ?20
  .bind(record.transcript_bytes_total)       // ?21
  .bind(record.transcript_delta_count)       // ?22
  .bind(record.transcript_error_count)       // ?23
  .bind(record.transcript_refusal_count)     // ?24
  .bind(coalesce_json(&record.signal_class_counts_json)) // ?25  ("" → "{}")
  .bind(record.compaction_count)             // ?26
  .bind(record.compaction_reread_count)      // ?27
  .bind(record.context_reload_pct)           // ?28
```

`coalesce_json(s)` = if `s.is_empty()` then `"{}"` else `s` — guarantees the NOT NULL '{}' contract.

## 2b. `store_cycle_review()` UPDATE binds (`cycle_review_index.rs:280`, step 2b)

```
EXTEND the UPDATE SET clause with the 16 new columns (each = ?N), keeping first_computed_at
OUT of the SET clause (unchanged ADR-001). Re-number placeholders consistently and add the
same 16 .bind() calls (same order/coalesce as 2a). The existing curation-health binds and
WHERE feature_cycle = ?1 stay as-is.
```

No new failure mode: the 4MB-ceiling check (on `summary_json`) and the two-step upsert (read existing `first_computed_at`, then INSERT-or-UPDATE on the single write connection) are unchanged. Integers cannot trip the ceiling.

## 2c. `build_cycle_review_record` extension (`tools.rs:3774`)

This server-side helper serializes a `RetrospectiveReport` into a `CycleReviewRecord`. Extend its signature to accept the computed aggregates so the full-pipeline return populates all new columns.

```
fn build_cycle_review_record(
    feature_cycle, report, snapshot /*crt-047 curation*/, first_computed_at,
    aggregates: &CycleAggregates,      // NEW — rank-1/2/3 + reload + compaction + fold
) -> Result<CycleReviewRecord, serde_json::Error>:
    summary_json = serde_json::to_string(report)?   // unchanged; content-free report
    computed_at  = now_unix_secs()
    (ct,ca,ch,cs,dt,od) = map curation snapshot (unchanged)
    return CycleReviewRecord {
        feature_cycle, schema_version: SUMMARY_SCHEMA_VERSION /* now 5 */,
        computed_at, raw_signals_available: 1, summary_json,
        corrections_total: ct, ... first_computed_at,
        // crt-055 new columns from aggregates:
        phase_count: aggregates.phase_count,
        ... (all 16, copied 1:1 from CycleAggregates) ...,
        signal_class_counts_json: aggregates.signal_class_counts_json.clone(),
        context_reload_pct: aggregates.context_reload_pct,
    }
```

`CycleAggregates` is the value bundle produced by Components 3/4/5/6 (see those files). It is plain `i64`/`String` — no content. Defined near the handler.

## 2d. Four-return discipline (handler, `tools.rs:1943` — per #4750)

The handler has FOUR success returns. The new columns are written at exactly ONE.

```
HANDLER context_cycle_review:
  RETURN 1 — purged-signals path (force=true + stored record, attributed empty):
      serve stored record; NO store_cycle_review; NO new-column write.   (no clobber)
  RETURN 2 — cached/empty-attributed (attributed empty, not force):
      serve cached MetricVector; NO write.
  RETURN 3 — memoization-hit (check_stored_review == Current):
      serve stored report verbatim; NO write.
  RETURN 4 — full-pipeline format dispatch:
      this is the ONE writer. Compute aggregates → build_cycle_review_record →
      store_cycle_review(&record) gated on the pipeline having run. (writes new cols)

  GUARDED-RECOMPUTE bridge (Staleness::Stale, #758):
      at the memo site, when check_stored_review returns Stale:
        if !attributed.is_empty():   // source data present
            clear memo_hit; FALL THROUGH to RETURN 4 (no second writer).
        else:                        // source purged
            retain stored record; emit stale_purged_advisory; NO write.  (RETURN-3-like)
```

The recompute path NEVER adds a writer near the memo / `check_stored_review` site (Constraint 1). It only clears the memo flag and lets control reach RETURN 4, which routes through the single writer — so empty-clobber is structurally impossible.

## Data flow

- IN: `CycleAggregates` (from 3/4/5/6), `RetrospectiveReport`, curation `snapshot`, `first_computed_at`.
- OUT: persisted `cycle_review_index` row (full pipeline only); or stored record served unchanged (returns 1–3).

## Error handling

- `store_cycle_review` Err is logged and the handler continues (existing crt-033 behavior at `tools.rs:2914` — "store failed … continuing"); the response is still served. New columns add no new Err class.
- Deserialize failure in `check_stored_review` → treat as cache miss → fall through to RETURN 4 (existing ADR-003 behavior).

## Key test scenarios (the three #5022 assertions + single-writer structural — AC-17, AC-18)

- (a) Stale pre-v5 row, source present → recompute writes fresh non-zero new columns at schema_version 5 via clear-memo-fall-through (one writer).
- (b) Stale pre-v5 row, source purged → stored row retained byte-identical; no write; advisory = "source purged, cannot recompute".
- (c) `force=true` on a purged row → `computed_at` does NOT advance; new columns NOT clobbered with zeros.
- Structural: exactly ONE `store_cycle_review()` call site writes the new columns; returns 1–3 contain no column write (grep/AST guard; no second writer near the memo site).
- Full `CycleReviewRecord` (all 16 populated) INSERT then UPDATE (force re-review) round-trips; `first_computed_at` preserved from first write (extends `test_store_cycle_review_preserves_first_computed_at_on_overwrite`).
