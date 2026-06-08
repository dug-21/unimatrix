# Test Plan — C8 Held-Buffer Store (Option B)

**Component**: `unimatrix-server/src/infra/transcript_hold.rs` —
`hold_on_drain`, `readopt`, `sweep_expired`, `purge_held_for_feature`; plus minimal `drain_and_signal_session`
diff and held-buffer delta routing in `listener.rs`. **ADRs**: ADR-008 (state machine), ADR-009
(audit-shape move + Wave B). **Wave**: B. **Tests live in**: `infra/transcript_hold.rs` `#[cfg(test)]`
+ a server-level lifecycle test (`continuity_simulated_lifecycle`). **Merge gate**: AC-11 — the ONLY
pre-merge primary-path proof. This is the dominant risk surface (R-01/R-02/R-03/R-05/R-16/R-17).

> All clock-dependent tests use an injectable/mockable clock; all eviction-order tests pin
> `last_activity_at`. No wall-clock, no sleeps — deterministic.

## R-01 — Re-adoption correctness (Critical, AC-11(b))
- `test_readopt_cycle_match_rebinds` — hold session S under `feature_cycle=X`; re-register S with
  `feature_cycle=X` → `readopt` returns the held `Arc`; the snapshot carries S's bytes (happy
  re-adopt).
- `test_readopt_cycle_mismatch_fails_loud` — hold S under X; re-register S with `feature_cycle=Y` →
  **fail loud**: held buffer DROPPED (treated as fresh), a metadata-only diagnostic emitted, NO
  content re-adopted under Y. Assert no silent re-adoption.
- `test_readopt_null_cycle_no_silent_adopt` (cite #981) — re-register S with `feature_cycle=None`/NULL
  → no silent re-adoption; the held buffer is not bound to a NULL cycle.
- `test_readopt_concurrent_during_review` (R-01 scenario 4) — a re-register arrives during a cycle
  review scanning the hold → assert no rebind to an in-flight reviewed cycle produces a torn/double
  snapshot. (`#[tokio::test]` + interleave.)
- `test_readopt_mismatch_diagnostic_metadata_only` (R-04 overlap) — the fail-loud diagnostic is
  metadata-only (session_id, held-cycle, attempted-cycle, bytes count) — no transcript content.

## R-02 — Memory bound: cap + independent TTL (Critical, AC-11(c)(d))
- `test_hold_cap_evicts_oldest` — hold `transcript_hold_max_sessions + 1` sessions → cap-hit eviction
  fires, oldest-`last_activity_at`-first; held-count NEVER exceeds the cap.
- `test_hold_at_exactly_cap_no_evict` — held-count at exactly the cap → no eviction; cap+1 → one
  eviction (boundary).
- `test_hold_ttl_sweep_without_review` — hold N sessions, advance the clock past
  `transcript_hold_ttl_secs`, run `sweep_expired` → TTL sweep reclaims them with NO cycle review
  firing (reclamation independent of review).
- `test_hold_ttl_boundary` — `last_activity_at` exactly at TTL; just under (retained) / just over
  (swept).
- `test_hold_memory_bounded_no_review` — disable cycle review entirely; assert memory stays bounded by
  `buffer_cap × max_sessions` via TTL + cap eviction alone (either mechanism bounds memory if the
  other is inert).
- `test_hold_memory_bound_under_churn` — adversarial hold churn → held bytes ≤
  `buffer_cap × max_sessions` always.

## R-03 — Audit exactly-once per held session (Critical, AC-11(e))
> Prerequisite gate: the ADR-009 no-consumer audit survey must be recorded clean before these tests
> are accepted as the gate evidence for the audit-shape move (see OVERVIEW Prerequisite Gate).
- `test_audit_once_at_review` — drain→hold→re-adopt→cycle review → `transcript_session_purged` fires
  ONCE (at review, `trigger=cycle_review`), NOT at the per-turn drains.
- `test_audit_once_at_sweep` — drain→hold→TTL sweep (no re-adopt) → one audit at sweep,
  `trigger=stale_sweep`.
- `test_audit_once_at_eviction` — drain→hold→cap eviction → one audit at eviction (eviction never
  silent, ADR-008 §1).
- `test_audit_once_across_multi_readopt` — drain→hold→re-adopt→drain→hold→review → still EXACTLY ONE
  audit at the terminal purge, not one per hold cycle.
- `test_audit_detail_content_free` — `detail` is `bytes=<n> trigger=<…>`, no transcript bytes (R-04).

## R-16 — Eviction / poison never silent
- `test_eviction_emits_audit` — cap-hit eviction → the evicted session emits the purge audit
  (overlaps R-03). Eviction is never silent loss.
- (Poison-recovery loss surfacing is in snapshot-seam.md / distill-handler.md — the held buffer's
  poisoned lock recovers treat-as-empty and surfaces as lossy.)

## R-13 — Seam scans registered ∪ held; purge clears the same set
- `test_purge_held_for_feature_clears_held` — `purge_held_for_feature(X)` clears all held buffers for
  cycle X.
- `test_no_held_survives_post_review` — after a cycle review for X, no held buffer for X survives
  (`purge_held_for_feature` fired post-distill).
- `test_held_and_registered_no_double_count` — a session both held and registered (same Arc) is
  snapshotted once and purged once (Arc identity). (Seam half in snapshot-seam.md.)

## R-17 — Delta routing does not regress hot path
- `test_held_lookup_is_o1_keyed` — held-buffer lookup on delta apply is O(1) keyed (no linear scan);
  structural assertion (HashMap keyed by session_id, not a Vec scan).
- `test_held_buffer_keeps_merging_deltas` — deltas applied to a held (drained, not re-registered)
  buffer are merged under the buffer lock only; the held buffer keeps accepting deltas between drain
  and re-register/sweep.
- `test_delta_apply_lock_class_unchanged_with_hold` — `apply_transcript_delta` lock holds are
  unchanged in class with the hold active (NFR-1 microsecond discipline; merge under buffer lock,
  Arc-clone under registry lock — vnc-025 ADR-001).

## R-19 — Content-bearing Debug
- `test_heldbuffer_debug_metadata_only` — `HeldBuffer` Debug shows `{session_id, feature_cycle,
  last_activity_at, bytes:<n>}` (or equivalent metadata) — NEVER transcript bytes. Manual Debug.

## AC-11 — `continuity_simulated_lifecycle` (**MERGE GATE — the only pre-merge primary-path proof**)
One named server-level test (`#[tokio::test]`), the faithful per-turn-drain lifecycle:

```
register(S, cycle=X)
  → apply deltas (turn 1 content)
  → drain (Stop→SessionClose)           [hold_on_drain]
  → apply deltas (turn 2 content, to held buffer)
  → drain                               [drain #2]
  → apply deltas (turn 3 content)
  → drain                               [drain #3 — ≥3 drains]
  → re-register(S, cycle=X)             [readopt rebinds]
  → context_cycle_review(X)             [snapshot → distill → purge]
```

Assertions (ALL required in this one test):
- **(a) cross-turn content** — the review snapshot/candidates contain content streamed across ALL
  turns (turn 1 + 2 + 3), not just the last turn. Proves merge-while-held, not last-turn survival.
- **(b) loud re-adopt / fail-loud mismatch** (R-01) — re-adopt on cycle match rebinds; a mismatch
  variant fails loud (cite #981).
- **(c) held-count bounded + observable eviction** (R-02) — held-count stays within cap; eviction is
  observable.
- **(d) stale sweep reclaims without review** (R-02) — TTL reclaim independent of cycle review.
- **(e) audit exactly-once per held session** (R-03) — `transcript_session_purged` fires once at the
  terminal purge.
- **(f) inter-drain deltas merged** — deltas applied to a held (drained, not re-registered) buffer are
  merged (held buffers keep accepting deltas).

**Negative / faithfulness guard (R-05)**: a single-turn happy path (one drain, no inter-drain deltas)
does NOT satisfy AC-11. The test MUST execute ≥3 drain cycles with deltas applied between each drain.
Reviewer-policed: the drain count, inter-drain delta application, and cross-turn content assertion are
non-negotiable. A test that asserts only last-turn content is REJECTED as AC-11 evidence.

## Wave-boundary (R-11)
- All of C8 is Wave B. Reverting it must leave Wave A compiling and shipping degraded (the
  dependency-direction assertion lives in distill-handler.md / selection-module.md: Wave A has zero
  reference TO `transcript_hold.rs`).

## Edge Cases (from Risk Strategy)
- Held-count at exactly cap; cap+1 (eviction).
- TTL boundary: `last_activity_at` exactly at TTL; just under/over.
- Re-register with same / different / NULL cycle.
- Multiple hold→re-adopt rounds before a terminal purge (audit once).
- Session both registered and held (Arc identity).

## Assertions Summary (concrete)
- `readopt` rebinds only on cycle match; mismatch/NULL fails loud, metadata-only diagnostic.
- Held-count ≤ cap always; cap eviction (oldest-first, audited) + independent TTL sweep each bound
  memory alone.
- `transcript_session_purged` fires exactly once per held session across review/sweep/eviction and
  across multi-readopt rounds; detail content-free.
- `continuity_simulated_lifecycle` is the ≥3-drain, inter-drain-delta, cross-turn-content proof — the
  sole pre-merge primary-path gate.
