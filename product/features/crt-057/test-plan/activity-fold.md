# Test Plan — Content-Opaque Fold Read (crt-054/055) `[UNCHANGED]` — sole surviving side-effect

**Files:** `mcp/activity_fold_handler.rs`, `session.rs:566` (`activity_snapshots_for_feature`),
gated at the four success returns
**Risks:** R-07 (High), R-14 · **ACs:** AC-04, AC-12

> After the purge leaves the seam, the content-opaque fold is the **ONLY** surviving success side-effect and
> must stay gated at all four `result.is_ok()` returns. Source-assertion counting sees the helper is *called*
> ×4 — it cannot see the fold LANDED. Coverage is **behavioral and path-proven**, not source-counting
> (ADR-004). This is where the migrated four-site source-assertion invariant re-anchors (OVERVIEW §4).

---

## R-07 — path-proven per-site fold rows (AC-04, AC-12)
For each of the four success returns (purged-signals, cached-metrics, **memo-hit (site 3)**, full-pipeline):
- `test_fold_lands_durable_ints_at_site_{n}` — run a review routed through that site and assert the durable
  `cycle_review_index` fold integers are written. The fixture must **PROVE which site executed** (assert a
  memo-hit indicator / no-recompute), not assume it (#4452). **Memo-hit is non-optional** — it is the
  easy-to-miss site (#4585 drift).
- `test_memohit_fold_parity_with_full_pipeline` — the memo-hit row's fold outcome EQUALS the full-pipeline
  row's for the same buffer state. Divergence between sites is the defect. (R-07 sc.2.)
- `test_fold_survives_non_purging_review` — because the buffer now survives, a subsequent review re-reads the
  same buffer; assert the fold still reads it (nothing lost sooner). (R-07 sc.3; couples R-14.)

## R-14 — idempotency across repeated non-purging reviews (AC-04)
- `test_fold_idempotent_across_repeated_reviews` — run a default (or `transcript:{}`) review **3×** on the
  same cycle; assert `cycle_review_index` fold metrics are **stable** across reviews (no accumulation). The
  common path now NEVER purges, so a non-idempotent fold double-counts. (R-14 sc.1.)
- `test_fold_output_content_free` — the fold output stays counter-only (`bytes_total`, `*_delta_count`,
  `class_counts`); NO byte-bearing field. (R-14 sc.2; AC-04 content-free.)

## R-11 — source-assertion (shared with `distill-before-purge.md`)
- The four-site count for the fold-read gate is PRESERVED as the surviving source-assertion invariant (the
  purge ×4 count is removed with rationale). Assert the fold-read gate string appears ×4 in the handler body.

## Integration anchor
`suites/test_lifecycle.py::test_cycle_review_fold_idempotent_across_repeats` and the existing
`test_cycle_review_persists_across_restart` (fold row persistence) — OVERVIEW §6b/6c.
