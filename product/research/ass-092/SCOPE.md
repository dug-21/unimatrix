# ass-092 (SPIKE): Contradiction Detection Restoration — what it takes to re-implement NLI as it was

> **uni-zero-authored scope.** Framed interactively with the human (uni-zero session, 2026-07-04).
> A research session begins only when the human confirms this SCOPE is complete. Read-only in Unimatrix.
> Produces FINDINGS.md — a restoration effort/scope, handoff-ready for a delivery/bugfix session.

**Tracking:** GH #899
**Capability:** SL-CONTRADICT (currently 🔴 regressed)
**Goal:** self-learning (#5219)
**Phase:** Assimilate (ass) — research spike
**Mode:** read-only code investigation. Do NOT store anything in Unimatrix.

---

## Why this spike exists (and what it deliberately does NOT do)

NLI-based contradiction detection was **functional and operational**, found **zero conflicts in the SDLC
corpus**, was **modified into a config attribute (enabled/disabled)**, and then **portions were removed** — it
is **non-functional now**. This spike scopes **ONLY the effort to re-implement it the way it was before**,
behind that existing enabled/disabled config gate.

**Value analysis is DEFERRED, deliberately.** Whether contradiction detection is *useful* — for the research
domain (where competing claims are normal) or as a poisoning defense (SLN1) — is NOT this spike's question.
The research corpus is too early to drive a confident value decision now; measuring it prematurely would
produce a verdict we couldn't base a decision on. So this spike answers *"what would it take to get it back?"*
so the capability is **ready to evaluate** when the research corpus matures — it does not answer *"is it worth
keeping?"* That evaluation is a separate future spike (see Deferred).

---

## Q1 (FOUNDATION) — Current-state inventory: removed vs disabled vs starved

File:line-grounded map of every piece of the contradiction path and its CURRENT status. Read the code AND the
removal commit(s) — do not assume.

- **NLI model** — loaded? disabled-by-default? removed? (crt-023/029; note crt-038/039 decoupled NLI from
  *ranking* — a SEPARATE decision, see Non-Goals.)
- **The enabled/disabled config attribute** — where is it, what does it gate today, is it still wired or left
  dangling after the removal? This is the restoration's anchor point.
- **Contradicts-edge WRITE path** — crt-029 was "Supports-only / Contradicts disabled." Current state.
- **Detection mechanism** — what the detector was at removal (NLI cross-encoder, and/or the original crt-003
  embedding-similarity >0.85 + conflicting-content check). What exactly was torn out.
- **Quarantine** (crt-003), **serve-time suppression** (col-030), **contradiction-density in Lambda**
  (crt-051) — which of these remain functional-but-input-starved vs removed.

**Deliverable:** the sharp line between *removed* (gone — must rebuild), *disabled* (present, off — must
re-enable), and *starved* (present, functional, no input — needs its upstream reconnected).

## Q2 — Restoration scope: what it takes to return to prior behavior

Given Q1, specify exactly what to re-implement to restore the **as-was** behavior behind the enabled/disabled
config gate:
- What to **rebuild** (removed), **re-enable** (disabled), **reconnect** (starved).
- The **config gate**: confirm `enabled=true` would drive the full path (detect → write Contradicts edge →
  quarantine/suppress → contradiction-density), and `enabled=false` (the default, SDLC's posture) leaves
  behavior byte-identical to today. Confirm this composes with the per-slug config overlay (vnc-040) so it can
  be a **per-domain** toggle later.
- **Graceful degradation (Principle 5):** absent/failed NLI model = previous behavior, never broken.
- **Effort + risk:** rough sizing (files/surfaces touched), migration/schema implications (if Contradicts
  edges reappear in GRAPH_EDGES), and the blast radius on Lambda/coherence and the enrichment tick.

## ★ Deliverable

A **restoration scope handoff-ready for delivery**: the removed/disabled/starved map (Q1), the exact
re-implement/re-enable/reconnect list (Q2), config-gate confirmation, effort + risk, and the minimal
regression tests to prove the restored path fires when enabled and is inert when disabled. **No value
verdict** — restoration readiness only.

---

## Deferred (explicitly OUT of scope for this spike — future work)

Revisit when the research corpus is mature enough to yield a confident signal:
- **Research-domain value** — does detection surface genuine contradictions at acceptable precision in a
  domain of competing claims? (The reason this is deferred: too little corpus now.)
- **SLN1 poisoning-defense contribution** — the adversarial-injection test.
These become a follow-on value-analysis spike once the capability is restored (enabled) and the corpus has
grown. This spike makes that evaluation *possible*; it does not perform it.

## Non-Goals / Guardrails

- **Restore AS-WAS** — re-implement the prior behavior; do NOT re-design the mechanism or evaluate
  alternatives. (Mechanism re-evaluation belongs to the deferred value spike, if ever.)
- **Do NOT reverse NLI-removal-from-RANKING** (crt-038/039) — a separate, presumed-sound decision. Touch only
  the contradiction-detection path.
- **Not the value decision** — usefulness is deferred (above).
- Preserve per-slug config invariants (global-locked vs overlayable, vnc-040) and graceful degradation.

## Dependencies

- Code paths + removal commit history ONLY: crt-003, crt-023, crt-029, crt-038/039, crt-051, col-030.
- **No research-corpus access needed** for this spike (value analysis deferred) — it can run now against the
  codebase alone.

## Q3 (ADDED by human, 2026-07-04) — Generalized small-ONNX-model substrate

Assess whether the restoration could be implemented so it supports **any** small ONNX model we later want to
add, loaded/invoked through the **same process** — i.e. the contradiction NLI cross-encoder becomes one
instance of a reusable small-ONNX-model path rather than a one-off. Feasibility, what the shared substrate
would look like, and the delta in effort/risk vs. restoring NLI alone. This is an assessment, not a mandate to
build it — but the restoration design should not foreclose it.

## Resolved Open Questions (human, 2026-07-04)

1. **As-was vs. modernization** — **Minimal modernization is acceptable** where an as-was piece no longer fits
   current code (e.g. the NLI substrate moved). Restore the *behavior*; modernize the implementation only as
   needed to land it. Flag any spot where modernization changes observable behavior.
2. **Restoration gate** — Confirm **all elements required to make the capability effective**, AND any
   modifications needed so it **operates correctly in BOTH deployment modes**. Identify the two deployment
   modes explicitly and confirm the restored path (and its config gate) works in each.
3. **Time-box** — **None.** Thorough identification of everything required to re-implement is the priority
   over speed.
4. **ONNX generalization** — see Q3 above (added).

## Knowledge Stewardship

- **Queried (carried in):** the 2026-07-04 uni-zero capability-graph session (SL-CONTRADICT regression,
  finding 7); the contradiction-path code + removal commits.
- **Declined:** Storing anything — read-only spike; findings live in FINDINGS.md.
