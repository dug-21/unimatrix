# Test Plan — `apply_delta` fold call (both routes)

**Component**: the fold call added after the merge in `TranscriptBuffer::apply_delta` (`session_transcript.rs:150`); runs on the registered route AND the held route by construction (accumulator embedded in the buffer, ADR-001).
**Pseudocode**: `pseudocode/apply-delta-fold.md` · **Layer**: unit (registered) + **integration (held — CRITICAL)**.
**Anchor ACs**: AC-05 (registered route), **AC-06 (held route — Critical)**, AC-13 (no single-event dependence). **Risks**: **R-01 (Critical)**, R-06.

> This is the believable-zero seam (#750/#5025). The held-route tests are NON-NEGOTIABLY integration tests on the crt-052/vnc-025 hold fixtures with a mandatory negative-mutation check. A registered-only or unit-only test does NOT satisfy AC-06 (pattern #3624).

## Registered route (AC-05) — unit/in-crate

`crates/unimatrix-server/src/infra/session_transcript_tests.rs` (extend existing).

1. `test_apply_delta_registered_route_folds_counters` — Arrange: a live registered `TranscriptBuffer`. Act: `apply_delta(off, bytes)` with two non-trivial deltas. Assert via `activity_snapshot()`: `bytes_total == sum(len)`, `delta_count == 2`, `class_counts` reflect any matches. (FR-B1/B3, AC-05.)
2. `test_apply_delta_fold_runs_after_merge` — the fold sees the merged delta bytes (ordering: merge then fold), so `bytes_total` equals the bytes actually merged.

## Held route (AC-06) — INTEGRATION, CRITICAL

`crates/unimatrix-server/src/infra/transcript_hold_tests.rs` (extend the crt-052 Wave B hold fixtures) — drive the real `SessionRegistry` + hold.

3. `test_held_route_fold_nonempty_at_review` — **Mandatory held-route regression guard.** Arrange: a representative TS-client cycle — declare a `feature_cycle`, register a session, stream multi-turn deltas, then **drain** the session (Stop/SessionClose) so its buffer rides the crt-052 Wave B hold, re-adopt / continue streaming deltas on the held `Arc` (`held_arc_for_session`, `session.rs:388-401`). Act: read `activity_snapshot()` (or `activity_snapshots_for_feature`) for the cycle's held sessions at "review" time (before purge). Assert: `bytes_total > 0`, `delta_count > 0`, and at least one declared session contributes. A registered-only path MUST NOT satisfy this — the test must route deltas through the held branch. (R-01, AC-06.)
4. `test_held_route_fold_continuity_across_drain` — Arrange: stream K bytes on the registered route, drain, then stream M more bytes on the held `Arc`. Assert: the snapshot reads `bytes_total == K+M` and `delta_count` counts both pre- and post-drain deltas — **continuity across the drain boundary**, not just two isolated non-zero reads. (R-01 scenario 2; edge: drained-then-redelivered delta.)
5. `test_held_route_fold_negative_mutation_guard` — **Mandatory negative-mutation check.** With the held-route `apply_delta` fold call removed (or the held branch bypassing the fold), `test_held_route_fold_nonempty_at_review` and `test_held_route_fold_continuity_across_drain` MUST fail red. Encode this as an explicit assertion-of-route: assert the post-drain bytes are reflected in the snapshot (so a held-route fold miss → `bytes_total == K`, not `K+M` → test fails). If removing the held-route fold leaves the test green, the test is INVALID and must be rejected at Gate 3a/3c. (R-01 scenario 3, ADR-009.)

## No single-event dependence (AC-13, R-06)

6. `test_no_pretooluse_or_single_hook_dependence` — grep/structural: neither the fold nor the writer reads `PreToolUse` or any single hook-event-class presence; Surface B derives from the delta stream, Surface A from the compaction seam. Covered transitively by AC-06 (the fold survives the held route, which exists BECAUSE of the per-turn drain, #4799). (NFR-9, AC-13.)

## Fixtures / dependencies
- Reuse `transcript_hold_tests.rs` / `transcript_hold_ac11_tests.rs` drain→hold helpers — do NOT build isolated hold scaffolding (test infra is cumulative).
- The "review read" uses `activity_snapshots_for_feature` (activity-collector.md); read-before-purge ordering is owned by activity-snapshot.md (AC-07).

## Gate note
Gate 3a/3c MUST verify the negative-mutation check exists and bites. A held-route AC-06 test without test 5 is incomplete and is the exact way #750 slips through (#3624).
