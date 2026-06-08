# Alignment Report: vnc-030

> Reviewed: 2026-06-08 (rev2 — regenerated after design rework: vnc-027 merged, ADR-006 canary rescope, ADR-002/007 rebased onto merged tree, AC-10/FR-29 added, 23 architecture risks)
> Artifacts reviewed:
>   - product/features/vnc-030/architecture/ARCHITECTURE.md
>   - product/features/vnc-030/architecture/ADR-001..007 (ADR-002, ADR-006, ADR-007 revised)
>   - product/features/vnc-030/specification/SPECIFICATION.md
>   - product/features/vnc-030/RISK-TEST-STRATEGY.md (23 risks, R-01..R-23)
> Scope source: product/features/vnc-030/SCOPE.md (approved 2026-06-08)
> Scope risk source: product/features/vnc-030/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md; goal #4677 (self-learning, owning goal), #4812 (personal-cloud, delivery-order coupling only)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Advances goal:self-learning by raising session-attribution input quality for the learning layer; the canary rescope (ADR-006 rev2) now embodies the provenance-not-zero-unattributed product principle directly; graceful-degradation / single-binary-adapter / no-secrets principles honored |
| Milestone Fit | PASS | Correct OSS-cloud-finalization sequencing (F4b; vnc-027 MERGED → vnc-030 next-up → crt-052); nothing built speculatively; `topic_source` justified now to open the F6 soak window |
| Scope Gaps | PASS | All SCOPE Goals 1-5 and AC-01..AC-10 (incl. the new AC-10/FR-29 UDS-path regression) trace to FR/component coverage |
| Scope Additions | PASS | No unrequested capability; design stays inside the SR-05 registry fence and SCOPE non-goals |
| Architecture Consistency | **PASS** (resolved 2026-06-08) | Authoritative ADR-006 rev2, ARCHITECTURE.md, and SPECIFICATION were already consistent on the rescoped canary; the residual ADR-001 §1 / ADR-002 §2.4 `anyOtherCycleFile` staleness (Watch Item 1) was corrected 2026-06-08 — ADR-001 → Unimatrix #4836, ADR-002 → #4837; `anyOtherCycleFile`, the readdir scan, and the 0.20 threshold removed; file-absent branch now describes the subagent-gated inheritance-drift rule per ADR-006 |
| Risk Completeness | PASS | 12 SR + 23 architecture risks (R-01..R-23), security risks, failure modes; AC-10→R-23, canary rescope→R-19, every SR traced; vision-critical degradation + contractual-mis-attribution risks covered |

No FAIL. No VARIANCE requiring human approval. Watch Item 1 (the only WARN-blocking item) is RESOLVED as of 2026-06-08; the remaining three watch items are ACCEPT/VERIFY tracking items, not WARN-blocking. Net: 0 WARN-blocking.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | All SCOPE Goals 1-5 and AC-01..AC-10 are addressed. AC-10 (new) → FR-29 → component C2/Integration Surface "Transport ride", verified at seam `transport-uds.encodeFrame` |
| Addition | (none) | Drive-by docstring fix (FR-25/C9), `apply_stamp`, the four registry touchpoints, the protocol re-declaration line, and AC-10's UDS proof all trace to SCOPE (OQ2 resolution, Goals 1/2/4/5, SR-05 fence, vnc-027 post-merge obligation owed to #699) |
| Simplification | MARKER tier deferred (not implemented) | Rationale: OQ1 resolved — review-time marker recovery does not exist today; deferred to named follow-up issue **#700** (ADR-007 §4) gated on crt-052's snapshot seam. Explicit SCOPE non-goal; degrades to vote/NULL floor (graceful). AC-04 keeps "marker when present" normative so the hole is disclosed |
| Simplification | AC-07 accuracy denominator = declared protocol sessions only | Rationale (OQ2, human decision 2026-06-08): declaration is the only ground truth; never-declare "vote successes" measure token recall, not accuracy. Never-declare floor protected separately by the strengthened SR-06 regression sample (multi-shape) + live-DB `topic_source` distribution check |
| Simplification | Production `stamp_miss` canary is delivery-probe-gated | Rationale (ADR-006 §7 / OQ-E): the test-time zero-tolerance invariant ships unconditionally; the production signal ships only if delivery proves subagent-context detection is independent of root-id inheritance (Branch A). Branch B → test-time-only. Honest narrowing, not scope loss |

## Variances Requiring Approval

**None.** No deviation from vision or scope rises to VARIANCE or FAIL. The three substantive simplifications are ratified by recorded human decisions on 2026-06-08 and documented with rationale. The items below are WARN-level (one doc-sync correction, three accept/verify items) — none requires approval of a design decision.

## Items Requiring Human Attention (WARN)

1. **RESOLVED (2026-06-08) — Residual staleness inside the ADR set: ADR-001/ADR-002 described the superseded `anyOtherCycleFile` canary.**
   - **Resolution**: Doc-sync fix applied 2026-06-08. Both ADRs were corrected — ADR-001 → Unimatrix #4836, ADR-002 → #4837. `anyOtherCycleFile`, the readdir scan, and the 0.20 threshold are removed; the file-absent branch now describes the subagent-gated inheritance-drift rule consistent with ADR-006. The ADR set is now consistent with the authoritative ADR-006 rev2, ARCHITECTURE.md, and SPECIFICATION. No further action before delivery.
   - **What (historical)**: ADR-006 rev2 is authoritative and explicitly *removes* the `anyOtherCycleFile`/concurrent-file rule and the 0.20 threshold, replacing them with the subagent-gated zero-tolerance invariant. ARCHITECTURE.md (data flow line 45; Integration Surface line 86 — "the removed `anyOtherCycleFile` concurrent-file check is gone per ADR-006 §2") and SPECIFICATION FR-09 already reflected the rescope correctly. The residual was that **ADR-001 §1** still listed `anyOtherCycleFile(stateDir, sessionId) -> bool` in the `cycles.js` API, and **ADR-002 §2.4 step 4 (file-absent branch)** still read "canary check (ADR-006): `cycles.anyOtherCycleFile()` (one readdir, miss branch only); if true → `state.bumpStampMiss`" — the pre-rev2 mechanism.
   - **Why it mattered**: A delivery agent implementing from ADR-002's data-flow step would have built the wrong, noisy-by-construction canary that ADR-006 rev2 was written to retire — re-introducing exactly the R-19 false-signal source the rework eliminated. Closed by #4836 / #4837.

2. **New canary residual: production signal depends on a *second* uncontracted behavior (OQ-E / ADR-006 §7 / R-19).**
   - **What**: The rescoped canary increments only on subagent-context inheritance drift. That assumes "I am a subagent" (depth ≥ 1 / SubagentStart) is a client-side signal *independent* of root-id inheritance. If a broken CLI strips both together (Branch B), the inheritance-break case is observationally identical to a never-declare session and the production canary cannot detect it.
   - **Why it matters**: This is the genuine successor to the prior report's canary concern — the noisy-tripwire problem is gone, but a narrower unproven-independence assumption replaces it, and it gates whether the production tripwire exists at all.
   - **Recommendation**: **Accept** — correctly bounded and disclosed. The design is honest: the test-time zero-tolerance invariant (`stamp_miss == 0`) ships either branch; only the production canary is probe-gated. Ensure delivery runs the OQ-E independence probe (inspect SubagentStart/depth indicators on hook stdin under an aged/simulated CLI) before committing any production canary, and treat Branch B (test-time-only) as the acceptable fallback rather than shipping a tripwire noisy by construction.

3. **C-13 / OQ-D — marker-recovery follow-up issue must exist before design-gate exit.**
   - **What**: SPECIFICATION C-13 and OQ-D require the marker-recovery follow-up (with the crt-052 transcript-snapshot-seam dependency) to be filed before the gate closes. ADR-007 §4 now cites it concretely as **#700** ("filed at design-gate exit per SR-07/OQ-D") — an improvement over the prior cut, which referenced the contract without a number.
   - **Why it matters**: The MARKER tier is advertised as normative ("marker when present", AC-04) while deferred; #700 is the mechanism that keeps the deferred tier from being silently forgotten and pins the single-buffer-reader constraint against crt-052's seam.
   - **Recommendation**: **Verify** issue #700 is actually filed and carries the binding dependency contract (consume crt-052's `take_transcripts_for_feature` snapshot seam, never a second `contiguous_tail` reader). Tracking action, not a design change.

4. **Dependence on uncontracted Claude Code behavior pinned to one CLI version (SR-01 → R-08).**
   - **What**: Attribution correctness rests on `--resume` session_id reuse and depth-1 root-session-id inheritance, empirical on claude 2.1.167 only. A CLI upgrade silently degrades declared sessions to the vote/NULL floor.
   - **Why it matters**: Both feed the self-learning goal's input quality; degradation has no error path other than the canary.
   - **Recommendation**: **Accept** — mitigated correctly. ADR-006 rev2 makes `stamp_miss` a zero-tolerance invariant (no threshold/denominator/baseline to desensitize), the pinned CLI version is recorded in test assumptions and the brief, and the re-run-AC-06-fixtures-on-CLI-bump check is part of the standard suite. The size-budget external dependency that compounded this in the prior cut is now RESOLVED (vnc-027 merged; ~43 KB raw / ~29 KB stripped headroom).

## Detailed Findings

### Vision Alignment
vnc-030 is tagged `goal:self-learning` (#4677); #4812 confirms only a *delivery-order* coupling to personal-cloud, not goal ownership. Goal #4677 names the learning inputs as "behavioral signals from agent workflows" with the explicit poisoning-resistance criterion that "implicit training labels [are] attributed to sessions not agent_ids." Contractual cycle attribution raises the trustworthiness of that session-attribution substrate — replacing ~90%-inference attribution (20.2% of live observations carry a contradicting extracted signal — SCOPE Problem Statement) with a write-time contract, while demoting-never-deleting the heuristic floor. This is input-quality work for the confidence/learning layer — squarely on goal #4677.

The canary rescope is the strongest vision-alignment improvement over the first cut. ADR-006 §1 states the product principle verbatim: "Driving unattributed sessions to zero is NOT the goal — the product goal is provenance: ensure sessions that ARE attributed to a topic get recorded that way." §3 makes depth-0 never-declare sessions (uni-zero, research, ad-hoc — the normal mode in this repo) explicit structural non-signal, and §2/§4 scope the canary to drift *within the declared population*. This is the design embodying the stated principle, not merely complying with it.

Architectural principles (PRODUCT-VISION §Architectural Principles):
- **#5 Graceful degradation** — strongly honored: fail-open client (NFR-03/C-04, R-03), heuristics demoted-never-deleted as the never-declare floor (FR-19, ADR-004 §7), mixed stamped/unstamped tolerance with no feature flag (NFR-05/FR-12), `cycle_stamp: None` → legacy chain. "Absent/failed = previous behavior, not broken behavior" applied verbatim.
- **#6 Single binary, client is an adapter** — honored: Rust `hook.rs` untouched (NFR-05); additions are TS-client + one additive wire field; the additive `topic_source` column supplies the F6 (#682) `hook.rs`-retirement evidence base — consistent with #4812's "the Rust hook.rs CLIENT path retires once TS+UDS reaches parity."
- **#7 In-memory hot path** — `FeatureSource` flag and `apply_stamp` operate on the in-memory `Arc<RwLock>` registry; no query-time DB read introduced.
- **#8 No secrets in any DB** — content-free `health.json` canary (count only, no topic/session-id/path — ADR-006 §1, Security Risks), and `stamp.topic` traverses the parameterized `?10` bind, never interpolation.

Principles #1-#4 (hash chain, append-only audit, capability checks, typed graph) are untouched by an attribution-pipeline change — legitimately N/A.

### Milestone Fit
F4b of the OSS-cloud finalization plan (#4812), split from #680 by a recorded uni-zero decision. **vnc-027 (F4a) MERGED 2026-06-08; vnc-030 is now next-up** (pinned vnc-027 → vnc-030 → crt-052). Nothing is built speculatively: marker recovery deferred (#700), server registry lifecycle redesign fenced (SR-05/C-09), #578 audit-log retention deferred post-OSS-cloud-v1, depth>1 inheritance unverifiable (canary tripwire only). The one forward-looking artifact — the additive `topic_source` column — is justified as needed *now* to open the F6 soak window. The first-cut's size-gate external dependency (SR-02, 3-byte headroom) is discharged by the vnc-027 merge. Milestone discipline is exemplary.

### Architecture Review
Components C1-C9 cover every SCOPE goal and AC-01..AC-10. Cross-document consistency is byte-clean on the load-bearing vocabulary, with one residual exception:
- Migration: ARCHITECTURE C7, ADR-005, and SPECIFICATION FR-20 agree on v27→v28, `CURRENT_SCHEMA_VERSION = 28`, pragma-guarded ALTER on the v9→v10 `topic_signal` precedent. R-11 carries the schema-version-collision risk against parallel features for delivery-time resolution.
- Precedence wording "stamp → marker(-when-present) → vote-on-NULL" is consistent across ARCHITECTURE, ADR-004, SPEC (AC-04, domain model), and RISK-TEST; the MARKER-tier deferral is uniformly disclosed.
- AC-10 / FR-29 is fully threaded: ARCHITECTURE Integration Surface ("Transport ride (HTTP+UDS)", seam `transport-uds.encodeFrame` :55-62), ADR-002 §7 (binding UDS-path regression obligation), SPEC FR-29, and RISK-TEST R-23 all pin the same seam and the same byte-equivalence assertion.
- The SR-05 Registry Touchpoint Fence enumerates exactly four touchpoints (ARCHITECTURE §Fence, ADR-004 §3) and names everything else (per-turn drain, re-register overwrite, #4140 absent-session no-op) as out-of-scope follow-ups — the open-ended "except where the precedence chain requires it" escape hatch is closed concretely.
- **Residual inconsistency (Watch Item 1) — RESOLVED 2026-06-08**: ADR-001 §1 (`cycles.js` API list) and ADR-002 §2.4 (file-absent branch) previously described the pre-rev2 `anyOtherCycleFile` canary, contradicting the authoritative ADR-006 rev2 and the (correct) ARCHITECTURE.md Integration Surface line 86 and SPEC FR-09. Corrected via the doc-sync fix on 2026-06-08 (ADR-001 → Unimatrix #4836, ADR-002 → #4837; `anyOtherCycleFile`/readdir scan/0.20 threshold removed, file-absent branch rewritten to the subagent-gated inheritance-drift rule). Architecture consistency is now byte-clean across the full ADR set; the prior WARN basis no longer holds, so Architecture Consistency is PASS.

### Specification Review
FR-01..FR-29 map cleanly to AC-01..AC-10 via the verification table; each carries a test hook. The canary rework is reflected correctly and completely in the spec: FR-09 states the subagent-gated increment ("iff depth ≥ 1 AND no tracker for the inherited root id"), explicitly removes the `anyOtherCycleFile` condition and the `fnf_record_send_count` denominator, and excludes depth-0 never-declare sessions; FR-10 states the zero-tolerance invariant and explicitly removes the 0.20 threshold and the baseline-measurement ritual; the domain-model entry for `stamp_miss canary` matches. The new FR-29 (AC-10) is precise about the seam (`transport-uds.encodeFrame`), the drive path (`runFireAndForget`, `config.mode="uds"`), and the byte-equivalence assertion against the HTTP body. OQ-A..OQ-E are correctly handed to delivery; OQ-E is the canary-independence crux (Watch Item 2) and OQ-D the gate-exit precondition (Watch Item 3). AC-07's declared-sessions-only accuracy methodology is well-reasoned and ratified; the never-declare floor it structurally excludes is separately protected by the strengthened SR-06 sample + live-DB distribution comparison.

### Risk Strategy Review
RISK-TEST-STRATEGY is rev2 and traceable: all 12 scope risks map to architecture risks in the Scope Risk Traceability table; **23 architecture risks (R-01..R-23)** each carry ≥1 scenario; security risks (path traversal via session_id, topic-content injection into DB and the content-free breadcrumb) and a failure-mode table are enumerated. The rework is reflected: **R-19** records that the original noisy-tripwire / 0.20-ratio risk is "RESOLVED by design — no threshold, no concurrent-file rule, no baseline" and names the NEW residual (subagent-context-detection independence, Branch A/B); **R-23** (new) covers the AC-10 UDS-path stamp-equivalence obligation; **R-07/SR-09** are updated to the merged-tree reality (seam now a post-merge regression tripwire against real anchors). Vision-critical coverage is present: graceful degradation (R-03 fail-open, R-09 never-declare floor regression) and the now-contractual mis-attribution risk (R-10 — a non-canonical topic stamped 'declared' with full weight, worse than the advisory #1469 class because the stamp suppresses the self-correcting extraction). The #3486 field-extracted-but-not-inserted class is escalated to Critical (R-01) with per-site round-trip evidence required. No vision-relevant risk is omitted.

## Prior Canary Watch Item — Resolution Status

**RESOLVED BY DESIGN.** The prior report's canary concern (Watch Item 2: the `stamp_miss` canary as a 0.20-threshold tripwire noisy-by-construction in a repo where unattributed sessions are the norm) is eliminated at its root by ADR-006 rev2: the 0.20 threshold, the `fnf_record_send_count` denominator, the `anyOtherCycleFile` concurrent-file rule, and the per-deployment baseline ritual are all removed. The canary is now a subagent-gated, zero-tolerance inheritance-drift invariant that counts only drift within the declared population and explicitly does not count depth-0 never-declare sessions — directly embodying the product principle that unattributed sessions are not, by their nature, a problem. R-19 confirms the false-signal source is removed.

This resolution introduces one *new, narrower* residual (Watch Item 2 above): the production canary now depends on subagent-context-detection being independent of root-id inheritance (OQ-E / ADR-006 §7, delivery-probe-gated, Branch A/B). It is correctly bounded and disclosed, the test-time invariant ships regardless, and Branch B (test-time-only) is an honest fallback. Net: the original concern is closed; the replacement is a smaller, properly-fenced delivery probe, not an open design risk.

## Knowledge Stewardship
- Queried: /uni-query-patterns (context_search, tags=[vision]) for recurring alignment/scope-addition/milestone misalignment patterns across prior features — only weak, non-applicable matches surfaced in the first cut (#2298 config-key divergence 0.40, #3337 arch-header/spec divergence 0.26); neither applies here.
- Stored: nothing novel to store. The one recurring-shaped observation is feature-internal, not cross-feature: a multi-revision ADR set can leave a *superseded mechanism stranded in upstream ADRs* (ADR-001/002 retaining `anyOtherCycleFile` after ADR-006 rev2 retired it) even when the synthesized ARCHITECTURE.md and SPECIFICATION are correct. This is one instance; if vnc-027/crt-052 retros show the same ADR-revision-drift shape, promote it to a stored `vision`/`pattern` entry ("regenerate/re-scan ALL ADRs on a mid-design rescope, not just the synthesized docs"). Feature-specific for now.
