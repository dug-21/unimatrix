# ASS-079: Re-imagining the `Informs` edge — is the behavioral signal learning, or blind reinforcement?

**Date**: 2026-06-14
**Spike type**: Premise interrogation + design-space re-imagining (investigation with corpus measurement; no PoC required)
**Status**: SCOPE (draft — pending human confirmation)
**Working number**: ass-079 (provisional)
**Tracking**: GH Issue #753
**Feeds**: a self-learning roadmap decision on the `Informs` edge type — retire, fix, or redefine; then design → delivery if a change is chosen.

## Origin

`Informs` edges have two live producers (NLI, the third, is dark per ASS-037):
- **behavioral** (`source='behavioral'`, crt-046 step 8b) — bidirectional edge for every pair of entries co-read via `context_get` within a cycle, `INSERT OR IGNORE`, weighted by cycle outcome (`success`→1.0, else→0.5).
- **structural** (graph inference tick, Phase 4b) — structural pre-filter + cosine floor (0.5 post-ASS-037).

The behavioral producer is sold as an outcome-conditioned *learning* signal — "knowledge that informed work that succeeded." This spike interrogates whether it is that, or whether it is co-access reinforcement wearing a learning costume.

**The thesis to test (human-originated, treat as challengeable, not given):**
1. **Outcome weighting is effectively a constant.** The delivery workflow is *designed* to catch and rework failure before a cycle closes — so closed cycles almost always carry `success`. If non-`success` outcomes are vanishingly rare, the 1.0/0.5 weight carries no discriminating information and the "outcome-conditioned" claim is hollow.
2. **What remains is blind reinforcement.** Stripped of a live outcome signal, behavioral `Informs` reinforces *every* co-access — meaningful and incidental/random alike — while claiming to learn. A pair read together once by coincidence is recorded as an "informs" relationship.
3. **No causation without inference in the loop.** "A informs B" is a causal/directional claim. Co-access is symmetric co-occurrence. There is currently **no inference** gating the promotion from "read together" to "informs" (NLI removed, ASS-037). Without it, the edge asserts more than the signal supports.
4. **First-write-wins compounds the problem.** `INSERT OR IGNORE` freezes the weight at the first cycle's outcome; later evidence never updates it. Even if outcomes *did* vary, the edge would not accumulate the signal.

Near-term posture (already decided, not reopened here): `Informs` stays **excluded from `context_correct` carry-forward** (vnc-035), and `context_get` continues to report whatever edges exist (ass-076). This spike is not urgent firefighting — it is a deliberate re-imagining.

## Goal — questions to answer

- **RQ-1 — Outcome distribution (validate or kill thesis #1).** Across the corpus's closed cycles, what is the actual distribution of `outcome`? What fraction are non-`success`? Is the behavioral weight a de-facto constant? Ground this in the data, not assumption.
- **RQ-2 — What behavioral `Informs` actually contributes.** Who consumes it (graph expansion / PPR, goal-conditioned briefing, GNN training as an ASS-038 signal origin)? How much does it overlap with `CoAccess` edges (fraction of `Informs` pairs that are also `CoAccess` pairs)? Does it measurably change retrieval/briefing output versus `CoAccess` alone, or is it redundant?
- **RQ-3 — The causation gap.** What would "inference in the loop" require to justify an `Informs` (vs. co-occurrence) claim — content entailment (NLI revival), within-session temporal/directional ordering, outcome-conditioned *lift* over base co-access rate, or something else? What is the minimum viable inference that turns co-access into a defensible "informs"?
- **RQ-4 — Where a real negative/failure signal could come from.** If cycle-close outcome is structurally always `success`, what *other* sources carry negative signal — gate rejections, rework counts, abandoned/non-closed cycles, retrieve-then-correct sequences (an entry read then immediately corrected), drift? Is any of them a usable learning signal the current design ignores?
- **RQ-5 — Re-imagined options (the deliverable).** Rank, with evidence: **(a) retire** behavioral `Informs`, fold into `CoAccess`; **(b) keep but fix** — accumulate/decay weight, add an inference gate, wire a real negative signal; **(c) redefine** `Informs` as a genuinely distinct relation (directional, inference-gated, not co-access-derived); **(d) status quo**. Each option states its consumer-migration cost (RQ-2) and what it does to the self-learning claim.
- **RQ-6 — Weight dynamics.** Should the signal accumulate and/or decay rather than first-write-wins? How does that relate to the existing confidence system (Wilson-score composite) — is `Informs` weight reinventing something the confidence layer already does better?

## Breadth

**`code+data`** (primary) — the behavioral/structural producers (`behavioral_signals.rs`, graph inference tick), `graph_edges` corpus measurement (outcome distribution, Informs/CoAccess overlap, consumer reads), and the consumers (graph expansion, briefing blend, GNN training spec). Light **`code+ecosystem`** only if a comparable system's "co-access vs. learned-relation" distinction informs RQ-3/RQ-5.

## Approach

**Investigation** (what the producers/consumers do; corpus measurement for RQ-1/RQ-2) + **design-space evaluation** (rank the RQ-5 options against contribution, causation defensibility, and consumer-migration cost). Premise-interrogation posture: the spike's first job is to confirm or falsify the four thesis claims with evidence before designing.

## Confidence required

**Directional** — a recommended disposition for `Informs` with ranked alternatives and the corpus evidence behind the call. No working PoC; FINDINGS.md is input to the roadmap decision and any subsequent design session.

## Target outputs

`FINDINGS.md` delivering:
- the measured outcome distribution (RQ-1) — thesis #1 confirmed or falsified with numbers
- the Informs/CoAccess overlap and consumer-impact assessment (RQ-2)
- a minimum-viable-inference definition for a defensible `Informs` (RQ-3)
- an inventory of candidate negative-signal sources (RQ-4)
- a single recommended disposition + ranked options with migration costs (RQ-5)
- a position on weight dynamics vs. the confidence layer (RQ-6)

## Constraints

**Hard** (changing requires rewriting shipped code):
- Any disposition that retires or redefines `Informs` must state a **migration path for live consumers** — graph expansion / PPR and the GNN training-data contract (ASS-038's signal origins) read these edges today; they cannot silently break.
- The spike does **not** revive NLI as a deliverable — NLI is an *input/option* for RQ-3, not under build here.
- `CoAccess` semantics are not under revision — this spike may recommend folding `Informs` *into* it, but does not redesign `CoAccess` itself.

**Hypothesis** (positions held going in — researcher must treat as challengeable, and must ground RQ-1 in data before accepting):
- Cycle outcomes are de-facto always `success` (workflow catches failure pre-close), so outcome weighting carries no signal.
- Without inference in the loop, behavioral `Informs` is co-access reinforcement, not learning.
- The near-term cleanest posture is unchanged: exclude from carry-forward, let `context_get` report what exists.

## Dependencies

- **Independent of ass-076** (edge surfacing) — surfacing reports whatever edges exist regardless of this disposition. No ordering constraint either way.
- **Confirms, does not reopen, vnc-035** — `Informs` excluded from carry-forward stands; this spike may add the *reasoning* but not change the decision.
- **Builds on crt-046** (behavioral signal origin), **ASS-037** (NLI infrastructure removal — why there is no inference in the loop now), **ASS-038** (GNN training data — `Informs` as a labeled signal origin).

## Non-Goals / Out of Scope

- **Implementing** any change — FINDINGS.md only; design → delivery follow if a change is chosen.
- **Reviving NLI** — an option to evaluate (RQ-3), not a build target.
- **Reopening carry-forward** — `Informs` exclusion is settled (vnc-035).
- **Redesigning `CoAccess`, the confidence system, or the GNN** — referenced as consumers/constraints, not under revision.

## Prior art

- `crates/unimatrix-server/src/services/behavioral_signals.rs` (behavioral producer, step 8b).
- The graph inference tick / Phase 4b structural producer; ASS-037 NLI audit (all four NLI sites removed/restructured).
- `graph_edges` schema (`source`, `weight`, `relation_type`); `CoAccess` promotion (`run_s8_tick`, count≥3).
- ASS-038 GNN training-data spec (5 signal origins, labeled edges).
- The confidence system (Wilson-score composite) as a comparison point for weight dynamics (RQ-6).
- vnc-035 carry-forward ADRs (#4983–#4987); ass-076 edge-surfacing FINDINGS.
