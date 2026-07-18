# Alignment Report: vnc-048

> Reviewed: 2026-07-18
> Artifacts reviewed:
>   - product/features/vnc-048/architecture/ARCHITECTURE.md
>   - product/features/vnc-048/specification/SPECIFICATION.md
>   - product/features/vnc-048/RISK-TEST-STRATEGY.md
> Scope source: product/features/vnc-048/SCOPE.md, SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md; goal #5673 (personal-cloud)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Delivers the trailing-edge operator-CLI capability for the personal-cloud destination (#5673); reuses the single-funnel isolation discipline the goal names; honors append-only-audit and hash-chain principles. |
| Milestone Fit | PASS | Correctly scoped to the delivery:proven runtime arc; refuses future-milestone work (other 6 CLIs, live-daemon import, new base mechanism). No forward over-build. |
| Scope Gaps | PASS | All 5 SCOPE goals and 13 ACs carried into spec FRs + verification methods and architecture ADRs. Nothing dropped. |
| Scope Additions | WARN | No new scope, but the export stderr count summary now also lands on **no-`--slug`** export — in tension with the AC-05 "byte-for-byte identical" claim. Faithful to SCOPE (SCOPE itself declares it "not a behavior change"), but the tension is unreconciled in the AC-05 verification. |
| Architecture Consistency | PASS | Architecture maps SCOPE approach exactly (one funnel, one base derivation, one validation edge); ADR-001..006 trace to SRs/ACs; integration surface names exact signatures; four-shape coverage axis matches spec NFR-3. |
| Risk Completeness | PASS | SR-01..11 traced to R-01..14; security (traversal, destructive-write blast radius), edge cases, integrity-relevant risks (chain/hash validation, append-only audit) all covered; gate non-negotiables named. |

**Counts:** PASS 5, WARN 1, VARIANCE 0, FAIL 0.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE goal (1–5) and AC (01–13) appears in SPECIFICATION FR-1..16 / AC-01..13 with a verification method, and in ARCHITECTURE ADR-001..006. |
| Addition | Export stderr summary on **no-`--slug`** export | Spec FR-8: "applies to export in both slug and no-slug modes." This adds a stderr line to existing single-project export. Traceable to SCOPE (AC-06 unrestricted; Proposed Approach declares it "not a behavior change"), so it is faithful to scope — but see WARN below. |
| Simplification | Base resolution reuses `data_dir.parent()` — no `--base` flag | Rationale: SCOPE Non-Goal — "a second configuration scheme for the same value is the single thing this design most refuses." Intentional narrowing, documented. |
| Simplification | Restore target limited to a freshly-registered (audit-empty) slug | Rationale: C-5 — append-only `audit_log` cannot be cleared; explicit-`event_id` INSERT would collide. Non-goal + fail-loud, documented. |

## Variances Requiring Approval

None at VARIANCE or FAIL level. One WARN and one advisory item for human/vision-session awareness:

### WARN-1 — "byte-for-byte identical" vs. the no-slug export stderr summary
1. **What**: SCOPE AC-05 and spec NFR-1 promise no-`--slug` export/import is "byte-for-byte identical" to today. Spec FR-8 applies the new stderr count summary to no-slug export as well. Adding stderr output is an observable change to the no-slug path.
2. **Why it matters**: Milestone-discipline / regression principle. The reconciliation ("stderr is not part of byte-for-byte") is implicit. The AC-05 verification ("existing export/import integration suites pass unchanged") will fail if any existing test asserts on empty/absent stderr — a seam the risk strategy does not explicitly cover (R-09 asserts path parity; R-10 asserts the summary is present; neither asserts existing stderr-sensitive tests still pass).
3. **Recommendation**: Accept (low-risk, faithful to SCOPE intent) but tighten one sentence in AC-05/NFR-1 to state explicitly that "byte-for-byte identical" scopes the **exported file and stdout / exit code**, not stderr; and add a one-line note to R-09 that no existing export test asserts stderr emptiness (or fix it if one does). No scope change required.

### ADVISORY (vision-session action, not a doc defect) — capability #5586 retag
1. **What**: All three source docs (SCOPE Background Research, ARCHITECTURE OQ, SPEC "NOT in Scope") correctly flag OQ-5: capability #5586 (BACKUP-RESTORE) is tagged `delivery:proven` but is proven only for local single-project — unproven for the cloud shape the personal-cloud goal names as the destination.
2. **Why it matters**: Knowledge-integrity goal ("accurate data, never stale... or contradictory"). A `delivery:proven` tag that overclaims the cloud shape is exactly the kind of stale/overclaimed knowledge the integrity goal guards against. The docs correctly refuse to fix it here (the vision session owns the tag).
3. **Recommendation**: The vision session should, on AC-09 + AC-10 evidence once vnc-048 delivers: flip `#5586 delivery:proven → partial`, tighten `proven_by` to name the resolver and shape, and restore `proven` only for the shape the evidence covers. Do not restore on an export-only fix. Use `context_correct`, not deprecate+store.

## Detailed Findings

### Vision Alignment
- **Goal #5673 (personal-cloud)** names "multi-PROJECT, multi-CLIENT" routed by operator-declared slug as *the destination*, with per-slug isolated stores. The goal's own success criteria state the runtime arc (per-slug routing, isolated stores) is delivered; SCOPE confirms the runtime is `delivery:proven` while the operator CLI never followed to multi-project. vnc-048 closes exactly that trailing edge. Direct advancement of the goal.
- **Single-funnel invariant.** Goal #5673: "One isolation seam... `resolve_store(request) -> Arc<Store>` is the single funnel through which all data resolves." The architecture mirrors this discipline for the CLI with `resolve_slug_store` as the single validate→derive-base→join→existence-gate funnel, `per_slug_data_dir` as the only join site, `&ProjectSlug` (never `&str`) as the only crossing type. Vision-consistent by construction.
- **Architectural Principle 2 (append-only, complete audit log).** The design is built around it: the `--skip-quarantined`/`audit_log` asymmetry is left untouched (a filtered audit log "would be the defect"), and the non-empty-audit restore refusal exists precisely because `drop_all_data` cannot clear the append-only `audit_log`. Alignment is a load-bearing part of the design, not incidental.
- **Principle 1 (hash chain integrity).** AC-10 round-trip validates content-hash + chain-link via `chain_verify`. Preserved.
- **Principle 6 (single binary, zero required infra).** No new crates, no external services, no new configuration scheme. Reinforced.
- **Principle 8 (no secrets in DB), 3/4/5/7.** N/A to this operator-CLI feature — legitimately untouched; this is a proportional infrastructure/CLI feature, not a core intelligence-pipeline change.
- **DR framing.** Goal #5673 success criterion states "backup = volume snapshot; recovery = restore + restart." SCOPE explicitly does NOT reframe this (Non-Goal: "Backup as disaster recovery"); vnc-048 delivers *per-project portability* ("owning your knowledge") which a volume snapshot cannot deliver. This is complementary to the goal's DR criterion, not in conflict — and the explicit refusal to reframe DR is evidence of vision discipline.

### Milestone Fit
- Targets the current personal-cloud arc (runtime already `delivery:proven`); builds only the operator-CLI catch-up. No future-milestone capability is built ahead of need: live-daemon import (locking/daemon-mediated) is refused outright; slug-awareness for the other six CLIs is explicitly deferred (the feature only establishes the `--slug` + `resolve_slug_store` pattern for them to copy); no new base mechanism. This is textbook milestone discipline — the narrowness the parent flagged as intended is honored across all three docs.

### Architecture Review
- ADR-001 (reuse triad), ADR-002 (pre-open existence gate on file existence not registration), ADR-003 (live-PID-only refusal → clobber structurally unreachable), ADR-004 (vector rebuild into `slug_dir/vector`, PID stays base-scoped), ADR-005 (non-empty-audit pre-flight refusal), ADR-006 (fail-loud with resolved path + export summary) each trace to specific SRs and ACs. Integration surface enumerates exact signatures and source line references; downstream agents are told to invent none. The four-shape coverage axis (in-container / local dev / `_with_base` hook / host bind-mount) matches spec NFR-3 and the risk strategy R-05/R-06. Consistent; no drift between architecture and spec.

### Specification Review
- FR-1..16 cover the full SCOPE surface; each AC carries a concrete verification method. OQ-1..OQ-4 resolutions from SCOPE are honored verbatim (live-PID-only hard error, non-empty-audit refusal, README-canonical sequence, export-only summary). "NOT in Scope" list matches SCOPE Non-Goals exactly. The single spec-level generalization — FR-8 stderr summary on no-slug export — is the WARN-1 tension above; it is traceable to SCOPE and not an unauthorized addition.

### Risk Strategy Review
- Scope Risk Traceability table maps every SR-01..11 to R-01..14 with resolution/coverage. Two gate non-negotiables are correctly identified (R-01 S1 AC-09 disagreement seam with disjoint non-empty hash set; R-03 S2 served vector search after the full `register→stop→import→start` sequence) — both target the exact failure mode (#5507 two-resolver trap, #4974 ceremonial-seam) the vision's integrity posture cares about. Security section bounds the destructive-write blast radius (live-PID + non-empty-audit refusals) and proves structural traversal closure at `ProjectSlug::try_from`. Behavioral-outcome coverage drives the operator's real CLI entry point, not a seam beneath it. Complete for the feature's risk surface. The one uncovered seam is the no-slug-stderr regression noted in WARN-1.

## Knowledge Stewardship
- Queried: /uni-query-patterns + context_search "vision alignment scope addition milestone discipline" — surfaced #3742 (optional-future-branch must match scope intent; WARN when architecture/risk diverge from scope deferral) and #2298 (config semantic divergence vs vision example). Neither fired here: vnc-048's docs do not diverge from SCOPE deferrals — the only tension (WARN-1) is internal to SCOPE itself and faithfully carried forward, not a doc-introduced divergence.
- Declined: nothing novel to store. WARN-1 is a feature-specific internal-consistency tension (stderr not part of "byte-for-byte"), not a recurring cross-feature misalignment class; #3742 already covers architecture/risk-vs-scope divergence. Promotable at retro only if the "no-op-side-channel output vs byte-for-byte-parity claim" tension recurs in the sibling CLI slug-awareness work.
