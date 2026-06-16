# Gate 3b Report: crt-054

> Gate: 3b (Code Review)
> Date: 2026-06-16
> Result: PASS
> Agent: crt-054-gate-3b

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | All 10 components implement the validated pseudocode 1:1; no significant departures. |
| 2. Architecture compliance | PASS | ADR-001..010 honored; component boundaries + integration seams as designed; sibling module avoids 500-line growth of session_transcript.rs. |
| 3. Interface implementation | PASS | Producer contract (widths/columns/index/MAX_SIGNAL_CLASSES==16) matches crt-055 §"Producer contract" exactly; cast-free producer side. |
| 4. Test-case alignment | PASS | Every R-01..R-15 has ≥1 named test; AC-06/AC-07 structural basis present + correctly deferred to Stage 3c integration (not falsely claimed done). |
| 5. Code quality | PASS (2 WARN) | Build clean (rc=0). No stubs/TODO/unwrap introduced by crt-054. WARN: two pre-existing oversized files extended; one pre-existing unrelated eval flake. |
| 6. Security | PASS | Parameterized INSERT (no injection); content-opaque by construction; no hardcoded secrets; cargo audit clean; deserialization (serde config) validated loudly. |
| 7. Knowledge stewardship | PASS | All five impl-agent reports carry `## Knowledge Stewardship` with `Queried:` + `Stored:`/reasoned-no-store. |

## Binding Invariants — all verified in actual code

| Invariant (AC) | Result | Evidence |
|----------------|--------|----------|
| Content-opacity (AC-08) | PASS | `transcript_activity.rs`: `ActivitySnapshot` has only `bytes_total:u64`/`delta_count:u32`/`class_counts:[u32;16]` — no `Vec<u8>`/`String`/`&[u8]`; hand-written metadata-only `Debug` (:130); no `Display` (asserted absent :140). |
| MAX_SIGNAL_CLASSES == 16 (AC-11) | PASS | `const MAX_SIGNAL_CLASSES:usize = 16` (:32) + compile-time `const _:() = assert!(== 16)` (:36). |
| Fold on ACCEPTED path only | PASS | `session_transcript.rs apply_delta`: overflow `return` (:176) and below-floor clip `return` (:204) both precede the fold (:266); only zero-len accepted (:188) and post-merge (:266) fold. |
| clear() does NOT reset accumulator (ADR-006) | PASS | `clear()` (:351) preserves `self.activity` with explicit do-not-reset comment; no reset statement. |
| Accumulator embedded in TranscriptBuffer (ADR-001) | PASS | `activity: ActivityCounters` field (:64); both routes fold the same Arc-shared buffer by construction — no route-specific fold code. |
| Surface A INSERT lock discipline (AC-04/AC-04a/AC-01a) | PASS | `listener.rs ~:1856`: `high_water` read in tight `{ lock_buffer(..).high_water() }` block, guard dropped before INSERT; no registry/session/buffer lock across INSERT; `compacted_at` via `unix_now_secs() as i64` (SECONDS); on Err logs ids only + falls through (non-blocking); named counter `compaction_events_insert_failed` bumped in `store_ops::insert_compaction_event` (:305). |
| Cast-free producer widths (AC-14) | PASS | `activity_snapshot()` returns native `u64`/`u32` (no `as i64`); only widening `usize→u64` of `bytes.len()` in fold (documented as allowed). The `as i64` at the listener is the persist boundary, distinct from producer rule. |
| Schema version 28→29, compaction_events ONLY (AC-15) | PASS | `migration.rs CURRENT_SCHEMA_VERSION = 29`; v28-block intra-stamps 28 then v29 block runs; diff touches neither `cycle_review_index` nor `SUMMARY_SCHEMA_VERSION`; no `token_*` symbol; no `reread`/`compaction` regex class — all only-in-comments matches. |
| "Unix SECONDS" comment in BOTH paths | PASS | `db.rs:1053` and `migration.rs` v29 block both carry `-- Unix SECONDS (NOT millis)`; DDL byte-identical. |
| Wave B precondition on BOTH paths (ADR-010) | PASS | `main.rs assert_wave_b_precondition` called on daemon (~:782) and stdio (~:1329) via shared helper; checks `has_transcript_hold()` + `transcript_hold_max_sessions != 0`; fails loud. |
| Default catalog error(0)/refusal(1) (AC-10/AC-10a) | PASS | `config.rs:2143` exactly two classes, fixed order, domain-neutral, anchored bytes-domain patterns; `#[serde(default)]`; calibration in `testing/CALIBRATION.md`. |

## Detailed Findings

### 1. Pseudocode fidelity — PASS
Component-by-component cross-check (delegated, independently confirmed against code): `ActivityCounters`/`ActivitySnapshot`/`SignatureScanner` (transcript_activity.rs), the `apply_delta` fold + `clear` preservation + `activity_snapshot()` (session_transcript.rs), the two-phase `activity_snapshots_for_feature` collector (session.rs), the v28→v29 migration + fresh-create DDL (migration.rs/db.rs), the single-autocommit `insert_compaction_event` (write_ext.rs), the `handle_compact_payload` Surface A writer + `store_ops` failure-counter wrapper (listener.rs/store_ops.rs), `TranscriptSignalsConfig`+`validate()` (config.rs), and `assert_wave_b_precondition`+`build_signature_scanner` (main.rs) all follow their pseudocode 1:1 (function names, steps, data shapes). No significant departures.

### 2. Architecture compliance — PASS
Sibling `transcript_activity.rs` (235 lines) exists precisely to avoid pushing the near-cap `session_transcript.rs` over 500. ADR-001 (fold inside buffer, both routes), ADR-002 (shared RegexSet from config), ADR-003 (Copy snapshot), ADR-004 (late-bind attribution), ADR-005 (content opacity), ADR-006 (fold survives clear), ADR-007 (compaction_events write seam + named failure counter via durable `counters` table), ADR-008 (schema-version ownership, intra-stamp on prior-last block), ADR-009 (believable-zero structural guard), ADR-010 (Wave B precondition both paths) are each reflected in code. Integration seam at `increment_compaction` (:1854) is preserved.

### 3. Interface implementation — PASS
`ActivitySnapshot` field set/widths/order and the `compaction_events` columns (`id`/`session_id`/`compacted_at`/`high_water`) + `idx_compaction_events_session` match crt-055 §"Producer contract" verbatim. `MAX_SIGNAL_CLASSES == 16` matches crt-055. Error handling uses the project error type with `.map_err()` context; loud `ConfigError`/`ScannerError`/`ServerError` on the config + scanner + precondition paths — no silent fallback.

### 4. Test-case alignment — PASS
Every R-01..R-15 has at least one named test. Confirmed load-bearing tests: const-assertion `test_max_signal_classes_is_exactly_16`; content-opacity `test_snapshot_debug_is_metadata_only` + `test_counters_and_snapshot_are_copy`; forced-INSERT-failure `test_insert_failure_increments_named_counter` (asserts the named counter, not a log); multi-class scan `test_scan_over_delta_matching_both_yields_zero_and_one`; near-u64::MAX `test_bytes_total_holds_large_value_un_narrowed`; migration `test_migration_v28_to_v29_adds_compaction_events` + `_idempotent` + `_columns_match_contract`; config rejects `test_config_over_cap_rejected`/`_invalid_regex_rejected`/`_duplicate_class_name_rejected`; AC-12 `test_collector_undeclared_session_no_entry` + `_absence_distinguishable_from_measured_zero`; AC-15 negatives `test_default_catalog_no_reread_or_compaction_class` + `_no_sdlc_literals`.

**AC-06 (held-route fold survival) and AC-07 (read-before-purge)** correctly carry UNIT structural basis here (`clear()` accumulator preservation: `test_activity_fold_continues_after_clear`) and are explicitly DEFERRED to Stage 3c as drain→hold→re-adopt INTEGRATION tests on cumulative crt-052/vnc-025 fixtures — not falsely claimed complete as unit tests (honors pattern #3624). This is the correct posture for Gate 3b; Gate 3c owns the integration proof. R-11 cross-gate ts/1000 boundary is co-owned with crt-055 (Stage 3c).

### 5. Code quality — PASS (2 WARN)
- `cargo build --workspace` rc=0. `cargo clippy --workspace --all-targets` produced only PRE-EXISTING warnings (`anndists` dependency; `unimatrix-server` collapsible-if lint not gated `-D warnings`) — none introduced by crt-054. `cargo audit` rc=0 (no CVEs).
- Anti-stub: no `todo!()`/`unimplemented!()`/`TODO`/`FIXME` introduced. The two `TODO(W2-4)` in main.rs and the `unreachable!()` in session.rs are PRE-EXISTING on `main`. No `.unwrap()` in crt-054 non-test code (all matches under `#[cfg(test)]`).
- `cargo test -p unimatrix-store` rc=0. `cargo test -p unimatrix-server --lib`: 4102 passed, 1 failed.
- **WARN-1 (non-blocking)**: the single failure `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` is a PRE-EXISTING flake in the unrelated `eval` module (crt-054 touches no `eval/` file). It passes deterministically in isolation (2/2 reruns green) — a parallelism-sensitive non-vacuity assertion, not a crt-054 regression.
- **WARN-2 (pre-existing)**: two source files exceed the 500-line rule — `config.rs` (12036 lines; 11791 on `main`) and `write_ext.rs` (860; 833 on `main`). crt-054 EXTENDED pre-existing oversized files rather than creating new ones; every file crt-054 authored is well under 500 (transcript_activity.rs 235, store_ops additions, etc.). Not introduced by crt-054; flagged for the standing split backlog, not a crt-054 blocker.

### 6. Security — PASS
- INSERT is parameterized (`?1`/`?2`/`?3`) — `session_id` bound as data, no SQL-injection surface.
- Content opacity is structural: no byte-bearing field can be persisted or logged; the failure counter name is a fixed literal carrying no ids/bytes; the writer logs ids/counts only.
- Config deserialization (`#[serde(default)]`) is followed by loud `validate()` (bounds, duplicate-name, bytes-domain regex compile) — malformed config aborts startup, cannot corrupt state or silently disable scanning.
- No hardcoded secrets. `cargo audit` clean.

### 7. Knowledge stewardship — PASS
All five implementation-agent reports (foundation, store, config, writer, fold-wiring) contain a `## Knowledge Stewardship` block with `Queried:` entries (context_briefing/context_search, load-bearing) and `Stored:` entries (#5052 migration intra-stamp pattern, #5053 best-effort counter bump, #5055 bytes-domain validate lockstep, #5056 pub(crate)→pub boundary trap) or a reasoned nothing-novel statement (foundation). No WARN.

## Rework Required

None.

## Scope Concerns

None. All binding invariants are structurally enforced in code, the producer contract with crt-055 is honored exactly, and the held-route/read-before-purge integration proofs are correctly deferred to Stage 3c. Stage 3c may proceed.
