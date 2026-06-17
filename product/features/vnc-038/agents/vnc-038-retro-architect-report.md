# vnc-038 Retrospective — Architect Findings

MODE: retrospective. Feature SHIPPED (PR #772, 5a42d54c). All gates PASS, rework 0/1.

## 1. Patterns

- **NEW #5103** — "Atomic dual-side wire-format change: bump both encoder+decoder in one diff, gate parity on a corpus regenerated from one oracle, make a single-side change fail by construction." Feature-agnostic abstraction of the v:2 bundle one-diff invariant; recurs across vnc-034 (#4956) and vnc-038. Supports→#5081 (ADR-002). Generalizes the within-feature mechanics already in #5092/#5095/#5096.
- **SKIPPED — dumb-client "server is sole route authority / client posts verbatim."** Already fully captured as ADR-001 #5080 (decision, stated generically) plus #5096 (closing the double-append) and #5095. A separate pattern entry would duplicate. Not stored.
- In-cycle patterns reviewed (#5079, #5089, #5090, #5091, #5092, #5093, #5094, #5095, #5096, #5097, #5098): all crisp, non-redundant, kept as-is. #5090 vs #5093 are distinct (generic shared-variant-deletion wave-handoff vs the specific Default-deletion⟹per-request-observe inseparable coupling); they already cross-link via Supports. No corrections/deprecations.

## 2. Procedures

- No standalone procedure stored. The "atomic dual-side v:2 bundle change" how-to is captured as pattern #5103 (it is a structural invariant, not a step sequence). The "full-CI-suite-as-backstop" how-to change is already encoded as the LEADER action in lesson #5099 (run node test/run-hook-client.js incl. --include-layer2 as a backstop). No new procedure needed.

## 3. ADR status — all validated by implementation

| ADR | Entry | Implementation verdict |
|-----|-------|------------------------|
| ADR-001 dumb-client | #5080 | VALIDATED — all 3 client compose sites deleted; verbatim post proven (3c GREEN). |
| ADR-002 v:2 bundle | #5081 | VALIDATED — byte-equal Rust↔JS over regenerated hex corpus; v:1 fails closed. |
| ADR-003 per-slug observe | #5082 | VALIDATED — top-level /observe removed; per-request funnel; N=2 observe GREEN. |
| ADR-004 delete Default | #5083 | VALIDATED — default_resolver.rs removed; Default arms gone; inverted tests prove loud-404. |
| ADR-005 reserved slugs | #5084 | VALIDATED — value retained, derivation re-documented; tools kept reserved. |
| ADR-006 local direct binding | #5085→#5087 | CORRECTED MID-DESIGN (#5085 deprecated, GATE-2 proved "make-local-a-resolver-key" framing wrong → #5087 "local-never-enters-the-resolver"). #5087 VALIDATED in code (local bypass guard). Supersession chain clean & linked. |
| ADR-007 register writes [[projects]] | #5086 | VALIDATED — atomic temp+fsync+rename; re-attach-safe; idempotent. |
| ADR-008 token via bundle only | #5088 | VALIDATED — token absent from stdout/stderr/tracing; bundle sole channel. |

No ADR revealed as wrong/incomplete by implementation. ADR-006's mid-design correction was the only one, and it was caught BEFORE delivery (GATE-2), exactly the intended behavior. No supersession proposed at retro (none needed; #5085→#5087 already done).

## 4. Lessons — 2 new

- **NEW #5102** — "Long-swarm completion must be confirmed by durable on-disk state, not task notifications; use run_in_background for waits." LEADER-side reconciliation of the F-05 notification-hang + 18× sleep_workaround outlier. Distinct from #5060 (agent-side offload anti-pattern).
- **NEW #5101** — "Swarm shared-worktree git hazard: ROOT TRIGGER is agents running git checkout/restore to isolation-test a red crate; fix is forbid-git + defer-tests-when-red." Sharpens the existing shared-worktree-hazard memory from "it can happen" to the causal trigger + the spawn-prompt hardening applied mid-cycle.
- **#5099 (pre-existing) VERIFIED WELL-FORMED** — closed-set blast-radius lesson (the post-gate CI failure). Crisp, has WRONG-LESSON + 2 root causes + HOW-TO-APPLY, Supports→#5080. Not duplicated.

## 5. Retrospective findings

- Stewardship review of ~19 in-cycle entries (8 ADRs #5080–5088, patterns/lessons #5079, #5089–5098, #5099): all crisp and non-redundant. #5094, #5095, #5099 confirmed crisp (per briefing). No corrections or deprecations issued — quality was high; #5085 was already correctly deprecated.
- The post-gate CI failure (75a1c689) is fully explained by #5099 + #5095/#5096 (double-append surface) + #5092 (corpus gating). The predictive entries #5095/#5096 did flag the verbatim-post fixture surface; #5099 correctly elevates the design-side root cause (blast radius under-enumeration).
- Advisory from gate-3a (stale ADR-006 Unimatrix IDs in the architect's agent report listing ADR-006=#5085) is moot at retro: the live chain is #5085(dep)→#5087(active); pseudocode/risk artifacts use the correct IDs. No action.
- TRANSCRIPT CANDIDATE (RECONSTRUCTED, 0.81 fidelity, scope-phase only) adds no decision content beyond the ADRs — not extracted, per ADR-007 weighting.

## 6. Edges

- Asserted: **Supports #5103→#5081** (the generalized wire-format pattern validates/abstracts ADR-002; a future agent designing a wire-format change should traverse ADR→pattern). One clause, traversal-necessary.
- NOT asserted for #5101/#5102: #5101 generalizes the caveat already in patterns #5090/#5093, but pattern→pattern "generalizes" is prose, not a must-traverse Supports (those are not decisions being validated); #5102 validates no decision. Bar not met — none.
- Intra-feature Supports spine (#5079→#5082, #5089→#5088, #5090→#5083, #5093→#5082/#5090, #5091→#5086, #5096→#5080, #5097→#5087, #5099→#5080) was already asserted in-cycle; complete. No additional spine edges needed.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search / context_get / context_graph — enumerated all ~19 vnc-038 in-cycle entries (#5079–5099) + ADR-006 supersession chain (#5085→#5087, clean); searched prior art for the two new lessons (#5060, #3936, #3561, #3026, #525). All assessed; in-cycle entries crisp, no corrections/deprecations.
- Stored: #5102 (lesson, on-disk completion / run_in_background), #5101 (lesson, shared-worktree git-hazard root trigger), #5103 (pattern, atomic dual-side wire-format change; Supports→#5081).
