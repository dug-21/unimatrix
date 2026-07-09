# Alignment Report: vnc-047

> Reviewed: 2026-07-08 · Re-verified: 2026-07-09 (post source-doc revision)
> Artifacts reviewed:
>   - product/features/vnc-047/architecture/ARCHITECTURE.md (+ ADR-002, ADR-007)
>   - product/features/vnc-047/specification/SPECIFICATION.md
>   - product/features/vnc-047/RISK-TEST-STRATEGY.md
> Scope source: product/features/vnc-047/SCOPE.md
> Scope-risk source: product/features/vnc-047/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md + strategic goal entries (#5474, #5516, #5517, #5518, #5519)

## Re-verification note (2026-07-09)

Four changes landed after the first review. Alignment impact assessed:
1. **Whole-set-once** (replaces per-row accumulate/first-write-wins): the first tag-bearing `cycle_start` freezes the entire set via an EXISTS guard inside a `BEGIN IMMEDIATE` transaction; later starts are a wholesale no-op; a tagless start does not lock. **Vision-positive** — ADR-002 explicitly rejects a per-key/namespace rule *because it would force namespace parsing and violate value-opacity* (vnc-045 SD-8). The freeze reads row existence only, never tag values. Still squarely observe/annotate, not orchestrate.
2. **General run-identity label reframing** (workflow version, run mode, confidence-required, arm — not workflow-only): a use-case broadening with **no mechanical scope change** (same opaque strings, same junction, same surface). Mildly strengthens domain-agnostic fit (labels are not SDLC-specific). Does not move the feature toward orchestration — the engine still never acts on a label.
3. **NEW best-effort ack echo** (ADR-007, FR-12, AC-09, R-16): a new scope addition vs SCOPE.md — see Scope Additions / WARN-2.
4. **My two prior accuracy notes were applied** — FR-8 now states GC "protection by omission"; FR-7 now states "no per-tag audit event." Both resolved; the SD-9-parity overstatement is fixed.

**Prior verdict holds: PASS.** One previously-accepted WARN (deferred external A/B payoff) stands; one new WARN (ack-echo scope addition) is added for human acknowledgment. No VARIANCE, no FAIL.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Observe/annotate lane intact; whole-set-once reinforces value-opacity |
| Milestone / Goal Fit | WARN | Advances no strategic goal's claim-floor now; in-product value is a substrate, cross-run A/B external + deferred. Human-accepted (SR-04) |
| Scope Gaps | PASS | Every SCOPE goal (1-6) maps to an FR + AC; nothing dropped |
| Scope Additions | WARN | Best-effort ack echo (ADR-007/FR-12/AC-09) is additive beyond SCOPE.md; minor, non-gating, no new interface, respects Non-Goal #6 — flag for acknowledgment |
| Architecture Consistency | PASS | ADRs trace to SCOPE; whole-set-once + BEGIN IMMEDIATE consistent; prior accuracy notes applied |
| Risk Completeness | PASS | Strengthened: R-15 (EXISTS-guard TOCTOU) and R-16 (ack echo) added; 16 risks + security + failure modes |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Coverage | SCOPE Goals 1-6 | 1(opacity)→FR-2/AC-01; 2(set-once via hook)→FR-3/FR-6/AC-02; 3(durable junction)→FR-5/FR-8/AC-03/AC-04; 4(surfaced in review)→FR-9/FR-10/AC-05; 5(prefix convention)→FR-2/AC-07; 6(one tag model + reserved mutation home)→FR-5/NFR-5/AC-06 + ADR-006. No gap. |
| **Addition** | **Best-effort ack echo (ADR-007, FR-12, AC-09)** | **Not in SCOPE.md.** On the existing `context_cycle` ack: start-with-tags → "N tags accepted for recording"; non-start-with-tags → "tags ignored — only recorded at cycle start"; plus a listener wrote-set/frozen-skip tracing line. See WARN-2. |
| Refinement (not addition) | Whole-set-once (FR-6, AC-02a) replaces per-row first-write-wins | Tighter freeze semantics for the same scope goal #2 (set-once). Deliberately avoids namespace parsing to preserve value-opacity. Within scope; strengthens run-identity integrity. |
| Reframing (not addition) | `cycle_tags` = general run-identity labels, not workflow-only | Broadens use cases, not mechanism. Same opaque-string storage/surface. Within scope. |
| Correction (not addition) | GC protection by **omission** (ADR-005/FR-8); no per-tag audit event (FR-7) | Factual corrections applied per prior review; accurate against HEAD. Not scope changes. |
| Deliberate divergence | Tags have no length cap (`goal` has `MAX_GOAL_BYTES=1024`) | Value-opacity parity with entry tags; DoS noted as accepted risk. Consistent with "one tag model." |

## Variances Requiring Approval

No item rises to VARIANCE or FAIL. Two WARNs surfaced for human awareness/acknowledgment.

**WARN-1 — Feature ships a substrate with no in-product consumer** (unchanged, already accepted)
1. **What**: The payoff — cross-run A/B analysis of workflow/run methodology — is explicitly external and deferred (SCOPE Non-Goal #4, SR-04, assumption A4). What ships is storage + per-run surfacing; value is realizable only by an out-of-band analyst joining `cycle_tags`/`summary_json` labels against `cycle_review` metrics.
2. **Why it matters**: "Milestone Discipline" / "Vision Over Convenience." A feature whose value cannot be demonstrated inside the product risks being an unrealized substrate if no external consumer materializes (A4). Closest goal is **self-learning intelligence** (#5518) — the `(tag)` index is shaped as substrate for the deferred filter/learn-by-tag direction — but that is north-star, not a claim-floor advance. The 2026-07-09 general-run-identity reframing also nudges toward **domain-agnostic** (#5517) since labels are no longer SDLC-specific; still not a claim-floor advance.
3. **Recommendation**: **Accept** (already decided; SR-04 assessed Med/Low). `(tag)` index shaped so the deferred query direction needs no re-migration (NFR-7). Acknowledge at demo time that no in-product A/B consumer ships.

**WARN-2 — Ack echo (ADR-007/FR-12/AC-09) is a scope addition beyond SCOPE.md** (new)
1. **What**: A best-effort echo on the *existing* `context_cycle` ack string, plus a listener wrote-set/frozen-skip tracing line. It is not among SCOPE.md's goals 1-6 and appeared only in the 2026-07-09 revision.
2. **Why it matters**: "User Intent is Authoritative — additions require explicit approval." SCOPE.md did not request caller feedback on tag intake. Mitigating factors are strong: (a) it adds **no new MCP interface** and reuses the exact `goal` fire-and-forget ack precedent (tools.rs:4154-4160); (b) it is explicitly **best-effort / non-gating** (AC-09 MUST NOT block a gate; R-16 Low); (c) it **respects Non-Goal #6** (no pre-review read surface) — it echoes the caller's own submitted input as "accepted for recording," never reads stored `cycle_tags`, and cannot report the freeze outcome (that stays trace-only, with `context_cycle_review` as the authoritative read-back); (d) it is honestly worded as accept-for-recording, not a durability guarantee.
3. **Recommendation**: **Accept, with acknowledgment.** Proportionate, interface-stable, and it improves operator ergonomics for the set-and-forget write. Human should acknowledge the addition since it was not in SCOPE.md; no rework warranted. If the human wants strict scope hygiene, the alternative is to defer FR-12/AC-09 to a follow-up — but the cost/benefit favors keeping it given zero interface surface.

## Detailed Findings

### Vision Alignment — PASS
The vision is explicit: "Unimatrix is not an orchestration engine. It does not coordinate agents, schedule work, or manage workflows." vnc-047 remains firmly on the correct side: it **annotates and observes** a cycle run (opaque run-identity labels frozen at start, surfaced at review); it does not run, schedule, or coordinate the workflow. SCOPE Non-Goal #4 fences off cross-run aggregation. The 2026-07-09 changes do not erode this:
- **Whole-set-once** is a freeze of an observational label set — annotation, not control flow.
- The **general run-identity reframing** describes the run for later analysis; the engine still never acts on `run mode` / `confidence-required` / `arm`. No orchestration semantics enter.
- The **ack echo** is caller feedback text, not workflow control.

The feature also honors the "one tag model" north star by cloning the `goal` path (col-025) and entry-tag opacity model (vnc-045/nxs-008). Notably, ADR-002's rejection of a per-namespace freeze rule *specifically to avoid namespace parsing* keeps value-opacity intact — a vision-consistent design choice, not merely convenient.

### Milestone / Goal Fit — WARN
No goal's claim-floor is advanced today. Nearest connections: self-learning intelligence (#5518, deferred learn-by-tag substrate) and, post-reframing, domain-agnostic (#5517, non-SDLC-specific labels). Value external and deferred (WARN-1). Proportionate and human-accepted; should not be characterized as advancing a goal now.

### Architecture Review — PASS
- ADR-001..007 trace cleanly to the SCOPE approach. Two independent version cascades (schema v30→31; SUMMARY v5→6) remain correctly separated (ADR-001 vs ADR-004) — the codebase's recurring gate miss (#4153, #4373) is treated as two discrete cascades.
- **Whole-set-once (ADR-002)** is internally consistent: `BEGIN IMMEDIATE` + EXISTS guard makes the freeze atomic and race-safe; the PK `(feature_cycle, tag)` is retained for integrity, not as the freeze mechanism. The freeze reads existence only — value-opacity preserved.
- **Ack echo (ADR-007)** is carefully bounded: no new interface, best-effort, frozen-skip deliberately kept out of the caller-visible response (trace only) to avoid a new read surface — consistent with Non-Goal #6.
- Load-bearing constraint (bare MCP handler persists nothing; tags ride the hook path; no second persistence route) restated in ARCHITECTURE and SPEC FR-3.
- Prior accuracy notes now resolved in-doc (FR-7 no per-tag audit event; FR-8 protection by omission).

### Specification Review — PASS
FR-1…FR-12 and AC-01…AC-09 are individually testable with verification methods. AC-02/AC-05 correctly marked `[assembled-path]` with a `proven_by`-must-cite obligation (mitigates SR-08). FR-6/AC-02a state whole-set-once (including exact-equality across changed/subset/superset/different re-starts and tagless-start-does-not-lock) as intended, tested behavior. FR-12/AC-09 are explicitly flagged best-effort / non-gating — correctly preventing an operator nicety from becoming a gate-critical MUST.

### Risk Strategy Review — PASS (strengthened)
16 risks with severity/likelihood/priority, mapped to scenarios and coverage requirements, plus SR→R traceability, security, and failure-mode tables. Changes since first review:
- **R-15 (EXISTS-guard TOCTOU)** added — correctly identifies that `sqlx pool.begin()` is DEFERRED and requires `BEGIN IMMEDIATE` for the whole-set-once guard to be race-safe under concurrent same-cycle starts, with a concurrency test asserting exactly one intact whole set (no merge). This is the atomicity guarantee that makes the freeze enforceable.
- **R-08** rewritten to whole-set-once semantics with exact-stored-set-equality coverage (replaces the old per-row first-write-wins).
- **R-16 (ack echo drift)** added as Low/non-gating.
- Security section unchanged and sound: value-opacity makes parameterized binds the *only* SQLi defense (load-bearing); blast radius bounded to opaque strings; `like_escape` correctly excluded (no namespace query ships).
- **Good alignment**: R-12 still pre-empts the exact recurring project pattern (Unimatrix #3337 — testers asserting against illustrative ARCHITECTURE header strings rather than the spec/`render_goal_section` parity contract).

### Proportionality note
This is an enabler/telemetry feature on the observation lane. Architectural Principles 1 (hash chain), 6/7/8 are legitimately N/A for a `cycle_tags` junction; Principle 3 (capability checks) is satisfied minimally via the single `Capability::Write` gate; Principle 5 (graceful degradation) is honored on write (fire-and-forget + warn) and read (degrade to `[]`). N/A items are justified, not silently skipped.

## Knowledge Stewardship
- Queried: /uni-query-patterns for vision alignment patterns (topic `vision`) -- nearest hit #3337 (architecture illustrative headers diverge from spec → testers assert wrong strings), which RISK-TEST-STRATEGY already applies verbatim in R-12; #2298 (config-key semantic divergence) not applicable to a telemetry-junction feature.
- Stored: nothing novel to store -- the findings here (external/deferred-payoff substrate; a late best-effort ack-echo addition beyond SCOPE) are feature-specific and observed once. Per stewardship guidance, store a vision pattern only when a misalignment type recurs across multiple features. No generalizable new vision pattern from vnc-047.
