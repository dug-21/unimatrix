# crt-054 Architecture — Transcript-Fold Producer (producer-only)

**Date**: 2026-06-16 — **FULL REDESIGN against the producer-only re-scope.**
**Feature**: crt-054 (#752) — Cortical (crt), learning & drift
**Goal**: self-learning (#4677)
**Binding contract**: `product/features/crt-055/SCOPE.md` §"Producer contract" — authoritative for every field crt-054 writes. On any conflict, that section wins.
**Inputs**: `product/features/crt-054/SCOPE.md`, `product/features/crt-054/SCOPE-RISK-ASSESSMENT.md`, ass-077/ass-078 FINDINGS.

> This document supersedes the 2026-06-14 ARCHITECTURE.md, which was written for the prior wider scope (crt-054 owning `cycle_review_index` columns, `store_cycle_review`, `SUMMARY_SCHEMA_VERSION`). That scope moved entirely to crt-055. Nothing from the removed scope is re-imported here.

---

## 1. System Overview

crt-054 is the **producer half** of a producer/consumer pair. crt-055 (#755) is the consumer: it owns the `cycle_review_index` schema, the `RetrospectiveReport`, `store_cycle_review`, `SUMMARY_SCHEMA_VERSION`, the reload reckoning, and all review-time aggregation. crt-054 supplies exactly two raw inputs that no durable source carries and that are observable only at the ingest/server seam:

- **Surface A — `compaction_events`**: a new durable, insert-only table, one row per compaction event, written at the authoritative server seam. It gives crt-055 the durable, timestamped compaction gate it needs to compute the "reload-after-compaction" tax. crt-054 writes the rows; crt-055 reads and reckons.

- **Surface B — `activity_snapshot()`**: a content-free, in-memory running fold over transcript deltas (`bytes_total`, `delta_count`, behavioral-signature `class_counts`), exposed as a `Copy` counter struct. crt-054 accumulates it at the delta merge boundary on both routes; crt-055 reads it at review and lands the columns. crt-054 **never persists** Surface B.

```
                 INGEST / SERVER SEAM (crt-054 produces)              REVIEW (crt-055 consumes)
  ┌──────────────────────────────────────────────────────┐   ┌──────────────────────────────────┐
  │ apply_delta (both routes) ── fold ──▶ ActivityCounters │   │ activity_snapshot() ──▶ columns   │
  │   session_transcript.rs:150            (in TranscriptBuffer)│ │  (read-before-purge, crt-055)    │
  │                                                        │   │                                   │
  │ handle_compact_payload ── INSERT ──▶ compaction_events │   │ SELECT compaction_events ──▶ gate │
  │   listener.rs:1854 (co-located w/ increment_compaction)│   │  (compaction_reread reckoning)    │
  └──────────────────────────────────────────────────────┘   └──────────────────────────────────┘
```

Vision boundary (RQ-8, hard edge): every signal crt-054 produces **informs, never controls.** Disqualifying test: "does this counter control / bill / schedule / block execution?" If yes, out of lane. The unit is **bytes**, never tokens, never cost.

---

## 2. Component Breakdown

| Component | Responsibility | Location (seam) |
|-----------|---------------|-----------------|
| `ActivityCounters` (fold accumulator) | Hold `bytes_total: u64`, `delta_count: u32`, `class_counts: [u32; MAX_SIGNAL_CLASSES]`. `Copy`/metadata-only. Embedded inside `TranscriptBuffer`. | new field in `TranscriptBuffer` (`infra/session_transcript.rs`) |
| `transcript_activity` module | The fold logic + `SignatureScanner`. Sibling module (buffer module is at the 500-line cap). | new `infra/transcript_activity.rs` |
| `SignatureScanner` | Compile the `[transcript_signals]` catalog into one shared `RegexSet`; one byte scan per delta. | `infra/transcript_activity.rs` |
| `apply_delta` fold call | After merge, run `self.activity.fold(bytes, &self.scanner)`. Runs on registered AND held routes by construction (accumulator lives in the buffer). | `session_transcript.rs:150` |
| `activity_snapshot()` | Counters-only read surface returning `ActivitySnapshot` (Copy, no bytes). | `TranscriptBuffer`, `infra/session_transcript.rs` |
| activity collector | Mirror `take_transcripts_for_feature` (dedup-by-`Arc`, registered ∪ held) but call `activity_snapshot()` only. Used by crt-055. | `SessionRegistry`, `infra/session.rs` |
| `compaction_events` writer | One INSERT per compaction at the handler. Content-free. | `handle_compact_payload`, `uds/listener.rs:1854` |
| `compaction_events` table + migration | New table; `CREATE TABLE IF NOT EXISTS`; next `CURRENT_SCHEMA_VERSION` bump. | `unimatrix-store` `migration.rs` + `db.rs` |
| `[transcript_signals]` config | `{ class_name, pattern, enabled }` per entry; `validate()`-bounded; compiled once at load. | `config.rs` (sibling to `[retention]`) |
| Wave B startup precondition | Fail-loud assert the `HeldBufferScan` handle is wired (Surface B durability depends on it). | `main.rs` startup, next to `RetentionConfig::validate()` |

What crt-054 does **not** build (owned by crt-055; SCOPE §Out-of-scope): `cycle_review_index` columns, `store_cycle_review`/`build_cycle_review_record`/`CycleReviewRecord`, `SUMMARY_SCHEMA_VERSION` bump, any reload/overlap/compaction-reread reckoning, any token/cost field, the vnc-036 wire change.

---

## 3. Component Interactions / Data Flow

### Surface B — the in-memory fold (ingest → review)
1. A transcript delta arrives. The delta is routed in `apply_delta_to_session` (`session.rs`): Phase 1 (registry lock) resolves the target `Arc` — either the registered buffer or, for a drained-but-held session, the held buffer via the `HeldBufferScan` branch (`session.rs:388-401`). Phase 2 (buffer lock) calls `buf.apply_delta(offset, bytes)` (`session_transcript.rs:401`).
2. Inside `apply_delta`, after the merge, the fold runs: `self.activity.fold(bytes, &self.scanner)` — `bytes_total += len`, `delta_count += 1`, and one `RegexSet` pass bumps each matched `class_counts[i]` by 1. Because the accumulator is embedded in the buffer, **both routes fold into the same accumulator** with no extra wiring (ADR-001).
3. The buffer (with its accumulator) rides the crt-052 Wave B hold across drains (ADR-006). crt-054 never zeroes or drops it.
4. At review, crt-055 calls the activity collector (modeled on `take_transcripts_for_feature`, dedup-by-`Arc`, registered ∪ held filtered by `feature_cycle`), reading `activity_snapshot()` per session **before** `purge_cycle_transcripts` zeroes the buffers (crt-055 read-before-purge, Constraint 6). crt-055 sums across the cycle's sessions and lands the columns.

### Surface A — the durable compaction event (handler → review)
1. A compaction request hits `handle_compact_payload` (`listener.rs:1737`). The handler does its async briefing build (`:1804`), reads the transcript tail under the buffer lock (`:1833`, guard dropped at `:1835`), then increments the in-memory compaction count (`increment_compaction`, `:1854`).
2. **Co-located at `:1854`, after `increment_compaction` returns**, crt-054 captures `high_water` (read the buffer's `high_water()` under its lock via the already-shared `Arc`, then drop the guard) and INSERTs one `compaction_events` row `{ session_id, compacted_at = now_secs (Unix SECONDS, server wall clock), high_water }` via a single **autocommit INSERT helper on `services.store_ops`** — no explicit transaction, no lock held across the INSERT (ADR-007 — SR-01 / Open Q4 resolved). On DB error: log ids/counts (no content) **and emit a named failure counter (`compaction_events_insert_failed`)**, then proceed — the compaction ACK is never blocked; a missing row reads as fail-loud absence at crt-055's review, and the counter makes systematic loss detectable + lets crt-055 flag row-vs-`increment_compaction` drift.
3. At review, crt-055 `SELECT`s `compaction_events` by `session_id` (joined to the cycle via the declaration chain), gates PostToolUse read `ts` against `compacted_at`, and lands `compaction_count` / `compaction_reread_count`.

### Error boundaries
- **Surface B fold**: cannot fail — it is integer arithmetic under the buffer lock; a poisoned buffer mutex degrades to empty (`#4764`), the same as `snapshot()`.
- **Surface A INSERT**: tolerated failure — logged + a named failure counter (`compaction_events_insert_failed`), non-blocking, never panics the handler (ADR-007).
- **Config**: `[transcript_signals]` invalid regex / over-cap fails **loud at startup** (`validate()`), not at runtime (ADR-002).
- **Wave B wiring**: absent `HeldBufferScan` handle fails **loud at startup** (ADR-010), not a silent degrade.

---

## 4. Technology Decisions (ADR index)

| ADR | Decision | Status vs prior |
|-----|----------|-----------------|
| ADR-001 | Fold lives inside `TranscriptBuffer`, folded at `apply_delta` on both routes | corrected (reload latch removed) |
| ADR-002 | Behavioral-signature catalog — one shared `RegexSet`, `[transcript_signals]` config, `validate()`-bounded, v1 = `error`/`refusal` only | corrected (no `reread`/`compaction` class, no `token_bytes_per_unit`, no `role`) |
| ADR-003 | `activity_snapshot()` — `Copy` counter struct carrying `bytes_total`, never transcript bytes | corrected (no `saw_compaction`/reload latch; no in-handler sum) |
| ADR-004 | Late-bind cycle attribution via the hold's filter; coverage = declaration coverage, never a fabricated zero | corrected (no persisted `activity_session_count`) |
| ADR-005 | Never-persist envelope — running-fold-only, content-opaque, no token-named field | corrected (no token estimate; columns are crt-055's) |
| ADR-006 | crt-054's obligation is survival-to-review — never zero/drop the counter before the crt-052 purge | heavily corrected (four-returns/persist moved to crt-055) |
| ADR-007 | **NEW** — durable `compaction_events` table, insert-only at `handle_compact_payload`, no lock held across the INSERT | new |
| ADR-008 | crt-054 owns only `compaction_events` + the next `CURRENT_SCHEMA_VERSION` bump; not `SUMMARY_SCHEMA_VERSION`, not `cycle_review_index` | corrected (was the STALE ADR) |
| ADR-009 | Believable-zero regression guard — non-empty fold on the held route + survival ordering | corrected (asserts `activity_snapshot()`, not a persisted row) |
| ADR-010 | crt-052 Wave B is a verified startup precondition | kept (scoped to Surface B) |

---

## 5. Integration Points

- **crt-055 (#755)** — the consumer and the binding contract. Reads `activity_snapshot()` + `compaction_events`; lands all columns. Any field/width/catalog change is negotiated in crt-055's §"Producer contract" first (SR-07).
- **crt-052 Wave B** — the transcript hold (ON by default, unconditional, non-disableable; `main.rs:698-718, 1234-1254`). Surface B's survival-to-review rests on it; asserted at startup (ADR-010). Surface A does NOT depend on it.
- **#758 / #750 (merged `7aca6c44`)** — provides live cross-session `context_reload` and `SUMMARY_SCHEMA_VERSION = 4`. crt-054 coexists by **not touching** `cycle_review_index` or `SUMMARY_SCHEMA_VERSION`.
- **`handle_compact_payload`** (`uds/listener.rs:1737`) + `increment_compaction` (`infra/session.rs:554-559`) — the Surface A write seam; `services.store_ops` gives the store; `session_state.transcript` `Arc` gives `high_water`.
- **`TranscriptBuffer`** (`infra/session_transcript.rs`) — `high_water` (`:53`, accessor `:333`), `apply_delta` (`:150`, the fold boundary), content-opacity contract already enforced (no `Display`, metadata-only `Debug`).
- **`unimatrix-store` migration** (`migration.rs:22` `CURRENT_SCHEMA_VERSION = 28`; `run_main_migrations` upgrade block; `db.rs` `create_tables_if_needed` fresh-create) — the three-path bump for the new table.

---

## 6. Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `apply_delta` (fold site) | `pub fn apply_delta(&mut self, offset: u64, bytes: &[u8])` — fold call added after merge | `infra/session_transcript.rs:150` (existing) |
| `TranscriptBuffer.high_water` | `high_water: u64` field; `pub fn high_water(&self) -> u64` | `infra/session_transcript.rs:53`, accessor `:333` (existing) |
| Held-route delta branch | `HeldBufferScan` handle → `held_arc_for_session(session_id) -> Option<Arc<..>>` | `infra/session.rs:388-401` (existing) |
| `increment_compaction` | `pub fn increment_compaction(&self, session_id: &str)` — takes+releases `sessions` lock internally | `infra/session.rs:554-559` (existing) |
| Surface A write seam | INSERT placed immediately after `session_registry.increment_compaction(session_id)` | `uds/listener.rs:1854` (existing call) |
| Store access at the seam | `services.store_ops` (`StoreService` owning `Arc<Store>`) | `services/mod.rs:239`; `ServiceLayer` |
| `ActivitySnapshot` (NEW) | `#[derive(Clone, Copy)] struct ActivitySnapshot { bytes_total: u64, delta_count: u32, class_counts: [u32; MAX_SIGNAL_CLASSES] }` — no `Display`, metadata-only `Debug` | new in `infra/session_transcript.rs` / `transcript_activity.rs` |
| `activity_snapshot()` (NEW) | `pub fn activity_snapshot(&self) -> ActivitySnapshot` on `TranscriptBuffer`; poison→empty (#4764) | new, `infra/session_transcript.rs` |
| activity collector (NEW) | `fn activity_snapshots_for_feature(&self, feature_cycle: &str) -> Vec<(String, ActivitySnapshot)>` — mirrors `take_transcripts_for_feature` selection | new, `SessionRegistry`, `infra/session.rs` |
| `compaction_events` table (NEW) | `id INTEGER PK, session_id TEXT NOT NULL, compacted_at INTEGER NOT NULL /* Unix SECONDS — DDL comment documents the unit */, high_water INTEGER NOT NULL DEFAULT 0`; INDEX on `session_id` | new, `unimatrix-store` |
| compaction-INSERT helper (NEW) | thin single-statement autocommit INSERT on `store_ops` (no explicit txn); named failure counter `compaction_events_insert_failed` on error | new, `unimatrix-store` / `services` |
| `MAX_SIGNAL_CLASSES` (NEW) | `const MAX_SIGNAL_CLASSES: usize = 16` — **PINNED, must equal crt-055's constant exactly**; v1 indices `0=error, 1=refusal` | new |
| `[transcript_signals]` config (NEW) | `Vec<{ class_name: String, pattern: String, enabled: bool }>`, `#[serde(default)]`, `validate()`-bounded | new, `config.rs` (sibling to `[retention]`) |
| `CURRENT_SCHEMA_VERSION` | `pub const CURRENT_SCHEMA_VERSION: u64 = 28` → next (29/30, merge-order coordinated) | `unimatrix-store/src/migration.rs:22` |

Field-width contract (crt-055 §"Surface B"): producer `bytes_total: u64`, `delta_count: u32`, `class_counts: [u32; N]`; crt-055 lands these into `i64` columns with checked/saturating conversion at the persist boundary (Open Q3 / SR-03). crt-054 owns producer-side widths; crt-055 owns the conversion.

---

## 7. Risk Coverage (SCOPE-RISK-ASSESSMENT)

| Risk | Where addressed |
|------|-----------------|
| SR-01 lock ordering at the write seam | ADR-007 — INSERT at `:1854` after `increment_compaction`, no lock held across it; `high_water` captured then guard dropped |
| SR-02 held-route believable-zero | ADR-001 (fold on both routes by construction) + ADR-009 (held-route regression guard) |
| SR-03 integer-width truncation | ADR-003 / §6 — producer widths fixed, checked/saturating conversion at crt-055 boundary |
| SR-04 schema-version sequencing collision | ADR-008 — distinct sequential versions, merge-order coordinated; disjoint tables |
| SR-05 stale-knowledge residue | full ADR regeneration + Unimatrix reconciliation; ADR-008 corrects #5006 |
| SR-06 token/cost exclusion drift | ADR-005 / ADR-002 — no token-named field anywhere; bytes-only |
| SR-07 producer-contract coupling | §5 — contract is single source; field/catalog changes negotiated in crt-055 first |
| SR-08 survival-to-review on the hold | ADR-006 (never zero/drop) + ADR-010 (Wave B precondition) |
| SR-09 cycle-declaration coverage gap | ADR-004 — never fabricate a zero; Surface A row is declaration-independent |
| SR-10 vnc-036 shelving / `high_water` reserved | ADR-007 — `high_water` server-captured, documented reserved, not over-trusted |

---

## 8. Open Questions

> The four producer-contract open questions are **RESOLVED** (crt-055 design complete, 2026-06-16). `MAX_SIGNAL_CLASSES = 16` (pinned), default catalog = `error`/`refusal` only (ADR-002); `compacted_at` = Unix seconds documented in DDL, gate-side `ts/1000` is crt-055's (ADR-007 / crt-055 Binding constraint 8); Surface A INSERT is a single autocommit helper, no transaction, with a named failure counter (ADR-007). Only the merge-order version number remains, and that is an SM coordination point, not a design question.

1. **Schema-version number at merge** (SR-04). crt-054 and crt-055 take distinct sequential bumps (29/30); the actual number is merge-order-dependent and set by the SM at merge. *Owner: SM coordination point (not a design decision).*
2. **Default `error`/`refusal` pattern calibration** (ADR-002). The two default patterns must be calibrated against real transcripts during delivery before locking, because content-opacity means their false-positive rate can never be audited post-ship; the counts are directional, not precise. *Owner: delivery — calibrate before lock.*
