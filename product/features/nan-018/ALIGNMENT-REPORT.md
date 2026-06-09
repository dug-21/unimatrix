# Alignment Report: nan-018

> Reviewed: 2026-06-09 (R2 — refreshed after human design decisions)
> Artifacts reviewed:
>   - product/features/nan-018/architecture/ARCHITECTURE.md (§3.1 penalty-site count corrected; §7 Locked Decisions added)
>   - product/features/nan-018/architecture/ADR-001…006-*.md (ADR-006 new)
>   - product/features/nan-018/specification/SPECIFICATION.md (FR-03, FR-22, FR-12a updated)
>   - product/features/nan-018/RISK-TEST-STRATEGY.md
> Scope source: product/features/nan-018/SCOPE.md
> Scope-risk source: product/features/nan-018/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md
> Goal source: Unimatrix entry #4677 (goal:self-learning)
> Code verification: services/search.rs:727,729 (two penalty-application sites); background.rs:583 (tracing::error! log string, not a site)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances goal:self-learning success criterion "eval harness confirms MRR improvement"; "verify, don't hope" thesis is the goal's own logic. ADR-006 reinforces the formula-authority boundary the goal depends on. |
| Milestone Fit | PASS | Nanoprobes (build/CI/test-infra) is the correct phase for an eval-instrument upgrade; no premature future-milestone capability built. |
| Scope Gaps | PASS | All 14 ACs and 8 goals map 1:1 into spec FR/NFR and architecture components; the new Locked Decisions resolve open questions without dropping scope. |
| Scope Additions | PASS | The optional 5th "dead-end" fixture shape is within AC-06's "at minimum" envelope. The former `background.rs` over-enumeration is **removed** (corrected to two search.rs sites). ADR-006 is a boundary clarification, not new scope. No unapproved additions remain. |
| Architecture Consistency | PASS | Architecture §3.1, spec FR-03, and the Integration Surface now agree on exactly two penalty sites (search.rs:727/:729). Locked Decisions (§7) are reflected identically in spec FR-12a/FR-22. Code-verified. |
| Risk Completeness | PASS | All 8 SRs traced to architecture risks with test treatments; R-04 elevated to a named human delivery gate; silent-wrongness weighting matches an instrument's failure profile. |

Counts: PASS 6, WARN 0, VARIANCE 0, FAIL 0.

**Change from prior report (R1):** WARN-1 (architecture/spec disagreement on penalty-site count, naming a second site in `background.rs`) is **RESOLVED and withdrawn**. Both source docs now correctly name two penalty-application sites, both in `services/search.rs`; `background.rs:583` is documented in both as a `tracing::error!` log string that is explicitly NOT a threading target. Verified against current code. Scope Additions and Architecture Consistency consequently move WARN→PASS. The new Locked Decisions and ADR-006 introduce **no** new variance.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE goal (1–8) and AC (01–14) is carried into the spec (FR-01…31, NFR-01…08, AC verification table) and the architecture component table. No requested capability is missing. |
| Addition (resolved) | `background.rs` FALLBACK_PENALTY consumption site | **No longer an addition.** The prior over-enumeration that named `background.rs` as a threading target is corrected. Architecture §3.1 and spec FR-03 now both name exactly two penalty-application sites — `search.rs:727` (fallback branch) and `search.rs:729` (graph_penalty) — and both explicitly classify `background.rs:583` as a `tracing::error!` log string, not a site. Code-verified: the only two non-test penalty applications are at search.rs:727/:729; background.rs:583 is the cycle-detected log line. The R-01 enumerated-site grep guard remains the source of truth. |
| Addition (within envelope) | Optional 5th fixture shape: "dead-end chain" (DEAD_END_PENALTY) | SCOPE AC-06 requires "at minimum" four shapes. Spec/architecture add an optional 5th dead-end-chain shape exercising `DEAD_END_PENALTY`. SCOPE explicitly uses "at minimum," and exercising DEAD_END_PENALTY keeps the AC-01 tunability sweep non-degenerate for that lever — within the authorized envelope, not creep. Unchanged from R1. |
| Addition (boundary clarification) | ADR-006 — penalty config exposure is eval-only | New since R1. ADR-006 does not add capability or scope; it records the deployment boundary already mandated by SCOPE C-02 and ASS-037 (#3984) authority — "the new GraphPenaltyConfig is measurement-only, deployed defaults stay fixed." It is the deployment-side complement to ADR-001's partial supersession of crt-014 ADR-006. This tightens, not broadens, scope. Aligned. |
| Simplification | Cost-proxy precision+k narrowing | NOT taken. ADR-003 / FR-14 keep the explicit token-weighted metric as primary. The §7.1 lock (ε=0.0, advisory) further pins cost as report-only. Correctly resolved. |
| Simplification | Drift-guard frozen vector sidecar (OQ-3 branch a) | NOT taken; branch (b) chosen (embed-model-id/dim in the hash), satisfying the binding "must not silently become embed-model-dependent" constraint. Stated in spec Durability branch and ADR-002. Correctly resolved. |

## Variances Requiring Approval

None. No VARIANCE or FAIL findings, and — following resolution of WARN-1 — no WARN findings either. The three human-ratified Locked Decisions and ADR-006 are all in-scope, on-vision boundary commitments; none requires further approval.

## Detailed Findings

### Vision Alignment (PASS)
The feature's thesis — "verifiable self-improvement of retrieval requires an instrument that measures trust and cost, not just relevance" — is the internal logic of goal:self-learning (#4677) made buildable.

- Goal #4677 success criterion: *"Eval harness confirms MRR improvement on behavioral scenarios after GNN training."* The product already designates the eval harness as the verification organ of self-learning. nan-018 widens that organ from positive-relevance-only to trust + cost — exactly the SCOPE Strategic Framing claim ("Improvement you cannot verify is hope").
- Goal #4677 success criterion: *"Manual weight tuning is unnecessary … the system converges from cold-start defaults."* **ADR-006 actively protects this criterion**: by recording that the new penalty config surface is eval-only and that adopting a swept value as a deployed default is an ASS-037 (#3984) decision outside nan-018, it prevents the new tunability levers from being mis-read as a manual-tuning surface. The boundary clarification is squarely on-vision — it keeps the instrument from becoming a backdoor into the deployed formula.
- Goal #4677 success criterion: *"Learned weight vectors checksummed and input-hashed per training run to detect tampering."* nan-018's retrieval-shape hash (AC-08) applies the same hash-discipline mental model to the eval corpus, mirroring PRODUCT-VISION §1 ("hash chain integrity is immutable"). The §7.2 lock — **HARD ERROR on primary-corpus shape mismatch** — strengthens this: it treats an invalid yardstick as a precondition failure (abort), not a degraded reading, which is the correct on-vision posture for protecting integrity of the measurement spine.
- Vision §"The function learns. Every session makes it better." A self-improving relevance function is unfalsifiable without an instrument that distinguishes "better" from "merely different." nan-018 supplies that discriminator. On-vision.
- No architectural principle is contradicted. Config exposure stays additive (C-02, NFR-01/02; ADR-006 deployment boundary), preserving principle 5 (graceful degradation / defaults unchanged) and ASS-037 authority. Eval remains internal Rust tooling (C-05, NFR-06), preserving principle 6 (single-edge-language JS/TS client; binary is the adapter).

### Milestone Fit (PASS)
nan-018 is filed under Nanoprobes (`nan`, build/deploy/CI) — the correct home for a test-instrument upgrade. The design holds milestone discipline:

- Does **not** build crt-053's retrieval behavior (Non-Goal 3) — Cortical phase.
- Does **not** answer the measurement questions itself (Non-Goal 2) — downstream (rewritten) ass-073 / ass-074.
- Does **not** wire eval-execution-as-a-quality-gate (Non-Goal 1) — explicitly deferred. The §7.1 ε=0.0 *advisory* cost-gate lock reinforces this: cost growth is reported, never blocks; eval is the instrument, not the referee.

The §7.2 HARD ERROR on primary-corpus shape mismatch is a deliberate, scoped exception to the eval exit-0 convention — and the docs justify it correctly as protecting *corpus validity* (a precondition), distinct from the *quality verdict* (which stays advisory/exit-0). This is not a workflow-gate breach; it does not turn eval results into a standing decision gate. Milestone boundary intact.

### Architecture Review (PASS)
- **Penalty-site count now consistent and code-verified.** Architecture §3.1 names exactly two penalty-application sites, both in `services/search.rs` (`:727` fallback branch, `:729` graph_penalty), and explicitly classifies `background.rs:583` as a `tracing::error!` log string that is NOT a threading target. The Integration Surface (§6) and spec FR-03 carry the same two-site enumeration. I verified directly against current code: the only two non-test penalty applications are search.rs:727/:729; background.rs:583 is the cycle-detected log line; all other references (search.rs:1973–2044) are in `#[cfg(test)]`. The prior R1 inconsistency is gone.
- **Locked Decisions (§7) are internally and cross-document consistent.** §7.1 (ε=0.0 advisory cost gate) ↔ spec FR-12a; §7.2 (HARD ERROR primary / WARN snapshot) ↔ spec FR-22 and AC-08(b); §7.3 (R-04 named human delivery gate) ↔ risk strategy R-04 item 3 and AC-08(f); §7.4 (penalty deployment boundary) ↔ ADR-006. No drift.
- **ADR-006 is well-formed and bounded.** It records the deployment boundary as a documentation/convention guarantee (honestly flagged as not type-enforced), names ASS-037 (#3984) as the authority for any future default adoption, and positions itself as the deployment-side complement to ADR-001's measurement-only supersession of crt-014 ADR-006. No scope expansion; it closes a mis-read risk on the new config surface.
- No new crate (C-01 / NFR-05); extends `unimatrix-server/src/eval/` plus additive engine/config touches; files ≤500 lines via dedicated submodules. Consistent with ADR-004.
- Corpus materialized as "just another snapshot source" consumed by the existing `EvalServiceLayer::from_profile` unchanged — cumulative test-infra reuse, not isolated scaffolding.
- Drift guard unifies OQ-3/OQ-4/OQ-5 into one hash definition (one "shape"); branch (b) stated explicitly; binding durability constraint satisfied by design.

### Specification Review (PASS)
- 1:1 AC traceability table maps every SCOPE AC to a concrete verification method. No AC is left without a test.
- **FR-03 corrected**: now states the two penalty-application sites are both in `search.rs` (`:727`/`:729`) and that `background.rs:583` is a log string, not a threading target — matching architecture and code. The R-01 enumerated-site grep guard remains the source of truth.
- **FR-22 updated**: corpus-dependent severity locked — HARD ERROR (abort, non-zero exit) on the primary fixture corpus; WARN (continue) on the production snapshot — with the rationale that the durable yardstick's numbers propagate to product ranking policy. AC-08(b) tests both tiers. Consistent.
- **FR-12a updated**: cost-growth gate advisory at ε=0.0, report-only, exit code unchanged. Consistent with NOT-in-scope #1 and §7.1.
- Boundary non-goals most at risk under delivery pressure are restated as hard constraints: FR-14 (token-weighted primary, narrowing only as justified non-deferral call) and FR-30/AC-13 (zero `.claude/protocols/` edits, git-diff hard gate). Correct defensive posture.
- Wave structure (Wave-1 = AC-01…09 + AC-14; Wave-2 = docs/Band-3) with NFR-04 zero-code-coupling preserves the AC-14 proof-by-use exit.
- Property-based-only ground truth for the primary corpus (FR-16, C-04) with loader rejection of null/literal-ID `expected` codifies crt-013 #703 and bans the ASS-037/ASS-039 self-consistency trap.

### Risk Strategy Review (PASS)
- All 8 scope risks (SR-01…08) trace to ≥1 architecture risk with a concrete test treatment; none accepted-without-treatment.
- Framing correctly weighted for an instrument: "silent-wrongness (false confidence) over loud-failure." For a tool whose numbers gate downstream decisions, this is the right lens and directly protects the goal:self-learning success criterion.
- **R-01 (Critical) is now fully aligned with the corrected docs**: the enumerated penalty sites are the two search.rs sites; the grep-guard / default-equivalence treatment is unchanged in force but no longer points readers at a phantom background.rs site. (The risk strategy's R-01 scenario-2 prose has been corrected to match — it now names the two `search.rs` sites (:727, :729) and classifies `background.rs:583` as a `tracing::error!` log string, excluded. All five source/coordination docs — ARCHITECTURE, SPECIFICATION, Integration Surface, RISK-TEST-STRATEGY, and the BRIEF — now carry the identical two-site enumeration. No residual lag.)
- R-04 (incomplete hash manifest → silent staleness) is correctly elevated to the §7.3 **named human delivery gate** — completeness against the real schema cannot be proven by a test alone; a human must certify no retrieval-relevant column was mis-classified as display-only. Honestly surfaced, not papered over. This obligation flows to the IMPLEMENTATION-BRIEF.
- R-15 (AC-14 passes trivially) remains the standout exit bar: proof-by-use means the instrument *measures* (non-vacuous trust signal across every required shape), not merely *executes*. Codified as the Wave-1 Exit Gate.

## Knowledge Stewardship
- Queried: `/uni-query-patterns` (context_search, topic=vision, category=pattern) for instrument/experiment-separation, scope-addition, and milestone-discipline patterns — surfaced only feature-specific divergence entries (#2298 config-key semantic divergence, #3337 architecture-diagram header drift, #4617 export-hash validity scope). None generalizes to nan-018's review; no recurring cross-feature vision-alignment pattern exists yet.
- Stored: nothing novel via `/uni-store-pattern`. The prior WARN-1 was a feature-specific instance of the already-recorded multi-site config-threading trap (#4070) and has now been resolved by document correction — it did not generalize. The candidate cross-feature pattern — "instrument-vs-experiment features must gate on proof-the-instrument-measures, not proof-it-executes" (R-15) — still appears in this one feature only; per stewardship rules (2+ features before storing) it remains a reassess-at-retro candidate, not a store-now.
