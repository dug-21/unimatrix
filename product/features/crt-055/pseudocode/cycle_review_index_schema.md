# Component 1 — cycle_review_index schema v5 (columns + CycleReviewRecord + migration)

**Crate**: `unimatrix-store`
**Files**: `cycle_review_index.rs` (const + struct + INSERT/UPDATE/SELECT), `migration.rs` (v29→v30 ALTER block), `db.rs` (fresh-create path), `tests/sqlite_parity.rs` (pinned-version assert)
**ADRs**: ADR-001 (#5051), ADR-004 (#5039), ADR-005 (#5047), ADR-007 (#5042) | **Risks**: R-02, R-10, R-18 | **Wave**: 2 (migration spans 2+3)

## Purpose

Own the full set of v5 aggregate columns on `cycle_review_index`, mirror them onto `CycleReviewRecord`, and land them through one three-path migration. This is the schema substrate every other consumer component persists into.

## Constraints honored

- Every metric column `INTEGER NOT NULL DEFAULT 0`; `signal_class_counts_json` is `TEXT NOT NULL DEFAULT '{}'`. No REAL/float column anywhere (Constraint 10).
- `context_reload_pct` stored as basis points (0–10000) — integer, not REAL.
- No content field added (structural leak gate, Constraint 5).
- One migration adds ALL v5 columns; one `SUMMARY_SCHEMA_VERSION` bump (Constraint 7).
- crt-054 already took `CURRENT_SCHEMA_VERSION = 29`. crt-055's `cycle_review_index` ALTER is **v29→v30** (disjoint table; no collision — R-18). Confirm the next-free number at merge (SM coordination, lesson #4095).

## 1a. Constant bump (`cycle_review_index.rs:49`)

```
CHANGE: pub const SUMMARY_SCHEMA_VERSION: u32 = 4
   TO:  pub const SUMMARY_SCHEMA_VERSION: u32 = 5
UPDATE the bump-policy doc-comment: append a v5 note —
  "crt-055: bumped 4 → 5; adds rank-1/2/3 aggregate columns, dual reload
   (context_reload_pct basis points, compaction_count, compaction_reread_count),
   and the transcript fold (transcript_* + signal_class_counts_json). These change
   CycleReviewRecord JSON round-trip fidelity, so pre-v5 rows are flagged stale and
   recomputed via #758 guarded-recompute when source data is present."
```

## 1b. `CycleReviewRecord` extension (`cycle_review_index.rs:71`)

```
APPEND these fields to struct CycleReviewRecord (derive(Debug, Clone, Default) already present;
Default covers the new fields so callers keep `..Default::default()`):

  // --- crt-055 rank-1 phase aggregates (cycle_events) ---
  pub phase_count: i64
  pub phase_transition_count: i64
  pub phase_rework_count: i64
  pub phase_unclosed_count: i64
  pub phase_total_duration_secs: i64
  // --- crt-055 rank-2 rework ratio (SessionRecord.outcome) — num/den pair ---
  pub rework_session_count: i64
  pub total_session_count: i64
  // --- crt-055 rank-3 knowledge reuse (#320 query_log ∪ injection_log) ---
  pub knowledge_reuse_served_count: i64
  // --- crt-055 transcript fold (ActivitySnapshot, summed across held sessions) ---
  pub transcript_bytes_total: i64
  pub transcript_delta_count: i64
  pub transcript_error_count: i64       // class_counts[0]
  pub transcript_refusal_count: i64     // class_counts[1]
  pub signal_class_counts_json: String  // full class_name→count map; Default = "" → see note
  // --- crt-055 dual reload + compaction ---
  pub compaction_count: i64
  pub compaction_reread_count: i64
  pub context_reload_pct: i64           // basis points 0–10000
```

NOTE on `signal_class_counts_json` default: the DB column DEFAULT is `'{}'`, but `String::default()` is `""`. The store layer MUST treat an empty string read back from a pre-v5 / default row as `"{}"` (normalize on read OR ensure the writer always binds a non-empty JSON object). Pseudocode choice: **the writer always binds a valid JSON object string** (Component 6 builds it, defaulting to `"{}"` when the fold map is empty), and the read mapper coalesces `""`→`"{}"`. This keeps round-trip honest and avoids a NOT NULL violation.

## 1c. SELECT + row mapping extension (`get_cycle_review`, `cycle_review_index.rs:153`)

```
EXTEND the SELECT column list (after first_computed_at) with the 16 new columns in a fixed order:
  ..., phase_count, phase_transition_count, phase_rework_count, phase_unclosed_count,
       phase_total_duration_secs, rework_session_count, total_session_count,
       knowledge_reuse_served_count, transcript_bytes_total, transcript_delta_count,
       transcript_error_count, transcript_refusal_count, signal_class_counts_json,
       compaction_count, compaction_reread_count, context_reload_pct

EXTEND the row → CycleReviewRecord mapping with matching `r.get::<i64,_>(idx)` for each
integer column and `r.get::<String,_>(idx)` for signal_class_counts_json, coalescing
""→"{}" as in 1b. Keep column indices sequential after index 11 (first_computed_at).
```

## 1d. INSERT extension (`store_cycle_review` step 2a, `cycle_review_index.rs:246`)

```
EXTEND the INSERT column list and VALUES placeholders to include all 16 new columns
(?13..?28), and add the matching `.bind(record.<field>)` calls after first_computed_at.
Order MUST match the SELECT order in 1c. (Full bind sequence detailed in store_cycle_review.md.)
```

## 1e. UPDATE extension (`store_cycle_review` step 2b, `cycle_review_index.rs:280`)

```
EXTEND the UPDATE SET clause with the 16 new columns (each = ?N), preserving the existing
rule that first_computed_at is NOT in the SET clause. Add matching `.bind()` calls.
(Detailed in store_cycle_review.md.)
```

## 1f. Migration block (`migration.rs`, copy crt-047 v23→v24 template at `:946-1125`)

State machine: one `if current_version < 30` block. Two ordered phases — **all pre-checks, then all ALTERs** — then an in-transaction version stamp, all inside the existing outer migration txn.

```
fn run_main_migrations(txn):
    ... existing blocks (… v28→v29 crt-054 compaction_events) …

    // crt-054's v28→v29 block is now NOT the last block once this lands.
    // Per pattern #5052: ensure the v28→v29 block ends with an intra-stamp
    //   UPDATE counters SET value = 29 WHERE name='schema_version'
    // so this new v29→v30 block observes the correct intermediate version.
    // (crt-054 already added this intra-stamp note; verify it is present.)

    if current_version < 30:
        // --- PHASE A: pre-check existence of all 16 columns (no ALTER yet) ---
        for col in [phase_count, phase_transition_count, phase_rework_count,
                    phase_unclosed_count, phase_total_duration_secs,
                    rework_session_count, total_session_count,
                    knowledge_reuse_served_count, transcript_bytes_total,
                    transcript_delta_count, transcript_error_count,
                    transcript_refusal_count, signal_class_counts_json,
                    compaction_count, compaction_reread_count, context_reload_pct]:
            has[col] = (SELECT COUNT(*) FROM pragma_table_info('cycle_review_index')
                        WHERE name = col) > 0      // map_err → StoreError::Migration

        // --- PHASE B: ALTER only the absent columns ---
        for col in the 15 INTEGER columns:
            if not has[col]:
                ALTER TABLE cycle_review_index ADD COLUMN <col> INTEGER NOT NULL DEFAULT 0
        if not has[signal_class_counts_json]:
            ALTER TABLE cycle_review_index ADD COLUMN signal_class_counts_json TEXT NOT NULL DEFAULT '{}'

        // --- PHASE C: in-transaction version stamp (template line :1120) ---
        UPDATE counters SET value = 30 WHERE name = 'schema_version'

    ... final INSERT OR REPLACE counters schema_version = CURRENT_SCHEMA_VERSION (end of fn) ...
```

Error handling: each pragma read and each ALTER maps its error to `StoreError::Migration { source }`. Any failure rolls back the outer txn; `schema_version` stays at 29; a retry re-runs the block idempotently (pre-checks skip already-added columns).

## 1g. Three-path bump siblings (move together — #4153)

```
1. migration.rs: bump CURRENT_SCHEMA_VERSION 29 → 30; add the block above.
2. db.rs fresh-create: add all 16 columns to the cycle_review_index CREATE TABLE
   DDL (INTEGER NOT NULL DEFAULT 0 ×15; TEXT NOT NULL DEFAULT '{}' ×1), byte-aligned
   with the ALTER types so fresh and upgraded DBs agree (pattern #4373).
3. tests/sqlite_parity.rs: update the pinned `test_schema_version_is_N` assert 29 → 30.
4. Verify the cascade file migration_v29_to_v30.rs exists (#4484) mirroring the
   v28_to_v29 test (fresh v29 DB → run → all 16 columns present, types/defaults correct,
   idempotent re-run is a no-op).
5. cycle_review_index.rs: bump SUMMARY_SCHEMA_VERSION test assertion 4 → 5 (1a).
```

## Error handling

- Migration: rollback-safe via the outer txn; idempotent via pragma pre-checks.
- `store_cycle_review`: existing 4MB-ceiling Err path unchanged; new binds add no new failure mode (integers can't exceed the ceiling).
- A pre-v5 row read via `get_cycle_review` returns DEFAULT 0 / `"{}"` for new columns until guarded recompute refreshes it (Component 2).

## Key test scenarios

- Fresh-create DB: `pragma_table_info('cycle_review_index')` lists all 16 new columns with correct type/default (AC-02).
- v29 → v30 upgrade: same column set appears; idempotent re-run is a no-op (AC-02, R-10).
- `SUMMARY_SCHEMA_VERSION == 5` and pinned migration-version assert moved in the same change (AC-03, R-10).
- Full `CycleReviewRecord` (all 16 new fields populated) store → get round-trips byte-identical, including `signal_class_counts_json` and `context_reload_pct` (extends `test_cycle_review_record_v24_round_trip`).
- `context_reload_pct` column type is INTEGER, not REAL (AC-20).
- No content field present; structural leak guard holds (AC-19, R-11).
- v28→v29 prior block carries its intra-stamp so v29→v30 sees the correct intermediate version (#5052).

## Open questions

- Exact next-free `CURRENT_SCHEMA_VERSION` integer (30 assumed) is an SM merge-coordination point vs crt-054 (lesson #4095). If crt-055 merges first, it is whatever follows the last landed migration; verify at merge.
