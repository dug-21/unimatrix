# CRT-055 — Specification

**Feature**: context_cycle_review redesign — durable per-cycle aggregates + dual reload metrics + transcript-fold surfacing
**Phase**: Cortical (crt) — learning & drift
**Tracking**: GH Issue #755 | **Goal**: self-learning (#4677)
**Status**: SPECIFICATION (design-session artifact)
**Inputs**: `product/features/crt-055/SCOPE.md` (authoritative §"Producer contract", §"Consumer persistence", §"Binding design constraints"), `product/features/crt-055/SCOPE-RISK-ASSESSMENT.md`
**Paired**: crt-054 (#752) — the PRODUCER. crt-055 is the CONSUMER of crt-054's `compaction_events` table and `activity_snapshot()`; it does not produce them.

---

## Objective

Redesign `context_cycle_review` so its per-cycle signals are durable, comparable across cycles, and fail-loud. crt-055 adds durable per-cycle aggregate columns to `cycle_review_index`, lands two distinct reload metrics (cross-session `context_reload` and compaction-gated `compaction_reread`), surfaces crt-054's content-free transcript fold (`activity_snapshot()`) and compaction events into columns, and replaces believable-zero rendering with an explicit "unavailable" presentation guard. It folds four standalone issues (#556, #320, #593, #206-item-4) as acceptance criteria within this single design session and single `cycle_review_index` migration. It informs the process; it never controls execution.

---

## Domain Models

### Ubiquitous Language

| Term | Definition |
|------|------------|
| **cycle / feature_cycle** | One feature delivery cycle, identified by a `feature_cycle` declaration. The unit of aggregation. Sessions attribute to a cycle via the session→`feature_cycle` declaration chain. |
| **cycle_review_index** | The review-time aggregate store: one durable row per cycle holding integer/text aggregates computed at review. crt-055 owns this table's schema entirely for v5. (Distinct from `cycle_events`, the in-flight structural audit trail — derived aggregates never go on `cycle_events`, pattern #4178.) |
| **store_cycle_review()** | The single writer of `cycle_review_index` rows in the full-pipeline block. The only place crt-055's new columns are written. |
| **RetrospectiveReport / CycleReviewRecord** | The response-time report and persisted record. Structurally content-free — they carry integers/aggregates, never transcript bytes. |
| **raw_signals_available** | Existing per-cycle flag indicating whether a metric's source data class is present (non-empty). Extended per-metric where source classes differ. Drives the fail-loud presentation guard. |
| **SUMMARY_SCHEMA_VERSION** | Version of the memoized review record schema (`cycle_review_index.rs:49`). #758 owns 4; crt-055 owns 5. crt-054 owns neither. |
| **compaction_events** | crt-054-owned durable table; insert-only; one row per compaction event (`session_id`, `compacted_at` seconds, `high_water` reserved). crt-055 reads it; does not write it. |
| **ActivitySnapshot** | crt-054-produced `Copy` metadata-only counter struct (`bytes_total: u64`, `delta_count: u32`, `class_counts: [u32; MAX_SIGNAL_CLASSES]`) returned by `activity_snapshot()`. crt-055 reads it at review; does not produce it. |
| **context_reload** | Cross-session file-overlap reload percentage (continuity/handoff cost). Live from #758 (`compute_context_reload_pct`, `session_metrics.rs:47`). crt-055 promotes it to a durable INTEGER column expressed in **basis points** (0–10000): percentage × 100 rounded to nearest integer (37.5% → 3750). Every `cycle_review_index` metric column is integer — uniform with crt-047; no REAL/float column. |
| **compaction_reread** | Post-compaction within-cycle re-read count (the compaction tax). crt-055 reckoning: PostToolUse file-overlap reads whose `ts >` a session's `compacted_at`. |
| **fail-loud / believable-zero** | When a metric's source class is empty/purged, render **"unavailable"** — never `0`, which is indistinguishable from a measured zero (the #750 class). |
| **read-before-purge** | crt-055 must read `activity_snapshot()` before the crt-052 Wave-B hold purge zeroes the transcript buffer. |
| **leak gate (structural)** | `RetrospectiveReport`/`CycleReviewRecord` carry no content field; only integers/aggregates persist. Enforced by `test_candidates_structurally_absent_from_memoized_report`. |
| **declared-but-never-closed phase** | A phase with a declared start in `cycle_events` and no corresponding close — surfaced as a hotspot (#556). |
| **knowledge-reuse all-served** | Count of ALL entries served to the cycle: union of `query_log` ∪ `injection_log`, not same-cycle-tagged only (#320). |
| **knowledge-that-helped** | The entries that contributed to this cycle, surfaced in the retrospective (#206 item 4). |

### Key Entities & Relationships

- A **cycle** has one **cycle_review_index** row (1:1, keyed by cycle identity).
- A **cycle** is composed of 0..N **sessions**; sessions attribute to the cycle via the `feature_cycle` declaration chain at review.
- A **session** has 0..N **compaction_events** rows (multiplicity 0..N; a session may compact multiple times).
- A **session** has one in-memory **ActivitySnapshot** (the running fold), read once at review before purge; the cycle aggregate is the sum across the cycle's held sessions.
- A **cycle** has phase lifecycle events in **cycle_events** (start/transition/close), from which rank-1 aggregates and declared-but-never-closed hotspots derive.

### Boundary (crt-055 consumes; crt-054 produces)

crt-055 reads `compaction_events` rows and `activity_snapshot()` results. It does NOT write `compaction_events`, implement the transcript fold, bump `CURRENT_SCHEMA_VERSION` for the producer table, or define the signal-class catalog beyond consuming its configured order. Per ADR-008-corrected (Unimatrix #5032): the two features migrate disjoint tables — no ALTER collision.

---

## Functional Requirements

Each requirement is testable. Source columns and types are authoritative per SCOPE §"Consumer persistence" and §"Producer contract".

### Fail-loud presentation guard (sequence first)

- **FR-01** — When a metric's source data class is empty for a cycle, the rendered report MUST present **"unavailable"** for that metric, never `0`. (SCOPE In-scope 1; Constraint 4a)
- **FR-02** — The guard MUST drive off the `raw_signals_available` flag, extended per-metric where source classes differ, so each metric reports its own availability rather than a single cycle-wide flag.

### Durable per-cycle aggregate columns

- **FR-03** — crt-055 MUST add all new aggregate columns to `cycle_review_index` in a single migration, following the crt-047 integer-column template, each `NOT NULL DEFAULT 0` (TEXT columns `NOT NULL DEFAULT '{}'`), `pragma_table_info` existence-guarded. (SCOPE §"The single migration")
- **FR-04** — crt-055 MUST bump `SUMMARY_SCHEMA_VERSION` from 4 to 5 (`cycle_review_index.rs:49`). crt-054 MUST NOT bump it.
- **FR-05 (Rank 1 — phase aggregates)** — From `cycle_events`, persist phase durations, transitions, and rework-loop aggregates. Exact column shapes set by the design session against ass-077 RQ-2. (SCOPE In-scope 2)
- **FR-06 (#556, feeds rank 1)** — Phases declared-but-never-closed MUST be surfaced as a hotspot and feed the rank-1 phase aggregate.
- **FR-07 (Rank 2 — rework ratio)** — Persist the rework/failure session ratio derived from `SessionRecord.outcome`.
- **FR-08 (Rank 3 / #320 — knowledge-reuse all-served)** — The knowledge-reuse aggregate MUST count ALL entries served to the cycle: the union of `query_log` ∪ `injection_log`, NOT only same-cycle-tagged entries.

### Transcript fold surfacing (consume crt-054 Surface B)

- **FR-09** — crt-055 MUST read `activity_snapshot()` for each of the cycle's held sessions during the review pipeline and land the fold into columns: `transcript_bytes_total` (sum of `ActivitySnapshot.bytes_total`), `transcript_delta_count`, `transcript_error_count` (`class_counts[0]`), `transcript_refusal_count` (`class_counts[1]`), and `signal_class_counts_json` (full `class_name`→count map, forward-compatible for classes beyond error/refusal).
- **FR-10** — crt-055 MUST read `activity_snapshot()` BEFORE the crt-052 Wave-B hold purge zeroes the buffer (read-before-purge ordering). (Constraint 6; SR-09)
- **FR-11** — crt-055 MUST NOT add any token estimate or token-named field. The throughput unit is bytes. (Out of scope; SR-05)
- **FR-11b (coarse-signal honesty)** — The behavioral-signal columns (`transcript_error_count`, `transcript_refusal_count`, `signal_class_counts_json`) are unvalidated, content-opaque regex matches — directional, not auditable post-hoc. The rendered report MUST present them with a coarse/directional qualifier, visually distinct from exactly-counted aggregates (phase counts, session ratios, compaction count). No behavioral-signal value may be rendered as if it were an exact, auditable count. (Presentation-honesty sibling to FR-01's "unavailable"-not-"0" guard; NFR-08)
- **FR-12** — Producer integer widths (`u64`/`u32`/`[u32; N]`) MUST land into i64 columns via checked/saturating conversion at the persist boundary (no truncation/overflow). (SR-10)

### Compaction metrics (consume crt-054 Surface A)

- **FR-13** — crt-055 MUST persist `compaction_count` = COUNT of `compaction_events` rows attributed to the cycle, attributed via the session→`feature_cycle` declaration chain at review.
- **FR-14** — crt-055 MUST compute and persist `compaction_reread_count` = PostToolUse file-overlap reads whose `ts >` a session's `compacted_at`. (SR-10)
- **FR-14b (clock/unit binding contract)** — BINDING: the `compaction_reread` gate compares a PostToolUse read `ts` against `compaction_events.compacted_at`, which is in **Unix SECONDS**. Every timestamp entering the gate MUST be normalized to seconds FIRST (millisecond timestamps divided to seconds where the source is millis) so the comparison is unit-consistent. A unit mismatch (millis compared against seconds) is a defect, not an acceptable approximation. (Promoted from prior open question to binding requirement; SR-10)
- **FR-15** — When a session has multiple `compaction_events` rows, crt-055 MUST apply a single defined boundary-selection rule (earliest / latest / per-boundary) for the `compaction_reread` gate, documented as a fixed reckoning detail. (Open Q2; SR-11)

### Dual reload metrics (two columns, two gates, one engine)

- **FR-16** — `context_reload_pct` MUST be a durable INTEGER column (basis points, range 0–10000) promoted from #758's `compute_context_reload_pct` (cross-session overlap window). The percentage value is encoded as basis points: `compute_context_reload_pct`'s percentage × 100, rounded to the nearest integer (e.g. 37.5% → 3750). The column is `NOT NULL DEFAULT 0` like every other metric column — uniform with crt-047, no REAL/float column on `cycle_review_index`. (Resolves Open Q4; SR-10)
- **FR-17** — `compaction_reread_count` MUST remain a distinct column with a distinct gate (post-compaction within-cycle overlap window). The two reload metrics MUST share one overlap engine but MUST NOT be collapsed into a single number or window. (Constraint 5; SR-06)

### auto_close parameter (#593)

- **FR-18** — `context_cycle_review` MUST accept an `auto_close: bool` parameter, default `false`.
- **FR-19** — When `auto_close = true` AND no `cycle_stop` event exists for the cycle, crt-055 MUST write the `cycle_stop` event synchronously BEFORE running the review pipeline. When `auto_close = false` or a `cycle_stop` already exists, no stop event is written.

### Knowledge-that-helped (#206 item 4)

- **FR-20** — The retrospective MUST surface the entries that contributed to (helped) this cycle. Durability (response-time enrichment vs. a persisted column) is a design-session decision (Open Q3); the surfacing itself is required.

### Single-writer / no-clobber discipline

- **FR-21** — The new columns MUST be written ONLY via the single `store_cycle_review()` in the full-pipeline block. No second writer may be added. (Constraint 2; SR-01)
- **FR-22** — The memo-hit return, the purged-retain return, and the force+purged stored-record return MUST NOT write the new columns (no zero-clobber of a purged cycle's columns). (Constraint 2)
- **FR-23** — crt-055 MUST coexist with #758's guarded-recompute + data-presence gate: the stale→recompute fallthrough auto-refreshes pre-v5 rows when source data is present; the purged-retain path stays no-write and returns byte-identical. Recompute MUST be via clear-memo-and-fall-through, never a second writer near the memo/`check_stored_review` site. (Constraint 3; SR-01, SR-02)

---

## Non-Functional Requirements

- **NFR-01 (Structural leak gate)** — `RetrospectiveReport` and `CycleReviewRecord` carry no content field; only integers/aggregates persist. No transcript bytes on the persist path. Measurable: `test_candidates_structurally_absent_from_memoized_report` holds. (Constraint 1; SR-05)
- **NFR-02 (Content opacity)** — All consumed producer surfaces are metadata-only; crt-055 introduces no content read on the persist path (R-A default NO). No `Display`/content serialization of transcript data. (Out of scope R-A)
- **NFR-03 (Migration hygiene — three-path bump, #4153)** — The `cycle_review_index` ALTER, the fresh-create path (`db.rs`), and the migration-version test assertion MUST move together; ALTERs are `pragma_table_info`-guarded (crt-047 v23→v24 template). Pre-v5 rows return defaults until guarded recompute refreshes them. (SR-03)
- **NFR-04 (Migration independence)** — crt-055's `cycle_review_index` column migration and crt-054's `compaction_events` table migration are independent ALTERs on disjoint tables; they take distinct sequential DB version numbers, merge order free. No shared-table collision. (SCOPE §migration; ADR-008-corrected #5032)
- **NFR-05 (Single byte scan, performance)** — Consuming the fold imposes no additional transcript scan; crt-055 reads the already-folded `ActivitySnapshot` counters (the producer's single shared RegexSet pass is crt-054's concern). Review-time cost is bounded by existing aggregation queries plus the per-session snapshot read.
- **NFR-06 (Forward compatibility)** — `signal_class_counts_json` carries the full `class_name`→count map so classes added beyond `error`/`refusal` require no new column or migration.
- **NFR-07 (Vision boundary — informs, never controls)** — No metric controls execution; no orchestration/FinOps surface; no token/cost field. Disqualifying test: "does this metric control execution?" — if yes, out of lane. (SCOPE Goal framing; RQ-8)
- **NFR-08 (Fail-loud honesty)** — No metric may imply completeness it does not have. Undeclared-session folds die fail-loud (surfaced as unavailable), never as a fabricated zero. (SCOPE coverage semantics; SR-08, SR-10)

---

## Acceptance Criteria

Each AC carries an AC-ID, traces to requirements, and a verification method. ACs marked **(folded)** absorb a standalone issue and ship as crt-055 ACs, not separately.

| AC-ID | Criterion | Verifies | Verification Method |
|-------|-----------|----------|---------------------|
| **AC-01** | When a metric's source data class is empty for a cycle, the report renders "unavailable", never "0". | FR-01, FR-02 | Unit/integration test: synthesize a cycle with an empty source class; assert rendered metric == "unavailable" and never the literal "0". |
| **AC-02** | All new aggregate columns exist on `cycle_review_index` after migration, each `NOT NULL DEFAULT 0` (TEXT `'{}'`), `pragma_table_info`-guarded; fresh-create and upgrade paths agree. | FR-03, NFR-03 | Migration test: `pragma_table_info` lists every new column with correct type/default on both a fresh DB and an upgraded DB; idempotent re-run. |
| **AC-03** | `SUMMARY_SCHEMA_VERSION` == 5; the pinned version assertion is updated in the same change; crt-054 does not bump it. | FR-04, NFR-03 | Test asserts the constant == 5 and the migration-version test reflects it. |
| **AC-04** (folded #556) | A cycle with a phase that has a declared start and no close surfaces that phase as a declared-but-never-closed hotspot feeding the rank-1 phase aggregate. | FR-05, FR-06 | Integration test: seed `cycle_events` with an unclosed phase; assert the phase appears as a hotspot in the rank-1 aggregate. |
| **AC-05** | Rank-1 phase durations/transitions/rework and rank-2 rework ratio persist to columns and recompute correctly from `cycle_events` / `SessionRecord.outcome`. | FR-05, FR-07 | Integration test with seeded phase events and session outcomes; assert persisted column values match expected aggregates. |
| **AC-06** (folded #320) | Knowledge-reuse counts the union of `query_log` ∪ `injection_log` (all entries served), not only same-cycle-tagged entries. | FR-08 | Test: seed served entries split across query_log and injection_log including cross-cycle-tagged; assert reuse count == size of the union. |
| **AC-07** | `activity_snapshot()` is read for the cycle's held sessions and landed into `transcript_bytes_total`, `transcript_delta_count`, `transcript_error_count` (class 0), `transcript_refusal_count` (class 1), and `signal_class_counts_json`. | FR-09 | Integration test with a known fold; assert each column equals the summed snapshot field and the JSON map matches the class catalog. |
| **AC-08** | `activity_snapshot()` is read BEFORE the crt-052 hold purge; reversing the order zeroes the columns. | FR-10, NFR-08 | Ordering test: assert the snapshot read site precedes `purge_cycle_transcripts`; a test inverting the order fails (columns zeroed). |
| **AC-09 (silent-zero regression guard)** | Aggregation reads a non-empty source for a representative TS-client cycle; the fold columns are non-zero for that cycle (the #750 silent-zero class cannot recur for the held route). | FR-09, NFR-08 | Regression test: representative TS-client cycle with held activity; assert the fold source is non-empty and columns are non-zero. |
| **AC-10** | No token estimate or token-named field exists on `CycleReviewRecord`/`RetrospectiveReport`; no `reread`/`compaction` regex class is introduced by crt-055. | FR-11, NFR-07 | Structural/guard test asserting absence of any token-named field; grep/AST guard for token-named columns. |
| **AC-11** | `compaction_count` equals the count of `compaction_events` rows attributed to the cycle via the session→`feature_cycle` chain; undeclared sessions' rows are not mis-attributed. | FR-13, NFR-08 | Integration test: seed `compaction_events` for declared and undeclared sessions; assert only declared-session rows count toward the cycle. |
| **AC-12** | `compaction_reread_count` equals PostToolUse file-overlap reads with `ts >` a session's `compacted_at` (seconds-aligned); the boundary-selection rule for multi-compaction sessions is applied consistently. | FR-14, FR-15 | Integration test: seed compaction_events + PostToolUse reads straddling `compacted_at`; assert only post-boundary overlapping reads count; assert multi-compaction selection rule. |
| **AC-22 (clock/unit consistency — integration test)** | All timestamps entering the `compaction_reread` gate are normalized to Unix seconds (PostToolUse read `ts` in epoch millis floored ÷1000 to seconds) before comparison; a read occurring +500ms AFTER a compaction is correctly counted, a read −500ms BEFORE the compaction is NOT counted, and a unit mismatch (millis vs seconds) would be caught. The sub-second ±500ms boundary exercises the ÷1000 floor — a looser ±1s boundary would pass even if floor normalization were wrong or absent. | FR-14b | INTEGRATION test: seed a `compaction_events.compacted_at` (seconds) and PostToolUse overlapping reads at the sub-second boundary: a read whose `ts` is +500ms after the compaction MUST count; a read whose `ts` is −500ms before MUST NOT count. Include a guard case where a millis-valued timestamp entering unnormalized would mis-compare (off by ~1000×) — assert normalization-to-seconds (floor ÷1000) prevents it, so the gate comparison is unit-consistent. |
| **AC-13 (dual reload not collapsed)** | `context_reload_pct` (cross-session) and `compaction_reread_count` (post-compaction within-cycle) are two distinct columns with distinct gates; neither is derived from the other's window. | FR-16, FR-17 | Test asserting both columns persist independently with distinct overlap windows; a single shared engine but two outputs. |
| **AC-20 (basis-points encoding)** | `context_reload_pct` is a `NOT NULL DEFAULT 0` INTEGER column storing basis points (0–10000); the persisted value equals `compute_context_reload_pct`'s percentage × 100 rounded to nearest integer; no REAL/float column is added to `cycle_review_index`. | FR-16 | Unit/integration test: a known overlap producing 37.5% persists 3750; rounding cases (e.g. 0.005%→1, 99.995%→10000) round to nearest; `pragma_table_info` shows the column type is INTEGER, not REAL. |
| **AC-21 (coarse-signal presentation honesty)** | The behavioral signals (`transcript_error_count`, `transcript_refusal_count`, `signal_class_counts_json`) render with a coarse/directional qualifier, visually distinct from exactly-counted aggregates (phase counts, session ratios, compaction count); no behavioral-signal value renders as an exact auditable count. | FR-11b, NFR-08 | Rendering test: assert the behavioral-signal fields carry the directional/coarse qualifier in the rendered report and that an exactly-counted aggregate (e.g. compaction_count, rework ratio) does NOT carry it — the two presentations are distinguishable. |
| **AC-14** | Producer widths convert to i64 columns without truncation/overflow (checked/saturating). | FR-12 | Test with near-`u64::MAX` / large `u32` fold values; assert persisted i64 value is correct or saturated, never wrapped. |
| **AC-15** (folded #593) | `context_cycle_review` accepts `auto_close: bool` default `false`; when `true` and no `cycle_stop` exists, a `cycle_stop` is written synchronously before the review pipeline; otherwise no stop is written. | FR-18, FR-19 | Handler test: (a) `auto_close=true`, no prior stop → stop written before pipeline; (b) `auto_close=true`, stop exists → no new stop; (c) `auto_close=false` → no stop. |
| **AC-16** (folded #206-4) | The retrospective surfaces the entries that helped this cycle (knowledge-that-helped). | FR-20 | Test asserting the retrospective output includes the cycle's contributing entries. |
| **AC-17 (single writer / no-clobber)** | New columns are written only by the one `store_cycle_review()`; the memo-hit, purged-retain, and force+purged returns do NOT write the new columns. | FR-21, FR-22 | The three #5022 assertions: (a) data-present recompute writes columns; (b) purged-retain returns byte-identical, no write; (c) force+purged does not clobber columns with zeros. |
| **AC-18 (guarded recompute coexistence)** | A pre-v5 stale row with source data present auto-refreshes via clear-memo-and-fall-through (no second writer); a purged stale row retains its stored columns. | FR-23 | Test: stale+source-present → recompute populates new columns; stale+purged → retain (no recompute, no force advisory). |
| **AC-19 (structural leak gate)** | `RetrospectiveReport`/`CycleReviewRecord` carry no content field; only integers/aggregates persist. | NFR-01, NFR-02 | `test_candidates_structurally_absent_from_memoized_report` holds; structural test asserts no content field. |

---

## User Workflows

### W1 — Reviewing a cycle (agent or human)
1. Caller invokes `context_cycle_review` for a cycle (optionally `auto_close=true`).
2. If `auto_close=true` and no `cycle_stop` exists → a `cycle_stop` is written synchronously (FR-19), then the pipeline runs.
3. Pipeline reads `activity_snapshot()` for held sessions BEFORE purge (FR-10), reads `compaction_events` rows, computes rank 1–3 aggregates, both reload metrics, and the fold columns.
4. `store_cycle_review()` persists all columns once (FR-21).
5. The rendered `RetrospectiveReport` shows aggregates, the dual reload metrics, declared-but-never-closed hotspots, knowledge-that-helped, and "unavailable" for any empty-source metric (FR-01).

### W2 — Cross-cycle comparison
1. Durable columns on `cycle_review_index` let agents/humans compare phase strain, rework ratio, reload, compaction tax, and throughput (bytes) across cycles to improve the process — informs, never controls (NFR-07).

### W3 — Re-review of a pre-v5 / stale cycle
1. A stale memoized row (pre-v5) with source data present auto-refreshes on review via guarded recompute (FR-23); a purged cycle retains its byte-identical stored record (no zero-clobber).

---

## Constraints

1. **Single writer, no zero-clobber** — new columns written only via the one full-pipeline `store_cycle_review()`; memo-hit / purged-retain / force+purged returns do not write them. (Constraint 2)
2. **Coexist with #758 guarded recompute + data-presence gate** — recompute via clear-memo-fall-through; purged-retain stays no-write. (Constraint 3)
3. **Dual reload never collapsed** — two columns, two gates, one engine; overlap windows pinned before building. (Constraint 5)
4. **Read-before-purge** — read `activity_snapshot()` before the crt-052 hold purge. (Constraint 6)
5. **Structural leak gate** — no content field on the report/record. (Constraint 1)
6. **Producer contract is binding** — consume `compaction_events` + `activity_snapshot()` exactly per SCOPE §"Producer contract"; any drift reconciled there first. (Constraint 7)
7. **Single migration / one version bump** — one `cycle_review_index` migration adding all columns, one `SUMMARY_SCHEMA_VERSION` 4→5 bump; disjoint from crt-054's `compaction_events` table migration.
8. **Bytes, not tokens** — throughput unit is bytes; no token-named field.
9. **Clock/unit contract (binding)** — the `compaction_reread` gate operates in Unix seconds; all timestamps are normalized to seconds before comparison (millis→seconds where needed). Unit-consistency is verified by integration test (AC-22). (FR-14b)
10. **Metric columns are integer** — every `cycle_review_index` metric column is INTEGER (uniform with crt-047); `context_reload_pct` is basis points (0–10000). No REAL/float column. (FR-16)

---

## Dependencies

- **crt-054 (#752)** — PRODUCER (re-scoped to SCOPE §"Producer contract"). Provides `compaction_events` table (Surface A) and `activity_snapshot()` / `ActivitySnapshot` (Surface B). crt-055 consumes both; crt-054 owns its own `CURRENT_SCHEMA_VERSION` bump for the new table (ADR-008-corrected, #5032).
- **#758 / #750 (MERGED, `7aca6c44`)** — cross-session `context_reload` (`compute_context_reload_pct`, `session_metrics.rs:47`), `SUMMARY_SCHEMA_VERSION = 4`, guarded-recompute / data-presence-gate / purged-retain logic crt-055 coexists with.
- **crt-047** — integer-columns + curation-health template (`cycle_review_index.rs:84-104`, `migration.rs:926-1043`); the column + migration pattern (#4178) to copy.
- **crt-052 Wave B** — the transcript hold (verified ON, unconditional); `activity_snapshot()` durability-to-review rests on it.
- **ass-077 / ass-078** — substrate decision, RQ-2 ranks, fold-at-ingest design.
- **Patterns**: #4178 (derived aggregates → cycle_review_index, single write, bump version), #4750 (four success returns), #4153 (three-path bump), #4484 (cascade-file existence).
- **Crates**: `unimatrix-store` (cycle_review_index, migration, store_cycle_review), `unimatrix-server` (the `context_cycle_review` handler, `auto_close` param).
- **Stale knowledge to correct at design**: ADR-008 (#5006) — architect `context_correct`s it (superseded by #5032).

---

## NOT in Scope (explicit exclusions)

- **Producing the transcript fold or compaction-event persistence** — crt-054 owns Surfaces A and B. crt-055 only consumes them.
- **The reload metric ingest** — owned by #758; crt-055 promotes/consumes only.
- **Any token estimate or token-named field** (`token_bytes_per_unit`, "tokens (est.)"). Bytes is the unit.
- **A `reread`/`compaction` regex class** — not introduced by crt-055.
- **Persisting any transcript-derived aggregate requiring a content read on the persist path** (R-A) — default NO; leak gate stays structural.
- **Orchestration / FinOps** — budget enforcement, cost dashboards, scheduling-by-cost, billing-grade attribution.
- **#569 + #604** — `context_cycle` handler hardening, relabeled standalone bugs.
- **#574** (cycle_events via MCP handler), **#602** (attribution naming) — separate parked tracks.
- **ass-077 ranks 6–8 response-only enrichment** beyond #206-item-4 (tool-mix per phase, attribution completeness, decision/lesson density, PreCompact-tail reload) — never persisted; revisit on measured need.
- **crt-054's `compaction_events` table migration / `CURRENT_SCHEMA_VERSION` bump** — crt-054's, not crt-055's.

---

## Open Questions (for architect / design session)

1. **Rank 1–3 column shapes** — exact columns for phase durations/transitions/rework, rework ratio, and knowledge-reuse-all-served, set against ass-077 RQ-2. (Pure persistence shape; no producer impact.) — *architect*
2. **`compaction_reread` boundary selection** for multi-compaction sessions — earliest, latest, or per-boundary segments (FR-15)? — *architect (crt-055 reckoning detail)*
3. **Knowledge-that-helped (#206-4) durability** — response-time-only enrichment or a durable column (FR-20)? — *design-session call*
4. **Default domain-neutral signature catalog** — which `error`/`refusal` patterns generalize (product judgment; keep tiny + high-precision, under-catalog and let domains extend). Note: catalog definition is crt-054's producer concern; crt-055 only consumes the configured index order — confirm consumption assumes `0=error`, `1=refusal`. — *human/product*
5. **Internal wave order** — recommended: fail-loud guard → durable aggregates + restructure → reload pair + fold surfacing. One migration regardless. — *design session*

### Resolved (binding decisions — product owner, 2026-06-16)
- **`context_reload_pct` storage type (prior Open Q4) — RESOLVED: basis-points INTEGER (0–10000), not REAL.** Every `cycle_review_index` metric column is integer (uniform with crt-047). Encoding: `compute_context_reload_pct` percentage × 100, rounded to nearest integer → basis points (37.5% → 3750). The integer storage removes the non-finite-float footgun for this column, so no `is_finite()`/non-finite-float guard AC applies to `context_reload_pct`. See FR-16, AC-20, Constraint 10.
- **Clock/unit for the `compaction_reread` gate — RESOLVED & PROMOTED to binding requirement (no longer open).** All timestamps entering the gate are normalized to Unix seconds before comparison (millis→seconds where needed); unit-consistency is verified by an integration test. See FR-14b, AC-22, Constraint 9.
- **Behavioral-signal presentation — RESOLVED: coarse/directional.** `transcript_error_count` / `transcript_refusal_count` / `signal_class_counts_json` are unvalidated, content-opaque regex matches and MUST render with a coarse/directional qualifier, distinct from exactly-counted aggregates. See FR-11b, AC-21.

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced crt-054 ADR-007/008/006 (#5035/#5032/#5031) confirming producer-only boundary, and pattern #4178 (derived aggregates belong on cycle_review_index via single store_cycle_review() write, bump SUMMARY_SCHEMA_VERSION). Applied both to anchor the consumer boundary and single-writer constraint. Read-only tier — no storage (spec decisions are feature-specific; retro may promote any that generalize).
