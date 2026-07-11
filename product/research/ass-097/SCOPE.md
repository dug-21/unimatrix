# SCOPE — ass-097: Retrieval-measurement trust on the current connected corpus — re-baseline the eval harness and assess the fixture property-assertion class as capability floor-proof

Origin: uni-zero consult on the self-learning goal (#5684) and its keystone **SL-METRIC** (#5572).
The goal's north-star — "retrieval *measurably* improves" — is blocked on a retrieval-quality measure
we trust for this corpus. Prior conclusion (uni-zero, and the reason nan-018 kept eval OUT of standing
gates): snapshot-MRR is a **comparator** (config A vs config B on a frozen shape), not an **odometer**
(absolute, longitudinal quality), and metric choice has been inconsistent on a corpus that will not hold
still. Two things have changed since that conclusion was formed and warrant re-examination:

1. **The corpus is materially different.** When PPR/co-access were last eval-assessed the graph had very
   few edges. We now have full typed-graph capabilities and a **substantially more connected corpus** —
   the exact input the graph-expansion stack was built to exploit.
2. **The measurement toolkit is richer.** nan-018 added the **fixture corpus + property-assertion class**
   (`redirect_to_head`, `forbidden_absent`, `rank_below`) — a durable, alias-based, **shape-invariant**
   measurement that asserts *relationships*, not IDs/scores, and survives re-snapshot (crt-013 #703,
   "assert outcomes, never constants"). It was scoped as steepness-sweep tooling; it may be the honest
   floor-proof instrument for the capability map.

Positive control worth stating up front: this harness has already produced a real, load-bearing
decision — it is how we determined **NLI was ineffective and did nothing for our intelligence**. So the
question is not "does the harness discriminate at all" (it demonstrably did) but "**which measurement,
on the corpus we have now, is trustworthy enough to stake the word *proven* on — and for what claim.**"

## Framing

Two distinct measurement jobs keep getting conflated, and the spike must keep them apart:
- **Comparator** (dev-time A/B): snapshot corpus → MRR/P@5/CC@k/ICD. Legitimate and working; only
  meaningful relative to a fixed shape.
- **Floor-proof / odometer** (capability-map evidence): an **absolute** verdict that a behavioral
  invariant holds (floor), and separately whether quality rises over a deployment's life (longitudinal).
  This is what SL-METRIC actually needs and what MRR structurally cannot supply.

The spike is empirical-first: **characterize the corpus, re-run the eval, and look at real numbers**
before recommending what "proven" should mean.

## Goal (answerable questions)

- **G1 — Corpus characterization (empirical).** What does the corpus look like NOW? Via `context_status`
  + graph queries: entry count by category/status, typed-edge count by type, graph connectivity/density,
  correction-chain depth distribution, co-access population. Contrast against the low-edge state at the
  original PPR eval. Establishes *what we are measuring against* and whether the graph is now dense
  enough for graph-expansion signal to show.
- **G2 — Re-baseline (empirical).** Run the offline harness end-to-end (snapshot → scenarios → run →
  report) on the current corpus. Report P@5/MRR/CC@k/ICD and compare to the recorded 2026-03-26 baseline
  (P@5 0.3058, MRR 0.4181, CC@5 0.2636, ICD 0.5244). Did the richer, more-connected graph move retrieval
  — and in which metric does any movement actually surface (MRR vs CC@k/ICD)?
- **G3 — Metric trust / consistency (empirical — the crux).** Is snapshot-MRR *consistent* and
  *interpretable* on our corpus, or does it wobble with corpus state (the #500 KB-drift trap)? Test its
  discriminating power with known-signal profiles (a deliberately-degraded config as a negative control,
  a plausible-better as a positive) — does it still separate good from bad the way it separated the NLI
  no-op? Deliver a defensible statement of **where MRR is trustworthy (A/B, same shape) and where it is
  categorically wrong-shaped (absolute / longitudinal)**.
- **G4 — Fixture property-assertion class as floor-proof (empirical/directional).** On the current
  more-connected corpus, does the durable fixture property-assertion measurement give a stable,
  **absolute pass/fail** that survives corpus mutation? Map floor capabilities → assertion types and
  test what actually holds:
  - SL2 (misleading recedes) → `rank_below[deprecated, active]`
  - integrity read-currency / SL7 → `redirect_to_head`
  - KI-CONTRADICT (no conflicting pair served) → `forbidden_absent` (needs a new contradiction shape)
  Report which floor caps this class can convert to **proven-on-evidence**, which need new shapes /
  assertion types (co-access→co-surface for SL4), and which are **out of the harness's reach** (SL1
  attribution is a session property, not a rank property — name it, don't force it).
- **G5 — SL-METRIC redefinition + the gate boundary (directional).** Given G1–G4: what should
  SL-METRIC's `done_when` actually be? Candidates to evaluate: (a) property-assertion pass on the durable
  corpus as the **floor** measure; (b) reuse-rate (SL-REUSE #5577) as the **longitudinal** odometer that
  MRR can't be. And the standing product decision: should eval **cross the nan-018 "not a standing gate"
  line** and become authoritative floor-proof for the capability map, or stay a dev aid with capability
  proofs authored as separate targeted tests? Recommend, with the tradeoff stated.
- **G6 — Go/no-go + recommendation.** Is there a trustworthy measurement path to floor-proof self-learning
  (and the integrity retrieval invariants) on the corpus we have now? Recommend the measurement(s) per
  capability + the SL-METRIC redefinition + the gate-boundary call, or report the blocking gap.

## Breadth
`code+ecosystem` — code-dominant. Internal: eval harness (D1–D4), fixture corpus, `context_status`,
graph/co-access stores, the recorded baseline log. Ecosystem: retrieval-eval metric prior art for
small/typed/behavior-driven corpora (lightly — reproduce, don't re-derive; the negative-signal problem
is known).

## Approach
`measurement` for G1–G3 (run the real harness + status/graph queries on the live corpus via a snapshot);
`measurement` + `proof-of-concept` for G4 (exercise / extend the fixture assertions on current state);
`investigation` for G5 (design recommendation + boundary call, no build required).

## Confidence required
`empirical` for G1–G4 (numbers from the real corpus). `directional` for G5–G6 (a defensible
redefinition + gate-boundary recommendation; not a validated build).

## Target outputs
- A **corpus characterization** snapshot (connectivity then-vs-now) — G1.
- A **current eval re-baseline** report + a new line appended to `product/test/eval-baselines/log.jsonl`
  (the one write this spike may make — it is the baseline log's designed purpose) — G2.
- A **metric-trust statement**: where MRR is trustworthy, where it is wrong-shaped, with the
  known-signal discrimination evidence — G3.
- A **floor-cap → measurement map**: which capabilities the fixture property-assertion class can prove,
  which need new shapes/assertion types, which are out of reach — G4.
- A **recommended SL-METRIC `done_when` redefinition** + the **eval-as-standing-gate boundary call** —
  G5.
- Go/no-go — G6.

## Constraints

**Hard (fixed):**
- Research only: no committed product code, no PR. The one permitted Unimatrix/repo write is the
  **eval-baseline log line** (G2) — that log is the designed record of platform quality over time.
  No capability-status writes (uni-zero applies those after reviewing findings).
- **Never commit snapshots** — full agent interaction history; scratch dir only, treat as sensitive.
- Never pass the live DB to `eval run` (the FR-44 live-DB path guard).
- **Paired-snapshot discipline**: scenarios and the run snapshot must originate from the same DB state
  (the #500 KB-drift trap — otherwise MRR measures drift, not retrieval).
- Reuse the existing harness + stack; the `[graph_penalty]`/weight levers are **eval-only**, never a
  license to re-tune deployed defaults (ASS-037 is the formula authority).
- Honor the two-corpus model: fixture = durable/trust authority (carries the shape-hash stamp);
  snapshot = realism/ephemeral.

**Hypothesis (challengeable — positions to test, not givens):**
- The more-connected corpus moves retrieval measurably vs the 2026-03-26 baseline (it may not — density
  ≠ better ranking).
- Snapshot-MRR is consistent enough to trust for A/B on the current corpus (the human's stated doubt —
  test it, don't assume it).
- The fixture property-assertion class is a shape-invariant floor-proof (it may still be too small /
  under-bracketed — nan-018 anticipated a revision pass).
- Reuse-rate is a viable longitudinal odometer (causation/attribution risk).
- Crossing the "eval-is-a-gate" line is the right move (it may be better to keep eval a dev aid and
  author separate capability proofs).

## Dependencies
Builds on **ass-073/074** (eval + PPR re-validation at corpus scale — read as prior art), **nan-018**
(trust metric class + fixture corpus + drift guard), **ass-039** (behavioral ground truth, 1,761
scenarios), and the uni-zero measurement discussion. If `go`, it unblocks capability-map floor-proofs
for SL2 / KI-CONTRADICT / integrity read-currency and a defensible SL-METRIC keystone under
self-learning's north-star.

## Prior art
- `docs/testing/eval-harness.md` — D1–D4 offline flow, metric definitions, the 2026-03-26 baseline, the
  #500 paired-snapshot lesson, "eval is NOT a workflow quality gate" boundary.
- `docs/testing/eval-fixture-authoring.md` + two-corpus model — the property-assertion class, alias
  discipline, the five status shapes, the authoring-depth (bracketing) obligation.
- The **NLI-ineffective decision** — the harness's proof-of-discrimination precedent.
- `product/test/eval-baselines/log.jsonl` — the longitudinal baseline record.
- Capability map: SL-METRIC #5572 (keystone), SL2 #5556, SL4 #5560, SL7 #5532, SL-REUSE #5577,
  KI-CONTRADICT #5548; self-learning goal #5684.
- crt-013 #703 (assert outcomes, never constants — the durability principle behind property assertions).
