# Component: Content-opaque fold read [UNCHANGED] — stays gated ×4

Files: `session.rs:566` (`activity_snapshots_for_feature`), `session_transcript.rs:392`
(`activity_snapshot()`), crt-054/055 fold landing (`activity_fold_handler.rs`,
`review_aggregates.rs`), review sites `tools.rs:2379/2558/3328/3451` region.

## Purpose

After the purge leaves the seam, the content-opaque integer fold read is the SOLE surviving success
side-effect. It STAYS gated at all four `result.is_ok()` returns per #4750 (ADR-004). It reads counters
only — never prose — and lands durable integers on `CycleReviewRecord` / `cycle_review_index`.

## Interface (UNCHANGED — do not modify)

```
fn activity_snapshots_for_feature(&self, feature_cycle) -> ...   # session.rs:566 — counters only
    # per-buffer buf.activity_snapshot() → ActivitySnapshot { bytes_total, *_delta_count, class_counts }
    # NO byte-bearing field; content-opaque (crt-054 Surface B)
```

The fold read uses `activity_snapshot()` (counters), NOT `snapshot()` (content). They are distinct
readers; only `snapshot()` reads content (snapshot-reuse.md). Keeping them distinct preserves the
single-content-reader invariant (#4848).

## Gating (CON-2 / #4750 / R-07) — the ONE assertion that still matters

Express the fold-read gate ONCE and apply identically at all four success returns:

```
if result.is_ok():
    self.land_activity_fold(&feature_cycle, ...)   # reads activity_snapshots_for_feature, lands ints
```

- Missing a site — especially memo-hit (site 3) — under-counts durable integers on a cached re-review
  (#4585 drift). The fold read is NOT `force`-reproducible once the buffer ages, so under-count is
  silent and permanent.
- The fold-read ×4 source assertion (`distill_handler.rs:651-726`) is PRESERVED. Only the
  purge-count / attach-before-purge assertions are removed (render-dispatch.md / distill-before-purge.md).

## Idempotency now newly load-bearing (NFR-4 / R-14 / SR-12)

Because the review NEVER purges, repeated non-destructive reviews re-read the SAME surviving buffer. The
fold MUST stay idempotent: repeated reviews of the same cycle must NOT accumulate/double-count durable
`cycle_review_index` integers. This is a property of the existing crt-055 fold; crt-057 does not change
it but makes it load-bearing (previously the purge made a second review find empty buffers). Confirm the
fold write is a keyed upsert/replace, not an increment.

## Error handling

- Fold read is on the success path only (`result.is_ok()`); error paths never reach it (UNCHANGED).
- Reads counters from a possibly-empty/aged buffer → yields measured zeros, distinct from absence
  (crt-055 semantics UNCHANGED).

## Key test scenarios

- Path-proven per-site fold-landed rows for all four returns incl. memo-hit; fixture PROVES which site
  executed (assert memo-hit / no-recompute indicator — #4452), not a vacuous full-pipeline pass (R-07 sc.1).
- Memo-hit fold outcome equals full-pipeline for the same buffer state (R-07 sc.2).
- Fold survives the non-purging review: a subsequent review re-reads the same buffer (R-07 sc.3).
- Idempotency: 3× review on one cycle → `cycle_review_index` metrics stable, no accumulation (R-14 sc.1).
- Fold output content-free: counters only, no byte-bearing field (R-14 sc.2 / AC-04).
