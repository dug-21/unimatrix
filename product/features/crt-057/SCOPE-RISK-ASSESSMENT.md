# Scope Risk Assessment: crt-057

Fully non-destructive `context_cycle_review` + scoped, honest `transcript{phase?,anchor?,match?,window?}`
retrieval. **Refreshed 2026-07-04** for the ass-091 redesign — the boolean-era version (fused
`include_transcript_candidates` emit+purge) is superseded. Product/scope-level risks feeding the
architect/spec; flags risks, does not recommend scope changes.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | **Memory-residency lengthening — no purge verb at all now.** Every path (default/json/force/`transcript{}`) leaves the buffer intact; raw (possibly secret-bearing) bytes reside until a backstop reclaims. Steady-state resident volume rises vs purge-at-every-review. | High | High | State the residency envelope plainly in the amending ADR (worst-case = 64 held buffers × per-buffer cap, ≤24h TTL). Still memory-only/bounded (NG-1). Spec must assert no-new-persistence on every changed path incl. reclamation-without-review (SR-02). |
| SR-02 | **Secrets/audit posture drift — content-free terminal audit now fires ONLY at the backstop**, never at review. "Purge audit ⇒ a review occurred" no longer holds; reclamation is entirely TTL/cap/session-close. | Med | Med | ADR must state disk posture UNCHANGED and that backstop reclamation still emits the content-free `transcript_session_purged` audit. Spec: assert AC-14 on every changed path, incl. the reclamation-without-review path. |
| SR-03 | **Backstops are the SOLE loss mode now.** A never-retrieved or >24h-delayed cycle loses verbatim candidates to TTL/cap; unlike the boolean era this is no longer compounded by an eager purge, but it is the primary candidate-loss vector. | Med | Med | Not a regression (backstops unchanged, NG-2) but must be named as the primary loss mode. Loss propagation (SR-05) makes the degradation VISIBLE (`Reconstructed`/loss), never silent. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | **Consumer-reconciliation coupling — the dominant scope risk.** Ship server (D-1..D-4) without D-6/D-7 and `/uni-retro` still calls the removed boolean (or the non-retrieving default) → self-learning harvest (#5219) starves with no error. | High | High | Treat server + `uni-retro/SKILL.md` + tool description + BOTH protocol files as one atomic unit (C-6). No partial-ship story. Spec: an AC that FAILS if any consumer still implies the old boolean / any-review-carries-candidates. `uni-agent-routing.md` excluded. |
| SR-05 | **Silent false negative is the redesign's raison d'être.** The whole point is that a `match` no-match over a lossy/`Reconstructed`/elided/holed session must read as INDETERMINATE, not "didn't happen." Risk that retrieval collapses to a bare boolean and loss is best-effort rather than structural. | High | Med | Spec must make per-session `SessionLossInfo` + `search_complete` (false iff `elided_bytes>0 ∥ has_holes ∥ Reconstructed`) a hard contract on EVERY returned session (AC-06). No bare-bool no-match anywhere. |
| SR-06 | **ass-090 / NG-7 line.** "Distill transcript signal INTO the summary" is deferred to ass-090 (#896). The adjacency (both touch buffer consumption) invites in-scope enrichment. | Med | Med | Hold NG-7 hard: crt-057 serves honest planes, never joins/attributes/enriches the summary. Zero test scenarios for in-summary distillation; flag any AC that enriches the report body. |
| SR-07 | **No destructive axis + dead `"summary"` alias.** The tool must expose NO purge verb (NG-6) and resolve the dead `"summary"` render alias, or a hidden third render path / a lingering destructive capability survives. | Low | Med | Architect: drop `"summary"` (→ `ERROR_INVALID_PARAMS`); assert `format` accepts exactly `markdown|json` with identical content, and no parameter/path purges (AC-03/AC-11). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | **Clock skew is now a first-class interface requirement (promoted).** Plane A `EvidenceRecord.ts` (u64 epoch-millis) vs Plane B `TranscriptCandidate.ts` (`Option<String>` JSONL) are independent clocks for `Primary` sessions. A bad join or a missing `ts:None` `byte_offset` fallback silently drops candidates from anchor/phase queries. | High | Med | Normalize server-side to a canonical epoch, join over a WINDOW never an exact match, `byte_offset` fallback for `ts:None`. Agent never sees Plane B's clock. Use a named conversion helper at the plane boundary (evidence #3385/#3372). |
| SR-09 | **Four-site #4750 lockstep — now gates ONLY the content-opaque fold read.** After the purge leaves the seam, missing the fold at any site (esp. memo-hit, site 3) under-counts non-`force`-reproducible durable integers (#5030). History (#4585) shows sites drift silently. | High | Med | Keep the fold gated at all four returns via the shared helper; behavioral (not just source-assertion) memo-hit rows. Scope threading is response-decoration (behaviorally caught), the fold read is the real invariant. |
| SR-10 | **Orphan deletion + exhaustive-match re-home (C-5, anti-stub).** `purge_cycle_transcripts` + `clear_transcripts_for_feature` + `purge_held_for_feature` lose all non-test callers → MUST be deleted (CLAUDE.md rule 2 / dead-code). The exhaustive `TranscriptRetention` match inside the deleted purge must be re-homed onto surviving backstops without changing retention behavior. | Med | Med | Delivery deletes the orphans; confirm the ×4 purge-count source assertions are deliberately removed with rationale; verify backstops (TTL/cap/session-close) still reclaim exhaustively (`RetainDays` no-op, no `_` arm). |
| SR-11 | **Two-protocol lifecycle restructure (D-7) — feature-wide blast radius.** merge→close→retro rewired in BOTH `uni-delivery-protocol.md` and `uni-bugfix-protocol.md`; a mis-wire silently breaks attribution/verbatim harvest for EVERY future session, not just crt-057. | High | Med | Verify end-to-end PER protocol (simulate full cycle), not at the server surface. The ordering composes only because close is non-purging (SR-12/ADR-005). |
| SR-12 | **crt-055 fold idempotency on the now-non-purging common path.** The content-opaque fold is a pre-existing buffer read on a path that no longer purges; repeated non-destructive reviews re-read the same buffer. | Med | Low | Confirm the fold reads counters only and is idempotent across repeated reviews (no double-count of durable `cycle_review_index`). |
| SR-13 | **ADR-amendment consistency (amends #4742/#4857).** A partial amendment (removing the purge trigger without plainly stating the residency change) leaves stored decisions internally contradictory. | Med | Med | Amend via `context_correct` (not deprecate+store). State all: purge removed / fully non-destructive, residency bounded by unchanged cap+TTL, disk posture unchanged, `force`/`format` orthogonal, fold read the sole surviving side-effect. |
| SR-14 | **Rebase conflict on `distill_handler.rs` (C-8).** Prior work (bugfix-891) touched the file; a stale base risks silent conflict at delivery. | Low | Low | Confirm no live conflict before delivery; flag to the delivery leader, not an architecture concern. |

## Assumptions

- **A-1 (SCOPE §Two Data Planes / ARCHITECTURE §5):** the report is 100% Plane-A-derived and buffer-content-independent (`build_report()` takes no transcript arg). If any summary field silently reads buffer content, render/force decoupling breaks and SR-06/SR-12 escalate. Architect re-verified the single non-test reader of `take_transcripts_for_feature` — hold that.
- **A-2 (SCOPE §Accepted Residency Trade-off / NG-2):** the unchanged 64-cap + 24h TTL + session-close are a sufficient sole reclamation backstop. If real retro cadence runs cycles that never retrieve, SR-01/SR-03 resident volume and loss rate are load-bearing — validate against expected cadence.
- **A-3 (SCOPE §Loss propagation / OQ-1):** `search_complete` derived from `SessionLossInfo` is the honesty guarantee; a session reclaimed/never-registered before retrieval emits no loss row (invisible). Downstream must treat completeness as best-effort AND re-retrievable (non-destructive), never certified.

## Design Recommendations

1. **Ship atomically (SR-04, SR-11, SR-13):** server + both consumers + both protocols + amended ADRs are one indivisible unit. No partial-ship narrative.
2. **Make loss propagation structural, not best-effort (SR-05):** every returned session carries `SessionLossInfo`; a no-match over `search_complete==false` is INDETERMINATE. No bare-bool no-match. This is the feature.
3. **Normalize clocks server-side behind a named boundary helper (SR-08):** windowed join, `byte_offset` fallback for `ts:None`, agent never knows Plane B's clock (evidence #3385).
4. **Delete the orphans; re-home the exhaustive match (SR-10):** no dead code (anti-stub); prove backstops still reclaim exhaustively after the eager purge is gone.
5. **State residency plainly in the amending ADR (SR-01, SR-02, SR-13):** resident-bytes envelope, unchanged backstops, disk-unchanged, orthogonality — or the amendment is inconsistent.
6. **Hold the NG-7 line (SR-06):** zero in-summary-distillation scenarios; that is ass-090.
