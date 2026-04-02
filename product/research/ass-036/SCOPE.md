# ASS-036: GGUF Assumption Validation for Relationship Detection

**Date**: 2026-04-01
**Spike type**: Assumption validation — pass/fail verdict required before any build commitment
**Depends on**: ASS-035 ground truth set (PAIRS.md), ASS-035 harness infrastructure

---

## Context

ASS-035 confirmed NLI entailment is the wrong task for detecting prescriptive knowledge
relationships in Unimatrix's corpus. The NLI architecture was built on an untested assumption
and did not pan out.

The hypothesis for the long-term detection mechanism is GGUF: a local embedded LLM that can
*reason* about whether two entries have a knowledge relationship, rather than *classify*
entailment probability. This is a different task — "does knowing A help you apply B?" — that
NLI was never designed for.

Before any build path commits to GGUF-based relationship detection, this spike validates the
assumption with the same rigor applied to NLI in ASS-035. The ground truth set already exists.
The harness already exists. Use both.

---

## Research Question

**Can a local GGUF model reliably detect prescriptive knowledge relationships between
compatible-category entry pairs — and is it a viable long-term mechanism for self-sustaining
graph edge detection in Unimatrix?**

---

## Bounded Questions

### Q1 — Quality: does GGUF solve the task NLI couldn't?

Score the ASS-035 20-pair ground truth set (PAIRS.md) using GGUF inference. The task is not
classification — it is reasoning: given two entries of compatible categories, does the model
correctly identify whether a knowledge relationship exists?

Test two task formulations against the same pairs:

**Formulation A — direct relationship query:**
```
Entry A [category: lesson-learned]:
{entry_a_body}

Entry B [category: procedure]:
{entry_b_body}

Does knowing Entry A help you correctly apply Entry B?
Answer YES, NO, or UNSURE with a one-sentence explanation.
```

**Formulation B — prescriptive framing:**
```
Entry A describes: {entry_a_topic}
Entry B describes: {entry_b_topic}

Category relationship: lesson-learned → procedure

Does Entry A contain knowledge that informs, motivates, or prevents misapplication of
Entry B? Answer YES, NO, or UNSURE.
```

Success criterion: ≥ 16/20 correct classifications (80%), with false positive rate < 20%
on the negative controls. Compare against cosine baseline from ASS-035.

The NLI failure mode was specific: problem-description + solution-prescription →
contradiction score. Test explicitly whether GGUF makes the same error on P04 (the worst
NLI case: Handle::current panic → pre-fetch before spawn, scored 0.990 contradiction by
NLI). If GGUF correctly identifies this as YES, the task mismatch is resolved.

### Q2 — Latency: is it viable for tick-based processing?

Measure per-pair inference latency in the harness environment. The tick budget is ~30 seconds
(current production tick runs 4–28 seconds depending on whether contradiction scan fires).

| Latency per pair | Tick viability | Assessment |
|-----------------|----------------|------------|
| ≤ 0.5s | 25 pairs/tick feasible | Ideal |
| 0.5–2s | 10–25 pairs/tick feasible | Acceptable with batching |
| 2–5s | 5–10 pairs/tick | Marginal; async scheduling required |
| > 5s | < 5 pairs/tick | Not viable in-tick; offline-only |

If latency is > 2s/pair, also evaluate: is an async offline pass (running between ticks,
not gating the tick) a viable alternative? The background daemon runs continuously — a
lower-frequency relationship detection pass (e.g., every N minutes, not every tick) may
be acceptable if quality justifies it.

### Q3 — Infrastructure: is llama.cpp FFI viable in this environment?

The production runtime is a long-running Rust daemon. Known risks from the W2-4 vision doc:
signal handler conflicts, memory management in long-running processes, platform-specific
compilation (ARM, x86).

The harness provides a controlled test environment. Attempt llama.cpp FFI loading and
inference in the harness binary. Document:

- Does the FFI load successfully on this platform?
- Are there signal handler conflicts with tokio or the rayon pool?
- Does memory usage stabilize after initial load, or does it grow per-inference?
- Does the process remain stable across 100 consecutive inferences (simulating tick activity)?

A clean FFI integration in the harness is a necessary but not sufficient condition for
production viability. Note all observed issues for the W2-4 implementation brief.

### Q4 — Scale: does quality hold at deployable model sizes?

Test the smallest viable model first. Recommended candidates (in order of preference):

1. **Phi-3-mini-4k Q4_K_M** (~2.2GB) — strong reasoning per parameter, fits in 4GB RAM
2. **Llama-3.2-1B Q4_K_M** (~0.7GB) — smallest viable reasoning model
3. **Llama-3.2-3B Q4_K_M** (~1.9GB) — if 1B fails on quality

Test at Q4_K_M quantization first. If quality is marginal, test Q8_0 on the same model to
isolate quantization impact — the DeBERTa Q8 result in ASS-035 showed quantization degraded
discriminability significantly. Do not assume Q4_K_M quality from a paper benchmark.

---

## What Passes, What Fails

### Passes — GGUF is the viable long-term mechanism

All three must hold:
1. Q1: ≥ 16/20 correct on ground truth, false positive rate < 20%
2. Q2: Latency ≤ 2s/pair OR an async scheduling alternative is identified as workable
3. Q3: FFI loads without signal conflicts or memory instability across 100 inferences

If all three pass: GGUF-based relationship detection is the long-term path. Scope the
delivery (W2-4 prerequisite, or a dedicated relationship-detection sub-feature using the
same GGUF infrastructure).

### Fails — GGUF is not the mechanism

Any of:
- Q1: Model misclassifies the same pairs NLI failed on (especially P04, P02, P03)
  — the reasoning capability isn't sufficient at deployable model sizes
- Q2: Latency > 5s/pair with no viable async alternative
- Q3: FFI is unstable or not viable in the Rust daemon environment

If GGUF fails: the path is cosine-only (pending ASS-035 cross-feature validation). The
prescriptive relationship reasoning capability defers to when W2-4 infrastructure matures
or a purpose-trained model becomes available.

---

## Output

1. **Pass/fail verdict** on each sub-question (Q1–Q4)
2. **Overall verdict**: GGUF is / is not the viable long-term relationship detection mechanism
3. If PASS: recommended model, quantization, task formulation, and scheduling approach
   (in-tick vs. async); input to W2-4 implementation brief
4. If FAIL: definitive evidence (score tables, latency measurements, error logs), with
   recommendation for interim path (cosine pending ASS-035 cross-feature validation)
5. Raw score table for all 20 pairs under GGUF vs. cosine baseline for future reference

---

## Constraints

- **Extend the ASS-035 harness** — do not rebuild. Add a `--mode gguf` or
  `--model phi3-q4` flag alongside existing modes.
- **Harness only** — no production app changes. All GGUF code is research-only.
- **Test both quantizations** on at least one model (Q4_K_M and Q8_0) to isolate
  quantization impact. Do not rely on a single quantization result.
- **Use the ASS-035 ground truth** — the 20-pair set is the consistent baseline across
  all detection mechanism evaluations. Any new mechanism is measured against the same set.
- **Security constraint awareness**: the production vision requires model SHA-256
  hash-pinning for any GGUF model (W2-4 security requirement). Note the hash of any
  model tested so it can be carried forward to the delivery brief.

---

## What This Is Not

This spike does not design or implement GGUF integration in the production app.
It does not replace the W2-4 scope (which covers context_cycle_review, status
explanations, and contradiction reasoning). It answers one question: is GGUF
viable for relationship detection? Everything else belongs to a delivery session.
