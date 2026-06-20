# ASS-075: Feasibility of Constrained Domain Auto-Calibration of Retrieval Parameters

**Date**: 2026-06-09
**Spike type**: Feasibility assessment — read-code + design-space exploration; small prototype only if cheap
**Status**: SCOPE (idea capture; not yet scheduled)
**Working number**: ass-075 (provisional — numbers uncommitted)
**Sequencing**: explicitly **downstream of crt-053** — must not start until crt-053 establishes the correctness constraints this feature would respect.

## Origin

Surfaced in a uni-zero session (2026-06-09) while guarding crt-053/ass-073 against overfitting the synthetic fixture corpus. The insight that resolved the apparent contradiction ("don't tune to the corpus" vs. "ship a thing that tunes to the corpus/domain"): overfitting to a small **synthetic fixture corpus** is bad; auto-calibrating to a **real domain's data distribution** is on-vision — and the two are distinguished by the same correctness-vs-optimal line that governs crt-053.

## Problem Statement

Can Unimatrix **automatically set retrieval configuration parameters tuned to a specific deployment's domain**, via evaluation runs, **without overfitting and without sacrificing correctness**? Today retrieval params (the crt-014 status penalties, and after nan-018 a sweepable config surface) are fixed cold-start defaults (ASS-037 authority); a new domain gets the SDLC-tuned defaults whether or not they fit. The vision asserts "any domain, **configured not rebuilt**" (`domain-agnostic`) and "manual weight tuning is **unnecessary**; converges from cold-start defaults" (`self-learning`) — an auto-calibrator is the logical realization of both, *if* it can be built safely.

## The shape: constrained optimization (the design's spine)

This is **not** "maximize a metric." It is a **constrained optimization**, which maps directly onto nan-018's two-corpus model:

- **Objective** — maximize relevance/cost on the deployment's **real domain data** (the realism snapshot / production query traffic). This is what we *want better*.
- **Hard constraints** — the trust/correctness properties on the **fixture corpus** (the crt-053 correctness floor: stale absent, redirect-to-head, deprecated-below-active). The optimizer **may not** trade a correctness guarantee for a metric gain (no "disable status penalties to boost recall").
- **Search space** — the sweepable nan-018 dials (the 7+ penalty levers, etc.).

So: **fixture corpus = constraint set; domain data = objective; nan-018 dials = search variables.** crt-053 establishes the constraints; this feature optimizes the objective subject to them. The correctness layer and the optimization layer of one problem — which is exactly why crt-053 must come first.

## nan-018 is the substrate (no nan-018 change required)

Verified in the same uni-zero session: nan-018 already builds everything this feature consumes — per-field sweepable dials (OQ-2 chose the most expressive surface), the trust metric class (the constraints), the token-weighted cost + P@5/MRR (the objective), and the two-corpus separation (constraints vs objective). nan-018 forecloses nothing. Two **additive, non-speculative** extensions this feature would want (do NOT add to nan-018 now — SR-06):

1. **A margin on trust outcomes** (how far A ranks below B, not just binary pass/fail) — a smoother search signal near the constraint boundary. Additive to `TrustOutcome`.
2. **A programmatic / in-process eval API** (vs. the CLI loop) — an optimizer calls eval in a tight loop. Additive entry point.

## Research Questions

- **RQ-1 — Feasibility & search method.** Is the param space small enough for tractable search (grid / random / Bayesian optimization)? What does one optimization run cost (eval-runs × param-points)? Is the objective landscape smooth enough for efficient search, or does the binary trust constraint make it discontinuous (motivating the RQ trust-margin)?
- **RQ-2 — Constraint encoding.** How are the fixture-corpus correctness properties expressed as **hard constraints** the optimizer cannot violate (feasibility filter vs. penalty term)? Confirm a config that fails any trust assertion is infeasible, not merely low-scoring.
- **RQ-3 — Anti-overfitting methodology.** Held-out validation on the **real domain data** (tune on one slice, validate on another) so the optimizer doesn't overfit even the real corpus. What slice sizes / cross-validation make a recommendation trustworthy? This is the ASS-037-class rigor ADR-006 requires before any deployed adoption.
- **RQ-4 — Autonomy dial.** Output a **recommended** config for human/ASS-037 review, or **auto-apply** behind the held-out + correctness-constraint gates? Where is the safe autonomy boundary? (Default lean: recommend-then-gate, consistent with nan-018 ADR-006 "deployment adoption is an ASS-037 decision" — the auto-calibrator becomes a *generator* of ASS-037-class evidence, not a bypass.)
- **RQ-5 — Domain corpus requirement.** What does a deployment need to provide as its "domain data" — accumulated `query_log` traffic? A minimum volume? How does a cold-start (no traffic yet) domain get calibrated, or does it stay on defaults until traffic accrues (the self-learning "converges from cold-start" path)?
- **RQ-6 — Naming & product surface.** A skill (`/uni-attune` / `/uni-calibrate` / "domain calibration")? A server command? Output format. ("Optimization" undersells the constrained-by-correctness nature — favor a name evoking *fit-to-domain-within-the-rules*.)

## Vision Alignment

- **`goal:domain-agnostic`** — "configured not rebuilt" taken to its conclusion: a domain's retrieval params auto-fit to its data. The strongest realization of the goal's claim.
- **`goal:self-learning`** — "manual tuning unnecessary." An auto-calibrator removes the manual step. (Mechanistically distinct from online learning-from-usage — this is offline batch optimization over eval evidence — but advances the same intent.)
- Strategic note: this feature **retroactively validates nan-018's investment** — nan-018 is not just crt-053's unblock; it is the instrument an entire class of auto-calibration features consumes.

## Non-Goals / Out of Scope

- **Building the auto-calibrator.** This spike assesses feasibility + shape + guards; it does not implement.
- **Tuning to the fixture corpus.** The fixture is the constraint set, never the objective — the same anti-overfitting guard crt-053/ass-073 carry.
- **Replacing ASS-037 (#3984) as formula authority.** A calibrator produces ASS-037-class evidence; it does not override the authority.
- **Changing nan-018.** nan-018 ships as-is; the two additive seams (trust margin, programmatic API) are this feature's future extensions, not nan-018 edits.
- **Online / per-session learning** — that is the existing self-learning pipeline; this is offline batch calibration.

## Dependencies

- **crt-053 (HOLD)** — MUST land first: it establishes the correctness constraints this feature respects. Building an optimizer before the correctness floor exists would let it overfit by sacrificing correctness.
- **nan-018 (#716)** — the instrument (dials + trust + cost + two-corpus model). The substrate.
- **ASS-037 (#3984)** — formula authority + the deployment-adoption gate (ADR-006 #4894).

## Tracking
GH Issue: to be created (`goal:domain-agnostic`, `goal:self-learning`, `research`). Provisional ass-075. **Downstream of crt-053** in dependency order. Origin: uni-zero correctness-vs-optimal discussion 2026-06-09.
