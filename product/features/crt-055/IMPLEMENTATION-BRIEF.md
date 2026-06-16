# crt-055 Implementation Brief

**Feature**: context_cycle_review redesign — durable per-cycle aggregates + dual reload metrics + transcript-fold surfacing
**Phase**: Cortical (crt) — learning & drift | **Goal**: self-learning (#4677) | **Tracking**: GH Issue #755
**Role**: CONSUMER of the crt-054 (#752) producer/consumer pair. crt-055 OWNS the producer contract crt-054 implements against.
**Alignment**: PASS 6/0 (no variances) — see ALIGNMENT-REPORT.md.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-055/SCOPE.md |
| Scope Risk Assessment | product/features/crt-055/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-055/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-055/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-055/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-055/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/crt-055/ACCEPTANCE-MAP.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| cycle_review_index schema v5 (columns + `CycleReviewRecord` + migration) | pseudocode/cycle_review_index_schema.md | test-plan/cycle_review_index_schema.md |
| store_cycle_review() extension (single writer, four returns) | pseudocode/store_cycle_review.md | test-plan/store_cycle_review.md |
| Aggregate reckoning (rank 1/2/3) | pseudocode/aggregate_reckoning.md | test-plan/aggregate_reckoning.md |
| Reload overlap engine (context_reload + compaction_reread) | pseudocode/reload_overlap_engine.md | test-plan/reload_overlap_engine.md |
| compaction_reread reckoning + compaction_events read accessor | pseudocode/compaction_reckoning.md | test-plan/compaction_reckoning.md |
| Activity-fold landing (read-before-purge, width conversion, JSON) | pseudocode/activity_fold_landing.md | test-plan/activity_fold_landing.md |
| Fail-loud presentation guard (per-metric availability) | pseudocode/fail_loud_guard.md | test-plan/fail_loud_guard.md |
| auto_close handler arm (#593) | pseudocode/auto_close.md | test-plan/auto_close.md |
| Review pipeline ordering (tools.rs context_cycle_review) | pseudocode/review_pipeline.md | test-plan/review_pipeline.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

> Component files are produced in Session 2 Stage 3a; paths above are expected, confirmed at delivery. Internal wave order (architecture §7): Wave 1 fail-loud guard → Wave 2 durable aggregates + v5 migration + store_cycle_review extension → Wave 3 dual reload pair + fold surfacing (consumes crt-054). One migration covers all v5 columns; `auto_close` and catalog config ride waves 2/3.

## Goal

Redesign `context_cycle_review` so its per-cycle signals are durable, comparable across cycles, and fail-loud. crt-055 adds durable per-cycle aggregate columns to `cycle_review_index`, lands two distinct reload metrics (cross-session `context_reload` and compaction-gated `compaction_reread`), surfaces crt-054's content-free transcript fold and compaction events into columns, and replaces believable-zero rendering with an explicit "unavailable" presentation guard. It folds four standalone issues (#556, #320, #593, #206-item-4) as ACs within one design session and one `cycle_review_index` migration. It informs the process; it never controls execution (RQ-8 hard edge — informs, never controls; bytes, never tokens; no orchestration/FinOps).

## Cross-Feature Delivery Dependency (crt-054 producer)

crt-055 is the CONSUMER; crt-054 (#752) is the already-designed PRODUCER, confirmed **fully aligned** to the Producer Contract (ARCHITECTURE §9, verified ALIGNED — no drift). crt-055's fold-surfacing and `compaction_reread`/`compaction_count` columns require crt-054's `compaction_events` table + `activity_snapshot()` / `activity_snapshots_for_feature()` to exist.

- **Disjoint-ownership boundary (R-12/R-18)** — a merge-time check confirms crt-054 does NOT bump `SUMMARY_SCHEMA_VERSION`, does NOT ALTER `cycle_review_index`, does NOT touch `store_cycle_review`/`CycleReviewRecord`. crt-054 owns only the `compaction_events` table + `ActivitySnapshot`.
- **Schema-version sequencing** — crt-054's `compaction_events` table migration and crt-055's `cycle_review_index` v5 ALTERs are **independent ALTERs on disjoint tables; order is free**. At merge they take distinct sequential `CURRENT_SCHEMA_VERSION` numbers (whoever merges first is N, the other N+1) — an SM coordination point (lesson #4095). `SUMMARY_SCHEMA_VERSION` 4→5 is crt-055's alone.
- **Index contract** — crt-055 reads `class_counts[0]=error`, `[1]=refusal` by fixed index per ADR-008; a producer catalog reorder corrupts every transcript column with no type error — pinned by AC and merge boundary test.

## Resolved Decisions

> Three binding human decisions (2026-06-16, this revision) are reflected below — ADR-005, ADR-003, and ADR-006 were `context_correct`ed (new Unimatrix ids: #5047, #5046, #5048). See "Binding Decisions (this revision)" section.

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Schema version ownership / migration | crt-055 owns `SUMMARY_SCHEMA_VERSION` 4→5 + the single `cycle_review_index` v5 migration; three-path bump, crt-047 v23→v24 template, pragma-guarded ALTERs | #758 owns v4 | architecture/ADR-001-summary-schema-version-v5-migration.md (Unimatrix #5051) |
| Writer discipline | Single `store_cycle_review()` writer, four success returns, no zero-clobber on memo-hit / purged-retain / force+purged; coexist with #758 guarded-recompute | SR-01/02, lesson #5022 | architecture/ADR-002-single-writer-four-returns-no-clobber.md (#5037) |
| Believable-zero + coarse-signal honesty | Per-metric source-presence flags render "unavailable", never `0`; sequenced first. **Plus (binding):** regex-class-derived behavioral counts (`transcript_error_count`/`refusal_count`/`signal_class_counts_json`) render with a coarse/directional qualifier, visually distinct from exactly-counted aggregates | SR-08, #4998, R-06 | architecture/ADR-003-fail-loud-presentation-guard.md (#5046) |
| Rank 1/2/3 column shapes | Durable aggregate columns from cycle_events, SessionRecord.outcome, query_log ∪ injection_log; num/den pairs, never pre-divided | ass-077 RQ-2, SR-07 | architecture/ADR-004-rank-aggregate-column-shapes.md (#5039) |
| Dual reload + basis-points encoding | Two columns, two gates, one overlap engine, never collapsed; pinned windows. **`context_reload_pct` stored as basis-points INTEGER (0–10000), NOT REAL** — `round(pct × 100)`; every metric column integer (uniform with crt-047); float footgun #4529/#4533 designed out (no `is_finite()` guard) | SR-06, R-09, Open Q4 | architecture/ADR-005-dual-reload-two-columns-two-gates-one-engine.md (#5047) |
| compaction_reread boundary + clock/unit | Gate on earliest `compacted_at` (MIN) per session; each re-read counted once. **Owns the binding seconds-normalization of the read `ts` (epoch millis ÷ 1000) before the gate** — PostToolUse `ts` is millis, `compacted_at` is seconds; integration test mandated | SR-11, R-08, Open Q2 | architecture/ADR-006-compaction-reread-boundary-selection.md (#5048) |
| Transcript-fold landing | Read-before-purge, sum across held sessions, checked/saturating u64/u32→i64, `signal_class_counts_json` | SR-09/10 | architecture/ADR-007-transcript-fold-landing-read-before-purge.md (#5042) |
| Signal catalog | `[transcript_signals]` shape + domain-neutral default (v1 `0=error`, `1=refusal`); `MAX_SIGNAL_CLASSES = 16`; co-decided with crt-054 ADR-002 | dsn-001/#4591 | architecture/ADR-008-transcript-signals-catalog-shared-surface.md (#5043) |
| Knowledge-that-helped (#206-4) | Response-time enrichment, NOT a durable column | Open Q3 | architecture/ADR-009-knowledge-that-helped-response-time-only.md (#5044) |
| auto_close (#593) | Writes `cycle_stop` synchronously before the pipeline when absent; idempotent | Open Q (folded) | architecture/ADR-010-auto-close-synchronous-cycle-stop.md (#5045) |

## Binding Decisions (this revision — human, 2026-06-16)

Three product-owner decisions promoted from open questions; each is binding and reflected in the columns, constraints, and acceptance map.

1. **`context_reload_pct` is a basis-points INTEGER (0–10000), not REAL.** Encoding: `compute_context_reload_pct`'s percentage × 100, rounded to nearest integer (37.5% → 3750). Every `cycle_review_index` metric column is now integer — uniform with crt-047, no REAL/float column. The non-finite-float footgun (`push_bind(f64)` without `is_finite()`, #4529/#4533) is **designed out by construction**, not deferred to a runtime guard — there is **no `is_finite()` AC** for this column. Resolves prior Open Q4. (FR-16, AC-20, Constraint 10, ADR-005 #5047)
2. **Behavioral signals are coarse/directional.** `transcript_error_count`, `transcript_refusal_count`, and `signal_class_counts_json` derive from unvalidated, content-opaque regex matches — directional, not auditable post-hoc. The rendered report MUST present them with a coarse/directional qualifier, visually distinct from exactly-counted aggregates (phase counts, session ratios, compaction_count). No behavioral-signal value may render as an exact, auditable count. Presentation-honesty sibling to FR-01's "unavailable"-not-"0" guard. (FR-11b, AC-21, NFR-08, ADR-003 #5046)
3. **Clock/unit is a binding contract clause + integration test.** The `compaction_reread` gate normalizes ALL timestamps to Unix **seconds** before comparison. PostToolUse read `ts` is epoch **milliseconds** (`observations.ts_millis: i64`, `ObservationRecord.ts: u64`) → `÷ 1000` (integer floor, `session_metrics.rs:115` convention); `compaction_events.compacted_at` is Unix **seconds** (untouched). A unit mismatch (millis vs seconds) makes every read pass or none do — a silent gate break. A mandated INTEGRATION test asserts a read at the boundary +500ms / +1s counts and −500ms does not. Promoted out of "open questions" to a binding requirement. (FR-14b, AC-22, Constraint 9, ADR-006 #5048)

## Files to Create/Modify

| File | Change |
|------|--------|
| `unimatrix-store/src/cycle_review_index.rs` | Bump `SUMMARY_SCHEMA_VERSION` 4→5 (`:49`); extend `CycleReviewRecord` (`:72`) with new `i64`/`String` fields — **every metric field integer (`i64`), no `f64`/REAL**, `context_reload_pct` is basis-points `i64`; no content field; extend `store_cycle_review()` (`:209`) INSERT (`:249`) + UPDATE (`:284`) to bind new columns |
| `unimatrix-store/src/migration.rs` | Add the v5 `cycle_review_index` ALTER block (pragma-guarded, crt-047 v23→v24 template, `migration.rs:926-1059`); update pinned-version test assertion in same change; cascade-file check (#4484) |
| `unimatrix-store/src/db.rs` | Fresh-create path for the new columns (three-path bump, #4153) |
| `unimatrix-store` (read accessor) | `compaction_events` read: `SELECT compacted_at FROM compaction_events WHERE session_id = ?1 ORDER BY compacted_at ASC` (read-side of crt-054's table) |
| `unimatrix-observe/src/session_metrics.rs` | Reload overlap engine: one primitive, two callers (`context_reload` cross-session via #758's `compute_context_reload_pct:47`; `compaction_reread` within-cycle) |
| `unimatrix-observe` (aggregate module) | Rank-1/2/3 reckoning from durable streams; `compaction_reread` gate `(ts_millis ÷ 1000) > compacted_at` — read `ts` normalized millis→seconds before the seconds-vs-seconds gate (FR-14b, ADR-006) |
| `unimatrix-server/src/mcp/tools.rs` | `context_cycle_review` review pipeline: `auto_close` arm, read-before-purge activity-fold landing, presence flags, single full-pipeline persist; `auto_close: bool` param |

## Data Structures

`ActivitySnapshot` (crt-054-produced, crt-055 reads — DO NOT define here):
```
#[derive(Clone, Copy)] struct ActivitySnapshot {
    bytes_total: u64, delta_count: u32, class_counts: [u32; MAX_SIGNAL_CLASSES]
}   // MAX_SIGNAL_CLASSES = 16; v1 indices 0=error, 1=refusal
```

`compaction_events` table (crt-054-owned — crt-055 reads only):
```
id INTEGER PK, session_id TEXT NOT NULL, compacted_at INTEGER NOT NULL (Unix seconds),
high_water INTEGER NOT NULL DEFAULT 0 (reserved; crt-055 v1 does not read it); INDEX on session_id
```

New `cycle_review_index` columns (v5) — crt-055 OWNS; extend `CycleReviewRecord` to mirror (**every metric field `i64`, no `f64`/REAL**):

| Column | Type | Source | ADR |
|--------|------|--------|-----|
| `phase_count` | INTEGER NOT NULL DEFAULT 0 | cycle_events — declared phases | 004 |
| `phase_transition_count` | INTEGER NOT NULL DEFAULT 0 | cycle_events — phase-end transitions | 004 |
| `phase_rework_count` | INTEGER NOT NULL DEFAULT 0 | cycle_events — phase re-entries (rework loops) | 004 |
| `phase_unclosed_count` | INTEGER NOT NULL DEFAULT 0 | cycle_events — declared-but-never-closed (#556) | 004 |
| `phase_total_duration_secs` | INTEGER NOT NULL DEFAULT 0 | cycle_events — Σ closed-phase durations | 004 |
| `rework_session_count` | INTEGER NOT NULL DEFAULT 0 | SessionRecord.outcome — rework/failure sessions | 004 |
| `total_session_count` | INTEGER NOT NULL DEFAULT 0 | SessionRecord — ratio denominator | 004 |
| `knowledge_reuse_served_count` | INTEGER NOT NULL DEFAULT 0 | query_log ∪ injection_log all-served (#320) | 004 |
| `transcript_bytes_total` | INTEGER NOT NULL DEFAULT 0 | `ActivitySnapshot.bytes_total` (summed) | 007 |
| `transcript_delta_count` | INTEGER NOT NULL DEFAULT 0 | `ActivitySnapshot.delta_count` | 007 |
| `transcript_error_count` | INTEGER NOT NULL DEFAULT 0 | `ActivitySnapshot.class_counts[0]` | 007 |
| `transcript_refusal_count` | INTEGER NOT NULL DEFAULT 0 | `ActivitySnapshot.class_counts[1]` | 007 |
| `signal_class_counts_json` | TEXT NOT NULL DEFAULT '{}' | full `class_name → count` map | 007 |
| `compaction_count` | INTEGER NOT NULL DEFAULT 0 | COUNT of attributed `compaction_events` rows | 005 |
| `compaction_reread_count` | INTEGER NOT NULL DEFAULT 0 | PostToolUse overlap reads with `read_ts_secs > compacted_at` (read `ts` normalized millis→seconds first, ADR-006 #5048) | 005/006 |
| `context_reload_pct` | **INTEGER NOT NULL DEFAULT 0 — basis points 0–10000** | promoted #758 `compute_context_reload_pct` (a percentage), stored as `round(pct × 100)` basis points | 005 |

> Ratios derived at presentation from stored num/den pairs — never a pre-divided number (so "0 of 0" ≠ "0 of N", R-17). **Every metric column is INTEGER** (uniform with crt-047 — no REAL/float column): `context_reload_pct` stores `round(pct × 100)` basis points (37.5% → 3750). This drops the float-bind guard (`is_finite()`/`push_bind(f64)`, the #4529/#4533 footgun) outright — there is no float reaching the bind. **Behavioral signals coarse/directional:** `transcript_error_count`, `transcript_refusal_count`, `signal_class_counts_json` derive from unvalidated regex matches against content-opaque deltas — render with a coarse/directional qualifier (ADR-003 #5046), distinct from exactly-counted aggregates.

## Function Signatures

| Symbol | Signature | Owner |
|--------|-----------|-------|
| `SUMMARY_SCHEMA_VERSION` | `pub const SUMMARY_SCHEMA_VERSION: u32 = 5` (was 4) | crt-055 (cycle_review_index.rs:49) |
| `store_cycle_review` | `pub async fn store_cycle_review(&self, record: &CycleReviewRecord) -> Result<()>` (extend binds) | crt-055 (cycle_review_index.rs:209) |
| `compute_context_reload_pct` | `pub fn compute_context_reload_pct(...) -> f64` (cross-session, returns a percentage). crt-055 converts to basis-points `i64` via `round(pct × 100)` at the persist boundary — **no f64 bound to the column** | #758 (session_metrics.rs:47) |
| PostToolUse read `ts` unit | `ObservationRecord.ts: u64` = epoch **millis** (`types.rs:39`); column `observations.ts_millis: i64`. Gate normalizes to seconds via `÷ 1000` (floor, `session_metrics.rs:115`) before `read_ts_secs > compacted_at` | #758 (existing) |
| `activity_snapshot` | `pub fn activity_snapshot(&self) -> ActivitySnapshot` on `TranscriptBuffer` | crt-054 (READ) |
| `activity_snapshots_for_feature` | `fn activity_snapshots_for_feature(&self, feature_cycle: &str) -> Vec<(String, ActivitySnapshot)>` | crt-054 (READ) |
| `context_cycle_review` handler | accepts new `auto_close: bool` param (default `false`) | crt-055 (tools.rs) |
| `purge_cycle_transcripts` | crt-052 hold purge — the read-before-purge ordering anchor | crt-052 (existing) |

## Review Pipeline Order (single full-pipeline block, tools.rs)

1. `auto_close` (#593) — if `true` and no `cycle_stop` row, write `cycle_stop` synchronously via the existing `cycle_events` event writer (NOT a second `store_cycle_review`), BEFORE rank-1 reads the timeline.
2. Read-before-purge — `activity_snapshots_for_feature()` to read each held session's snapshot BEFORE `purge_cycle_transcripts`; sum across the cycle's sessions.
3. Aggregate reckoning — rank-1 (cycle_events, incl. #556 never-closed), rank-2 (SessionRecord.outcome), rank-3 (query_log ∪ injection_log #320).
4. Reload reckoning — `context_reload_pct` (cross-session, converted to basis points `round(pct × 100)` at the persist boundary — INTEGER, no float bind) + `compaction_reread_count` (within-cycle, gated on earliest `compacted_at`; read `ts` normalized millis→seconds first — `(ts_millis ÷ 1000) > compacted_at`).
5. Per-metric presence flags — set each metric's `available` from source non-empty (drives fail-loud guard).
6. Persist — build `CycleReviewRecord`, write via the single `store_cycle_review()` (full-pipeline return only).

## Constraints

1. **Single writer, no zero-clobber** — new columns written ONLY via the one full-pipeline `store_cycle_review()`; memo-hit / purged-retain / force+purged returns do NOT write them. No second writer near the memo/`check_stored_review` site. (Constraint 2; SR-01)
2. **Coexist with #758 guarded-recompute + data-presence gate** — recompute via clear-memo-and-fall-through (routes through the single writer); purged-retain stays no-write, returns byte-identical. The three #5022 assertions must hold. (Constraint 3; SR-02)
3. **Dual reload never collapsed** — two columns, two gates, one engine; overlap windows pinned before building. (Constraint 5; SR-06)
4. **Read-before-purge** — read `activity_snapshot()` strictly before the crt-052 hold purge; the ordering is load-bearing and asserted (inversion test zeroes columns). (Constraint 6; SR-09)
5. **Structural leak gate** — no content field on `RetrospectiveReport`/`CycleReviewRecord`; persist integers/aggregates only. `test_candidates_structurally_absent_from_memoized_report` holds. (Constraint 1; SR-05)
6. **Producer contract is binding** — consume `compaction_events` + `activity_snapshot()` exactly; any drift reconciled in SCOPE §"Producer contract" first. (Constraint 7)
7. **Single migration / one version bump** — one `cycle_review_index` migration adding all v5 columns, one `SUMMARY_SCHEMA_VERSION` 4→5; three-path bump (ALTER + db.rs fresh-create + pinned-version test move together), pragma-guarded, idempotent.
8. **Bytes, not tokens** — throughput unit is bytes; no token-named field; no `reread`/`compaction` regex class. (SR-05)
9. **Clock/unit contract (binding)** — the `compaction_reread` gate operates in Unix **seconds**; all timestamps normalized to seconds before comparison (PostToolUse read `ts` millis ÷ 1000; `compacted_at` seconds, untouched). Unit-consistency verified by a mandated INTEGRATION test (read at boundary +500ms/+1s counts, −500ms does not). A millis-vs-seconds mismatch is a defect, not an approximation. (FR-14b, AC-22, Constraint 9, ADR-006 #5048; SR-11/R-08)
10. **Metric columns are integer** — every `cycle_review_index` metric column is INTEGER (uniform with crt-047); `context_reload_pct` is basis points (0–10000), `round(pct × 100)`. No REAL/float column — the `is_finite()`/`push_bind(f64)` footgun (#4529/#4533) is designed out, **no float guard AC**. Width safety: checked/saturating u64/u32→i64 (never wrap), range-clamp basis points to 0–10000 before bind. (FR-16, AC-20, Constraint 10, ADR-005 #5047; SR-10/R-09)
11. **Informs, never controls** — no metric controls/bills/schedules/blocks execution; no orchestration/FinOps surface. (NFR-07, RQ-8)

## Dependencies

- **crt-054 (#752)** — PRODUCER (re-scoped to the contract, fully aligned). Provides `compaction_events` table + `activity_snapshot()` / `activity_snapshots_for_feature()`. **Cross-feature delivery dependency** — fold/compaction columns require its surfaces. crt-054 owns its own `CURRENT_SCHEMA_VERSION` bump for `compaction_events`.
- **#758 / #750 (MERGED `7aca6c44`)** — cross-session `context_reload` (`compute_context_reload_pct`), `SUMMARY_SCHEMA_VERSION = 4`, guarded-recompute / data-presence-gate / purged-retain logic crt-055 coexists with.
- **crt-047 (v24)** — integer-column + curation-health migration template to copy (`cycle_review_index.rs:84-104`, `migration.rs:926-1059`).
- **crt-052 Wave B** — the transcript hold (ON, unconditional); `purge_cycle_transcripts` is the read-before-purge anchor.
- **ass-077 / ass-078** — substrate decision, RQ-2 ranks, fold-at-ingest design.
- **Patterns**: #4178 (derived aggregates on cycle_review_index, single write, bump version), #4750 (four success returns), #4153 (three-path bump), #4484 (cascade-file existence). **Lessons**: #5022 (#750 empty-clobber three assertions), #4140 (declaration-chain silent no-op on evicted session), #4529/#4533 (`push_bind(f64)` non-finite silent wrong SQL), #4095 (migration version handshake).
- **Crates**: `unimatrix-store`, `unimatrix-observe`, `unimatrix-server`.

## NOT in Scope

- Producing the transcript fold or compaction-event persistence — crt-054 owns Surfaces A and B.
- The reload metric ingest — owned by #758; crt-055 promotes/consumes only.
- Any token estimate or token-named field (`token_bytes_per_unit`, "tokens (est.)"); a `reread`/`compaction` regex class.
- Persisting any transcript-derived aggregate requiring a content read on the persist path (R-A) — default NO; leak gate stays structural.
- Orchestration / FinOps — budget enforcement, cost dashboards, scheduling-by-cost, billing-grade attribution.
- #569 + #604 (`context_cycle` handler hardening — standalone bugs); #574 (cycle_events via MCP handler), #602 (attribution naming) — separate parked tracks.
- ass-077 ranks 6–8 response-only enrichment beyond #206-item-4 — never persisted; revisit on measured need.
- crt-054's `compaction_events` table migration / `CURRENT_SCHEMA_VERSION` bump.

## Alignment Status

**PASS 6 / WARN 0 / VARIANCE 0 / FAIL 0** (uni-vision-guardian, 2026-06-16) — **verdict unchanged by the three binding refinements this revision** (basis-points INTEGER, coarse-signal honesty, clock/unit contract); all three tighten honesty/integrity within the existing boundary and introduce no new variance. The feature advances self-learning (#4677); the informs-never-controls boundary, bytes-not-tokens unit, and structural leak gate are stated and structurally bound across SCOPE/ARCHITECTURE/SPECIFICATION/RISK-TEST. Two milestone-disciplined simplifications noted and accepted: #206-4 left non-durable (ADR-009, resolves Open Q3) and `compaction_reread` gates on earliest boundary only (ADR-006, resolves Open Q2; `high_water` reserved, not built). The basis-points-INTEGER decision additionally retires the #4529/#4533 float footgun (R-09 designed out) and the coarse/directional rendering (R-06 directional-honesty) and seconds-normalization (R-08, elevated to Critical) close the remaining believable-wrong-number surfaces. Producer-contract reconciliation (ARCHITECTURE §9) verified ALIGNED against crt-054 ADRs #5030/#5032; stale #5006 already deprecated through proper provenance (no `context_correct` needed).
