# Gate 3a Report: crt-054

> Gate: 3a (Component Design Review)
> Date: 2026-06-16
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | 10 components map 1:1 to ARCHITECTURE §2; ADR-001..010 each cited at component level; codebase seams verified accurate. |
| 2. Specification coverage | PASS | Every FR-A1..A7 / FR-B1..B10 / FR-C1..C4 and NFR-1..9 lands in pseudocode; no scope additions. |
| 3. Risk coverage (test plans) | PASS | R-01..R-15 each have ≥1 named test; AC-06/AC-07 planned as held-route drain→hold→review integration tests with mandatory negative-mutation guards (pattern #3624). |
| 4. Interface consistency | PASS | Shared types in OVERVIEW.md used verbatim across components; producer contract widths/columns/indices match crt-055 §"Producer contract"; `MAX_SIGNAL_CLASSES == 16` matches crt-055. |
| 5. Knowledge stewardship compliance | PASS (1 WARN) | Architect has `Stored:`/context_correct entries; spec + risk-strategist + pseudocode have `Queried:` + reasoned no-store. Pseudocode WARN: MCP tools were deferred/unloadable in its thread (proceeded from source docs — non-blocking). |

## Detailed Findings

### 1. Architecture alignment
**Status**: PASS
**Evidence**: The pseudocode OVERVIEW lists 10 components mapping 1:1 to ARCHITECTURE §2 Component Breakdown (ActivityCounters, transcript_activity/SignatureScanner, apply_delta fold, activity_snapshot()+ActivitySnapshot, activity collector, compaction_events writer, table+migration, INSERT helper, config, Wave B precondition). Each component file cites the governing ADRs (e.g. apply-delta-fold.md → ADR-001/006/009; compaction-events-writer.md → ADR-007/004; migration → ADR-008; wave-b → ADR-010). Codebase anchors were independently verified: `TranscriptBuffer`/`high_water`/`apply_delta`/`clear`/`snapshot` (session_transcript.rs), `take_transcripts_for_feature` (:469), `increment_compaction` (:554), `handle_compact_payload` (:1737) with the `increment_compaction` call at :1854, `CURRENT_SCHEMA_VERSION = 28` (migration.rs:22), `create_tables_if_needed` (db.rs:534), `increment_counter` (counters.rs:62), `with_transcript_hold` on both main.rs paths (:716, :1252) — all exist exactly as the pseudocode states. Technology choices (`regex::bytes::RegexSet`, durable `counters` table for the failure counter, single autocommit INSERT) are consistent with the ADRs.

### 2. Specification coverage
**Status**: PASS
**Evidence**: Surface A — FR-A1/A4 (DDL columns + Unix-seconds comment) in compaction-events-migration.md; FR-A2/A3 insert-only at the seam + FR-A5 declaration-independent in compaction-events-writer.md; FR-A6 lock-safety + FR-A7 next-version-bump covered. Surface B — FR-B1/B3 (fold arithmetic), FR-B2 (both routes by construction), FR-B5 (one shared scan), FR-B4/B7 (Copy/content-opaque snapshot), FR-B6 (no new lock), FR-B8/B9 (never persist/zero), FR-B10 (no fabricated zero) each have a home. Config FR-C1..C4 + C2a in transcript-signals-config.md. NFR-1..9 all addressed (content-opacity structural, bytes-only, cast-free widths NFR-5, MAX_SIGNAL_CLASSES=16 NFR-6, Wave-B dependency NFR-7, schema sequencing NFR-8, no single-event NFR-9). No unrequested features: out-of-scope items (cycle_review_index, SUMMARY_SCHEMA_VERSION, reload reckoning, token fields, latch fields) are explicitly excluded and guarded by AC-15/R-12 tests.

### 3. Risk coverage (test plans)
**Status**: PASS
**Evidence**: OVERVIEW §2 maps every R-01..R-15 to ≥1 named test. The four Critical risks get the mandated coverage: R-01/AC-06 (`test_held_route_fold_nonempty_at_review` + `_continuity_across_drain` + `_negative_mutation_guard`, integration on crt-052/vnc-025 hold fixtures); R-02/AC-07 (`test_read_before_purge_ordering` + survival + no-zero-path, integration); R-03/AC-04 (`test_compaction_insert_under_lock_contention_no_deadlock` + guard-dropped-before-insert); R-04/AC-01 (fresh + v28→vNN upgrade + sqlite_parity). Integration and edge scenarios are present (drained-then-redelivered continuity, multi-compaction monotonic rows, undeclared session, poison→empty, empty-delta, near-u64::MAX). R-15 forced-failure named-counter test is mandatory and content-free. Risk priority is reflected: held-route/sequencing/lock/migration concentrate the heaviest (integration + negative-mutation) coverage.

### 4. Interface consistency
**Status**: PASS
**Evidence**: OVERVIEW "Shared types" block is the single source for `MAX_SIGNAL_CLASSES`, `ActivityCounters`, `ActivitySnapshot`, the `compaction_events` DDL, `TranscriptSignal`, and the failure-counter constant; component files reference rather than redefine them. Producer-contract conformance is explicit: ActivitySnapshot field set/widths/order (`bytes_total: u64`, `delta_count: u32`, `class_counts: [u32; MAX_SIGNAL_CLASSES]`) and the compaction_events columns match crt-055 §"Producer contract" verbatim; `error→0`, `refusal→1` index mapping is pinned in both config and scanner components. `MAX_SIGNAL_CLASSES = 16` was independently confirmed consistent with crt-055's ADR-008/SCOPE/ARCHITECTURE (value 16). Data flow across the two seams (Surface B ingest→review, Surface A handler→review) is coherent and matches ARCHITECTURE §3. No contradictions found between component files.

**Binding invariants — all verified satisfied:**
- Content-opacity (AC-08): ActivitySnapshot has no byte-bearing field, hand-written metadata-only Debug, no Display — explicit in activity-snapshot.md + activity-counters.md, structural test planned.
- `MAX_SIGNAL_CLASSES == 16` exactly (AC-11): pinned in OVERVIEW + Component 1/2/9; equals crt-055's; const-assertion test planned.
- Fold on BOTH routes via the same embedded accumulator (AC-06/AC-07): structural (accumulator inside the buffer, no route-specific call); AC-06/AC-07 planned as drain→hold→review INTEGRATION tests with mandatory negative-mutation guards — not registered-only/unit-only (pattern #3624 explicitly honored in apply-delta-fold.md + OVERVIEW §1).
- Surface A INSERT (AC-04/AC-04a/AC-01a): no registry/session/buffer lock across the INSERT; high_water captured then guard dropped (#3753); `compacted_at` in Unix SECONDS; named counter `compaction_events_insert_failed` — all explicit in compaction-events-writer.md + compaction-insert-helper.md.
- Cast-free producer widths u64/u32 (AC-14): producer-side no-narrowing-cast asserted; widening `usize→u64` of length explicitly distinguished from forbidden `→i64` narrowing.
- Next `CURRENT_SCHEMA_VERSION` bump for compaction_events ONLY (AC-15): does NOT touch cycle_review_index or SUMMARY_SCHEMA_VERSION; grep guards planned.
- AC-16 cross-gate ownership resolved: crt-054 physically lands the seconds-producer half (integration, not unit-only), references crt-055 for the `ts/1000` normalization half (OVERVIEW §5 + compaction-events-writer.md §AC-16).

### 5. Knowledge stewardship compliance
**Status**: PASS (1 WARN)
**Evidence**:
- Architect (active-storage tier): `## Knowledge Stewardship` present with `Stored:` entries (#5035 ADR-007 via context_store; #5027→#5049, #5035→#5050 via context_correct; #5006 reconciliation; #5000 deprecate) — compliant, correct update method used.
- Risk-strategist: no separate agent report file, but RISK-TEST-STRATEGY.md carries a full `## Knowledge Stewardship` block — `Queried:` (context_search, load-bearing #5025/#4799/#4095/#3753 etc.) + reasoned `Stored: nothing novel to store` with a steward-boundary rationale — compliant.
- Spec (read-only tier): `Queried:` + reasoned no-store — compliant.
- Pseudocode (read-only tier): `## Knowledge Stewardship` present with `Queried:` + no-store reason. **WARN**: the agent reports the Unimatrix MCP tools were deferred and not loadable in its thread (`ToolSearch` returned no match), so it proceeded from the three source docs + ADR references + direct codebase verification. Per spec this is explicitly non-blocking and the block is present with a reason; flagged as WARN, not a fail, because the query was attempted and the fallback was sound (codebase anchors all verified accurate).

## Rework Required

None.

## Scope Concerns

None. The producer-only re-scope is internally consistent, the producer contract with crt-055 is honored exactly (including the pinned constant and AC-16 ownership split), and every binding invariant is structurally enforced in the design. Stage 3b may proceed.
