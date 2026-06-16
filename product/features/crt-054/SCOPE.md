# CRT-054 — Transcript-fold producer: durable compaction-event table + in-memory throughput/signature fold

**Date**: 2026-06-16 — **re-scoped to the crt-055 producer contract — producer-only**
**Status**: SCOPE (design-session input — pre-design)
**Phase**: Cortical (crt) — learning & drift
**Tracking**: GH Issue #752
**Goal**: self-learning (#4677)
**Design input**: `product/research/ass-078/FINDINGS.md` (fold-at-ingest design), `product/research/ass-077/FINDINGS.md` (faithful-reload definition)
**Binding contract**: **`product/features/crt-055/SCOPE.md` §"Producer contract"** — the authoritative field-level definition of everything crt-054 writes. **On any conflict, that section wins.** This SCOPE describes the producer-side *implementation approach*; the contract fixes the *interface*.
**Paired feature**: crt-055 (#755) — the consumer. crt-055 owns the entire `cycle_review_index` schema, the report, `store_cycle_review`, the reload reckoning, and `SUMMARY_SCHEMA_VERSION`. crt-054 supplies the two raw inputs and nothing else.

> Pre-design scope handed to a design session. It captures the validated problem and the v1 boundary for the PRODUCER half of the cycle-review-telemetry pair. It is NOT an implementation brief; the design session produces ARCHITECTURE / SPECIFICATION / IMPLEMENTATION-BRIEF and the ADRs.

---

> ## Re-scope note (2026-06-16) — crt-054 is now producer-only. Read this first.
>
> crt-054 originally owned both the transcript fold AND the durable landing of throughput columns on `cycle_review_index` (the prior In-scope item 5 + the #758 guarded-recompute reconciliation). **That ownership has moved entirely to crt-055.** The consolidation (uni-zero roadmap session) made crt-055 the single owner of the `cycle_review_index` schema, `store_cycle_review`, the four-success-returns coexistence, and `SUMMARY_SCHEMA_VERSION`. crt-054 keeps only what it is uniquely positioned to produce at the ingest/server seam.
>
> **What changed vs the prior crt-054 SCOPE:**
> - **Removed: all `cycle_review_index` work.** No columns, no `store_cycle_review` / `build_cycle_review_record` change, no `CycleReviewRecord` change, no v28→v29 column migration, no #758 guarded-recompute reconciliation. (This was crt-054's highest-risk surface — it is gone.)
> - **Removed: the `SUMMARY_SCHEMA_VERSION` bump.** crt-054 does NOT bump it. crt-055 takes 4→5. #758 owns 4.
> - **Removed: the "consume the now-live `context_reload`" item.** crt-054 does nothing with reload; crt-055 owns reload end-to-end (cross-session promote + the compaction-gated reckoning over crt-054's rows).
> - **Kept + sharpened:** the durable `compaction_events` table (crt-054's own new table) and the in-memory throughput/signature fold exposed as `activity_snapshot()`.
> - **Reconciled: `high_water` is now populated** on every `compaction_events` row (was "optional / deferred"). It is reserved for a future precise byte-boundary gate; populating it now (the buffer already tracks it) avoids a second migration. No wire change (vnc-036 stays shelved).

---

## Problem

`context_cycle_review` needs two raw inputs that no durable source carries, both observable only at the ingest/server seam:

1. **A durable, timestamped compaction event.** Compaction is authoritative server-side but lives only as an in-memory counter (`increment_compaction`, `infra/session.rs:554-559`). The now-live `context_reload` (#758) is cross-session file overlap, **not** compaction-gated, because no persisted compaction timestamp exists. "Reload *after* compaction" — ass-077's most-faithful reload — requires a durable gate that does not exist. crt-055 computes that gate at review; it cannot, without a durable compaction timestamp to compare against.
2. **Throughput / total conversation volume + behavioral signatures per cycle.** PostToolUse rows hold tool I/O, not total agent conversation volume; a model error/refusal is not a tool call and has no structured source. Both are observable only in the transcript byte stream. No durable stream carries them.

crt-054 produces exactly these two inputs. crt-055 persists, gates, and surfaces them.

## Goal / value framing

Self-learning bar (ass-077): produce trustworthy **information about the process** — where a cycle ran heavy, where it strained against a compaction boundary. **Informs, never controls.** Disqualifying test: *does this counter control execution?* If yes, out of lane (RQ-8 vision boundary). The fold is content-free; bytes is the honest unit (never tokens, never cost).

---

## In scope — the two producer surfaces (and only these)

crt-054 = **Surface A (durable `compaction_events` table)** + **Surface B (in-memory throughput/signature fold via `activity_snapshot()`)** + the **`[transcript_signals]` config** that feeds B. Field-level definitions are in crt-055 §"Producer contract" (binding); restated here for self-containedness.

### Surface A — `compaction_events` table (crt-054 owns the table + the writes)

1. **A new durable table, insert-only, one row per compaction event**, written at the authoritative seam `handle_compact_payload` (`uds/listener.rs:1737`), co-located with the existing `increment_compaction` call (`infra/session.rs:554-559`). Columns (per contract): `id` (PK), `session_id` TEXT NOT NULL, `compacted_at` INTEGER NOT NULL (Unix **seconds** — the gate boundary), `high_water` INTEGER NOT NULL DEFAULT 0 (`TranscriptBuffer.high_water`, `session_transcript.rs:52`, at compaction — populated, reserved for future precise gating). Index on `session_id`.
2. **`feature_cycle` is deliberately NOT stored** — the row is written regardless of declaration; cycle attribution is resolved at review by crt-055 via the session→`feature_cycle` declaration chain. This is what dissolves the held/registered-route edge case: the event is durable and session-keyed, independent of whether the session's buffer was held at drain.
3. **Content-free.** No payload, no `tracing` of content (ADR-002 content-opacity). Exactly one INSERT per compaction event; never updated.

### Surface B — `activity_snapshot()` in-memory throughput/signature fold (crt-054 produces; crt-055 reads at review)

4. **A running, content-free fold over transcript deltas at the merge boundary `apply_delta` (`session_transcript.rs:150`), on BOTH the registered and the held-delta routes (`infra/session.rs:388-401`)** — a held-route miss is the believable-zero trap and the #1 regression risk. Exposed via one metadata-only `activity_snapshot()` returning a `Copy` counter struct: `bytes_total` (u64, bytes), `delta_count` (u32), `class_counts` (`[u32; MAX_SIGNAL_CLASSES]`; indices `0=error, 1=refusal`). One shared `RegexSet`/Aho-Corasick scan per delta (one byte scan, not one pass per pattern). No new lock; no content stored; metadata-only `Debug`.
5. **Never persisted by crt-054.** The fold is read by crt-055 during the review pipeline (before the crt-052 hold purge zeroes the buffer). crt-054's obligation is that the counter is accurate and remains readable until review — it writes no column.
6. **`[transcript_signals]` config** (sibling to `[retention]`): per-entry `{ class_name, pattern, enabled }`, `#[serde(default)]`, a small **domain-neutral** default set (behavioral signatures — model refusal phrasings, provider hard/overload errors — never SDLC literals), compiled once at load into the one `RegexSet`, `validate()`-bounded (`MAX_SIGNAL_CLASSES` ≤ ~16; invalid regex rejected loudly; dsn-001 / #4591 precedent). v1 default classes: `error`, `refusal`. **No `reread` class, no `compaction` class** (there is no in-stream marker; compaction comes from Surface A).

## The migration (a NEW table only — crt-054 does NOT touch `cycle_review_index`)

- crt-054 adds the `compaction_events` table and takes the **next `CURRENT_SCHEMA_VERSION` bump** (`migration.rs:22`, currently 28 → 29). crt-054 and crt-055 migrate **different tables** (`compaction_events` vs `cycle_review_index`), so merge order is free; the two migrations must take **distinct sequential** version numbers (whichever merges first is 29, the other 30).
- **No `SUMMARY_SCHEMA_VERSION` change** (`cycle_review_index.rs:49` stays crt-055's to bump 4→5). **No `cycle_review_index` ALTER. No `store_cycle_review` change.**
- Standard bump hygiene for the new table: db.rs fresh-create includes it; migration.rs upgrade block adds it; `pragma_table_info`/existence-guarded; cascade-file existence verified (#4484).

## Out of scope

- **The entire `cycle_review_index` surface** — columns, `store_cycle_review` / `build_cycle_review_record`, `CycleReviewRecord`, the four-success-returns coexistence, the #758 guarded-recompute reconciliation. **All owned by crt-055.**
- **`SUMMARY_SCHEMA_VERSION`** — crt-055 owns the 4→5 bump.
- **Reload of any kind** — cross-session `context_reload` (owned by #758) and the compaction-gated `compaction_reread` reckoning (computed by crt-055 at review over crt-054's `compaction_events` rows). crt-054 computes no reload, no overlap, no review-time aggregate.
- **The `compaction_reread` boundary-selection semantics** (which `compacted_at` to gate on when a session has multiple compactions) — a crt-055 reckoning detail, not a producer concern.
- **Any token estimate or token-named field** (`token_bytes_per_unit` included) — bytes is the honest unit; token/cost is a separate harness-usage-stream feature. (Resolves the prior crt-054↔crt-055 contradiction in favor of bytes-only.)
- **A precise per-compaction byte boundary on the wire (vnc-036)** — SHELVED. crt-054 captures `high_water` server-side at the handler; no wire/client change. Reopen only with measured need.
- **Deferred signals (ass-078):** turn-size percentiles, thrash/rolling-hash, per-delta entropy, language/code-fence detection, mean-turn-size. Revisit with measured evidence only.
- **Orchestration / FinOps surfaces.** ("Not an orchestration engine.")

## Binding constraints

1. **R-A guardrail (the bright line vs ass-077's rejected option):** every signal crt-054 produces is a **running fold over deltas (a counter)** or a **discrete server-seam event** — never a query over the assembled transcript buffer. No content field escapes; metadata-only `Debug`/no `Display` on the snapshot (content-opacity, ADR-002 #4740).
2. **Held-route coverage = the believable-zero trap.** The fold MUST run on both the registered and held-delta routes (`session.rs:388-401`). A regression guard test must assert the fold reads a non-empty source for a representative TS-client cycle, so the next edge-event-set change fails a test instead of silently zeroing (the #750 class).
3. **Fold survives to review (read-before-purge is crt-055's read, crt-054's survival).** The in-memory counter must remain accurate and readable until crt-055 reads `activity_snapshot()` at review; it rides the crt-052 hold. crt-054 must not zero or drop the counter before the hold purge.
4. **Counter/event coverage = cycle-declaration coverage.** The drain→hold seam holds buffers only for sessions with a non-empty `feature_cycle`; an undeclared session purges at drain and its fold dies — correct fail-loud (crt-055 surfaces this via a `raw_signals_available`-style flag; crt-054 must never fabricate a zero). The `compaction_events` row, keyed by `session_id` at the handler, is written regardless of declaration and attributed at review.
5. **Lock ordering at the write seam.** The `compaction_events` INSERT at `handle_compact_payload` must not deadlock against the registry/session locks held there; co-locate with `increment_compaction` and confirm ordering in design.
6. **Never reintroduce the #750 class.** crt-054 must not let any of its outputs depend on the retired `PreToolUse` event or any single hook-event presence that can vanish under a client change.

## Dependencies / prior art

- **crt-055 (#755) — the binding contract.** `product/features/crt-055/SCOPE.md` §"Producer contract" is authoritative for every field crt-054 writes. crt-054 implements to it exactly; any needed change is negotiated there first.
- **crt-052 Wave B** — the transcript hold (verified ON by default, unconditional, non-disableable; `main.rs:698-718, 1234-1254`; `config.rs validate()` forbids `transcript_hold_max_sessions=0`). The fold's survival-to-review rests on it.
- **#758 / #750 — MERGED** (`7aca6c44`, 2026-06-15). Provides the live cross-session `context_reload` and the guarded-recompute logic crt-054 no longer interacts with. crt-054 must coexist by *not touching* `cycle_review_index` or `SUMMARY_SCHEMA_VERSION`.
- **Authoritative compaction seam:** `handle_compact_payload` (`uds/listener.rs:1737`) + `increment_compaction` (`infra/session.rs:554-559`); `SessionState.transcript` Arc gives buffer + `high_water` access already.
- **Transcript buffer:** `TranscriptBuffer` (`infra/session_transcript.rs`) — `high_water` (I3, monotonic, bytes sent), `apply_delta` (the fold merge boundary). Content-opacity contract already enforced here (no `Display`, metadata-only `Debug`).
- **Stale knowledge to correct in Phase 2:** ADR-008 (entry #5006) says "crt-054 first mover / owns v4/v29 on `cycle_review_index`" — **false on two counts**: #758 owns `SUMMARY_SCHEMA_VERSION` 4, and crt-054 no longer touches `cycle_review_index` or `SUMMARY_SCHEMA_VERSION` at all. Architect `context_correct`s #5006 to: "crt-055 owns `SUMMARY_SCHEMA_VERSION` 5 + the `cycle_review_index` migration; crt-054 owns the `compaction_events` table only; #758 owns 4." The prior ARCHITECTURE/ADR-003 residuals (`reread`/`compaction` regex classes, `token_bytes_per_unit`, cycle_review_index columns) are regenerated against this SCOPE, not edited.

## Open questions for the design session

1. **Lock ordering + transaction shape** for the `compaction_events` INSERT at `handle_compact_payload` (Constraint 5) — confirm no deadlock against locks held at the seam, and whether the write is on the hot drain path or deferrable.
2. **`MAX_SIGNAL_CLASSES` value + the default catalog contents** — which `error`/`refusal` signatures generalize across domains (product judgment; keep tiny + high-precision; under-catalog and let domains extend via config). *(The catalog is shared with crt-055's surfacing; align the default set with crt-055's design session.)*
3. **`delta_count` / `bytes_total` integer widths** at the in-memory fold vs the i64 columns crt-055 lands them in — confirm no truncation across the `activity_snapshot()` → `store_cycle_review()` boundary.

## Vision alignment

Advances self-learning (#4677): supplies the two durable, content-free inputs (compaction event + throughput/signature fold) that let crt-055 surface trustworthy process information — where a cycle strained against a compaction boundary, how heavy it ran. Held strictly to a knowledge surface; the RQ-8 no-execution-control boundary is a hard scope edge.

## Tracking

GH Issue #752. Re-scoped against the crt-055 producer contract (2026-06-16) — producer-only; `cycle_review_index` ownership moved to crt-055. To be reflected on the issue at the next design-session gate.
