# CRT-055 — context_cycle_review redesign: durable per-cycle aggregates + dual reload metrics + transcript-fold surfacing

**Date**: 2026-06-16 — **consolidated scope; owns the cycle_review schema + the producer contract crt-054 implements against**
**Status**: SCOPE (design-session input — pre-design)
**Phase**: Cortical (crt) — learning & drift
**Tracking**: GH Issue #755
**Goal**: self-learning (#4677)
**Design input**: `product/research/ass-077/FINDINGS.md` (substrate decision + RQ-2 ranks), `product/research/ass-078/FINDINGS.md` (fold-at-ingest design)
**Origin session**: uni-zero (consolidation + roadmap)
**Paired feature**: crt-054 (#752) — the PRODUCER. crt-055 defines the contract (this document, §"Producer contract"); crt-054 implements to it. **crt-054 is re-scoped against this contract before it executes.**

> Pre-design scope handed to a design session. It captures the validated problem, the v1 boundary, the binding constraints, and — centrally — the **complete field-level contract for everything the producer (crt-054) writes**, so crt-054 can be executed against fixed definitions. It is NOT an implementation brief; the design session produces ARCHITECTURE / SPECIFICATION / IMPLEMENTATION-BRIEF and the ADRs.

---

## Why this feature exists (consolidation)

`context_cycle_review` is the consuming surface for a cluster of fixes/features, but no feature tracked the review redesign itself. crt-055 is that redesign. It operationalizes the **ass-077 substrate decision** (durable per-cycle aggregates on `cycle_review_index`; the transcript is response-time enrichment, never the persisted substrate) and lands the **ass-078 fold outputs** produced by crt-054.

It absorbs the following standalone issues as acceptance criteria (they are folded here, not shipped separately):

| Issue | Folded as |
|-------|-----------|
| #556 | declared-but-never-closed phases surfaced as a hotspot (feeds rank-1 phase aggregate) |
| #320 | knowledge-reuse counts all entries served, not just same-cycle |
| #593 | `auto_close` parameter — write `cycle_stop` atomically on final retrospective |
| #206 (item 4 only) | "knowledge that helped this cycle" surfaced in the retrospective |

Out of this consolidation: #569 + #604 (pure `context_cycle` handler hardening, now relabeled **bug** — no schema/report coupling, land as standalone fixes), #574 (cycle_events via MCP handler), and #602 (attribution-identifier naming) are **explicitly NOT in crt-055** — separate tracks.

## Problem

The review report's per-cycle signals are not durable, comparable, or fail-loud:
1. **No durable per-cycle aggregates for cross-cycle comparison.** Phase durations/transitions/rework, rework ratio, knowledge-reuse all live in `summary_json` or are recomputed; there is no column surface for cross-cycle baselines beyond crt-047's curation-health columns.
2. **Reload is single-signal and partly unsurfaced.** #758 restored cross-session `context_reload`, but the "reload *after compaction*" signal (the compaction tax — ass-077's most faithful reload) has no gate: there is no durable compaction timestamp to gate against.
3. **No throughput / behavioral-signature surface.** Total conversation volume and model error/refusal events are transcript-only; nothing durable carries them.
4. **Believable-zeros still render as "0".** When a metric's source is empty/purged the report shows `0`, indistinguishable from a measured zero (the #750 class).

crt-054 produces the raw inputs for (2) and (3); crt-055 owns persisting and surfacing all of (1)–(4).

## Goal / value framing

Self-learning bar (ass-077): produce trustworthy **information about the process** that agents and humans use to improve it — where a cycle ran heavy, where it strained against a compaction boundary, where it thrashed. **Informs, never controls.** Disqualifying test for any element: *does this metric control execution?* If yes, out of lane (RQ-8 vision boundary, hard scope edge). No orchestration/FinOps surface; no token/cost field (see Out of scope).

---

## In scope — the v1 boundary

crt-055 = **(durable per-cycle aggregate columns)** + **(dual reload metrics: cross-session + compaction-gated)** + **(surface crt-054's transcript fold)** + **(fail-loud presentation guard)** + **(the folded point-issues)**. One design session, one `cycle_review_index` migration adding all columns at once, one `SUMMARY_SCHEMA_VERSION` bump.

1. **Fail-loud presentation guard.** Render **"unavailable"**, never `0`, when a metric's source class is empty for the cycle. Drives off the `raw_signals_available` flag (existing) extended per-metric where sources differ. ass-077:119. *This is the lowest-risk increment; the design session should sequence it first within the feature.*
2. **Durable per-cycle aggregate columns** on `cycle_review_index` (crt-047 integer-column template), single `store_cycle_review()` writer, ass-077 RQ-2 ranks 1–3:
   - **Rank 1 — phase durations + transitions + rework loops** from `cycle_events`. Includes #556 (phases declared-but-never-closed surfaced as a hotspot).
   - **Rank 2 — rework/failure session ratio** from `SessionRecord.outcome`.
   - **Rank 3 — knowledge-reuse all-served** (#320): union of query_log + injection_log, not same-cycle-tagged only.
   - (Rank 4 curation health already shipped crt-047 — the column template to copy.)
3. **Dual reload metrics — two distinct columns, one overlap engine, two gates. Never collapsed into one number.**
   - `context_reload` — **cross-session** file overlap (continuity/handoff cost). Already live from #758 (`compute_context_reload_pct`, `session_metrics.rs:47`). crt-055 promotes it to a durable column for cross-cycle comparison.
   - `compaction_reread` — **post-compaction within-cycle** re-read (the compaction tax). Computed at review by crt-055 as the PostToolUse file-overlap reads whose `ts >` a `compaction_events.compacted_at` for the same session. Gated entirely on crt-054's `compaction_events` rows (§Producer contract).
4. **Surface crt-054's transcript fold** as durable columns, sourced at review from `activity_snapshot()` (§Producer contract): `bytes_total`, `delta_count`, `error`/`refusal` class-counts, and a forward-compatible `signal_class_counts_json`. **No token field** (Out of scope).
5. **Knowledge-that-helped** (#206 item 4): surface the entries that contributed to this cycle in the retrospective (response-time enrichment — not necessarily a persisted column; design-session call on whether it needs durability).
6. **`auto_close` parameter** (#593): `auto_close: bool` (default `false`); when `true` and no `cycle_stop` exists, write the stop event synchronously before the review pipeline.

## The single migration (crt-055 owns `cycle_review_index` entirely)

- `SUMMARY_SCHEMA_VERSION` **4 → 5** (`cycle_review_index.rs:49`). #758 owns 4; crt-055 owns 5. **crt-054 does NOT bump `SUMMARY_SCHEMA_VERSION`** (it never touches the report or this table).
- DB schema: crt-055 adds the cycle_review_index columns. **crt-054's `compaction_events` table is a separate migration owned by crt-054** (§Producer contract) — the two features never ALTER the same table, so there is no migration collision. Sequence the DB version numbers at design time (crt-054's table migration and crt-055's column migration are independent ALTERs on different tables; order between them is free).
- All new columns `NOT NULL DEFAULT 0` (TEXT default `'{}'`), crt-047 v23→v24 template, `pragma_table_info` existence-guarded. Pre-v5 rows return defaults until the guarded recompute (below) refreshes them.
- Coexist with #758's guarded-recompute / data-presence-gate / purged-retain (see Constraints): write the new columns ONLY via the single `store_cycle_review()` in the full-pipeline block; the purged-retain and force+purged paths stay no-write so a purged cycle's columns are never clobbered with zeros.

---

## Producer contract — the complete definition of every field crt-054 writes

> This section is the binding interface. crt-054 (#752) is re-scoped to implement exactly these definitions and nothing more. crt-055 consumes them. Anything crt-054 needs beyond this is a contract change negotiated here first.

crt-054 produces **two surfaces** and nothing else. It does **not** touch `cycle_review_index`, `store_cycle_review`, the `RetrospectiveReport`, or `SUMMARY_SCHEMA_VERSION`.

### Surface A — `compaction_events` table (durable; crt-054 owns the table + the writes)

A new table. One row per compaction event. Written at the authoritative server seam `handle_compact_payload` (`uds/listener.rs:1737`), co-located with the existing `increment_compaction` call (`infra/session.rs:554-559`). Never updated; insert-only. Content-free (metadata only — ADR-002 content-opacity).

| Column | Type | Null/Default | Semantics |
|--------|------|--------------|-----------|
| `id` | INTEGER | PK (rowid) | surrogate key |
| `session_id` | TEXT | NOT NULL | the session that compacted. The join key to a cycle is resolved at review time via the existing session→`feature_cycle` declaration chain — **`feature_cycle` is deliberately NOT stored here** (the row is written regardless of declaration; attribution happens at review). |
| `compacted_at` | INTEGER | NOT NULL | Unix timestamp **seconds** of the compaction event. This is the gate boundary `compaction_reread` compares PostToolUse read `ts` against. |
| `high_water` | INTEGER | NOT NULL DEFAULT 0 | `TranscriptBuffer.high_water` (bytes *sent*, monotonic — `session_transcript.rs:52`, invariant I3) at the moment of compaction. **Reserved**: populated in v1 for a future precise byte-boundary gate; crt-055 v1 gates on `compacted_at` only. Populating it now avoids a second migration later. |

Indexing: index on `session_id` (review-time lookup is by session). Multiplicity: **0..N rows per session** — a session may compact multiple times; `compaction_reread` gates on the relevant (e.g. earliest, or per-boundary) `compacted_at` — the exact selection is a crt-055 reckoning detail, not a producer concern.

Write discipline: exactly one INSERT per compaction event at the seam. No content. No `tracing` of payload. Lock ordering against the registry lock is a crt-054 design-session call; the contract only fixes the columns and the write site.

DB migration: crt-054 adds this table (its own DB schema-version bump on a NEW table — no `cycle_review_index` change, no `SUMMARY_SCHEMA_VERSION` change).

### Surface B — `ActivitySnapshot` (in-memory fold; crt-054 produces, crt-055 reads at review)

A `Copy` counter struct returned by a single metadata-only `activity_snapshot()` method on the session's transcript state. Accumulated by a running fold at the delta merge boundary `apply_delta` (`session_transcript.rs:150`), on **both** the registered and the held-delta routes (`session.rs:388-401`) — a held-route miss is the believable-zero trap and is the #1 regression risk. **Never persisted by crt-054**; crt-055 reads it during the review pipeline (before the crt-052 hold purge zeroes the buffer — read-before-purge ordering) and lands it into the columns in §"Consumer persistence".

Content-opacity: metadata-only `Debug`, no content stored, no `Display`. One shared `RegexSet`/Aho-Corasick scan per delta (one byte scan, **not** one pass per pattern).

| Field | Type | Unit | Semantics | Accumulation rule |
|-------|------|------|-----------|-------------------|
| `bytes_total` | `u64` | bytes | total transcript delta payload volume folded for the session — the honest throughput proxy. **Bytes, not tokens.** | monotonic sum of each delta's payload length at `apply_delta`, both routes |
| `delta_count` | `u32` | count | number of deltas folded; pairs with `bytes_total` for mean-delta-size | +1 per delta merged, both routes |
| `class_counts` | `[u32; MAX_SIGNAL_CLASSES]` | count | per-class match counts for the configured `[transcript_signals]` classes; index → class is the config order | += per delta from one shared `RegexSet` pass; a delta may match multiple classes |

`MAX_SIGNAL_CLASSES`: a small fixed bound (≤ 16; dsn-001 / #4591 precedent). v1 configured classes (and their fixed indices): **`0 = error`, `1 = refusal`**. The class catalog is config (`[transcript_signals]`, sibling to `[retention]`): per-entry `{ class_name, pattern, enabled }`, `#[serde(default)]`, a small **domain-neutral** default set (behavioral signatures — model refusal phrasings, provider hard/overload errors — never SDLC literals), compiled once at load into the one `RegexSet`, `validate()`-bounded (invalid regex rejected loudly).

Coverage / attribution semantics (fail-loud — never imply completeness, #4828):
- Counter coverage = **cycle-declaration coverage**. The drain→hold seam holds buffers only for sessions with a non-empty `feature_cycle`; an undeclared session purges at drain and its fold dies — correct fail-loud. Surfaced via the `raw_signals_available`-style flag, never as a silent `0`.
- `compaction_events` (Surface A) is written regardless of declaration (it is server-authoritative at the handler), but is only *attributable* to a cycle through the same declaration chain at review.

### What crt-054 explicitly does NOT do (boundary, vs its prior scope)

- Does **not** add columns to `cycle_review_index`, modify `store_cycle_review` / `build_cycle_review_record`, or change `CycleReviewRecord`. *(Prior crt-054 SCOPE had it doing this — superseded by this contract.)*
- Does **not** bump `SUMMARY_SCHEMA_VERSION` (prior scope said 4→5 — now crt-055 owns 5; crt-054 owns only its own `compaction_events` table migration).
- Does **not** compute reload, the compaction-gated reckoning, or any review-time aggregate. It supplies `compaction_events` rows + `activity_snapshot()`; crt-055 does all reckoning.
- Does **not** add a `reread`/`compaction` regex class or any token/`token_bytes_per_unit` field.

---

## Consumer persistence — the `cycle_review_index` columns crt-055 lands (sourced from the producer)

crt-055-owned columns, crt-047 integer template, written by the single `store_cycle_review()`:

| Column | Type | Source |
|--------|------|--------|
| `transcript_bytes_total` | INTEGER NOT NULL DEFAULT 0 | `ActivitySnapshot.bytes_total` (summed across the cycle's held sessions) |
| `transcript_delta_count` | INTEGER NOT NULL DEFAULT 0 | `ActivitySnapshot.delta_count` |
| `transcript_error_count` | INTEGER NOT NULL DEFAULT 0 | `ActivitySnapshot.class_counts[0]` (error) |
| `transcript_refusal_count` | INTEGER NOT NULL DEFAULT 0 | `ActivitySnapshot.class_counts[1]` (refusal) |
| `signal_class_counts_json` | TEXT NOT NULL DEFAULT '{}' | full class_name→count map (forward-compat for classes added beyond error/refusal) |
| `compaction_count` | INTEGER NOT NULL DEFAULT 0 | COUNT of `compaction_events` rows attributed to the cycle |
| `compaction_reread_count` | INTEGER NOT NULL DEFAULT 0 | crt-055 reckoning: PostToolUse file-overlap reads with `ts >` a session's `compacted_at`, **both normalized to Unix seconds first** (read `ts` is epoch millis → ÷1000; see Binding constraint 8) |
| `context_reload_pct` | INTEGER NOT NULL DEFAULT 0 — basis points 0–10000 | promoted from #758's `compute_context_reload_pct` (a percentage); stored as basis points: `round(pct × 100)` (e.g. 37.5% → 3750). INTEGER, not REAL — keeps every metric column integer (uniform with crt-047) and avoids the float-bind footgun (#4529/#4533). Resolves Open Q4. |

(Plus rank-1/2/3 aggregate columns — phase durations/transitions/rework, rework ratio, knowledge-reuse-all-served — shapes set by the design session against ass-077 RQ-2.)

---

## Out of scope

- **Any token estimate or token-named field** (`token_bytes_per_unit`, "tokens (est.)"). A bytes/N heuristic implies a precise, model-defined, billing-relevant unit it cannot deliver and degrades on code/JSON-heavy transcripts. **Bytes is the honest unit.** Real token accounting, if ever needed, comes from the harness usage stream — a separate feature. (Resolves the prior crt-054↔crt-055 contradiction in favor of bytes-only.)
- **The reload metric's ingest** — owned by #758. crt-055 consumes/promotes it.
- **The transcript fold mechanism + compaction-event persistence** — owned by crt-054 (§Producer contract). crt-055 consumes both.
- **Orchestration / FinOps:** budget enforcement, cost dashboards-as-product, scheduling-by-cost, billing-grade attribution. ("Not an orchestration engine.")
- **R-A — persisting a transcript-derived aggregate that needs a content read on the persist path.** Flagged for human; **default NO.** The leak gate stays structural: no content field on `RetrospectiveReport`/`CycleReviewRecord`.
- **#574 (cycle_events via MCP handler), #602 (attribution naming)** — separate tracks, parked.
- **Response-time-only enrichment beyond #206-item-4** (tool-mix per phase, attribution completeness, decision/lesson density, the most-faithful PreCompact-tail reload) — ass-077 ranks 6–8, response-only, never persisted; revisit with measured need.

## Binding design constraints

1. **The leak gate stays structural.** `RetrospectiveReport`/`CycleReviewRecord` carry no content field; persist only integers/aggregates, never transcript bytes (`test_candidates_structurally_absent_from_memoized_report` must hold).
2. **Single writer, four success returns (#4750).** New columns written ONLY via the one `store_cycle_review()` in the full-pipeline block. The memo-hit return, the purged-retain return, and the force+purged stored-record return must **not** write the new columns (no zero-clobber). Never add a second writer.
3. **Coexist with #758's guarded recompute + data-presence gate.** The stale→recompute fallthrough is the desired auto-refresh path for pre-v5 rows when source data is present; the purged-retain path stays no-write.
4. **Two guards (ass-077):** (a) fail-loud presentation — render "unavailable" not "0" when a source class is empty; (b) regression guard — a test asserting the aggregation reads a non-empty source for a representative TS-client cycle (prevents the #750 silent-zero class).
5. **Dual reload metrics are two columns, two gates, one engine — never re-collapsed.** Pin each metric's exact overlap window (`context_reload` cross-session; `compaction_reread` post-compaction within-cycle) before building.
6. **Read-before-purge.** crt-055 reads `activity_snapshot()` before the crt-052 hold purge zeroes the buffer.
7. **Producer contract is binding.** Build the consumer against §"Producer contract" exactly; any drift is reconciled in that section first, then crt-054's scope.
8. **Canonical gate unit is Unix SECONDS.** Any timestamp entering the `compaction_reread` gate comparison is normalized to Unix **seconds** before comparison. PostToolUse read `ts` is persisted as **epoch milliseconds** (column `observations.ts_millis`, `i64`; `ObservationRecord.ts`, `u64`, "epoch millis"); `compaction_events.compacted_at` is Unix **seconds** (Surface A). The gate normalizes the read `ts` to seconds (`ts_millis / 1000`, integer floor — the existing `session_metrics.rs:115` convention) at the reckoning site *before* the `read_ts_secs > compacted_at` test. Without this, every read counts (or none do) — a silent gate break. This is no longer an open question; it is a binding contract clause (resolves the former clock-unit open item).

## Dependencies / prior art

- **crt-054 (#752)** — the producer. Re-scoped against §"Producer contract" before it executes. Provides `compaction_events` + `activity_snapshot()`.
- **#758 / #750 — MERGED** (`7aca6c44`). Provides live cross-session `context_reload`, `SUMMARY_SCHEMA_VERSION = 4`, the guarded-recompute / data-presence-gate / purged-retain logic crt-055 coexists with.
- **crt-047** — the integer-columns-on-`CycleReviewRecord` + curation-health precedent; the column + migration template to copy (`cycle_review_index.rs:84-104`, `migration.rs:926-1043`).
- **crt-052 Wave B** — the transcript hold (verified ON, unconditional); the `activity_snapshot()` durability-to-review rests on it.
- **ass-077 / ass-078** — substrate decision, RQ-2 ranks, fold-at-ingest design. Finalized here.
- **Patterns:** #4178 (derived aggregates, single write, bump version), #4750 (four success returns), #4153 (three-path bump), #4484 (cascade-file existence).
- **Stale knowledge to correct in Phase 2:** ADR-008 (#5006) "crt-054 first mover / owns v4/v29" — now false on two counts (#758 owns 4; crt-054 no longer touches `cycle_review_index` or `SUMMARY_SCHEMA_VERSION`). Architect `context_correct`s it to "crt-055 owns SUMMARY_SCHEMA_VERSION 5 + the cycle_review_index migration; crt-054 owns the compaction_events table only; #758 owns 4."

## Open questions for the design session

1. **Aggregate column shapes (rank 1–3).** Exact columns for phase durations/transitions/rework, rework ratio, knowledge-reuse-all-served — set against ass-077 RQ-2. (Pure persistence shape; no producer impact.)
2. **`compaction_reread` boundary selection** when a session has multiple `compaction_events` rows — gate on earliest, latest, or per-boundary segments? (crt-055 reckoning detail.) *Resolved by ADR-006: earliest `compacted_at` per session, counted once.* The companion clock-unit concern (read `ts` millis vs `compacted_at` seconds) is **no longer open** — it is a binding contract clause (Binding constraint 8, ADR-006): normalize the read `ts` to seconds first.
3. **Knowledge-that-helped (#206-4)** — response-time only, or does it warrant a durable column?
4. **Default domain-neutral signature catalog** — which `error`/`refusal` patterns generalize across domains (product judgment; keep tiny + high-precision, under-catalog and let domains extend).
5. **Internal wave order** — recommend: fail-loud guard → durable aggregates + restructure → reload pair + fold surfacing (consumes crt-054). One migration regardless.

## Vision alignment

Advances self-learning (#4677): replaces a believable-zero-prone, single-event-dependent report with durable, multi-source, fail-loud per-cycle aggregates that agents and humans use to improve the process. Informs, never controls (RQ-8 hard edge). No orchestration/FinOps surface; the throughput unit is bytes, never a cost.

## Tracking

GH Issue #755 (re-scoped to this consolidated definition). Folds #556, #320, #593, #206(item 4) as ACs — those issues closed/annotated as folded at the next gate. #569 + #604 descoped to standalone bug fixes.
