# Scope Risk Assessment: crt-052

Mode: scope-risk. Restart on an approved, re-verified SCOPE (six OQs resolved; three positions pinned).
Product-level risks only — flagged to inform architecture/spec, not to reopen scope.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Goal 8 / Option B held-buffer lifecycle is unbounded by default: held sessions keep merging deltas but only purge at cycle-review or stale-sweep. A never-reviewed or never-swept session (drift, crash, mis-attribution) leaks one buffer per session up to cap. Memory bound = cap × held-count; held-count has no natural ceiling. | High | Med | Architect must pin an explicit held-session count cap and an independent stale-sweep TTL, not rely on cycle-review to reclaim. Surface eviction policy when the cap is hit. |
| SR-02 | Re-adoption correctness: a held buffer must rebind to the *same* feature_cycle on re-registration. vnc-030 contract-attribution helps, but #981 shows NULL/mis-set feature_cycle silently breaks the retrospective pipeline. A held buffer re-adopted under the wrong cycle silently mis-scopes candidates. | High | Med | Treat re-adoption key derivation as a first-class design decision; fail loud (not silent) on key mismatch. Cite #981 in the spec. |
| SR-03 | Audit-shape change (OQ-3 open): moving `transcript_session_purged` off per-turn-close onto cycle-review/stale-sweep changes vnc-025's shipped audit timing. A downstream consumer keying on per-close audit cadence would silently break. | Med | Med | Architect verifies no consumer keys on per-close audit timing before moving the points; document the new "exactly once per held session at review/sweep" contract (AC-11). |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | #700 (MARKER recovery) seam-shape coupling: `take_transcripts_for_feature` must expose bytes + elision/hole metadata in a form #700 reuses without a second `contiguous_tail` reader (Constraint 4). A seam shaped only for crt-052's candidate path forces an expensive retrofit and breaks the load-bearing single-reader invariant. | High | Med | Pin the return type against both consumers now (OQ-2). Treat #700's marker-parsing need as a design input, not a future concern. |
| SR-05 | Four-success-return drift (#4750): distillation must gate all four `result.is_ok()` purge sites (`tools.rs:2110/2236/2925/3027`). Wiring only the tail return silently skips cache-hit and degraded paths — invisible in happy-path tests. | Med | Med | One shared helper called at all four sites; an exhaustiveness test that fails if a fifth return appears. |
| SR-06 | `topic_source` scope creep (OQ-1, pinned soft preference): it is a soft recall preference for fallback selection, never a hard filter. Hardening it into a filter would drop legitimately-attributed sessions and is out of scope. | Low | Low | Spec states `topic_source` as ordering preference only; the candidates remain feature-match-scoped, never `topic_source`-gated. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Secrets/never-persist vs crt-033 memoization persist (#3793): the cycle-review report is memoized synchronously to `cycle_review_index`. Candidates riding that struct would persist raw transcript excerpts to SQL — a direct breach of the #4721 secrets guarantee (AC-06). | High | Med | Attach candidates at response-assembly level outside the memoized struct, or strip on the persist path. The invariant, not the mechanism, is mandated — verify with a grep/log gate over new code paths. |
| SR-08 | Hole/elision threshold mis-calibration drives the reconstruction-fallback trigger (whole-session either/or). #3359 shows threshold/window mismatches cause silent over-firing. Buffer is tail-window-equivalent, not lossless (Constraint 9) — a threshold assuming losslessness mis-triggers fallback or misses real loss. | Med | Med | Define the trigger against vnc-025 ADR-002 (#4740) / ADR-008 (#4764) semantics explicitly; test at the cap boundary and under ring-tail overflow. Cite #4764 (active), not #4746. |
| SR-09 | Untrusted input: buffer content is client-disk JSONL. A corrupt/adversarial line must degrade to skip-with-count, never panic the cycle-review handler (Constraint 7). | Med | Med | Parser hardening + per-line skip counting is an AC, not a nicety; fuzz/malformed-line tests at architecture-risk stage. |

## Assumptions

- **AC-11 is the only pre-merge proof of the primary path** (Goals 1/8). Until the dogfooding switchover lands, the primary (non-fallback) path is unobserved in real sessions — all confidence rests on simulated turn boundaries and synthetic fixtures. If Option B's hold has a re-adoption gap not exercised by the ≥3-turn simulation, it surfaces only post-merge. *(Goal 8, AC-11)*
- **AC-03 recall depends on fixture independence** (Goals 2, AC-03). If the labeled corpus is authored from the same regex set it validates, ≥0.90 recall is self-fulfilling and meaningless. Independent authorship (anchors-before-port, or different author — OQ-6) is the only guard. *(AC-03, OQ-6)*
- **vnc-030 contract-attribution holds across the held-buffer hold** (Goal 8). Goal 8 assumes a declared session's feature_cycle survives drain→hold→re-adopt. vnc-030 fixed attribution across drain but did not exercise the new hold structure. *(Background "per-turn drain", Constraint 13)*
- **Diffs to `drain_and_signal_session` / `clear_transcripts_for_feature` stay minimal against vnc-030 ADR-007 §2** (#4819, stale `deprecated` label but binding per PR #702 / #700). If Option B requires non-trivial rework of these vnc-030-untouched functions, the cite-don't-rework constraint (13) is at risk. *(Constraint 13)*

## Design Recommendations

- SR-01/SR-02/SR-03: Architect treats Option B as its own design area with explicit, written decisions on (a) held-session count cap + eviction, (b) re-adoption key derivation with loud failure, (c) audit-timing contract. This is the dominant risk surface; do not fold it into the seam work.
- SR-04: Resolve OQ-2 (seam return shape) with #700's marker-parsing need as a co-equal input before pinning the type. Retrofit is the expensive mistake.
- SR-07: Decide the candidate-vs-memoization mechanism early (assembly-level attach vs persist-path strip) and bind it to a content-leak test gate extending vnc-025 AC-12.
- SR-08/SR-09: Design the fallback trigger and the JSONL parser against tail-window-equivalence and untrusted-input semantics respectively — both are fidelity/safety floors, not optimizations.
- AC-03 / AC-11: Spec must make fixture-independence (AC-03) and the dogfooding-switchover dependency (AC-11) explicit gate conditions, since each is the sole proof of its path.

Top 3 for architect attention: SR-01 (held-buffer memory bound), SR-04 (#700 seam-shape coupling), SR-07 (memoization persist secrets breach).
