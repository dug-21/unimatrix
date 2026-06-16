# crt-055 Pseudocode Overview

**Feature**: context_cycle_review redesign — durable per-cycle aggregates + dual reload metrics + transcript-fold surfacing
**Consumer of**: crt-054 (PRODUCER — `compaction_events` table + `ActivitySnapshot` / `activity_snapshots_for_feature`, both already landed)
**Source of truth**: ARCHITECTURE.md §6 Integration Surface, ADR-001..010 (#5037/#5039/#5042/#5044/#5045/#5046/#5047/#5048/#5051), RISK-TEST-STRATEGY R-01..R-18.

> All interface names below are read from the architecture's Integration Surface and verified against the live codebase (see "Verified codebase anchors"). No interface is invented here.

---

## Verified codebase anchors (state at design time)

| Anchor | Location | Note |
|--------|----------|------|
| `SUMMARY_SCHEMA_VERSION: u32 = 4` | `unimatrix-store/src/cycle_review_index.rs:49` | crt-055 bumps → 5 |
| `CycleReviewRecord` | `cycle_review_index.rs:71` (`derive(Debug, Clone, Default)`) | extend with new `i64`/`String` fields |
| `store_cycle_review()` INSERT `:246`, UPDATE `:280` | `cycle_review_index.rs:209` | extend both bind lists |
| `get_cycle_review()` SELECT | `cycle_review_index.rs:153` | extend SELECT + row mapping |
| `CURRENT_SCHEMA_VERSION: u64 = 29` | `migration.rs:24` | crt-054 already took v29; crt-055 v5 migration is **v29→v30** |
| crt-047 v23→v24 ALTER template | `migration.rs:946-1125` | copy: pre-check-all-then-ALTER-each, in-txn version stamp |
| `compute_context_reload_pct(...) -> f64` | `session_metrics.rs:47` | returns a **fraction 0.0–1.0** (NOT a percent — see note) |
| `ObservationRecord.ts: u64` epoch **millis**; `÷1000` floor precedent | `session_metrics.rs:115` | gate normalization site |
| `ActivitySnapshot { bytes_total:u64, delta_count:u32, class_counts:[u32;16] }` | `infra/transcript_activity.rs:106`; `MAX_SIGNAL_CLASSES=16` (`:32`) | crt-054 READ-ONLY surface |
| `activity_snapshots_for_feature(&str) -> Vec<(String, ActivitySnapshot)>` | `infra/session.rs:560` | crt-054 collector; undeclared sessions absent (not zero) |
| `insert_compaction_event(session_id, compacted_at_secs, high_water)` | `write_ext.rs:195` | crt-054 writer; `compacted_at` = Unix seconds |
| `compaction_events(id, session_id, compacted_at, high_water)` + `idx_compaction_events_session` | crt-054 (v29) | crt-055 adds the READ accessor only |
| `Staleness` enum + `check_stored_review` + `build_cycle_review_record` | `tools.rs:3681 / :3703 / :3774` | #758 guarded-recompute machinery crt-055 threads through |
| `context_cycle_review` handler (4 returns) | `tools.rs:1943` | per #4750 |
| `purge_cycle_transcripts(&feature_cycle)` | `tools.rs` / `server.rs` | read-before-purge anchor |

> **NOTE / OPEN-Q (load-bearing).** ADR-005 (#5047) and the brief say `compute_context_reload_pct` "returns a percentage" and basis points = `round(pct × 100)`. The **live function returns a fraction in [0.0, 1.0]** (e.g. `2.0/3.0`), not a 0–100 percentage. Basis-points encoding must therefore be `round(fraction × 10000)` (0.375 → 3750), NOT `round(fraction × 100)`. This pseudocode uses `round(fraction × 10000)` and flags it as a confirm-at-impl item (see `reload_overlap_engine.md`, Open Questions). The architecture's worked example (37.5% → 3750) is honored either way; only the multiplier wording differs.

---

## Components (Component Map → file)

| # | Component | File | Crate(s) |
|---|-----------|------|----------|
| 1 | cycle_review_index schema v5 (columns + `CycleReviewRecord` + migration) | `cycle_review_index_schema.md` | unimatrix-store |
| 2 | `store_cycle_review()` extension (single writer, four returns, no zero-clobber) | `store_cycle_review.md` | unimatrix-store + unimatrix-server |
| 3 | Aggregate reckoning (rank 1/2/3) | `aggregate_reckoning.md` | unimatrix-observe |
| 4 | Reload overlap engine (`context_reload` + `compaction_reread`, one engine) | `reload_overlap_engine.md` | unimatrix-observe |
| 5 | `compaction_reread` reckoning + `compaction_events` read accessor | `compaction_reckoning.md` | unimatrix-observe + unimatrix-store |
| 6 | Activity-fold landing (read-before-purge, width conversion, JSON) | `activity_fold_landing.md` | unimatrix-server |
| 7 | Fail-loud presentation guard (per-metric availability) | `fail_loud_guard.md` | unimatrix-observe (report) + presentation |
| 8 | `auto_close` handler arm (#593) | `auto_close.md` | unimatrix-server |
| 9 | Review pipeline ordering (tools.rs `context_cycle_review`) | `review_pipeline.md` | unimatrix-server |

---

## Shared types & values (defined once, used by component files)

### `CycleReviewRecord` new fields (Component 1 owns; every metric field `i64`, no `f64`/REAL, no content field)

```
// appended to CycleReviewRecord (cycle_review_index.rs)
phase_count: i64                  // rank-1, cycle_events declared phases
phase_transition_count: i64       // rank-1, phase-end transitions
phase_rework_count: i64           // rank-1, phase re-entries (loops)
phase_unclosed_count: i64         // rank-1, #556 declared-but-never-closed
phase_total_duration_secs: i64    // rank-1, Σ closed-phase durations
rework_session_count: i64         // rank-2, SessionRecord.outcome rework/failure
total_session_count: i64          // rank-2, ratio denominator
knowledge_reuse_served_count: i64 // rank-3, #320 query_log ∪ injection_log union
transcript_bytes_total: i64       // fold, Σ ActivitySnapshot.bytes_total
transcript_delta_count: i64       // fold, Σ ActivitySnapshot.delta_count
transcript_error_count: i64       // fold, Σ class_counts[0]
transcript_refusal_count: i64     // fold, Σ class_counts[1]
signal_class_counts_json: String  // fold, full class_name→count map, default "{}"
compaction_count: i64             // Σ attributed compaction_events rows
compaction_reread_count: i64      // within-cycle post-compaction overlap reads
context_reload_pct: i64           // basis points 0–10000 (round(fraction × 10000))
```

### `MetricAvailability` (Component 7 owns; presentation-layer, NOT persisted on the record)

```
// per-metric source-presence + coarse/directional marking, carried on the
// RetrospectiveReport presentation layer (NOT a CycleReviewRecord column).
struct MetricAvailability {
    phase_metrics_available: bool       // cycle_events non-empty
    rework_ratio_available: bool        // total_session_count > 0 (SessionRecord present)
    knowledge_reuse_available: bool     // query_log ∪ injection_log non-empty
    transcript_fold_available: bool     // ≥1 declared session produced a fold
    compaction_available: bool          // ≥1 attributed compaction_events row
    context_reload_available: bool      // ≥2 sessions in cycle (cross-session window exists)
}
// Behavioral signals (transcript_error_count, transcript_refusal_count,
// signal_class_counts_json) are ALWAYS coarse/directional by construction — no flag,
// a constant presentation rule (ADR-003 #5046).
```

### `ReloadWindow` (Component 4 owns; parameterizes the one overlap primitive)

```
enum ReloadWindow {
    CrossSession,                       // context_reload: later-session reads overlapping any prior session
    PostCompaction { boundary_secs: i64 }, // compaction_reread: within-session reads after a session's earliest compacted_at
}
```

### Catalog index contract (ADR-008, pinned)

`class_counts[0] = error`, `class_counts[1] = refusal`. `MAX_SIGNAL_CLASSES = 16`. Read by fixed index; a producer reorder corrupts every transcript column with no type error (R-12 — pinned by AC + merge boundary test).

---

## Data flow (boundary crossings)

```
                          context_cycle_review handler (Component 9 — tools.rs)
  ┌──────────────────────────────────────────────────────────────────────────────┐
  │ auto_close arm (8) ── writes cycle_stop into cycle_events BEFORE rank-1 reads  │
  │                                                                                │
  │ read-before-purge (6): activity_snapshots_for_feature(fc) ─► Vec<(sid,Snap)>   │
  │        ── sum + width-convert ─► transcript_* + signal_class_counts_json       │
  │        ── STRICTLY BEFORE purge_cycle_transcripts(fc)                          │
  │                                                                                │
  │ aggregate reckoning (3): cycle_events + SessionRecord.outcome + query/injection│
  │        ── rank-1/2/3 i64 num/den ─► phase_* / rework_* / knowledge_reuse_*     │
  │                                                                                │
  │ reload reckoning (4+5):                                                        │
  │   context_reload  = round(compute_context_reload_pct(...) × 10000) ─► i64 bps  │
  │   compaction_reread = overlap(PostCompaction{MIN(compacted_at)}) per session   │
  │        gate: (read_ts_millis ÷ 1000) > compacted_at_secs   (seconds-vs-seconds)│
  │   compaction_count  = COUNT(attributed compaction_events rows)                 │
  │                                                                                │
  │ presence flags (7): MetricAvailability from source-non-empty per metric        │
  │                                                                                │
  │ persist (2): build CycleReviewRecord (all new cols) ─► store_cycle_review()    │
  │        ── ONLY at the full-pipeline return; 3 other returns DO NOT write       │
  └──────────────────────────────────────────────────────────────────────────────┘
                          │                              │
                          ▼                              ▼
              cycle_review_index (v5)          RetrospectiveReport (presentation)
              durable i64/TEXT columns         "unavailable" | "~directional" rendering
```

Boundary types that cross between components:
- crt-054 → Component 6: `Vec<(String /*session_id*/, ActivitySnapshot)>` (u64/u32 widths).
- Component 5 store-accessor → Component 5 reckoning: `Vec<i64 /*compacted_at_secs*/>` and `MIN(compacted_at)` per session.
- Component 3/4/5/6 → Component 2: the populated `CycleReviewRecord` (all `i64`/`String`).
- Component 7 → presentation: `MetricAvailability` + the record (formatter branches).

---

## Wave order (architecture §7 — one v5 migration spans Waves 2+3)

1. **Wave 1 — fail-loud guard (Component 7).** Lowest risk; de-risks believable-zero before any column lands. Per-metric `MetricAvailability`. No migration coupling.
2. **Wave 2 — durable aggregates + v5 migration (Components 1, 2, 3).** Single three-path bump (`migration.rs` v29→v30 + `db.rs` fresh-create + pinned-version test, all in one change), `store_cycle_review` extension, four-return discipline, rank-1/2/3 reckoning. The migration adds **every** v5 column (Waves 2 and 3 share it). `auto_close` (Component 8) rides here.
3. **Wave 3 — dual reload + fold surfacing (Components 4, 5, 6).** Consumes crt-054. Read-before-purge landing, overlap engine, compaction gate. Columns are part of the Wave-2 migration. Pipeline ordering (Component 9) integrates all of it.

Sequencing constraints:
- Component 1's migration must exist before Components 2/3/4/5/6 can persist (Wave 2 first).
- Component 8 (`auto_close`) must run **before** Component 3's rank-1 read (timeline must be closed). See `review_pipeline.md`.
- Component 6 (fold read) must run **before** `purge_cycle_transcripts` (read-before-purge). See `review_pipeline.md`.
- Component 2 (the single writer) is the only write site; Components 3/4/5/6 produce values into the record, never write the table themselves.

---

## Binding constraints (apply across all component files)

1. **Single writer / no zero-clobber** — new columns written ONLY at the full-pipeline `store_cycle_review()`; memo-hit / purged-retain / force+purged returns never write them (ADR-002 #5037).
2. **Every metric column INTEGER** — `context_reload_pct` is basis points `round(fraction × 10000)` clamped 0–10000; no `f64`/REAL, no `is_finite()` guard (designed out, ADR-005 #5047). Producer `u64`/`u32` → `i64` via checked/saturating conversion (R-09).
3. **Seconds-normalization** — `compaction_reread` gate compares `(read_ts_millis ÷ 1000) > compacted_at_secs`; normalize the read side only, never the boundary (ADR-006 #5048).
4. **Read-before-purge** — fold read strictly precedes `purge_cycle_transcripts`; asserted, inversion zeroes columns (ADR-007 #5042).
5. **Structural leak gate** — no content field on `CycleReviewRecord`/`RetrospectiveReport`; integers/aggregates only (NFR-01).
6. **Coarse/directional behavioral signals** — `transcript_error_count`/`refusal_count`/`signal_class_counts_json` render with a directional qualifier, visually distinct from exactly-counted aggregates (ADR-003 #5046).
7. **Bytes, not tokens** — no token-named field, no `reread`/`compaction` regex class.
8. **Informs, never controls** — no metric controls/bills/schedules/blocks execution.
