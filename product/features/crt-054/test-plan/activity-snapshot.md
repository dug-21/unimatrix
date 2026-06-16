# Test Plan — `activity_snapshot()` + `ActivitySnapshot` read surface

**Component**: `pub fn activity_snapshot(&self) -> ActivitySnapshot` on `TranscriptBuffer`; `#[derive(Clone, Copy)] struct ActivitySnapshot { bytes_total: u64, delta_count: u32, class_counts: [u32; MAX_SIGNAL_CLASSES] }`. Metadata-only `Debug`, no `Display`; poison→empty (#4764).
**Pseudocode**: `pseudocode/activity-snapshot.md` · **Layer**: unit + **integration (read-before-purge — CRITICAL)**.
**Anchor ACs**: AC-08 (shape/content-opacity), **AC-07 (read-before-purge — Critical)**, AC-14 (cast-free width), AC-15 (no residue). **Risks**: **R-02 (Critical)**, R-05, R-07, R-12.

## Shape & content-opacity (AC-08, R-05) — unit

`crates/unimatrix-server/src/infra/session_transcript_tests_snapshot.rs` (extend existing snapshot tests).

1. `test_activity_snapshot_is_copy` — compile-time: `ActivitySnapshot: Copy`.
2. `test_activity_snapshot_shape_matches_contract` — fields are exactly `bytes_total: u64`, `delta_count: u32`, `class_counts: [u32; MAX_SIGNAL_CLASSES]`; mirrors crt-055 §"Producer contract" Surface B verbatim (single source). (R-05.)
3. `test_activity_snapshot_no_content_field` — structural (mirrors `test_candidates_structurally_absent`): no `Vec<u8>`/`String`/`&[u8]` content field. (NFR-1, AC-08.)
4. `test_activity_snapshot_debug_is_metadata_only` — `format!("{:?}", snap)` prints only the scalar counters; no transcript bytes. (FR-B7.)
5. `test_activity_snapshot_no_display_impl` — assert no `Display` impl (compile-time / structural). (AC-08.)
6. `test_activity_snapshot_no_latch_field` — no `saw_compaction`/`reload_after_compaction` latch (R-12 stale residue).
7. `test_activity_snapshot_poison_returns_empty` — Arrange: poison the buffer mutex. Act: `activity_snapshot()`. Assert: returns an empty/zeroed snapshot (#4764), same as `snapshot()` — does NOT panic. Document: this empty must be distinguishable from a real zero at crt-055 (absence flag), but crt-054 only guarantees poison→empty without panic.

## Read-before-purge / survival (AC-07, R-02) — INTEGRATION, CRITICAL

`crates/unimatrix-server/src/uds/listener/tests/` (alongside `purge_audit.rs`) or `transcript_hold_tests.rs`.

8. `test_read_before_purge_ordering` — Arrange: a held cycle with non-trivial folded deltas (drain→hold). Act: read `activity_snapshot()`, capture non-zero counters; THEN call `purge_cycle_transcripts` (`server.rs:561` → `clear()` + `purge_held_for_feature`); read again. Assert: the first read returns non-zero AND the post-purge buffer is zeroed/dropped — i.e. the read provably happens before purge. (R-02, AC-07.)
9. `test_no_crt054_path_zeroes_accumulator` — assert no crt-054 code path zeroes/drops/resets the accumulator between fold and review: the accumulator's only lifecycle is the buffer's lifecycle (ADR-006). Structural + behavioral: drive drain→hold→review and assert the snapshot is the full sum (no partial/reset) until purge. (R-02 scenario 2.)
10. `test_snapshot_survives_drain_hold_review` — snapshot at review equals the sum of all folded deltas across the full drain→hold lifecycle, not a partial or reset value. (R-02 scenario 3.)

### Negative-mutation (AC-07)
- A regression that zeroed/reset the accumulator before purge (e.g. a per-turn flush) must make `test_read_before_purge_ordering` / `test_snapshot_survives_drain_hold_review` fail red (snapshot reads 0 or partial).

## Cast-free producer width (AC-14, R-07)

11. `test_producer_path_has_no_narrowing_cast` — grep/structural: no `as i64`/`as i32`/narrowing cast of `bytes_total`/`delta_count`/`class_counts` on the producer path (`activity_snapshot()`, accessors, the collector). The checked/saturating `→ i64` is crt-055's at persist; a producer `as` cast would truncate before crt-055's guard runs. (NFR-5, AC-14.)
12. `test_bytes_total_near_u64_max_round_trips` — Arrange counters with `bytes_total` near `u64::MAX`. Assert the value round-trips through `activity_snapshot()` un-narrowed at native `u64`. (R-07.)

## No-residue (AC-15, R-12) — shared with transcript-signals-config.md

13. `test_snapshot_no_token_named_field` — grep: `ActivitySnapshot` and its module carry no `token_*` symbol, no `token_bytes_per_unit`. (AC-15.)

## Notes
- The CRITICAL read-before-purge test (8) reuses the crt-052 hold fixtures + `purge_audit.rs` purge precedent — extend, do not re-scaffold.
- This component owns AC-07; AC-06 (held-route fold) is owned by apply-delta-fold.md; both share the drain→hold fixture.
