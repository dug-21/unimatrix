# Component 4 — Reload overlap engine (context_reload + compaction_reread, one engine two callers)

**Crate**: `unimatrix-observe`
**Files**: `session_metrics.rs` (existing `compute_context_reload_pct:47`; new shared primitive + `compaction_reread` caller) + `cycle_aggregates.rs` persist-boundary conversion
**ADRs**: ADR-005 (#5047), ADR-006 (#5048) | **Risks**: R-07 (collapse), R-08 (clock/unit), R-09 (int width) | **Wave**: 3

## Purpose

One file-set-intersection overlap primitive, two callers with two windows, two columns, never collapsed:
- `context_reload_pct` — CROSS-SESSION continuity, promoted from #758's `compute_context_reload_pct`, stored basis points 0–10000.
- `compaction_reread_count` — WITHIN-CYCLE post-compaction tax (gate detail owned by Component 5; this file owns the shared primitive + the basis-points conversion + the two-window split).

## Constraints honored

- Two columns / two gates / one engine; windows pinned before building (Constraint 3 / R-07).
- `context_reload_pct` is INTEGER basis points; no `f64`/REAL reaches the bind; no `is_finite()` guard (Constraint 10, ADR-005). The only float in the system is the existing `compute_context_reload_pct` return, converted to i64 at the persist boundary.
- Seconds normalization for the compaction window (Constraint 9, delegated to Component 5).

## 4a. Shared overlap primitive (the one engine)

`compute_context_reload_pct` already encodes the cross-session intersection logic (`session_metrics.rs:47-105`: per-session file sets, walk chronological, count later-session reads that hit the cumulative prior set). Factor the reusable file-set-intersection core into a parameterized primitive so both callers share it without collapsing windows.

```
enum ReloadWindow {
    CrossSession,                          // prior = union of all earlier sessions' files
    PostCompaction { boundary_secs: i64 }, // prior = files read BEFORE boundary in the SAME session
}

fn overlap_count(records: &[ObservationRecord], window: ReloadWindow) -> (overlap, total):
    // Shared file-extraction (PostToolUse only; extract_file_path as today).
    match window:
      CrossSession:
          // existing compute_context_reload_pct body, but returning (reload_files, total_files_in_subsequent)
          // instead of the fraction — the fraction is reload/total.
      PostCompaction { boundary_secs }:
          // owned by Component 5 (compaction_reckoning.md); per session:
          //   prior_set = files read at (ts_millis/1000) <= boundary_secs
          //   for each read at (ts_millis/1000) > boundary_secs whose file ∈ prior_set: count once
          // returns (reread_count, _) — see Component 5 for the per-session gate + MIN(compacted_at).
```

Refactor discipline (R-07): keep `compute_context_reload_pct` as the thin cross-session caller of the primitive (or leave it intact and add the `PostCompaction` caller alongside, sharing the file-extraction helper). The two windows MUST stay distinct call sites — a single merged window is the failure mode the AC pins.

## 4b. context_reload_pct — promote + basis-points conversion

```
fn reckon_context_reload_bps(summaries, records) -> i64:
    fraction = compute_context_reload_pct(summaries, records)   // existing #758, returns [0.0, 1.0]
    bps = round(fraction * 10000.0) as i64                      // 0.375 → 3750
    return clamp(bps, 0, 10000)                                 // range guard before bind
```

> **BINDING ENCODING NOTE (load-bearing — see OVERVIEW Open-Q).** ADR-005/the brief phrase the source as "a percentage" with `round(pct × 100)`. The **live `compute_context_reload_pct` returns a fraction in [0.0, 1.0]**, not a 0–100 percentage. Correct basis-points encoding from a fraction is `round(fraction × 10000)` (0.375 → 3750). If, at implementation, the function is changed to return a 0–100 percentage, use `round(pct × 100)` instead. EITHER WAY the worked example 37.5% → 3750 holds. Confirm which form the function returns at implementation; the round-trip test (37.5% → 3750) is the guard.

The conversion happens at the persist boundary (in `cycle_aggregates.rs` / the handler), NOT inside `compute_context_reload_pct` — that function stays `-> f64` (its other callers, e.g. the live `RetrospectiveReport.context_reload_pct: Option<f64>`, are unchanged). Presentation divides the stored bps by 100 to display a percentage (Component 7).

Width safety (R-09): `round(...) as i64` cannot overflow (range 0–10000); the `clamp` is defensive against a future out-of-range fraction. No `f64` is bound to any column.

## 4c. Two-column wiring

```
aggregates.context_reload_pct      = reckon_context_reload_bps(summaries, records)     // 4b
aggregates.compaction_reread_count = reckon_compaction_reread(records, store, sessions) // Component 5
aggregates.compaction_count        = reckon_compaction_count(store, sessions)           // Component 5
```

Each metric's presence flag is set independently (Component 7): `context_reload_available` = (≥2 sessions in cycle); `compaction_available` = (≥1 attributed compaction_events row). A cycle with cross-session reload but zero compactions → `context_reload_pct` non-zero, `compaction_reread` unavailable, and vice versa (R-07 scenarios).

## Data flow

- IN: session summaries + `ObservationRecord`s for the cycle (cross-session); per-session records + compaction boundaries (post-compaction).
- OUT: `context_reload_pct: i64` (bps), and (via Component 5) `compaction_reread_count: i64`, `compaction_count: i64`.

## Error handling

- Single-session or empty cycle → `compute_context_reload_pct` returns 0.0 → bps 0; Component 7 marks `context_reload_available=false` (don't render a measured 0 for a window that can't exist).
- No compaction boundary → reread reckoning returns 0; `compaction_available=false`.

## Key test scenarios

- Both columns persist independently from distinct windows; neither derived from the other's window (AC-13, R-07).
- Cross-session reload present, zero compactions → `context_reload_pct` non-zero, `compaction_reread` unavailable.
- Compaction-rereads present, single session → `compaction_reread` non-zero, `context_reload` near-zero/unavailable.
- Basis-points round-trip: a fraction producing 0.375 persists 3750; rounding cases (0.00005→1, 0.99995→10000) round to nearest; out-of-range candidate clamped to 0–10000 (AC-20, R-09).
- Column type is INTEGER, no `f64` bound, no `is_finite()` guard present (AC-20).
