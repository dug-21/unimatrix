# C3 — Store read getter `get_cycle_tags`

> File: `crates/unimatrix-store/src/db.rs` (NEW). `SELECT tag FROM cycle_tags WHERE feature_cycle=?1
> ORDER BY tag`. Parity `get_cycle_start_goal` (db.rs:371).
> Risks: R-04 scenario 3 (degrade). ACs: supports AC-05 (read side), AC-EXTRA-2 (degrade).

## Reuse
Model on `goal_clusters.rs` getter unit tests (`test_get_cycle_start_goal_embedding_*`): seed via the
write primitive / raw insert, then read back. Err-path model: `listener.rs:8590` (T-RES-03) closes
the pool to force a store `Err`.

## Unit test expectations
- `test_get_cycle_tags_returns_sorted` — seed `{arm:B, arm:A, foo}` for `fc`; assert returns
  `["arm:A", "arm:B", "foo"]` (deterministic ORDER BY tag).
- `test_get_cycle_tags_empty_when_none` — no rows for `fc` → returns `Ok(vec![])` (NOT an error, NOT
  a spurious row). This is what makes a tag-less cycle render no section (C9) and a v5/absent cycle
  return empty (AC-08).
- `test_get_cycle_tags_scoped_to_feature_cycle` — seed tags for `fc1` and `fc2`; `get_cycle_tags(fc1)`
  returns only `fc1`'s set (no cross-cycle leakage).
- `test_get_cycle_tags_verbatim` — tags with colons / unicode returned byte-for-byte.

## Degrade contract (R-04 s3)
- The review handler (C8) converts a getter `Err` into `report.tags = []` + `tracing::warn`. The
  getter itself returns `Result`; assert an error propagates as `Err` (so the handler's
  `.unwrap_or_default()` degrade is exercised) — the assembled degrade test lives in
  listener-persistence.md / review-handler.md via a closed pool.

## Source-of-truth note
`get_cycle_tags` reads `cycle_tags` (source of truth) — NOT the `summary_json` mirror. Asserted at
the review-handler tier (review-handler.md): a review must reflect the live `cycle_tags` rows.
