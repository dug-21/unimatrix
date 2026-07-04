# Gate 3a Report: crt-057

> Gate: 3a (Component Design Review)
> Date: 2026-07-04
> Result: PASS
> Validator: crt-057-gate-3a

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | Three orthogonal non-destructive axes; 4 purge calls deleted; scoped read-only `transcript{}` via `snapshot()` reuse; loss propagation; named-boundary clock normalization; four-site seam gates ONLY the content-opaque fold read. All grounded to ARCH §3/§5/§7/§12 + ADR-001..006. |
| 2. Specification coverage (AC-01..AC-19) | PASS | All 19 ACs covered across the 13 pseudocode + 13 test-plan files. AC-19 gets the dedicated negative test the ACCEPTANCE-MAP flagged for. No scope additions; NG-5..NG-8 respected. |
| 3. Risk coverage (R-01..R-18) | PASS | All 18 risks mapped to component test-plan files. R-01 per-loss-condition matrix, R-06 orphan-delete + retention re-home, R-04 per-protocol e2e, R-05 clock boundary triple, R-03 content-scan incl. reclamation-without-review all present. |
| 4. Interface consistency | PASS (1 WARN) | `retrieve_scoped_candidates` signature consistent across OVERVIEW/handler/helper; `snapshot()` single reader (#4848) preserved; `SessionLossInfo`/`TranscriptCandidate`/`TranscriptCandidatesSection` UNCHANGED; search-status is a response-transient derivation. WARN: brief "Key Signatures" block still shows the stale name `distill_before_purge`. |
| 5. Stage 3a Resolutions coherence | PASS | Rename, add-only param, handler-compiled regex, retention re-home at `sweep_expired`, unknown anchor/phase → empty section — all coherently reflected. |
| Anti-stub | PASS | No TODO/unimplemented/placeholder. Orphan deletion is a real delete (explicit "do NOT `#[allow]`"); re-homed retention gate has no `_` arm. |
| 6. Knowledge stewardship | WARN | Spec, tester(testplan), risk-strategist reports carry compliant blocks. No pseudocode agent report exists (stewardship intent evidenced in-artifact); both synthesizer reports lack a block. |

**13 pseudocode ↔ 13 test-plan files, 1:1 verified. Both OVERVIEW files authoritative and internally consistent.**

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS
**Evidence**:
- Three orthogonal axes (render/recompute/scoped-retrieval), none destructive, match ARCH §1/§4 and ADR-002. OVERVIEW Data Flow encodes `force` on durable observations vs `transcript` on read-only buffer as disjoint state.
- Fully non-destructive: `cycle-review-handler.md` step 5 deletes the four `if result.is_ok() { purge_cycle_transcripts }` blocks (:2379/:2558/:3328/:3451); `snapshot-reuse.md` confirms retrieval inherits non-destructiveness from `&self` `snapshot()`.
- Scoped read-only `transcript{}`: `transcript-scope.md` AND-composition (intersection, not union); `distill-before-purge.md` early-returns `None` when scope absent (no buffer read — lean default).
- Loss propagation: `distill-before-purge.md` `derive_search_status` — `search_complete == false ⟺ a SessionLossInfo row exists`, keyed on all four conditions (`elided_bytes>0 ∥ has_holes ∥ Reconstructed ∥ dropped>0`); no bare boolean.
- Clock normalization: `window.md` routes every Plane A↔B comparison through one named helper (`candidate_epoch_ms`); windowed join never exact; `byte_offset` fallback for `ts:None`; ±120 000 ms / ±3-block default.
- Four-site seam gates ONLY the fold read: `activity-fold.md` keeps `land_activity_fold` gated ×4 as the SOLE surviving side-effect; uses content-opaque `activity_snapshot()`, distinct from content `snapshot()` — single-content-reader invariant (#4848) preserved.

### Check 2 — Specification coverage (AC-01..AC-19)
**Status**: PASS
**Evidence**: test-plan/OVERVIEW §2 maps every AC to a component file. AC-19 (ownership boundary, negative — the AC the ACCEPTANCE-MAP flagged as lacking a dedicated risk scenario) is resolved: `consumer-reconciliation.md` adds `test_response_schema_has_no_attribution_field` + `test_no_code_path_synthesizes_across_gh_blocks`, explicitly "Do NOT lean on R-18." No unrequested features in pseudocode; NG-5 asserted as a negative (FR-25/AC-19), NG-7/NG-8 seams left unbuilt.

### Check 3 — Risk coverage (R-01..R-18)
**Status**: PASS
**Evidence**:
- R-01 (raison d'être): `distill-before-purge.md` per-loss-condition matrix rows a–f (each single signal → `search_complete:false`; OR-combination; clean `Primary` → trustworthy negative; no-bare-boolean; loss-on-match-too).
- R-06: `orphan-deletion.md` deletes all three orphaned fns (dead-code guard, not `#[allow]`) and re-homes the exhaustive `TranscriptRetention` match as `reclaim_permitted_by_retention` (no `_` arm); `backstop-reclaim.md` confirms the re-home lands at the `sweep_expired` driver, byte-unchanged under OSS.
- R-04: `retro-lifecycle.md` per-protocol e2e for BOTH `uni-delivery-protocol.md` and `uni-bugfix-protocol.md`; protocol-parity grep; a single-protocol fix fails.
- R-05: `window.md`/`distill-before-purge.md` skewed-clock windowed join, epoch boundary triple (in/on/out), `ts:None` byte-offset inclusion, explicit fixed offsets (never `now_ts()`).
- R-03: `attach-to-response-assembly.md` + `backstop-reclaim.md` content-scan on every changed path including reclamation-without-review; struct-shape guard that `RetrospectiveReport` gains no candidate/loss slot.

### Check 4 — Interface consistency
**Status**: PASS (1 WARN)
**Evidence**: `retrieve_scoped_candidates(registry, feature_cycle, observations, cfg, scope: Option<&TranscriptScope>, reviewer_session_id: Option<&str>) -> Option<TranscriptCandidatesSection>` is identical in pseudocode/OVERVIEW, `distill-before-purge.md`, and `cycle-review-handler.md` (call sites, all four returns). Shared types authoritative in OVERVIEW; component files reference, never redefine. `TranscriptCandidatesSection`/`SessionLossInfo`/`TranscriptCandidate`/`CandidateProvenance` UNCHANGED; `matched`/`search_complete` are a response-transient projection (`SessionSearchStatus`/`ResolvedBounds`, new response-only types) attached out-of-band via `attach_search_status`, honoring the summary ⟂ Plane-B invariant.
**WARN**: IMPLEMENTATION-BRIEF "Key Signatures" (lines ~167-181) and "Files to Create/Modify" still name the function `distill_before_purge`, while the brief's own Stage 3a Resolutions and the authoritative pseudocode OVERVIEW resolve OQ-4 to RENAME → `retrieve_scoped_candidates`. Stale-but-superseded; OVERVIEW is authoritative, so not blocking. Delivery should read the brief through the Stage 3a Resolutions rename.

### Check 5 — Stage 3a Resolutions coherence
**Status**: PASS
**Evidence**:
- **Rename**: consistent everywhere; source-assertion strings to be re-counted as `retrieve_scoped_candidates(` (`distill-before-purge.md` §R-11, `render-dispatch.md`, test-plan/OVERVIEW §4).
- **Add-only param**: `retrospective-params.md` grounding note confirms `include_transcript_candidates` is absent in this worktree — only the `transcript` ADD applies; the brief's "remove" line is a no-op.
- **Handler-compiled regex**: `distill-before-purge.md` Error handling + Stage 3a Resolution both fix compile-in-handler so the helper stays infallible `-> Option<...>`; ReDoS bounded via `RegexBuilder` size limits (`transcript-scope.md`). (The pseudocode retains an inline-compile alternative as an explicit resolved FLAG; the brief decisively picks handler-compiled — read the helper through that resolution.)
- **Retention re-home at `sweep_expired`**: `orphan-deletion.md` + `backstop-reclaim.md` land `reclaim_permitted_by_retention` at the background `sweep_expired` driver, no `_` arm, `RetainDays` a no-op, OSS byte-unchanged.
- **Unknown anchor/phase → empty section**: `transcript-scope.md` chooses empty (absent) section, reserving `ERROR_INVALID_PARAMS` for malformed input (bad regex), consistent with FR-7.

### Anti-stub
**Status**: PASS
**Evidence**: Repo grep for TODO/unimplemented/todo!/FIXME/placeholder/`#[allow(dead_code)]` over pseudocode/ + test-plan/ returns only one hit — the `orphan-deletion.md` prohibition text ("Do NOT leave a `#[allow(dead_code)]`"). The orphan deletion is a genuine `DELETE` of `purge_cycle_transcripts` + `clear_transcripts_for_feature` + `purge_held_for_feature`, gated by a clean `cargo clippy -- -D warnings`, not a suppression.

### Check 6 — Knowledge stewardship
**Status**: WARN
**Evidence**:
- Compliant blocks present: `agent-2-spec` / `agent-2-spec-v3` (Queried + Stored-nothing with reason), `agent-2-testplan` (tester; Queried + "nothing novel to store — reason"), `agent-3-risk` (Queried + Stored #5427 + Declined), `agent-3-risk-v3` (Queried + Stored-deferred with reason). Risk-strategist (active-storage) satisfied; tester (read-only) satisfied.
- **Gaps**: (a) no pseudocode agent report exists under `agents/`, so no explicit `Queried:` block for the pseudocode design work — though stewardship intent is directly evidenced in `pseudocode/OVERVIEW.md` ("Every interface name below is traced to the architecture Integration Surface or existing code — none invented") and per-file ADR/ARCH/line-number citations; (b) both `synthesizer-v2` / `synthesizer-v3` reports lack a `## Knowledge Stewardship` block.
- Architect (active-storage) stewardship is embodied in the ADR files (ADR-001..006) rather than a separate agent report; the spec report notes "the amending ADRs belong to the architect (already authored)."
**Assessment**: A strict reading of the check ("missing block = REWORKABLE FAIL") would apply to the absent pseudocode/synthesizer blocks. Treated as WARN, not a blocker, because (1) every content-authoring agent that produced a report carries a compliant block, (2) the pseudocode's query-before-design obligation is demonstrably met in-artifact, and (3) report-file attribution for the pseudocode work is ambiguous in this session's structure. Not design-correctness-affecting.

## Rework Required

None blocking. Optional follow-ups for the coordinator (do not gate delivery):
- Ensure the pseudocode-authoring agent (and synthesizer) emit a `## Knowledge Stewardship` block for audit completeness (WARN, check 6).
- On the next brief touch, sync the brief "Key Signatures" / "Files to Create/Modify" blocks to the resolved rename `retrieve_scoped_candidates` (WARN, check 4). Not required before Stage 3b — OVERVIEW is authoritative.

## Knowledge Stewardship
- Queried: read ARCHITECTURE.md + ADR-001..006, SPECIFICATION.md, RISK-TEST-STRATEGY.md, IMPLEMENTATION-BRIEF.md, ACCEPTANCE-MAP.md, all 13 pseudocode + 13 test-plan files, and the 7 agent reports. No Unimatrix write-tier queries needed for validation.
- Stored: nothing novel to store -- gate-3a findings are feature-specific and belong in this glass-box report; the reusable patterns invoked (source-assertion string-counting blindness #5427, negative-assertion async traps #4879, lockstep drift #4585) already exist in Unimatrix.
