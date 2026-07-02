# ass-087 — Calibrate a retrieval-quality signal we TRUST for the live corpus (SL-METRIC, clause 2)

> Re-scope of the metric question previously mis-numbered ass-081. The on-disk `ass-081/`
> dir holds a different, completed spike (D6 isolation parity). GH issue **#803**
> (`research(ass-081): a retrieval-quality metric we can trust for this corpus`) is this
> work — retitle it to ass-087.

## Problem Statement

SL-METRIC (capability #5373, `keystone`) is the ruler the entire **self-learning** goal hangs
from: SL-ROLLUP ("measurably smarter the more it's used") and SL-COLDSTART are `Prerequisite`-gated
on it and can never go green until it does. SL-METRIC is **partial**, not missing — the measurement
instrument already exists:

- **nan-018** shipped a non-MRR **trust metric class** (`forbidden_absent`, `rank_below`,
  `redirect_to_head` — property assertions, never literal IDs), a hand-authored **fixture corpus**
  (five status shapes, property ground truth), token-weighted **cost**, and a correlated report
  section (**§5C Trust / Relevance / Cost**) wired into the zero-regression check.
- **AC-14** (`eval/runner/sweep_tests::test_ac14_correlated_sweep_non_vacuous`) proves the metric
  **moves** on known-good vs known-bad — i.e. it discriminates **on the fixture corpus**.

The gap is exactly the word **trust**. The harness docs draw the line themselves: *"AC-14 only
proves the corpus measures something. It does NOT prove the corpus is a good-enough yardstick"*
(`eval-fixture-authoring.md`, ADR-004 §5), and SL-METRIC's note records *"none yet trusted for
interpretation."* So **done_when clause (1) is met** (a non-MRR signal discriminates on a
hand-authored corpus); **clause (2) is open** — that signal is **calibrated/validated as trusted
for interpreting LIVE-corpus retrieval quality** (not just fixture shapes) and adopted as the
standing accepted quality verdict.

This is a **calibration/trust-validation** spike, NOT a metric-from-scratch hunt. The instrument
exists; the question is whether (and in what form) we trust it on the live corpus — **and whether
the trusted live signal is this fixture instrument at all, or a behavioral one mined from usage
(see the reframe below).**

## Why it matters to the vision

Until the ruler is trusted, the marquee self-learning promise — *every deployment gets measurably
smarter the more it is used* — is **unprovable by construction** (you cannot measure "smarter"
without a trusted measure). SL-METRIC is the single highest-leverage blocker on the self-learning
board; closing clause (2) is what lets SL-ROLLUP move off `claimed`.

## The reframe: the live-trust gap is a missing-negative-signal problem — and it's what the GNN needs

Clause (2) has resisted for a structural reason, not a metric-tuning one: **there is no reliable
negative signal at the outcome level.** Every delivery cycle succeeds (the swarm always ships), so
"did the session succeed" has no variance and cannot discriminate helpful knowledge from noise. A
trusted live measure needs a negative, and the outcome layer has none. The fixture-property approach
above manufactures negatives synthetically (`forbidden_absent`); the open question is whether a
**live behavioral** negative exists to anchor real-corpus trust.

It does — one altitude below session outcome, where there IS variance:

1. **Within-offer non-engagement (cheapest, exposure-controlled).** A briefing surfaces N entries;
   the agent engages some and ignores the rest. The ignored-but-surfaced entries are negatives
   *relative to their offer-mates* — same context, same exposure, one chosen, the others not. This
   also **de-biases GET**: GET today is positive-only and absolute, so any weight entrenches whatever
   is exposed; made *contrastive* (this entry vs its un-engaged siblings) it self-corrects. Shrinking
   GET's weight slowed the drift but could not cancel it — the cure is a denominator, not a smaller
   numerator.
2. **GET-without-use.** Opened, never referenced again — attention invested that didn't pay off.
3. **Rework / gate-rejection / post-merge regression.** Every REWORKABLE-FAIL, CI red, and retro
   "we should have caught X" is a labeled negative living *inside* a successful cycle: the pipeline
   could have surfaced the preventing knowledge and either didn't (retrieval miss) or did and it
   wasn't used/sufficient (delivery miss).

**Critical layer correction — which target the signal trains.** Within-offer non-engagement is a
negative for a **(context → entry)** pair, not for the entry globally (an entry ignored under goal X
may be gold under goal Z). So it must move the **conditioning** layer (phase/goal → entry/category
routing — PhaseFreqTable, goal_clusters, crt-046 behavioral Informs), **never** the entry's global
confidence. A *global* negative is earned only by **aggregation**: an entry surfaced across many
diverse contexts and engaged nowhere is a **dead-knowledge candidate** (#370, human-gated, not
auto-decremented). One offer = "not relevant here"; sustained cross-context non-engagement =
"globally dead." Same raw signal, two aggregation levels, two different actions.

**Engagement, defined (dogfooding makes it observable).** Hooks see every tool call; entries are
cited by ID in PRs/retros/commits when load-bearing. So the funnel is: **GET** (attention) →
**cited in the produced artifact** (used) → **artifact survives without rework/revert** (helpful) vs
**cited in something reworked** (used-but-misleading — the SL2 "misleading recedes" negative). Stage
2→3 is where "all cycles succeed" dissolves: the question is not "did the cycle pass," it is "did the
specific artifact this knowledge fed survive."

**Honesty on noise.** Even at the conditioning layer, non-engagement is confounded — an entry may be
ignored because it was redundant (already known) or because its summary sufficed (the briefing worked
so well no GET was needed — non-monotonic). Exposure-control kills position bias, not these. So the
signal should *move* weights, not *set* them — directional evidence, not a hard label. Tiers 2–3 add
attribution plumbing (GET→citation→survival) and citation ≠ causation; **start at tier 1** (within-
offer contrast) — zero new labels, computable from the offer log + the folded read stream
(crt-054/055), and it fixes the GET bias immediately.

## Downstream: this IS the GNN's training signal (sequencing + hazard)

The conditioning layer is what the deferred **GNN** was meant to learn. The GNN is the *model*; the
within-offer conditioning-negative is its *training signal* — so the GNN was never a parallel track,
it was blocked on this same missing signal. Per the capability model the GNN is a **ceiling-raiser**
that earns its place *only by proving value against the ruler* (SL-METRIC) — which cannot happen
until the ruler exists, which needs this signal. The order is therefore fixed:

> **conditioning-negative signal → SL-METRIC becomes a trusted live measure → the GNN gains an honest
> training target AND a way to prove it beats the current fusion → SL5 / SL-ROLLUP move.**

**Hazard that makes the order load-bearing:** a GNN trained on today's positive-only, exposure-biased
signal (GET-auto-helpful) is a *high-capacity blind-reinforcement engine* — it entrenches exposure
faster and deeper than the current scalar loop (ass-079's worry, amplified). Building the GNN before
the conditioning-negative exists is worse than not building it. Get the contrastive signal first;
then the GNN has something honest to learn.

**Implication for this spike's output:** the trusted live signal must be captured in a **durable,
GNN-consumable form** — (context, offer, per-entry engagement + downstream survival) tuples persisted
for training — not merely computed for a report.

## What to explore (bounded)

1. **Inventory & validation surface.** For each signal nan-018 exposes (property trust metrics,
   P@K, MRR, CC@k, ICD, cost) — what is it validated for today, and on which corpus (durable
   **fixture** vs ephemeral **snapshot**)? Where does each carry trust, where doesn't it?
2. **The fixture→live bridge (the crux).** Property assertions need **known-good/known-bad ground
   truth**; the live snapshot has only **soft** ground truth (baseline = what production returned).
   Can property-based trust be extended to (a sample of) the live corpus, or is it structurally
   fixture-only? If fixture-only, what is the defensible bridge from "trusted on fixtures" to "trusted
   for live interpretation"?
3. **Is the fixture corpus a good-enough yardstick?** The ADR-004 §5 depth obligation (and the
   deferred ass-073 measurement question) flagged the Wave-1 corpus may need more bracketing points.
   Does the corpus need revision/expansion to be a trustworthy yardstick, and against what criterion?
4. **MRR/P@K disposition.** The docs call MRR interpretation "suspect" for this small, typed,
   behavior-driven corpus. Quantify why. Can it be calibrated/normalized into something trustworthy,
   or should it be demoted from the trusted signal entirely?
5. **Proof stability.** `test_ac14_correlated_sweep_non_vacuous` — the non-vacuity proof underwriting
   clause (1) — is **flaky** (#833 / #790). Assess whether the metric's own proof must be stabilized
   before it can be called trusted.
6. **Adoption bar.** Define concretely what "adopted as the standing accepted quality verdict" means:
   a signal? a verdict? a tracked number on the baseline log? What makes SL-METRIC `proven` vs still
   `partial` — i.e. the precise, runnable clause-(2) `done_when`.
7. **Live behavioral signal feasibility (the reframe's crux).** Is within-offer engagement — which
   entries in a briefing/search offer were subsequently GET'd / cited — capturable from the current
   hook + folded-transcript stream (crt-054/055, the offer/selection pipeline #394) without new
   instrumentation? If not, what is the minimal addition? Is tier-1 (within-offer contrast) reachable
   as an analysis, not a build?
8. **Conditioning-vs-confidence routing.** Confirm the plumbing that lets a within-offer negative move
   *conditioning* weights (phase/goal → entry/category) without touching *global confidence*, and the
   aggregation rule that promotes sustained cross-context non-engagement to a dead-knowledge candidate
   (#370). Getting this wrong (folding a conditional negative into global confidence) is the failure
   mode the human flagged.
9. **GNN-consumable shape.** What durable record shape — per-offer, per-entry engagement + downstream
   survival — does the deferred GNN need as training data, and can it be persisted from the *same*
   signal the ruler consumes (one instrument, two consumers)?

## Expected output (decision / recommendation — not implementation)

A `FINDINGS.md` that delivers:
- A **recommendation**: which signal(s) constitute the trusted SL-METRIC, and how each is validated
  for the **live** corpus (the fixture→live bridge, concretely) — **including whether the primary
  live anchor is the fixture-property instrument or the behavioral within-offer conditioning-negative,
  cross-checked against each other.**
- A **live behavioral signal design** (if feasible): the capture path for within-offer engagement,
  the conditioning-not-confidence routing, the dead-knowledge aggregation rule (#370), and the noise
  caveats (redundancy / summary-sufficiency) — with tier-1 (within-offer contrast) as the start point.
- The **GNN-consumable signal shape**: a durable per-offer / per-entry engagement + survival record,
  so the *same* signal that anchors the ruler feeds the GNN — with the **signal → ruler → GNN**
  sequencing and the biased-signal hazard recorded as a firewall on build order.
- A **sharpened clause-(2) `done_when`** for SL-METRIC — the runnable bar that flips it to `proven`.
- A **disposition** for the fixture corpus (sufficient as-is / needs which revisions) and for MRR
  (calibrate / demote).
- A call on whether the AC-14 flake (#833/#790) is a blocker for trust.
- Explicitly OUT of scope: *building* the GNN or the signal pipeline (this spike designs the signal
  and proves feasibility; instrumentation is downstream delivery), changing the metric, re-tuning
  deployed weights (ASS-037 is the formula authority), wiring eval into a CI gate (the harness is an
  instrument, not a workflow gate).

## Known constraints & prior art (build on, do not re-derive)

- **nan-018** eval harness — `docs/testing/eval-harness.md`, `docs/testing/eval-fixture-authoring.md`,
  `docs/testing/eval-two-corpus-model.md`, `eval-config-knobs.md`, `eval-corpus-migration.md`.
- **ADR-004 §5** — fixture authoring DEPTH obligation (deprecated-but-connected bracketing).
- **ass-073** — the downstream measurement spike that may find the corpus needs more bracketing points.
- **ASS-037** — confidence-formula authority (out of bounds for re-tuning here).
- **Baseline log** — `product/test/eval-baselines/log.jsonl` (the over-time quality record SL-ROLLUP
  would read).
- Current platform baseline (2026-03-26): P@5 0.3058, MRR 0.4181.

## Capability linkage

- Advances/unblocks: **SL-METRIC #5373** (clause 2) → `Prerequisite` of **SL-ROLLUP #5369**,
  **SL-COLDSTART #5370**.
- The behavioral signal also feeds **SL5 #5224** (fused relevance — the conditioning layer) and is
  the **training-signal prerequisite for the deferred GNN** (a ceiling-raiser that `Motivates`
  SL-ROLLUP/SL5, gated on the ruler). The **signal → ruler → GNN** dependency is the sequencing this
  spike must lock.
- Related: **ass-079** (Informs edge — learning signal vs blind reinforcement) is the sibling
  validity question for the behavioral signal; **#370** (dead-knowledge surfacing) is where the
  aggregated global negative lands.
- On completion: `Motivates` edge from this spike's entry → SL-METRIC; uni-zero applies the
  sharpened `done_when` and any status move (firewall: `proven` only on attached live-corpus
  trust evidence).
