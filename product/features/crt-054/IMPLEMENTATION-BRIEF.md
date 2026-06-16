# crt-054 — Implementation Brief

**Feature**: crt-054 — Transcript-fold producer: durable compaction-event table + in-memory throughput/signature fold
**Date**: 2026-06-16 — **regenerated after the four design open questions were RESOLVED** (rework). Supersedes the prior producer-only brief and the 2026-06-14 wider-scope brief.
**Phase**: Cortical (crt) — learning & drift · **Goal**: self-learning (#4677) · **Tracking**: GH Issue #752
**Binding contract**: `product/features/crt-055/SCOPE.md` §"Producer contract" — authoritative for every field crt-054 writes. On any conflict, the contract wins.
**Design status**: FINAL pending delivery. All four open questions resolved (ADR-007 / ADR-002 / ADR-008 + crt-055 final producer contract). Three non-blocking coordination items carried to delivery/SM (below).

> crt-054 is the **producer half** of a producer/consumer pair. It supplies exactly two raw inputs observable only at the ingest/server seam — Surface A (durable `compaction_events` table) and Surface B (in-memory `activity_snapshot()` fold) — plus the `[transcript_signals]` config that feeds B. crt-055 (#755) persists, gates, and surfaces them. crt-054 persists nothing of Surface B and touches neither `cycle_review_index` nor `SUMMARY_SCHEMA_VERSION`.

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | `product/features/crt-054/SCOPE.md` (read the Re-scope note 2026-06-16 first) |
| Scope Risk Assessment | `product/features/crt-054/SCOPE-RISK-ASSESSMENT.md` |
| Architecture | `product/features/crt-054/architecture/ARCHITECTURE.md` |
| Specification | `product/features/crt-054/specification/SPECIFICATION.md` |
| Risk-Based Test Strategy | `product/features/crt-054/RISK-TEST-STRATEGY.md` |
| Alignment Report | `product/features/crt-054/ALIGNMENT-REPORT.md` |
| Acceptance Map | `product/features/crt-054/ACCEPTANCE-MAP.md` |
| Binding producer contract | `product/features/crt-055/SCOPE.md` §"Producer contract" |
| Design input (fold-at-ingest) | `product/research/ass-078/FINDINGS.md` |
| Design input (faithful reload) | `product/research/ass-077/FINDINGS.md` |

### ADR Files

| ADR | Decision | File |
|-----|----------|------|
| ADR-001 | Fold inside `TranscriptBuffer`, folded at `apply_delta` on both routes | `architecture/ADR-001-fold-inside-transcript-buffer-both-routes.md` |
| ADR-002 | Behavioral-signature catalog: one shared `RegexSet`, `[transcript_signals]` config, `validate()`-bounded, `MAX_SIGNAL_CLASSES = 16` pinned, v1 = `error`/`refusal` only, calibrated in delivery | `architecture/ADR-002-signature-catalog-shared-regexset-config.md` |
| ADR-003 | `activity_snapshot()` — `Copy` counter struct, cast-free producer widths | `architecture/ADR-003-activity-snapshot-copy-struct.md` |
| ADR-004 | Late-bind cycle attribution; coverage = declaration coverage, never a fabricated zero | `architecture/ADR-004-late-bind-attribution-coverage-honesty.md` |
| ADR-005 | Never-persist envelope, content-opaque, no token-named field | `architecture/ADR-005-never-persist-envelope-content-opacity.md` |
| ADR-006 | crt-054's obligation is survival-to-review (never zero/drop before purge) | `architecture/ADR-006-fold-survives-to-review.md` |
| ADR-007 | Durable `compaction_events` table, insert-only at `handle_compact_payload`, single autocommit INSERT, no lock across it, named failure counter | `architecture/ADR-007-compaction-events-table-write-seam.md` |
| ADR-008 | crt-054 owns only `compaction_events` + the next `CURRENT_SCHEMA_VERSION` bump (29/30 by merge order); not `SUMMARY_SCHEMA_VERSION`, not `cycle_review_index` | `architecture/ADR-008-schema-version-ownership.md` |
| ADR-009 | Believable-zero regression guard — non-empty held-route fold + survival ordering | `architecture/ADR-009-believable-zero-regression-guard.md` |
| ADR-010 | crt-052 Wave B is a verified startup precondition (Surface B only) | `architecture/ADR-010-wave-b-verified-precondition.md` |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| `ActivityCounters` (fold accumulator, embedded in `TranscriptBuffer`) | pseudocode/activity-counters.md | test-plan/activity-counters.md |
| `transcript_activity` module + `SignatureScanner` | pseudocode/transcript-activity.md | test-plan/transcript-activity.md |
| `apply_delta` fold call (both routes) | pseudocode/apply-delta-fold.md | test-plan/apply-delta-fold.md |
| `activity_snapshot()` + `ActivitySnapshot` read surface | pseudocode/activity-snapshot.md | test-plan/activity-snapshot.md |
| activity collector (`activity_snapshots_for_feature`) | pseudocode/activity-collector.md | test-plan/activity-collector.md |
| `compaction_events` writer (at `handle_compact_payload`) | pseudocode/compaction-events-writer.md | test-plan/compaction-events-writer.md |
| `compaction_events` table + migration | pseudocode/compaction-events-migration.md | test-plan/compaction-events-migration.md |
| compaction-INSERT helper + failure counter (`store_ops`) | pseudocode/compaction-insert-helper.md | test-plan/compaction-insert-helper.md |
| `[transcript_signals]` config + `validate()` | pseudocode/transcript-signals-config.md | test-plan/transcript-signals-config.md |
| Wave B startup precondition assert | pseudocode/wave-b-precondition.md | test-plan/wave-b-precondition.md |

### Cross-Cutting Artifacts (confirmed — Stage 3a complete)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files produced in Stage 3a — all 10 components landed 1:1 with the Component Map filenames above, plus pseudocode/OVERVIEW.md and test-plan/OVERVIEW.md. AC-16 (cross-gate seconds boundary) physically owned by crt-054 as the seconds-producer half (test-plan/OVERVIEW.md §5), referencing crt-055 for the ts/1000 normalization half.

---

## Goal

Produce the two durable, content-free inputs that `context_cycle_review` needs and that no current source carries: a timestamped, session-keyed compaction event (Surface A — the gate boundary crt-055 needs for "reload after compaction") and an in-memory throughput/behavioral-signature fold over transcript deltas (Surface B — `bytes_total`, `delta_count`, `class_counts`). The fold **informs, never controls** (RQ-8); **bytes is the honest unit — never tokens, never cost**. crt-055 reads both and lands all columns; crt-054 supplies the raw inputs and nothing else.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Fold location & routing | Fold lives inside `TranscriptBuffer`, folded at `apply_delta`; both registered and held routes fold the same embedded accumulator by construction | ARCHITECTURE §2-3 | `architecture/ADR-001-fold-inside-transcript-buffer-both-routes.md` |
| Signature catalog mechanism | One shared `RegexSet`, externalized as `[transcript_signals]` config, `validate()`-bounded; one byte scan per delta | ARCHITECTURE §4 | `architecture/ADR-002-signature-catalog-shared-regexset-config.md` |
| **Q2 — `MAX_SIGNAL_CLASSES` value** | **RESOLVED: pinned at exactly 16** (not `≤ 16`), must EQUAL crt-055's constant (crosses the boundary via `ActivitySnapshot.class_counts`); crt-055 fixed it at 16 | ADR-002; SPEC NFR-6 | `architecture/ADR-002-signature-catalog-shared-regexset-config.md` |
| **Q2 — default catalog contents** | **RESOLVED: `error` (index 0) + `refusal` (index 1) only**, domain-neutral/high-precision/anchored; **calibrated against real transcripts during delivery before locking**; counts are **directional, not precise** | ADR-002; SPEC FR-C2/C2a | `architecture/ADR-002-signature-catalog-shared-regexset-config.md` |
| Snapshot shape | `ActivitySnapshot { bytes_total: u64, delta_count: u32, class_counts: [u32; MAX_SIGNAL_CLASSES] }`, `Copy`, no `Display`, metadata-only `Debug`, no latch fields | ARCHITECTURE §6 | `architecture/ADR-003-activity-snapshot-copy-struct.md` |
| **Q3 — integer widths** | **RESOLVED: producer side is cast-free** — `activity_snapshot()` returns native `u64`/`u32`; the checked/saturating `→ i64` conversion is crt-055's at persist | SPEC NFR-5; ADR-003 | `architecture/ADR-003-activity-snapshot-copy-struct.md` |
| Cycle attribution | Late-bind at review via the hold's `feature_cycle` filter; never fabricate a zero for an undeclared/purged session | ARCHITECTURE §3 | `architecture/ADR-004-late-bind-attribution-coverage-honesty.md` |
| Persistence envelope | crt-054 persists nothing of Surface B; structurally content-free; no token-named field anywhere | SCOPE §Out-of-scope | `architecture/ADR-005-never-persist-envelope-content-opacity.md` |
| Survival to review | Never zero/drop the counter independently; it rides the crt-052 hold; only `clear()`/purge (crt-055/crt-052-owned, after the review read) zeroes it | ADR-006 | `architecture/ADR-006-fold-survives-to-review.md` |
| **Q1 — lock ordering + transaction shape (Surface A INSERT)** | **RESOLVED: INSERT at `listener.rs:1854` after `increment_compaction` returns and the buffer-tail guard dropped; no registry/session/buffer lock held across it. Single autocommit INSERT helper on `store_ops`, no explicit transaction. Non-blocking on the hot path** | ADR-007 | `architecture/ADR-007-compaction-events-table-write-seam.md` |
| **Q1 — INSERT-failure observability** | **RESOLVED: on INSERT failure emit a named counter `compaction_events_insert_failed` (not a generic log), log ids/counts only, and let the compaction ACK proceed; missing row = fail-loud absence at crt-055's review** | ADR-007 §6; RISK R-15 | `architecture/ADR-007-compaction-events-table-write-seam.md` |
| **Q3 — `compacted_at` unit** | **RESOLVED: Unix SECONDS (server wall clock), documented explicitly in the DDL/migration comment. The PostToolUse `ts/1000` normalization at the gate is crt-055's, not crt-054's** | ADR-007 §4; SPEC AC-01a | `architecture/ADR-007-compaction-events-table-write-seam.md` |
| `high_water` semantics | Server-captured at the handler under the buffer lock (guard then dropped); populated now (reserved for future precise byte-boundary gating); not wire-precise (vnc-036 shelved) | ADR-007 | `architecture/ADR-007-compaction-events-table-write-seam.md` |
| **Q4 — schema version ownership** | **RESOLVED: crt-054 takes the next `CURRENT_SCHEMA_VERSION` bump for `compaction_events` ONLY (28 → 29/30); does NOT bump `SUMMARY_SCHEMA_VERSION` and does NOT ALTER `cycle_review_index`. 29-vs-30 is set by merge order at the SM gate** | ADR-008 | `architecture/ADR-008-schema-version-ownership.md` |
| Believable-zero guard | Mandatory held-route, non-empty-source integration test + survival-to-review ordering test; negative-mutation check required | ADR-009 | `architecture/ADR-009-believable-zero-regression-guard.md` |
| Wave B dependency | Verified startup precondition (fail-loud if the `HeldBufferScan` handle is unwired); guards Surface B only | ADR-010 | `architecture/ADR-010-wave-b-verified-precondition.md` |

**The four prior design open questions (Q1 lock-ordering/transaction shape, Q2 `MAX_SIGNAL_CLASSES` + default catalog, Q3 integer widths / `compacted_at` unit, Q4 schema-version sequencing) are ALL RESOLVED and folded into the sources above.** See the Coordination Items section for the three remaining non-blocking handoffs.

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/.../infra/transcript_activity.rs` | create | New sibling module: `ActivityCounters` fold logic + `SignatureScanner` (compiles the `RegexSet`, one byte scan per delta) |
| `crates/unimatrix-server/.../infra/session_transcript.rs` | modify | Embed the accumulator field + scanner in `TranscriptBuffer`; add fold call after the merge in `apply_delta` (`:150`); add `activity_snapshot()`; thread scanner through `TranscriptBuffer::new` construction sites; extend metadata-only `Debug` |
| `crates/unimatrix-server/.../infra/session.rs` | modify | Add `activity_snapshots_for_feature` collector mirroring `take_transcripts_for_feature` (dedup-by-`Arc`, registered ∪ held); held-route fold continuity (`:388-401`) |
| `crates/unimatrix-server/.../uds/listener.rs` | modify | Surface A write: at `:1854` after `increment_compaction`, capture `high_water` (guard dropped), INSERT one `compaction_events` row via the helper; named failure counter on error; non-blocking |
| `crates/unimatrix-store/.../migration.rs` | modify | Bump `CURRENT_SCHEMA_VERSION` (28 → 29/30); add `compaction_events` `CREATE TABLE IF NOT EXISTS` + index in the upgrade block under an `if current_version < N` guard; update the pinned-version assert; DDL comment names "Unix SECONDS" |
| `crates/unimatrix-store/.../db.rs` | modify | Add `compaction_events` (+ index) to the `create_tables_if_needed` fresh-create path; same DDL "Unix SECONDS" comment |
| `crates/unimatrix-store/.../` (store_ops) | modify | Add a thin single-statement autocommit INSERT helper for `compaction_events` (no explicit transaction); parameterized |
| `crates/unimatrix-server/.../config.rs` | modify | Add `[transcript_signals]` config (sibling to `[retention]`), `#[serde(default)]`; `validate()` enforces `MAX_SIGNAL_CLASSES`, rejects invalid regex + duplicate `class_name` loudly at load |
| `crates/unimatrix-server/.../main.rs` | modify | Wave B startup precondition assert (next to `RetentionConfig::validate()` / `[transcript_signals]` validate), on both server-construction paths (~700 and ~1236) |

(Exact crate paths confirmed during Stage 3a pseudocode.)

---

## Data Structures

```rust
// Pinned shared constant — MUST equal crt-055's exactly (crosses the boundary
// via ActivitySnapshot.class_counts). v1 indices: 0 = error, 1 = refusal.
const MAX_SIGNAL_CLASSES: usize = 16;

// In-memory fold accumulator, embedded inside TranscriptBuffer.
// Scalars only — derive(Debug, Clone, Copy)-safe; never a content field.
struct ActivityCounters {
    bytes_total: u64,
    delta_count: u32,
    class_counts: [u32; MAX_SIGNAL_CLASSES],
}

// Counters-only read surface returned by activity_snapshot().
// No Vec<u8>/String/&[u8]; no Display; metadata-only Debug.
#[derive(Clone, Copy)]
struct ActivitySnapshot {
    bytes_total: u64,
    delta_count: u32,
    class_counts: [u32; MAX_SIGNAL_CLASSES],
}

// compaction_events table (Surface A) — content-free, insert-only:
//   id           INTEGER PRIMARY KEY
//   session_id   TEXT    NOT NULL
//   compacted_at INTEGER NOT NULL   -- Unix SECONDS (DDL comment documents the unit)
//   high_water   INTEGER NOT NULL DEFAULT 0
//   INDEX idx_compaction_events_session ON compaction_events(session_id)

// [transcript_signals] config entry — sibling to [retention], #[serde(default)]:
struct TranscriptSignal {
    class_name: String,
    pattern: String,
    enabled: bool,
}
```

---

## Function Signatures

```rust
// Fold call, added after the merge in apply_delta (both routes fold the same accumulator):
pub fn apply_delta(&mut self, offset: u64, bytes: &[u8]); // existing; gains self.activity.fold(bytes, &self.scanner)

// Counters-only read surface on TranscriptBuffer; poison -> empty (#4764):
pub fn activity_snapshot(&self) -> ActivitySnapshot;

// Registry-level collector mirroring take_transcripts_for_feature (registered ∪ held, dedup by Arc):
fn activity_snapshots_for_feature(&self, feature_cycle: &str) -> Vec<(String, ActivitySnapshot)>;

// Existing seam call the Surface A INSERT is co-located after:
pub fn increment_compaction(&self, session_id: &str); // session.rs:554-559 (existing)

// Thin single-statement autocommit INSERT helper on store_ops (no explicit transaction);
// on failure: increment the named counter `compaction_events_insert_failed`, log ids/counts only, return Err.
fn insert_compaction_event(&self, session_id: &str, compacted_at_secs: i64, high_water: i64) -> Result<()>;

// Config validation — loud at load:
fn validate(&self) -> Result<()>; // rejects > MAX_SIGNAL_CLASSES, invalid regex, duplicate class_name
```

---

## Constraints (binding — from SCOPE / SPEC)

1. **R-A guardrail / content-opacity** — every signal is a running fold (counter) or a discrete server-seam event, never a query over the assembled buffer; no content field escapes; metadata-only `Debug`, no `Display`.
2. **Held-route coverage** — the fold MUST run on both routes; a mandatory regression guard asserts a non-empty source for a representative TS-client cycle (drain→hold→re-adopt), with a negative-mutation check so it cannot degrade to a no-op.
3. **Fold survives to review** — never zero/drop the counter before the crt-052 hold purge; it rides the hold to crt-055's read.
4. **Coverage = cycle-declaration coverage; never fabricate a zero** — undeclared sessions purge at drain and their fold dies (correct fail-loud). The Surface A row is written regardless of declaration (session-keyed, attributed at review).
5. **Lock ordering at the write seam** — the Surface A INSERT acquires only the DB connection; no registry/session/buffer lock held across it; `high_water` captured then guard dropped (ADR-007).
6. **Never reintroduce the #750 class** — no output depends on the retired `PreToolUse` or any single hook-event presence.
7. **Bytes-only honest unit** — no token estimate, no token-named field, no cost surface.
8. **Schema-version sequencing** — `compaction_events` only; next `CURRENT_SCHEMA_VERSION` bump; reconcile 29/30 by merge order at the SM gate.

---

## Dependencies

- **crt-055 (#755)** — the binding producer contract and the consumer. Reads `activity_snapshot()` + `compaction_events`; lands all columns; owns the `ts/1000` gate normalization, the `compaction_reread` reckoning, `SUMMARY_SCHEMA_VERSION` 4→5, and the checked/saturating `→ i64` conversion at persist. Any field/width/catalog change is negotiated in its §"Producer contract" first.
- **crt-052 Wave B** — the transcript hold (ON by default, unconditional, non-disableable; `main.rs:698-718, 1234-1254`; `config.rs validate()` forbids `transcript_hold_max_sessions=0`). Surface B survival rests on it; asserted at startup (ADR-010). Surface A does not depend on it.
- **#758 / #750 — MERGED** (`7aca6c44`). Provides live cross-session `context_reload` and `SUMMARY_SCHEMA_VERSION = 4`. crt-054 coexists by not touching `cycle_review_index` or `SUMMARY_SCHEMA_VERSION`.
- **Crates / facilities** — Rust workspace (`unimatrix-server`, `unimatrix-store`); SQLite migration (`migration.rs:22`, `db.rs`); the `regex` crate (`RegexSet`, linear-time, no catastrophic backtracking) for signature matching; config (`config.rs validate()`).
- **Seams** — `handle_compact_payload` (`uds/listener.rs:1737`/`:1854`) + `increment_compaction` (`infra/session.rs:554-559`); `TranscriptBuffer` (`infra/session_transcript.rs`): `high_water` (`:53`, accessor `:333`), `apply_delta` (`:150`).
- **Stale knowledge corrected** — ADR-008 (#5006) corrected via `context_correct`; prior ADR-001/004/009 residue (snapshot latches, `[u32;16]` literal, `reread`/`compaction` classes, `token_bytes_per_unit`) regenerated against the new SCOPE, not edited.

---

## NOT in Scope

- The entire `cycle_review_index` surface — columns, `store_cycle_review`/`build_cycle_review_record`, `CycleReviewRecord`, the four-success-returns coexistence, the #758 guarded-recompute reconciliation. **All crt-055's.**
- `SUMMARY_SCHEMA_VERSION` (crt-055 owns 4→5; #758 owns 4). crt-054 bumps only `CURRENT_SCHEMA_VERSION` for its own new table.
- Reload of any kind — cross-session `context_reload` (#758) and the compaction-gated `compaction_reread` reckoning (crt-055 at review). No reload, no overlap, no review-time aggregate.
- The `compaction_reread` boundary selection across multiple compactions — a crt-055 reckoning detail.
- The gate-side `ts/1000` normalization and the `read_ts_secs > compacted_at` comparison — crt-055's (crt-055 Binding constraint 8).
- Any token estimate or token-named field (`token_bytes_per_unit` included). Bytes only.
- A precise per-compaction byte boundary on the wire (vnc-036) — SHELVED; `high_water` captured server-side, no wire/client change.
- No `saw_compaction`/`reload_after_compaction` latch on `ActivitySnapshot`; no `reread`/`compaction` regex class.
- Deferred ass-078 signals (turn-size percentiles, thrash/rolling-hash, entropy, language/code-fence, mean-turn-size).
- Orchestration / FinOps surfaces.

---

## Coordination Items (non-blocking — for delivery / SM)

These do not block delivery; they are handoffs to resolve during the delivery session.

1. **AC-16 cross-gate test physical ownership.** The seconds-gate boundary integration test is **co-owned with crt-055**. Exactly one feature's test plan must physically land it, with the other referencing it — so it is neither dropped nor duplicated. crt-054 owns the seconds-producer half (rows in seconds); crt-055 owns the `ts/1000` normalization + the `read_ts_secs > compacted_at` gate half. The SM resolves physical ownership at the producer/consumer test-plan handoff (Stage 3a).
2. **Schema version 29 vs 30 at merge.** crt-054 and crt-055 take distinct sequential `CURRENT_SCHEMA_VERSION` bumps on disjoint tables. Whichever merges first is 29; the other retroactively becomes 30. The second-merged feature updates its migration guard + pinned-version assert in one change. SM merge-order coordination point (`grep CURRENT_SCHEMA_VERSION migration.rs` before finalizing; lesson #4095).
3. **Default-catalog calibration during delivery.** The default `error`/`refusal` patterns must be calibrated against real transcripts before locking (content-opacity means their false-positive rate can never be audited post-ship). Keep minimal (under-catalog; domains extend via config); record the calibration sample/observations in the delivery artifact; surface the counts as directional. (AC-10a.)

---

## Alignment Status

ALIGNMENT-REPORT.md (2026-06-16, producer-only): **PASS 6, WARN 0, VARIANCE 0, FAIL 0.** No variances requiring approval.

- **Vision Alignment — PASS.** Advances self-learning (#4677) as a content-free knowledge surface; the RQ-8 "informs, never controls" boundary is held as a hard edge across all three docs and is structurally foreclosed (`ActivitySnapshot` cannot carry transcript bytes; bytes-only, no token field).
- **Milestone Fit — PASS.** Tight v1 boundary; ass-078 deferred signals and the vnc-036 wire change explicitly shelved to measured need.
- **Scope Gaps / Additions — PASS.** Both surfaces + config fully covered by FR/AC; no item dropped; `high_water`-populated and the Wave B startup assert are SCOPE-sanctioned, not new scope.
- **Architecture Consistency / Risk Completeness — PASS.** ADR index, surfaces, seams, and the producer/consumer split agree across docs; SR-01..SR-10 each map to ≥1 architecture risk, an ADR, and an AC; the believable-zero family is the correctly-prioritized Critical cluster.

No human-attention variances surfaced. The design is final pending delivery; the only residual items are the three non-blocking coordination handoffs above.
