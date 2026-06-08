# Test Plan — C1 Snapshot Seam

**Component**: `take_transcripts_for_feature(&self, feature_cycle) -> Vec<(String, TranscriptSnapshot)>`
(`infra/session.rs`), sibling to `clear_transcripts_for_feature`. **ADRs**: ADR-001 (+ ADR-008 §4 for
held-buffer scanning, Wave B). **Wave**: A (registered scan) + B (held scan). **Tests live in**:
`infra/session.rs` `#[cfg(test)]` or a sibling `session_seam_tests.rs`. **Merge gates**: AC-01,
AC-V-SEAM.

## Unit / Component Test Expectations

### Two-phase lock discipline (AC-01, R-08)
- `test_seam_phase1_registry_lock_arc_clone_only` — source/structural assertion: phase 1 scans
  `state.feature.as_deref() == Some(feature_cycle)` under the registry lock and only Arc-clones
  matching buffers; the registry lock is released before phase 2.
- `test_seam_phase2_buffer_lock_byte_copy_only` — phase 2 takes each buffer lock, calls `snapshot()`
  (byte copy + metadata), releases. Assert NO parse / marker-match inside either lock scope (the byte
  copy is the only content work under the buffer lock; per #3753 use the snapshot, never re-acquire).
- `test_seam_no_parse_under_lock` (AC-01(a), **merge gate**) — source assertion that no JSONL parse or
  marker-match symbol is referenced within any lock-guard scope in `take_transcripts_for_feature`.
- `test_seam_none_feature_never_matches` — sessions with `feature == None` never match (declared
  sessions contract-attributed by vnc-030 ADR-007 §2 #4819; no vote-flip). Assert a `None`-feature
  session is excluded from the snapshot set.
- `test_seam_does_not_clear_buffers` — assert the seam does NOT clear/purge buffers (snapshot reads;
  purge is the separate `clear_transcripts_for_feature` / `purge_held_for_feature` step, ADR-005).
  Reviewer-policed invariant: no caller of the new method clears as a side effect.

### Poison recovery (R-16)
- `test_seam_poisoned_buffer_recovers_treat_as_empty` — a poisoned buffer in phase 2 → recover per
  #4764 (`into_inner`, treat-as-empty, `clear_poison`); the session still appears in the result as an
  empty/lossy snapshot (surfaced downstream as loss, not silently dropped).

### Concurrency / stress (AC-01(b), **merge gate**, R-08)
- `test_concurrent_deltas_during_seam_consistent` (`#[tokio::test]` + stress loop, or loom) — stream
  `apply_delta` writes concurrently to a buffer while `take_transcripts_for_feature` snapshots it;
  assert: no deadlock, no torn read (returned `bytes` is a valid prefix-consistent snapshot under the
  buffer lock), consistent metadata. Run N iterations with a seeded interleave.

## Wave B — registered ∪ held scan (R-13)
(Held-store mechanics in held-buffer-store.md; these assert the SEAM scans both sets.)
- `test_seam_scans_registered_and_held` — one session registered + one held under the same
  `feature_cycle`; assert BOTH appear in the snapshot result.
- `test_seam_no_double_snapshot_arc_identity` — a session that is BOTH registered and held (same Arc)
  is snapshotted ONCE (Arc identity dedup), not twice.
- `test_seam_registered_between_scans_no_leak` — a session registered between the snapshot scan and
  the purge scan is either in both or neither for this review (no orphan snapshot, no orphan purge).
- `test_seam_wave_a_only_registered_scan` (R-11) — with Wave B absent, the seam scans only the
  registry and still returns correct snapshots; zero compile-time reference to `transcript_hold.rs`.

## Merge-Gate Tests
- AC-01 source assertion + concurrency/stress — owned here (see above).
- AC-V-SEAM — the seam returns owned `TranscriptSnapshot`s carrying all four metadata fields (asserted
  in snapshot-types.md); the seam half is `test_seam_returns_owned_snapshots_with_metadata`.

## Edge Cases
- Zero sessions match the feature → empty Vec (drives AC-04 absent-when-empty downstream).
- Feature with only held sessions (all drained, none re-registered) → snapshot set is the held set
  (Wave B); under Wave A only this degrades to empty → fallback.

## Assertions Summary (concrete)
- `take_transcripts_for_feature("X")` returns exactly the snapshots for sessions attributed to `X`,
  registered ∪ held, deduped by Arc identity; buffers unchanged (not cleared).
- No parse/marker symbol reachable inside any `MutexGuard`/`RwLockGuard` scope (source assertion).
- Concurrency test is the AC-01(b) merge-gate evidence — must run, must be deterministic.
