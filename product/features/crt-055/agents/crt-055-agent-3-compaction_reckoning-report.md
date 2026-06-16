# Agent Report — crt-055 Component 5: compaction_reread reckoning + compaction_events read accessor

**Agent ID:** crt-055-agent-3-compaction_reckoning
**Crates:** unimatrix-store (read accessor) + unimatrix-observe (reckoning)
**ADRs:** ADR-006 (#5048 — clock/unit + earliest boundary), ADR-005 (#5047 — two columns never collapsed)

## Summary

Two parts delivered:

1. **STORE read accessor** (`unimatrix-store`): three READ-ONLY accessors on crt-054's
   `compaction_events` table. No schema/row mutation (crt-054 owns the table). `compacted_at`
   read verbatim as Unix seconds; `high_water` never read (reserved, ADR-006).
2. **OBSERVE reckoning** (`unimatrix-observe`): `reckon_compaction_reread` drives the shared
   `overlap_count` primitive (Component 4) per session with each session's own
   `MIN(compacted_at)` boundary, summing per-session overlaps. The binding floor + strict-`>`
   gate is enforced.

`compaction_count` is the store accessor `compaction_count_for_sessions` itself (COUNT over the
cycle's declared session ids) — no derivation in observe; undeclared/evicted sessions (#4140) are
absent from the declared list and never mis-attribute (R-05/AC-11).

## Files modified

- `/workspaces/unimatrix/crates/unimatrix-store/src/compaction_read.rs` (new — 3 read accessors + 8 tests)
- `/workspaces/unimatrix/crates/unimatrix-store/src/lib.rs` (registered `mod compaction_read`)
- `/workspaces/unimatrix/crates/unimatrix-observe/src/cycle_aggregates/compaction_reckoning.rs` (new — `reckon_compaction_reread` + 11 tests)
- `/workspaces/unimatrix/crates/unimatrix-observe/src/cycle_aggregates.rs` (declared + re-exported submodule)
- `/workspaces/unimatrix/crates/unimatrix-observe/src/lib.rs` (re-exported `reckon_compaction_reread`)

## Read accessor API (unimatrix-store, all `read_pool()`, parameterized)

- `min_compacted_at(session_id) -> Result<Option<i64>>` — `MIN(compacted_at)`; `None` when no rows (≠ boundary 0).
- `compaction_boundaries(session_id) -> Result<Vec<i64>>` — `ORDER BY compacted_at ASC`.
- `compaction_count_for_sessions(&[String]) -> Result<i64>` — `COUNT(*) ... WHERE session_id IN (...)`; empty list → 0 (no scan).

## Reckoning API (unimatrix-observe)

- `reckon_compaction_reread(records, boundaries: &HashMap<session_id, MIN_secs>) -> i64`.
- Pure (no DB) — the caller (Component 9 pipeline) resolves `boundaries` from `min_compacted_at`
  over declared sessions, then drives this. Keeps the gate unit-testable without a DB.

## Handoff to Component 9 (pipeline)

The pipeline must, over the cycle's DECLARED session ids:
1. `compaction_count = store.compaction_count_for_sessions(&declared_ids)`.
2. Build `boundaries: HashMap<sid, MIN>` from `store.min_compacted_at(sid)` (skip `None`).
3. `compaction_reread_count = reckon_compaction_reread(&records, &boundaries)`.
Both land on `CycleAggregates` (fields already present); the single writer (Component 2) persists.

## Binding gate (ADR-006) — verified by tests

`counts IFF (read.ts_millis ÷ 1000) > compacted_at` — integer FLOOR (normalize READ side only),
STRICT `>`. Canonical AC-22 case asserted with expected count = 1:
- exact boundary `T*1000` → floor `T` → `T > T` false → NOT counted (strict `>`)
- −500ms `T*1000−500` → floor `T−1` → `T−1 > T` false → NOT counted (floor-catching guard)
- +1s `T*1000+1000` → floor `T+1` → `T+1 > T` true → counted

A dedicated test (`test_gate_unnormalized_millis_would_overcount_floor_prevents`) pins that the
−500ms read does NOT count, catching an unnormalized millis-vs-seconds compare (~1000× over).

## Tests

- `cargo test -p unimatrix-observe` (compaction filter): **11 passed, 0 failed**
- `cargo test -p unimatrix-store --features test-support --lib` (compaction_read filter): **8 passed, 0 failed**
- `cargo build -p unimatrix-observe` / `-p unimatrix-store`: clean
- `cargo clippy` on both target files: no warnings/errors
- `cargo fmt`: applied

Test coverage maps to plan: read-accessor ordering + attribution + injection guard (R-12/R-05);
gate floor/strict-`>`/before/after/exact (R-08/AC-12); multi-compaction MIN, each-read-once,
per-session no-bleed, no-boundary→0, compacts-but-never-rereads→0; canonical AC-22 unit case.

## Scope boundaries respected

- Rust unit-level gate + seconds-normalization done here (AC-11, AC-12, AC-22 unit).
- The cross-table AC-22 INTEGRATION (pytest) is Stage 3c — NOT touched.
- No git commands run (Delivery Leader owns git).

## Issues / blockers

None. One design note for the pipeline integrator: `overlap_count(PostCompaction{boundary})`
applies one boundary to ALL sessions in the slice, so the per-session MIN gating is achieved by
filtering records per session and calling the primitive once per session (see stored pattern #5065).
Pre-existing: `write_ext.rs` (860 lines) and `cycle_review_index.rs` exceed 500 lines but are
out of my scope; my new files are 196 (store) / 222 (observe) lines.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) + context_get #5048 (ADR-006) -- surfaced ADR-006 binding clock/unit + earliest-boundary contract; no prior compaction-gate pattern existed.
- Stored: entry #5065 "compaction_reread gate must drive overlap_count per-session with that session's MIN boundary, not one shared boundary" via /uni-store-pattern.
