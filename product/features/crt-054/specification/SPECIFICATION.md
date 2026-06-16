# CRT-054 — Specification (producer-only)

**Feature**: crt-054 — Transcript-fold producer: durable compaction-event table + in-memory throughput/signature fold
**Date**: 2026-06-16 — **REGENERATED for the producer-only re-scope.** Supersedes the 2026-06-14 specification (prior wider scope).
**Status**: SPECIFICATION (design-session artifact)
**Phase**: Cortical (crt) — learning & drift
**Tracking**: GH Issue #752 · **Goal**: self-learning (#4677)
**Inputs**: `product/features/crt-054/SCOPE.md`, `product/features/crt-054/SCOPE-RISK-ASSESSMENT.md`
**Binding interface**: `product/features/crt-055/SCOPE.md` §"Producer contract" — authoritative field-level definition of everything crt-054 writes. **On any conflict, the contract wins; this spec restates it for self-containedness.**

---

## Objective

crt-054 produces exactly two raw inputs that no durable source carries today, both observable only at the ingest/server seam, and supplies them to crt-055 (the consumer). Surface A is a new durable, insert-only `compaction_events` table giving each compaction a timestamped, session-keyed row (the gate boundary crt-055 needs for "reload after compaction"). Surface B is an in-memory, content-free fold over transcript deltas — `bytes_total`, `delta_count`, behavioral `class_counts` — exposed as `activity_snapshot()` and read by crt-055 at review. The fold informs, never controls (RQ-8); bytes is the honest unit — never tokens, never cost.

---

## Domain Models

### Ubiquitous language

| Term | Definition |
|------|-----------|
| **Compaction event** | The authoritative server-side act of compacting a session's context, handled at `handle_compact_payload` (`uds/listener.rs:1737`), co-located with `increment_compaction` (`infra/session.rs:554-559`). crt-054 lands one durable row per event. |
| **`compaction_events`** | Surface A. The new durable, insert-only table; one row per compaction event; columns `id`, `session_id`, `compacted_at`, `high_water`. Owned by crt-054. |
| **`compacted_at`** | Unix timestamp in **seconds** of a compaction event. The gate boundary crt-055's `compaction_reread` reckoning compares PostToolUse read `ts` against. |
| **`high_water`** | `TranscriptBuffer.high_water` (`session_transcript.rs:52`, invariant I3 — monotonic bytes *sent*) captured server-side at the moment of compaction. **Reserved**: populated in v1 for a future precise byte-boundary gate; crt-054 reads it at the handler, never on the wire (vnc-036 shelved). Server-captured, not wire-precise. |
| **Transcript delta** | A streamed increment of transcript content merged into the session's `TranscriptBuffer` at `apply_delta` (`session_transcript.rs:150`). The fold's unit of accumulation. |
| **The fold** | Surface B's running, content-free accumulation over deltas at `apply_delta`: a monotonic byte sum, a delta count, and per-class match counts. A counter — never a query over the assembled buffer. |
| **`activity_snapshot()`** | The single metadata-only read surface returning a `Copy` `ActivitySnapshot` counter struct. crt-054 produces it; crt-055 reads it at review. |
| **`ActivitySnapshot`** | The `Copy` struct returned by `activity_snapshot()`. Fields: `bytes_total: u64`, `delta_count: u32`, `class_counts: [u32; MAX_SIGNAL_CLASSES]`. Structurally bytes-free; metadata-only `Debug`; no `Display`. |
| **Registered route** | The delta route for a session whose buffer is live in the registry (`session.rs:400-401`). |
| **Held route** | The delta route for a drained session whose buffer rides crt-052 Wave B's hold (`session.rs:388-395`). A fold miss on this route is the **believable-zero trap** (#750 class). |
| **Signal class** | A configured behavioral signature (e.g. `error`, `refusal`) matched against delta bytes. Indexed by config order into `class_counts`. v1: `0 = error`, `1 = refusal`. |
| **`[transcript_signals]`** | The config table (sibling to `[retention]`) defining the signal classes: per-entry `{ class_name, pattern, enabled }`, `#[serde(default)]`, compiled once into one shared `RegexSet`, `validate()`-bounded. |
| **`MAX_SIGNAL_CLASSES`** | The fixed signal-class bound, **pinned at exactly 16** (not "≤ 16"). A shared compile-time constant that crosses the producer/consumer boundary via the `ActivitySnapshot.class_counts: [u32; MAX_SIGNAL_CLASSES]` array, so it must **equal** crt-055's constant exactly — crt-055 has fixed it at 16 (dsn-001 / #4591 precedent). Sizes the `class_counts` array. |
| **Cycle-declaration coverage** | The fold's reach: the drain→hold seam holds buffers only for sessions with a non-empty `feature_cycle`. An undeclared session purges at drain and its fold dies — correct fail-loud. crt-054 never fabricates a zero for it. |
| **Late-bind attribution** | The `compaction_events` row is written keyed by `session_id` regardless of declaration; attribution to a `feature_cycle` is resolved at review by crt-055 via the session→`feature_cycle` chain. |

### Entities & relationships

```
SessionState ──Arc──> TranscriptBuffer ──folds──> ActivitySnapshot   (Surface B, in-memory)
     │                       └─ high_water (I3, monotonic bytes sent)
     │
session_id (TEXT)
     │
     └─ compaction event @ handle_compact_payload ──INSERT──> compaction_events row   (Surface A, durable)
                                                              { id, session_id, compacted_at, high_water }

[transcript_signals] config ──compile once──> RegexSet ──one scan per delta──> class_counts
```

- A session produces **0..N** `compaction_events` rows (it may compact multiple times). Which `compacted_at` to gate on is a crt-055 reckoning detail, **not** a producer concern.
- A session produces **one** `ActivitySnapshot` (its buffer's accumulated counters), read once at review.
- Surface A (durable, written regardless of declaration) and Surface B (in-memory, alive only while the buffer is held) are **independent paths** — a compaction row exists even when the fold has died with an undeclared session.

---

## Functional Requirements

Each FR is testable. Verification methods are stated in Acceptance Criteria.

### Surface A — `compaction_events` table

- **FR-A1** — A new durable table `compaction_events` exists with columns: `id` INTEGER PRIMARY KEY, `session_id` TEXT NOT NULL, `compacted_at` INTEGER NOT NULL (Unix **seconds**), `high_water` INTEGER NOT NULL DEFAULT 0. An index exists on `session_id`.
- **FR-A2** — The table is **insert-only**: exactly one INSERT per compaction event; rows are never UPDATEd or DELETEd by crt-054.
- **FR-A3** — The INSERT is written at the authoritative seam `handle_compact_payload` (`uds/listener.rs:1737`), co-located with the existing `increment_compaction` call.
- **FR-A4** — `compacted_at` is written as Unix **seconds**. `high_water` is written from `TranscriptBuffer.high_water` captured at the moment of compaction.
- **FR-A5** — The row carries **no `feature_cycle` column** and no content/payload; attribution is deferred to review (late-bind). The row is written **regardless of whether the session declared a `feature_cycle`**.
- **FR-A6** — The INSERT does not deadlock against, or stall, the locks held at `handle_compact_payload` (registry/session locks); it does not block the compaction acknowledgement on a hot path beyond what the design seam permits.
- **FR-A7** — crt-054 takes the **next `CURRENT_SCHEMA_VERSION` bump** (`migration.rs:22`, currently 28 → 29) for the new table only. Standard bump hygiene: fresh-create in `db.rs` includes the table; the migration upgrade block adds it; existence-guarded via `pragma_table_info`/table existence; cascade-file existence verified (#4484). crt-054 does **not** bump `SUMMARY_SCHEMA_VERSION` and does **not** ALTER `cycle_review_index`.

### Surface B — `activity_snapshot()` in-memory fold

- **FR-B1** — A running, content-free fold accumulates over transcript deltas at the merge boundary `apply_delta` (`session_transcript.rs:150`).
- **FR-B2** — The fold runs on **both** the registered route (`session.rs:400-401`) **and** the held-delta route (`session.rs:388-395`). No route bypasses the fold.
- **FR-B3** — Accumulation rules: `bytes_total` is the monotonic sum of each delta's payload byte length; `delta_count` increments by 1 per delta merged; `class_counts[i]` increments per delta per matched class from a single shared scan (a delta may match multiple classes).
- **FR-B4** — A single metadata-only method `activity_snapshot()` returns a `Copy` `ActivitySnapshot { bytes_total: u64, delta_count: u32, class_counts: [u32; MAX_SIGNAL_CLASSES] }`. The struct is structurally incapable of carrying transcript bytes (no `Vec<u8>`/`String`/`&[u8]` content field).
- **FR-B5** — Class matching uses **one shared `RegexSet`/Aho-Corasick scan per delta** (one byte scan, not one pass per pattern), compiled once at config load.
- **FR-B6** — The fold introduces **no new lock**; it accumulates under the buffer lock already held for the delta merge.
- **FR-B7** — The fold and its snapshot store **no content**: metadata-only `Debug`, no `Display`. (ADR-002 content-opacity, #4740.)
- **FR-B8** — crt-054 **never persists** the fold: it writes no column and no row from Surface B. The counter remains accurate and readable until crt-055 reads `activity_snapshot()` at review.
- **FR-B9** — crt-054 does **not** zero, drop, or reset the fold before the crt-052 hold purge; the counter rides the hold to review.
- **FR-B10** — For an undeclared/purged session whose fold has died, crt-054 emits **no fabricated zero** — absence is not a measured zero. (crt-055 surfaces absence via its `raw_signals_available`-style flag.)

### `[transcript_signals]` configuration

- **FR-C1** — A `[transcript_signals]` config table exists, sibling to `[retention]`, with per-entry `{ class_name, pattern, enabled }` and `#[serde(default)]`.
- **FR-C2** — A small, **domain-neutral** default set ships with v1: behavioral signatures only (model refusal phrasings, provider hard/overload errors) — **never SDLC literals**. v1 default classes: **`error` (index 0) and `refusal` (index 1) only** — high-precision, anchored patterns. No `reread` class, no `compaction` class.
- **FR-C2a** (delivery-time calibration) — Because the fold is content-opaque (ADR-005), the false-positive rate of these counts can never be audited after ship — there is no stored text to inspect. The default `error`/`refusal` patterns MUST therefore be **calibrated against real transcripts during delivery before locking**, kept minimal (under-catalog; domains extend via config), and the resulting counts treated as **directional, not precise**. (ADR-002.)
- **FR-C3** — `validate()` enforces `MAX_SIGNAL_CLASSES` (number of enabled classes ≤ `MAX_SIGNAL_CLASSES`) and rejects an invalid regex **loudly** at load (dsn-001 / #4591 precedent). The patterns compile once into the single `RegexSet`.
- **FR-C4** — Class-to-index mapping follows config order and is stable for the configured set, so `class_counts[0] = error`, `class_counts[1] = refusal` for the v1 default.

---

## Non-Functional Requirements

- **NFR-1 (content-opacity)** — No content field escapes either surface. Every produced signal is a counter (running fold over deltas) or a discrete server-seam event — never a query over the assembled transcript buffer. Metadata-only `Debug`, no `Display`, on `ActivitySnapshot`. (R-A guardrail; ADR-002 #4740.)
- **NFR-2 (honest unit)** — Throughput is measured in **bytes**. No token estimate, no token-named field (`token_bytes_per_unit` explicitly excluded), no cost surface. (RQ-8; SR-06.)
- **NFR-3 (fold performance)** — The fold is O(bytes) per delta, allocation-free, single scan per delta, under the already-held buffer lock; it adds no new lock and no measurable extra contention on the delta merge path.
- **NFR-4 (write-seam safety)** — The Surface A INSERT must not deadlock or materially stall the compaction hot path. Lock ordering against registry/session locks at the seam is confirmed in design. (SR-01.)
- **NFR-5 (no truncation; producer-clean width)** — `activity_snapshot()` returns `bytes_total: u64` and `delta_count: u32` (and `class_counts: [u32; …]`) with **no casts on the producer side** — crt-054 emits the native unsigned widths cast-free. The checked/saturating `u64`/`u32` → `i64` conversion happens **at persist on crt-055's side**, not in crt-054. The producer guarantees the values are honest unsigned counters; the consumer owns the narrowing. (SR-03.)
- **NFR-6 (config bound)** — `MAX_SIGNAL_CLASSES` is **pinned at exactly 16** (not "≤ 16") — it must equal crt-055's compile-time constant because the `class_counts: [u32; MAX_SIGNAL_CLASSES]` array crosses the producer/consumer boundary. The fixed-size array keeps `ActivitySnapshot` `Copy` and small. (ADR-002; resolves Open Q2.)
- **NFR-7 (durability dependency)** — Fold survival-to-review rests on crt-052 Wave B being ON by default, unconditional, and non-disableable. crt-054 depends on this and must fail loudly if it regresses (a config that re-enables purge-before-read breaks the fold). (SR-08; ADR-010 #5008.)
- **NFR-8 (schema-version sequencing)** — crt-054's table migration takes a version number assigned by merge order relative to crt-055 (first-merged is 29, second retroactively 30). Both migrate **different tables**, so there is no collision on table content, but the version number must be reconciled at merge. (SR-04.)
- **NFR-9 (no single-event dependence)** — No crt-054 output may depend on the retired `PreToolUse` event or any single hook-event presence that can vanish under a client change. (Never reintroduce the #750 class; SR-02/SR-05.)

---

## Acceptance Criteria

Each AC has an ID, ties to the SCOPE binding constraints / SR-XX risks, and a verification method.

| AC-ID | Criterion | Ties to | Verification method |
|-------|-----------|---------|---------------------|
| **AC-01** | `compaction_events` table is created with the exact contract columns/types (`id` PK, `session_id` TEXT NOT NULL, `compacted_at` INTEGER NOT NULL, `high_water` INTEGER NOT NULL DEFAULT 0) and an index on `session_id`; present in both fresh-create and the migration upgrade path. | FR-A1, FR-A7; SCOPE §Migration | Schema/migration test: fresh DB and an upgraded-from-v28 DB both expose the table via `pragma_table_info`; index present; cascade-file existence asserted (#4484). |
| **AC-01a** | `compaction_events.compacted_at` is documented as Unix **seconds** explicitly in the DDL/migration (a schema comment naming "Unix SECONDS"), so no consumer mis-reads it as millis. | FR-A1, FR-A4; ADR-007 | Inspect the DDL/migration source: assert the `compacted_at` column carries an explicit "Unix SECONDS" comment in both the fresh-create DDL and the migration upgrade block. The PostToolUse `ts/1000` normalization at the gate is **crt-055's**, not crt-054's — out of scope here. |
| **AC-02** | Exactly one row is inserted per compaction event at `handle_compact_payload`, with `compacted_at` in Unix seconds and `high_water` equal to the buffer's `high_water` at compaction; no UPDATE/DELETE path exists. | FR-A2, FR-A3, FR-A4; Constraint 1 | Integration test: drive a compaction through the handler; assert one new row with correct `session_id`, `compacted_at` (seconds, within tolerance of now) and `high_water`; assert a second compaction adds a second row (0..N). |
| **AC-03** | The `compaction_events` row is written even when the session has **no** `feature_cycle` declared, and carries no `feature_cycle` column and no content. | FR-A5; Constraint 4; SR-09 | Integration test: compact an **undeclared** session; assert a row is written and is session-keyed; assert the schema has no `feature_cycle`/content column. |
| **AC-04** | The Surface A INSERT does not deadlock or stall against the locks held at `handle_compact_payload`; the compaction acknowledgement completes. | FR-A6, NFR-4; Constraint 5; SR-01 | Lock-ordering review documented in architecture + a concurrency test driving compaction under registry/session lock contention without deadlock/timeout. |
| **AC-04a** | A failed `compaction_events` INSERT emits a **named metric/counter** (e.g. `compaction_events_insert_failed`), not a generic log line, and the compaction proceeds **non-blocking** (the ACK is never blocked by INSERT failure). | FR-A6, NFR-4; ADR-007 | Fault-injection test: force an INSERT failure at the seam; assert the named counter increments (not merely a log emitted) and the compaction response still completes; assert no content is logged. |
| **AC-05** | The fold accumulates `bytes_total`, `delta_count`, and `class_counts` at `apply_delta` on the **registered** route. | FR-B1, FR-B3; Constraint 1 | Unit/integration test: apply deltas on a registered session; assert counters advance as specified. |
| **AC-06** | The fold accumulates on the **held** route: a representative TS-client cycle driven through drain→hold→re-adopt yields `bytes_total > 0`, `delta_count > 0` at review — a held-route miss fails this test red, not silently zero. | FR-B2; Constraint 2; SR-02; ADR-009 #5007 | **Mandatory regression guard** (held-route, non-empty-source): multi-turn/multi-session cycle through the HELD route with non-trivial bytes; assert non-empty fold at review. A registered-only or no-op test does **not** satisfy this AC (pattern #3624). |
| **AC-07** | The fold counter remains accurate and readable until crt-055 reads `activity_snapshot()`; crt-054 does not zero/drop it before the crt-052 hold purge. The read observes non-zero counters and the buffers are zeroed only **after** purge. | FR-B8, FR-B9; Constraint 3; SR-08; ADR-009 #5007 | Read-before-purge integration test: assert `activity_snapshot()` returns non-zero, then assert `purge_cycle_transcripts` zeroes the buffer — i.e. the read happens first. |
| **AC-08** | `activity_snapshot()` returns a `Copy` `ActivitySnapshot { bytes_total: u64, delta_count: u32, class_counts: [u32; MAX_SIGNAL_CLASSES] }` with no content field; `Debug` prints only the scalars; no `Display` impl exists. | FR-B4, FR-B7, NFR-1; Constraint 1 | Type/structural test + a content-opacity test asserting no byte-bearing field and metadata-only `Debug` (mirrors `test_candidates_structurally_absent`-style guard). |
| **AC-09** | One shared `RegexSet`/Aho-Corasick scan runs per delta (not one pass per pattern); a single delta matching multiple classes increments multiple `class_counts`. | FR-B3, FR-B5, NFR-3 | Unit test: a crafted delta matching both `error` and `refusal` increments `class_counts[0]` and `class_counts[1]`; scan invoked once per delta. |
| **AC-10** | `[transcript_signals]` config parses with `#[serde(default)]`, ships the domain-neutral `error`/`refusal` default set (only these two; no SDLC literals, no `reread`/`compaction` class), and maps `error→0`, `refusal→1`. | FR-C1, FR-C2, FR-C4 | Config test: default config yields exactly the two v1 classes at the fixed indices; assert no SDLC literal patterns and no `reread`/`compaction` class. |
| **AC-10a** | The default `error`/`refusal` patterns are calibrated against real transcripts before locking (high-precision, anchored); the counts are documented as directional, not precise. | FR-C2a; ADR-002 | Delivery-time check: a calibration sample of real transcript deltas exercises each default pattern; the precision/false-positive observations are recorded in the delivery artifact and the pattern set finalized before merge. Spec/doc review asserts counts are surfaced as directional. |
| **AC-11** | `MAX_SIGNAL_CLASSES == 16` exactly (matching crt-055's constant), and `validate()` rejects, loudly at load, a config exceeding `MAX_SIGNAL_CLASSES` or containing an invalid regex. | FR-C3, NFR-6; SR-05 | Constant assertion (`MAX_SIGNAL_CLASSES == 16`); negative config tests: a >16-class set and an unparseable regex each fail `validate()` with a clear error; no silent fallback. |
| **AC-12** | For an undeclared/purged session whose fold died, crt-054 emits no fabricated zero; absence is distinguishable from a measured zero at the producer boundary. | FR-B10; Constraint 4; SR-09 | Integration test: an undeclared session contributes no entry to the activity read set; assert its bytes do **not** appear and no zero is fabricated on its behalf. |
| **AC-13** | No crt-054 output depends on the retired `PreToolUse` event or any single hook-event presence; both outputs derive from the delta stream (Surface B) or the server-authoritative compaction seam (Surface A). | NFR-9; Constraint 6; SR-02 | Design/code review asserting neither surface reads `PreToolUse`/single-hook presence; covered transitively by AC-06's route coverage. |
| **AC-14** | `activity_snapshot()` returns native `u64`/`u32` widths **cast-free on the producer side** — crt-054 performs no `as i64` / narrowing cast on these counters. The checked/saturating `→ i64` conversion is crt-055's, at persist. | NFR-5; SR-03 | Producer-side structural/grep test asserting no narrowing cast of `bytes_total`/`delta_count`/`class_counts` in crt-054's code; the snapshot returns `u64`/`u32`. (The boundary checked/saturating conversion is verified on crt-055's side.) |
| **AC-15** | crt-054 does **not** bump `SUMMARY_SCHEMA_VERSION`, does **not** ALTER `cycle_review_index`, and produces no token-named field or `reread`/`compaction` class. | FR-A7, NFR-2; SR-05, SR-06; SCOPE §Out-of-scope | Negative/structural review + grep-level test that crt-054's diff touches neither `cycle_review_index` nor `SUMMARY_SCHEMA_VERSION` and introduces no token-named symbol. |
| **AC-16** (co-owned with crt-055) | Across the producer→consumer seam, the seconds-gate semantics hold: a PostToolUse read taken just **after** a compaction boundary counts as **post-compaction**, and one taken just **before** counts as **pre-compaction**, when crt-055 compares the normalized read `ts` against crt-054's `compaction_events.compacted_at` (seconds). | FR-A4; ADR-007; crt-055 Binding constraint 8 | **Co-owned cross-gate integration test** (crt-054 produces rows in seconds; crt-055 owns the `ts/1000` normalization and the `read_ts_secs > compacted_at` test): drive a compaction, then assert a read with `ts` just past the boundary is classified post-compaction and a read just before is classified pre. Ownership noted in both feature artifacts. |

---

## User / Agent Workflows

crt-054 has no human-facing surface; its "users" are the server runtime and the crt-055 consumer.

1. **Compaction write (Surface A).** A client triggers compaction → `handle_compact_payload` runs server-side → co-located with `increment_compaction`, crt-054 INSERTs one `compaction_events` row `{ session_id, compacted_at (seconds), high_water }`. Written regardless of declaration. No further action; the row is durable.
2. **Delta fold (Surface B).** As transcript deltas stream in, each `apply_delta` (registered **or** held route) folds the delta into the buffer's counters: byte length summed, count incremented, one `RegexSet` scan updating `class_counts`. No persistence, no I/O.
3. **Review read (consumed by crt-055).** At cycle review crt-055 calls `activity_snapshot()` over the cycle's held sessions (before the crt-052 hold purge) and queries `compaction_events` by `session_id`, resolving attribution via the session→`feature_cycle` chain. crt-054 does no reckoning, no summing into columns, no reload computation.

---

## Constraints (binding — from SCOPE §"Binding constraints")

1. **R-A guardrail / content-opacity** — every signal is a running fold (counter) or a discrete server-seam event, never a query over the assembled buffer; no content field escapes; metadata-only `Debug`/no `Display`. (NFR-1, AC-08.)
2. **Held-route coverage** — the fold MUST run on both registered and held routes; a regression guard asserts a non-empty source for a representative TS-client cycle. (FR-B2, AC-06; SR-02.)
3. **Fold survives to review** — the counter stays accurate/readable until crt-055's read; crt-054 must not zero/drop it before the hold purge. (FR-B8/B9, AC-07; SR-08.)
4. **Counter coverage = cycle-declaration coverage; late-bind attribution honesty** — undeclared sessions purge at drain and their fold dies (correct fail-loud); crt-054 never fabricates a zero. The `compaction_events` row, session-keyed at the handler, is written regardless of declaration and attributed at review. (FR-A5, FR-B10, AC-03/AC-12.)
5. **Lock ordering at the write seam** — the Surface A INSERT must not deadlock against registry/session locks at `handle_compact_payload`. (NFR-4, AC-04; SR-01.)
6. **Never reintroduce the #750 class** — no output depends on a single hook-event presence (retired `PreToolUse`) that can vanish under a client change. (NFR-9, AC-13; SR-02.)
7. **Bytes-only honest unit** — no token estimate, no token-named field, no cost surface. (NFR-2, AC-15; SR-06.)
8. **Schema-version sequencing** — crt-054 takes the next `CURRENT_SCHEMA_VERSION` bump on a NEW table only; the number reconciles with crt-055 by merge order. (NFR-8, AC-01; SR-04.)

---

## Dependencies

- **crt-055 (#755) — binding producer contract.** `crt-055/SCOPE.md` §"Producer contract" fixes every field crt-054 writes. Any change is negotiated there first. (SR-07.)
- **crt-052 Wave B** — the transcript hold (ON by default, unconditional, non-disableable; `main.rs:698-718, 1234-1254`; `config.rs validate()` forbids `transcript_hold_max_sessions=0`). Fold survival-to-review rests on it. (NFR-7; ADR-010 #5008.)
- **#758 / #750 — MERGED** (`7aca6c44`). Provides live cross-session `context_reload`, the guarded-recompute logic, and `SUMMARY_SCHEMA_VERSION = 4`. crt-054 coexists by **not touching** `cycle_review_index` or `SUMMARY_SCHEMA_VERSION`.
- **Compaction seam** — `handle_compact_payload` (`uds/listener.rs:1737`) + `increment_compaction` (`infra/session.rs:554-559`); `SessionState.transcript` Arc gives buffer + `high_water` access.
- **TranscriptBuffer** — `infra/session_transcript.rs`: `high_water` (`:52`, I3 monotonic bytes sent), `apply_delta` (`:150`, fold merge boundary). Content-opacity already enforced (no `Display`, metadata-only `Debug`).
- **Crates / facilities** — Rust workspace store/server; SQLite migration (`migration.rs:22`, `db.rs`); a `RegexSet`/Aho-Corasick scanner for class matching; config (`config.rs validate()`).
- **Stale knowledge to correct in Phase 2** — ADR-008 (#5006) wrongly claims crt-054 owns v4/v29 on `cycle_review_index`; the architect `context_correct`s it to "crt-055 owns `SUMMARY_SCHEMA_VERSION` 5 + the `cycle_review_index` migration; crt-054 owns the `compaction_events` table only; #758 owns 4." Prior crt-054 ADR-001/004/009 (#4999/#5002/#5007) carry residue from the wider scope (e.g. `saw_compaction`/`reload_after_compaction` latches on the snapshot, a `[u32;16]` literal) — the `ActivitySnapshot` shape in this spec (`bytes_total`, `delta_count`, `class_counts[MAX_SIGNAL_CLASSES]`, no latches) follows the binding contract and supersedes that residue; the architect regenerates those ADRs. (SR-05.)

---

## NOT in Scope (explicit exclusions)

- **The entire `cycle_review_index` surface** — columns, `store_cycle_review` / `build_cycle_review_record`, `CycleReviewRecord`, the four-success-returns coexistence, the #758 guarded-recompute reconciliation. **All owned by crt-055.**
- **`SUMMARY_SCHEMA_VERSION`** — crt-055 owns the 4→5 bump. crt-054 bumps only `CURRENT_SCHEMA_VERSION` for its own new table.
- **Reload of any kind** — cross-session `context_reload` (#758) and the compaction-gated `compaction_reread` reckoning (crt-055 at review over crt-054's rows). crt-054 computes no reload, no overlap, no review-time aggregate.
- **`compaction_reread` boundary selection** (which `compacted_at` to gate on across multiple compactions) — a crt-055 reckoning detail.
- **Any token estimate or token-named field** (`token_bytes_per_unit` included). Bytes is the honest unit; token/cost is a separate harness-usage-stream feature.
- **A precise per-compaction byte boundary on the wire (vnc-036)** — SHELVED. `high_water` is captured server-side at the handler; no wire/client change.
- **No `saw_compaction` / `reload_after_compaction` latch** on `ActivitySnapshot` (prior-scope residue) and **no `reread`/`compaction` regex class** (no in-stream marker; compaction comes from Surface A).
- **Deferred signals (ass-078)** — turn-size percentiles, thrash/rolling-hash, per-delta entropy, language/code-fence detection, mean-turn-size. Revisit with measured evidence only.
- **Orchestration / FinOps surfaces.** No budget enforcement, cost dashboards, scheduling-by-cost. ("Not an orchestration engine.")

---

## Open Questions

All four prior open questions are **RESOLVED** (architecture + producer contract finalized 2026-06-16):

1. **Lock ordering + transaction shape — RESOLVED (ADR-007).** The INSERT is placed at `listener.rs:1854`, after `increment_compaction` returns and the buffer-tail guard has dropped; no registry/session/buffer lock is held across it. Single autocommit INSERT, no explicit transaction. On INSERT failure it logs ids/counts (no content), increments a named failure counter, and lets the compaction ACK proceed (non-blocking). (FR-A6, AC-04/AC-04a.)
2. **`MAX_SIGNAL_CLASSES` + default catalog — RESOLVED (ADR-002).** `MAX_SIGNAL_CLASSES = 16`, pinned exactly equal to crt-055's constant (it crosses the boundary via `ActivitySnapshot.class_counts`). Default catalog = `error` + `refusal` only, high-precision anchored patterns, calibrated against real transcripts before locking; counts are directional, not precise. (NFR-6, FR-C2/C2a, AC-10/10a/11.)
3. **Integer-width conversion — RESOLVED (producer-clean).** The producer side is cast-free: `activity_snapshot()` returns native `u64`/`u32`; the checked/saturating `→ i64` conversion is owned by crt-055 at persist. (NFR-5, AC-14.)
4. **Schema-version merge sequencing — RESOLVED (ADR-008 / SM coordination).** crt-054 takes the next `CURRENT_SCHEMA_VERSION` bump on its NEW table only; the number reconciles with crt-055 by merge order (first-merged 29, second retroactively 30). The two ALTER different tables — no collision. (NFR-8, AC-01.)

**Residual coordination item (not blocking):** AC-16 is **co-owned with crt-055** — the cross-gate seconds-semantics integration test must be claimed in exactly one feature's test plan with the other referencing it, so it is neither dropped nor duplicated. The SM resolves ownership at the producer/consumer test-plan handoff.

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — returned the updated crt-054 ADR-002 (#? signature catalog, `MAX_SIGNAL_CLASSES = 16` pinned, default `error`/`refusal` calibration), crt-055 ADR-008 (#5043 `[transcript_signals]` shape), crt-055 ADR-006 (#5048 compaction_reread boundary), crt-054 ADR-005 (#5030 never-persist content-opaque envelope), and ADR-010 (#5034 Wave B precondition). Findings: the four prior open questions are resolved by the updated ADR-002 / ADR-007 and crt-055's FINAL producer contract (§"Producer contract", Binding constraint 8 = canonical seconds gate). No new knowledge stored (read-only tier; spec decisions are feature-specific).
