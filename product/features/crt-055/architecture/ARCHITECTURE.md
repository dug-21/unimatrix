# crt-055 Architecture — context_cycle_review redesign (consumer; owns cycle_review_index schema + the producer contract)

**Date**: 2026-06-16
**Feature**: crt-055 (#755) — Cortical (crt), learning & drift
**Goal**: self-learning (#4677)
**Binding inputs**: `product/features/crt-055/SCOPE.md` (§"Producer contract", §"Consumer persistence", §"Binding design constraints" are authoritative), `product/features/crt-055/SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-11), ass-077 FINDINGS (substrate + RQ-2 ranks), ass-078 FINDINGS (fold-at-ingest).
**Paired feature**: crt-054 (#752) — the PRODUCER. crt-055 OWNS the contract crt-054 implements against; this document reconciles crt-054's design field-by-field (§9).

> crt-055 is the **consumer half** of the producer/consumer pair. It owns `cycle_review_index`, `RetrospectiveReport`/`CycleReviewRecord`, `store_cycle_review`, `SUMMARY_SCHEMA_VERSION` (4→5), all review-time reckoning, the fail-loud presentation guard, and the folded point-issues. crt-054 supplies two raw inputs (`compaction_events` rows + `activity_snapshot()`); crt-055 does all reckoning and persistence.

---

## 1. System Overview

`context_cycle_review` is the consuming surface for a cluster of fixes/features but no feature owned the review redesign itself. crt-055 is that redesign. It operationalizes the **ass-077 substrate decision**: durable per-cycle aggregates on `cycle_review_index` sourced from durable streams (PostToolUse + cycle_events + corpus + the producer's two surfaces); the transcript is response-time enrichment, never the persisted substrate. The structural leak gate stays intact (no content field on `RetrospectiveReport`/`CycleReviewRecord`).

Five increments, one migration, one `SUMMARY_SCHEMA_VERSION` bump (4→5):

1. **Fail-loud presentation guard** — render "unavailable", never `0`, when a metric's source class is empty for the cycle (per-metric `raw_signals_available`).
2. **Durable per-cycle aggregate columns** (ass-077 RQ-2 ranks 1–3) — phase durations/transitions/rework, rework ratio, knowledge-reuse-all-served.
3. **Dual reload metrics** — `context_reload` (cross-session, promoted from #758) and `compaction_reread` (compaction-gated within-cycle). Two columns, two gates, one overlap engine, never collapsed.
4. **Surface crt-054's transcript fold** — `bytes_total`, `delta_count`, `error`/`refusal` class-counts, forward-compatible `signal_class_counts_json`. Read at review via `activity_snapshot()` before the crt-052 hold purge (read-before-purge).
5. **Folded point-issues** — #556 (never-closed phases → rank-1 hotspot), #320 (knowledge-reuse all-served), #593 (`auto_close`), #206-item-4 (knowledge-that-helped, response-time enrichment).

```
        DURABLE STREAMS (review-time reads)            REVIEW PIPELINE (crt-055)            cycle_review_index (crt-055 owns)
  ┌────────────────────────────────────────────┐  ┌──────────────────────────────────┐  ┌──────────────────────────────┐
  │ cycle_events  ───────────────────────────▶ │  │ rank-1/2/3 aggregate reckoning   │  │ phase_* / rework_* / reuse_* │
  │ SessionRecord.outcome ────────────────────▶│  │                                  │  │                              │
  │ query_log ∪ injection_log (#320) ─────────▶│  │ knowledge-reuse-all-served       │  │ knowledge_reuse_*            │
  │ PostToolUse observations ─────────────────▶│  │ overlap engine ── context_reload │  │ context_reload_pct           │
  │                                            │  │                └ compaction_reread│  │ compaction_count / _reread   │
  │ compaction_events (crt-054 Surface A) ────▶│  │ gate: ts÷1000 > compacted_at(s)  │  │                              │
  │ activity_snapshot() (crt-054 Surface B) ──▶│  │ read-before-purge, sum per cycle │  │ transcript_* / signal_*_json │
  └────────────────────────────────────────────┘  │  ↓ single store_cycle_review()   │  └──────────────────────────────┘
                                                   │  4 returns, no zero-clobber      │
                                                   └──────────────────────────────────┘
```

**Vision boundary (RQ-8, hard edge).** Every column **informs, never controls**. Disqualifying test for any element: *does this metric control / bill / schedule / block execution?* If yes, out of lane. The throughput unit is **bytes**, never tokens, never cost. No orchestration/FinOps surface.

---

## 2. Component Breakdown

| Component | Responsibility | Location (seam) |
|-----------|---------------|-----------------|
| `cycle_review_index` schema (v5) | The full set of crt-055-owned aggregate columns (rank 1–3, dual reload, transcript fold). Single owner. | `unimatrix-store/src/cycle_review_index.rs`, `migration.rs` |
| `CycleReviewRecord` extension | New `i64`/`f64`/`String` fields mirroring the new columns; `Default`-derived; **no content field**. | `cycle_review_index.rs:72` (the struct) |
| `store_cycle_review()` (extended) | The single writer. Binds the new columns in INSERT + UPDATE. No second writer. | `cycle_review_index.rs:209` |
| Aggregate reckoning module | Rank-1/2/3 derivations from durable streams (cycle_events, SessionRecord.outcome, query_log ∪ injection_log). | `unimatrix-observe` (sibling to `session_metrics.rs`) |
| Reload overlap engine | One overlap primitive, two callers: `context_reload` (cross-session) and `compaction_reread` (compaction-gated within-cycle). | `unimatrix-observe/src/session_metrics.rs` |
| `compaction_reread` reckoning | PostToolUse file-overlap reads with `(ts_millis ÷ 1000) > compacted_at` per session (read `ts` normalized millis→seconds, ADR-006), attributed to the cycle via the declaration chain. | `unimatrix-observe` + `compaction_events` read in `unimatrix-store` |
| Activity-fold landing | Sum `activity_snapshot()` across the cycle's held sessions; checked/saturating `u64/u32`→`i64`; build `signal_class_counts_json`. | `unimatrix-server/src/mcp/tools.rs` review pipeline |
| Fail-loud presentation guard | Per-metric source-presence flags; formatter renders "unavailable" not `0` when a source class is empty. | `RetrospectiveReport` presentation + `unimatrix-observe` |
| `auto_close` handler arm (#593) | `auto_close: bool` param; when `true` and no `cycle_stop` exists, write the stop event synchronously before the pipeline. | `unimatrix-server/src/mcp/tools.rs` |
| `compaction_events` read accessor | `SELECT … WHERE session_id IN (…)`; attribute to cycle via declaration chain. | `unimatrix-store` (read-side of crt-054's table) |

What crt-055 does **NOT** build (owned elsewhere): the transcript fold mechanism + `ActivitySnapshot` struct + `compaction_events` table/writer + `[transcript_signals]` config compilation (all crt-054); `compute_context_reload_pct`'s ingest (#758, crt-055 promotes it to a column); the crt-052 Wave-B hold + `purge_cycle_transcripts`.

---

## 3. Component Interactions / Data Flow

### Review pipeline order (single full-pipeline block, `tools.rs`)
1. **`auto_close` (#593)** — if `auto_close == true` and no `cycle_stop` row exists for the cycle, write `cycle_stop` synchronously (so the phase timeline closes before rank-1 reckoning reads it).
2. **Read-before-purge (Constraint 6, SR-09)** — call the activity collector (`activity_snapshots_for_feature`, crt-054) to read each held session's `activity_snapshot()` **before** `purge_cycle_transcripts` zeroes the buffers. Sum `bytes_total`/`delta_count`/`class_counts` across the cycle's sessions.
3. **Aggregate reckoning** — rank-1 (cycle_events: durations, transitions, rework loops, #556 never-closed phases), rank-2 (SessionRecord.outcome rework ratio), rank-3 (query_log ∪ injection_log knowledge-reuse-all-served, #320).
4. **Reload reckoning** — `context_reload_pct` from #758's `compute_context_reload_pct` (cross-session), converted to basis points (`round(pct × 100)`, integer) at the persist boundary; `compaction_reread_count` from PostToolUse overlap gated on `compaction_events.compacted_at` (within-cycle). **Clock-unit normalization (binding, ADR-006):** the read `ts` is epoch millis (`observations.ts_millis`), `compacted_at` is Unix seconds; normalize the read `ts` to seconds (`ts_millis / 1000`, floor) at this reckoning site *before* the `read_ts_secs > compacted_at` gate. Skipping this makes every read pass (millis ≫ seconds) — a silent gate break.
5. **Per-metric presence flags** — set each metric's `available` flag from whether its source class is non-empty (drives the fail-loud guard).
6. **Persist** — build the `CycleReviewRecord` with all new columns and write via the single `store_cycle_review()` (full-pipeline return only).

### The four success returns (#4750 — single writer, no zero-clobber, SR-01)
The new columns are written **only** at the full-pipeline return. The other three returns do not write them:
- **memo-hit return** — serve the stored record; no recompute, no write.
- **purged-retain return** (#758 data-presence gate) — source purged, schema stale: **retain stored bytes untouched**, surface the "source purged, cannot recompute" advisory; no write.
- **force + purged stored-record return** — serve stored record; no clobber.
- **full-pipeline return** — the one writer (INSERT-or-UPDATE) that lands the new columns.

The #758 guarded-recompute (stale schema + source present → clear-memo and fall through to full pipeline) is the auto-refresh path for pre-v5 rows; it routes through the same single writer, so empty-clobber is structurally impossible (Constraint 3, SR-02).

### Error / source boundaries
- **Activity fold absent** (undeclared session, held-route miss): the fold dies fail-loud at the producer; crt-055 surfaces "unavailable" via the presence flag, never a fabricated `0` (SR-08).
- **`compaction_events` empty for the cycle**: `compaction_count = 0`, `compaction_reread` rendered "unavailable" (no compaction boundary to gate against) — distinct from a measured zero.
- **Integer width**: producer `u64`/`u32`/`[u32;N]` → `i64` columns via checked/saturating conversion at the persist boundary (SR-10).
- **`signal_class_counts_json`**: TEXT default `'{}'`; built from the full `class_name → count` map; forward-compatible for classes added beyond error/refusal.

---

## 4. Technology Decisions (ADR index)

| ADR | Decision |
|-----|----------|
| ADR-001 | crt-055 owns `SUMMARY_SCHEMA_VERSION` 4→5 + the single `cycle_review_index` v5 migration; three-path bump, crt-047 v23→v24 template, pragma-guarded ALTERs |
| ADR-002 | Single `store_cycle_review()` writer, four success returns, no zero-clobber on memo-hit / purged-retain / force+purged paths; coexist with #758 guarded-recompute |
| ADR-003 | Fail-loud presentation guard — per-metric source-presence flags render "unavailable", never `0`; sequenced first. Plus: regex-class-derived behavioral counts render coarse/directional (sibling honesty rule) |
| ADR-004 | Rank-1/2/3 durable aggregate column shapes from durable streams (cycle_events, SessionRecord.outcome, query_log ∪ injection_log) |
| ADR-005 | Dual reload metrics — two columns, two gates, one overlap engine, never collapsed; pinned overlap windows. `context_reload_pct` stored as basis-points INTEGER (not REAL) |
| ADR-006 | `compaction_reread` boundary selection — gate on the earliest `compacted_at` per session (single within-cycle boundary), counted once. Owns the binding seconds-normalization of the read `ts` (millis→seconds) before the gate |
| ADR-007 | Transcript-fold landing — read-before-purge, sum across held sessions, checked/saturating width conversion, `signal_class_counts_json` |
| ADR-008 | `[transcript_signals]` config shape + default domain-neutral catalog (v1: `0=error`, `1=refusal`), co-decided with crt-054 ADR-002; `MAX_SIGNAL_CLASSES = 16` |
| ADR-009 | Knowledge-that-helped (#206-4) is response-time enrichment, **not** a durable column |
| ADR-010 | `auto_close` (#593) writes `cycle_stop` synchronously before the pipeline when absent; idempotent |

---

## 5. Integration Points

- **crt-054 (#752)** — the producer. crt-055 reads `activity_snapshot()` (via `activity_snapshots_for_feature`) and `compaction_events`; lands all columns. Any field/width/catalog change is negotiated in crt-055's §"Producer contract" first.
- **#758 / #750 (merged `7aca6c44`)** — provides live cross-session `context_reload` (`compute_context_reload_pct`, `unimatrix-observe/src/session_metrics.rs:47`), `SUMMARY_SCHEMA_VERSION = 4`, and the guarded-recompute / data-presence-gate / purged-retain logic crt-055 coexists with.
- **crt-047 (v24)** — the integer-column-on-`cycle_review_index` + curation-health precedent; the column + migration template to copy (`cycle_review_index.rs:84-104`, `migration.rs:926-1059`).
- **crt-052 Wave B** — the transcript hold (ON, unconditional); `activity_snapshot()` durability-to-review rests on it; `purge_cycle_transcripts` is the read-before-purge ordering anchor.
- **`store_cycle_review()`** (`cycle_review_index.rs:209`) — the single writer to extend (INSERT at `:249`, UPDATE at `:284`).
- **The four success returns** (`tools.rs` `context_cycle_review`) — #4750: new columns written only at the full-pipeline return.

---

## 6. Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `SUMMARY_SCHEMA_VERSION` | `pub const SUMMARY_SCHEMA_VERSION: u32 = 4` → `5` | `unimatrix-store/src/cycle_review_index.rs:49` (existing) |
| `CycleReviewRecord` | `#[derive(Debug, Clone, Default)] pub struct CycleReviewRecord { … }` — extend with new `i64`/`f64`/`String` fields; **no content field** | `cycle_review_index.rs:72` (existing) |
| `store_cycle_review()` | `pub async fn store_cycle_review(&self, record: &CycleReviewRecord) -> Result<()>` — bind new columns in INSERT (`:249`) + UPDATE (`:284`) | `cycle_review_index.rs:209` (existing) |
| `compute_context_reload_pct` | `pub fn compute_context_reload_pct(...) -> f64` (cross-session overlap, returns a percentage). crt-055 converts to basis-points `i64` via `round(pct × 100)` at the persist boundary — no f64 bound to the column. | `unimatrix-observe/src/session_metrics.rs:47` (existing, #758) |
| PostToolUse read `ts` unit | `ObservationRecord.ts: u64` = **epoch millis** (`types.rs:39`); persisted column `observations.ts_millis: i64` (`observations.rs:16`). Gate normalizes to seconds via `/ 1000` (floor) — precedent `session_metrics.rs:115`. | `unimatrix-observe/src/types.rs:39`, `unimatrix-store/src/observations.rs:16` (existing) |
| `activity_snapshot()` | `pub fn activity_snapshot(&self) -> ActivitySnapshot` on `TranscriptBuffer` | crt-054 (NEW), `infra/session_transcript.rs` |
| `ActivitySnapshot` | `#[derive(Clone, Copy)] struct ActivitySnapshot { bytes_total: u64, delta_count: u32, class_counts: [u32; MAX_SIGNAL_CLASSES] }` | crt-054 (NEW) |
| activity collector | `fn activity_snapshots_for_feature(&self, feature_cycle: &str) -> Vec<(String, ActivitySnapshot)>` | crt-054 (NEW), `SessionRegistry`, `infra/session.rs` |
| `compaction_events` table | `id INTEGER PK, session_id TEXT NOT NULL, compacted_at INTEGER NOT NULL, high_water INTEGER NOT NULL DEFAULT 0`; INDEX on `session_id` | crt-054 (NEW) |
| `compaction_events` read | `SELECT compacted_at FROM compaction_events WHERE session_id = ?1 ORDER BY compacted_at ASC` (read-side, crt-055) | NEW, `unimatrix-store` |
| `MAX_SIGNAL_CLASSES` | `const MAX_SIGNAL_CLASSES: usize = 16`; v1 indices `0=error, 1=refusal` | crt-054 (NEW); value pinned here jointly (ADR-008) |
| `[transcript_signals]` config | `Vec<{ class_name: String, pattern: String, enabled: bool }>`, `#[serde(default)]`, `validate()`-bounded | crt-054 (NEW); shape co-decided here (ADR-008) |
| `purge_cycle_transcripts` | the crt-052 hold purge — the read-before-purge ordering anchor | crt-052 Wave B (existing) |

### New cycle_review_index columns (v5) — the complete consumer-persistence set

| Column | Type | Source | ADR |
|--------|------|--------|-----|
| `phase_count` | INTEGER NOT NULL DEFAULT 0 | cycle_events — declared phases | ADR-004 |
| `phase_transition_count` | INTEGER NOT NULL DEFAULT 0 | cycle_events — phase-end transitions | ADR-004 |
| `phase_rework_count` | INTEGER NOT NULL DEFAULT 0 | cycle_events — phase re-entries (rework loops) | ADR-004 |
| `phase_unclosed_count` | INTEGER NOT NULL DEFAULT 0 | cycle_events — declared-but-never-closed (#556) | ADR-004 |
| `phase_total_duration_secs` | INTEGER NOT NULL DEFAULT 0 | cycle_events — Σ closed-phase durations | ADR-004 |
| `rework_session_count` | INTEGER NOT NULL DEFAULT 0 | SessionRecord.outcome — rework/failure sessions | ADR-004 |
| `total_session_count` | INTEGER NOT NULL DEFAULT 0 | SessionRecord — denominator for the ratio | ADR-004 |
| `knowledge_reuse_served_count` | INTEGER NOT NULL DEFAULT 0 | query_log ∪ injection_log all-served (#320) | ADR-004 |
| `transcript_bytes_total` | INTEGER NOT NULL DEFAULT 0 | `ActivitySnapshot.bytes_total` (summed) | ADR-007 |
| `transcript_delta_count` | INTEGER NOT NULL DEFAULT 0 | `ActivitySnapshot.delta_count` | ADR-007 |
| `transcript_error_count` | INTEGER NOT NULL DEFAULT 0 | `ActivitySnapshot.class_counts[0]` | ADR-007 |
| `transcript_refusal_count` | INTEGER NOT NULL DEFAULT 0 | `ActivitySnapshot.class_counts[1]` | ADR-007 |
| `signal_class_counts_json` | TEXT NOT NULL DEFAULT '{}' | full `class_name → count` map | ADR-007 |
| `compaction_count` | INTEGER NOT NULL DEFAULT 0 | COUNT of attributed `compaction_events` rows | ADR-005 |
| `compaction_reread_count` | INTEGER NOT NULL DEFAULT 0 | PostToolUse overlap reads with `read_ts_secs > compacted_at` (read `ts` normalized millis→seconds first, ADR-006) | ADR-005/006 |
| `context_reload_pct` | INTEGER NOT NULL DEFAULT 0 — basis points 0–10000 | promoted #758 `compute_context_reload_pct` (a percentage), stored as `round(pct × 100)` basis points | ADR-005 |

> Ratios (rework ratio, knowledge-reuse rate) are derived at presentation from the stored numerator/denominator pairs — not stored as a single pre-divided number — so a "0 of 0" is distinguishable from "0 of N" (fail-loud, SR-08). **Every metric column is INTEGER** (uniform with crt-047, no REAL): `context_reload_pct` stores `compute_context_reload_pct`'s percentage as basis points (`round(pct × 100)`, 0–10000), so 37.5% → 3750. This drops the float-bind guard (`is_finite()`/`push_bind(f64)`, the #4529/#4533 footgun) outright and keeps cross-cycle baseline queries clean (ADR-005, human decision 2026-06-16).
>
> **Behavioral signals are coarse/directional.** `transcript_error_count`, `transcript_refusal_count`, and `signal_class_counts_json` derive from unvalidated regex matches against content-opaque deltas — they cannot be audited post-hoc and MUST render with a coarse/directional qualifier (ADR-003), distinct from exactly-counted aggregates (phase counts, session ratios).

---

## 7. Internal wave order (recommended)

Per SCOPE Open Q5 and SR-03's lowest-risk-first guidance:

1. **Wave 1 — fail-loud presentation guard (ADR-003).** Lowest risk; de-risks the believable-zero class (SR-08) before any column lands. Per-metric `available` flags on the existing `raw_signals_available` substrate.
2. **Wave 2 — durable aggregates + v5 migration (ADR-001, ADR-004).** The rank-1/2/3 columns + the single three-path migration + `store_cycle_review` extension + four-return discipline (ADR-002). One migration covers all v5 columns (waves 2 and 3 share it).
3. **Wave 3 — dual reload pair + fold surfacing (ADR-005, ADR-006, ADR-007).** Consumes crt-054. Read-before-purge landing, overlap engine, compaction gate. The columns are part of the same v5 migration from wave 2.

`auto_close` (ADR-010) and the catalog config (ADR-008) ride wave 2/3; #206-4 (ADR-009) is response-time only, no migration coupling.

---

## 8. Risk Coverage (SCOPE-RISK-ASSESSMENT)

| Risk | Where addressed |
|------|-----------------|
| SR-01 silent-zero / empty-clobber | ADR-002 — single writer, four returns, no write on memo-hit/purged-retain/force+purged; recompute via clear-memo-fall-through |
| SR-02 stale-version no-flush | ADR-001/002 — reuse #758 typed staleness + data-presence gate; three #5022 assertions (data-present recompute, purged retain, force+purged no-clobber) |
| SR-03 three-path bump | ADR-001 — crt-047 v23→v24 template, pragma-guarded ALTERs, pinned-version test moved in same change |
| SR-04 producer-contract drift | §9 reconciliation — #5006 already deprecated/corrected (#5032); field-by-field diff confirms alignment |
| SR-05 bytes-vs-tokens | §9 + ADR-007/008 — no token-named field; crt-054 ADR-005 (#5030) confirmed bytes-only; guard test asserts no token column |
| SR-06 dual-reload collapse | ADR-005 — two columns, two gates, one engine; pinned windows |
| SR-07 scope creep on point-issues | ADR-004 (locked rank shapes), ADR-009 (#206-4 response-only), ADR-008 (tiny catalog) |
| SR-08 held-route believable-zero | ADR-003/007 — per-metric presence flags; regression test asserts non-empty fold for a representative TS-client cycle |
| SR-09 read-before-purge | ADR-007 — review-pipeline read pinned ahead of `purge_cycle_transcripts`; ordering asserted in test |
| SR-10 attribution + int-width | ADR-006/007 — declaration-chain attribution; checked/saturating `u64/u32`→`i64`; undeclared folds die fail-loud |
| SR-11 multi-compaction boundary | ADR-006 — gate on earliest `compacted_at` per session, counted once |

---

## 9. crt-054 Producer-Contract Reconciliation

Field-by-field diff of crt-054's delivered design (ARCHITECTURE.md + ADR-001..010, Unimatrix #5026–#5034) against crt-055 SCOPE §"Producer contract". **Result: fully aligned. No drift requiring a contract change.**

### ALIGNED (verified)

| Contract element | crt-054 design | Status |
|------------------|----------------|--------|
| Ownership split: #758 owns v4; crt-055 owns v5 + cycle_review_index migration; crt-054 owns only compaction_events table | crt-054 ADR-008 (#5032, active) states exactly this; crt-054 ARCHITECTURE §1, §4, §5 confirm producer-only, no `SUMMARY_SCHEMA_VERSION`, no `cycle_review_index`, no `store_cycle_review` | ALIGNED |
| `compaction_events` columns: `id, session_id, compacted_at, high_water` | crt-054 §6 Integration Surface + ADR-007: `id INTEGER PK, session_id TEXT NOT NULL, compacted_at INTEGER NOT NULL, high_water INTEGER NOT NULL DEFAULT 0`, INDEX on session_id | ALIGNED (exact) |
| `feature_cycle` NOT stored on compaction_events (late-bind at review) | crt-054 ADR-004 + ARCHITECTURE §3: row written regardless of declaration; attribution via declaration chain at review | ALIGNED |
| `compacted_at` = Unix seconds (gate boundary for PostToolUse `ts`) | crt-054 ARCHITECTURE §3 + Open Q3: `compacted_at` is seconds | ALIGNED on `compacted_at` = seconds; **CORRECTED on the gate comparison** — the read `ts` is **epoch millis** (`observations.ts_millis: i64`, `ObservationRecord.ts: u64` "epoch millis"), NOT seconds. The prior "seconds-vs-seconds" assumption was wrong. The gate normalizes the read `ts` to seconds (`÷1000`) before comparison — binding (Binding constraint 8, ADR-006). Not a producer-contract change: `compacted_at` stays seconds; the normalization is entirely consumer-side. |
| `ActivitySnapshot` fields: `bytes_total: u64, delta_count: u32, class_counts: [u32; MAX_SIGNAL_CLASSES]` | crt-054 ADR-003 + §6: exact, `Copy`, no `Display`, metadata-only `Debug` | ALIGNED (exact) |
| Fold on BOTH registered and held routes (held-route miss = #1 regression) | crt-054 ADR-001: accumulator embedded in `TranscriptBuffer`, folds on both routes by construction; ADR-009 held-route regression guard | ALIGNED |
| bytes-only; NO token/`token_bytes_per_unit` field or class | crt-054 ADR-005 (#5030) + ADR-002 (#5027): no token-named field anywhere; ADR-002 removed `reread`/`compaction` regex class | ALIGNED |
| Shared single `RegexSet` pass per delta; `[transcript_signals]` config; v1 `0=error, 1=refusal`; `validate()`-bounded | crt-054 ADR-002 (#5027): one shared `RegexSet`, `{class_name, pattern, enabled}`, `#[serde(default)]`, validate()-bounded, domain-neutral, v1 0=error/1=refusal | ALIGNED |
| Never-persisted by crt-054; crt-055 reads before crt-052 purge | crt-054 ADR-006 (#5031): survival-to-review, never zero/drop before the crt-052 hold purge | ALIGNED |

### Stale-knowledge action (SR-04, SR-05)

- **#5006** (prior crt-054 ADR-008 "first mover / owns v4/v29 / SUMMARY_SCHEMA_VERSION") — **already deprecated**; superseded by **#5032** (active, correct: crt-054 owns only `compaction_events` + next `CURRENT_SCHEMA_VERSION`; #758 owns 4; crt-055 owns 5). **No `context_correct` required** — the correction the SCOPE anticipated (line 170) was already applied during crt-054's regeneration. Verified via `context_get(5006)` (status: deprecated) and `context_get(5032)` (status: active). No action taken; the stale entry is already retired through proper provenance.
- **No token field** (SR-05): crt-054 ADR-005 (#5030) and ADR-002 (#5027) both confirm no token-named field and no `reread`/`compaction` regex class. crt-055 adds a guard test asserting no token-named column on `CycleReviewRecord`/`RetrospectiveReport`.

### Residual coordination points (not drift)

- **Schema-version number at merge** — crt-054 takes one `CURRENT_SCHEMA_VERSION` bump for `compaction_events`; crt-055 takes one for the `cycle_review_index` v5 ALTERs. Disjoint tables → no ALTER collision; the two take **distinct sequential numbers** (whoever merges first is N, the other N+1), an SM coordination point at merge (crt-054 ADR-008 / #5032; lesson #4095). `SUMMARY_SCHEMA_VERSION` 4→5 is crt-055's alone.
- **`high_water`** — crt-054 populates it (reserved for a future precise byte-boundary gate). crt-055 v1 gates on `compacted_at` only (ADR-006); does not read `high_water`. No drift — the column is reserved by design.

---

## 10. Open Questions

1. **Rank-3 knowledge-reuse-all-served exact source tables** (ADR-004). SCOPE/§"#320" specifies `query_log ∪ injection_log`. Confirm at spec time the exact table/column names for the injection log (the query log is established; the injection log surface should be verified against the current schema). *Owner: spec/pseudocode — does not change the column shape.*
2. **`auto_close` write path for `cycle_stop`** (ADR-010). Confirm the existing `cycle_stop` event-write helper is callable synchronously from the review handler without a second writer or lock contention with the pipeline. *Owner: spec/pseudocode.*
3. **Schema-version number** (residual, §9). The actual `CURRENT_SCHEMA_VERSION` integer (29 vs 30) is set at merge by the SM per merge order with crt-054. *Owner: SM coordination point.*
4. **RESOLVED (human decision, 2026-06-16): `context_reload_pct` is a basis-points INTEGER (0–10000), NOT REAL.** `compute_context_reload_pct` returns a percentage; multiply by 100 and round to nearest integer → basis points (37.5% → 3750). Drops the REAL column and the `is_finite()`/`push_bind(f64)` guard (the #4529/#4533 footgun) entirely — every metric column stays integer (uniform with crt-047), and cross-cycle baseline queries are cleaner. See ADR-005 and §6 column table. No longer open.
