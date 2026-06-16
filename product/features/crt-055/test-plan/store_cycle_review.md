# Test Plan — store_cycle_review() extension (single writer, four returns)

**Component**: `unimatrix-store/src/cycle_review_index.rs:209` (writer), `CycleReviewRecord` (`:72`), INSERT (`:249`) + UPDATE (`:284`)
**Risks**: R-01 (second writer / empty-clobber — CRITICAL), R-09 (integer-width), R-11 (leak gate), R-13 (token field)
**ACs**: AC-17, AC-18, AC-14, AC-20, AC-19, AC-10

> Load-bearing lesson #5022: the ONLY `store_cycle_review()` writer sits PAST two presence guards; the store layer will persist an empty record if called directly. Safety is entirely "who calls the writer and when". These tests pin that invariant.

## Unit tests

### Single-writer / no-clobber (R-01, AC-17) — the three #5022 assertions
> The *behavioral* three returns run through the handler (see review_pipeline.md). Here, the **structural** guards:
- `test_exactly_one_store_cycle_review_writer_site` — static/structural assertion: exactly ONE call site writes the new v5 columns; no second `store_cycle_review` near the memo / `check_stored_review` site (R-01).
- `test_record_roundtrip_all_v5_columns` — build a `CycleReviewRecord` with known non-zero values for every v5 field; INSERT then re-read; assert every column round-trips byte-identical (INSERT bind correctness).
- `test_update_path_binds_all_v5_columns` — pre-existing row, UPDATE via the writer; assert the UPDATE binds every new column (not just INSERT) — a missing UPDATE bind silently leaves stale values.

### Integer-width / basis-points (R-09, AC-14, AC-20)
- `test_width_conversion_saturates_not_wraps` — `CycleReviewRecord` built from near-`u64::MAX` / large `u32` fold values via checked/saturating conversion; assert persisted `i64` is correct or saturated-and-warned, NEVER wrapped to negative.
- `test_basis_points_roundtrip` (AC-20) — `compute_context_reload_pct` FRACTION in [0.0,1.0] → `round(fraction × 10000)` basis-points `i64` → store → re-read equals stored. Cases: 0.375→3750; 0.00005→1 (round to nearest); 0.99995→10000.
- `test_basis_points_out_of_range_clamped` (AC-20/14) — candidate >10000 or negative → clamped/rejected BEFORE bind, never silently truncated.
- `test_no_float_reaches_bind` (AC-20) — structural: no `push_bind(f64)` / `is_finite()` path on the new columns — the #4529/#4533 footgun is designed out by integer storage (no float guard AC).

### Structural leak gate (R-11, AC-19)
- `test_candidates_structurally_absent_from_memoized_report` — the existing leak-gate test still HOLDS after the v5 fields are added.
- `test_no_content_field_on_record` — structural: every new `CycleReviewRecord` field is `i64` / `String`-aggregate (`signal_class_counts_json` is a count map, NOT content) / metadata; no transcript bytes.

### Token-field guard (R-13, AC-10)
- `test_no_token_named_field_on_record` — no `token_bytes_per_unit` / "tokens" field on `CycleReviewRecord` / `RetrospectiveReport`.

## Integration tests (behavioral #5022 — see review_pipeline.md)
- The (a) data-present-recompute-writes, (b) purged-retain-no-write byte-identical, (c) force+purged-no-clobber assertions run end-to-end through the handler; this file owns their STORE-layer contract (the writer binds correctly; the writer is the only writer).

## Edge cases
- Empty `CycleReviewRecord` passed directly to the store layer → it WILL persist zeros (by design — the store is dumb). The guard is the caller (review_pipeline). This test documents the hazard the single-writer invariant protects against.
- `signal_class_counts_json` empty map → stored as `'{}'`, re-reads as empty map (round-trip integrity, not null).

## Expected behaviors / assertions summary
- Exactly one writer site; INSERT and UPDATE both bind every v5 column.
- Width conversion saturates, never wraps; basis-points clamp 0–10000; no float bind.
- Leak gate holds; no content field; no token field.
