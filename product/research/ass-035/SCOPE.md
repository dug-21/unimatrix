# ASS-035: NLI Input Extraction Strategy for Supports Edge Detection

**Date**: 2026-04-01
**Spike type**: Empirical — measure, then decide

---

## Problem

The NLI graph inference pass consistently produces max entailment scores of ~0.32 against
Unimatrix knowledge entries, regardless of threshold configuration. With threshold now at 0.45,
zero Supports edges are being written per tick.

The root hypothesis: the NLI model (MiniLM2 Q8) was selected via crt-023 (#329) using a
4-way comparison on 1-2 sentence entry descriptions. Production entries are full ADR documents —
context sections, rationale, code blocks, decision headers — a fundamentally different input
shape than what the model was evaluated on. The model was not validated against the data it
actually runs on.

Informs edges, by contrast, write 4/tick successfully because ASS-034 detects them via
structural/pattern matching (feature_id references, field text), not NLI scoring.

---

## Research Question

**Which input extraction strategy, if any, produces viable NLI entailment scores for detecting
Supports relationships between Unimatrix knowledge entries — and if none does, what does that
tell us about the path forward?**

---

## Why It Matters

The Supports edge type is the entailment backbone of the typed relationship graph. PPR traverses
Supports + CoAccess + Prerequisite + Informs edges. At current trajectory, Supports edges are
not being written by the NLI pass, which means PPR runs on a graph that is CoAccess and
Informs only. The diversity gains PPR was designed to provide depend on Supports edges existing.

If the extraction strategy fixes this, it's a cheap config/code change. If it doesn't, the
research output is a definitive decision: Supports detection needs a non-NLI mechanism, and we
should design one informed by what works (the Informs heuristic approach).

---

## Bounded Questions

1. **Input length effect**: For the same entry pairs, what score distributions do these three
   extraction strategies produce?
   - Strategy A: `topic` field only (~5-15 words, matches original eval distribution)
   - Strategy B: first paragraph / operative claim sentence (ADR "Decision:" section or
     equivalent — 1-3 sentences)
   - Strategy C: full `body` field (current behavior, baseline)

2. **Ground truth construction**: Of the ~1,000 active entries, identify 20 pairs where a
   human reviewer can judge whether a Supports relationship genuinely exists. These become the
   seed validation set. The judgment question is: "Does knowing entry A make entry B more
   trustworthy, better understood, or less surprising?" If yes → Supports. The labeled set does
   not need to be exhaustive — it just needs to tell us whether score distributions separate on
   pairs we *know* should score high vs. pairs we *know* shouldn't.

3. **Model swap viability**: If extraction strategy B or A lifts max scores meaningfully (toward
   0.6+), the model is viable and no swap is needed. If scores remain low even on extracted
   claim sentences, test one alternative model — a DeBERTa-based cross-encoder in the same size
   class — to determine whether the problem is MiniLM2 specifically or entailment as a task.

4. **Task mismatch confirmation or rejection**: Do any entry pairs, under any extraction
   strategy, score above 0.6? If yes, the path is clear. If no pair crosses 0.6 even for
   pairs labeled as "clearly Supports," the task is mismatched and NLI entailment cannot detect
   this relationship in Unimatrix's knowledge corpus.

---

## What a Researcher Should Explore

1. Pull 20 entry pairs from the live knowledge base. Select a mix:
   - 8-10 pairs from the same feature cycle (likely higher semantic cohesion)
   - 5-6 cross-feature pairs that share a topic area
   - 4-5 clearly unrelated pairs (negative control)

   Label each: `should_support: true/false/borderline` with a 1-sentence rationale.

2. For each pair, run NLI scoring under Strategies A, B, and C (current). Record:
   - Score per strategy
   - Whether the score crosses 0.45, 0.5, 0.6 thresholds
   - Score separation between `should_support: true` and `should_support: false` groups

3. If Strategy A or B shows clear score lift with good true/false separation: recommend
   extraction point and minimum viable threshold. Implementation is a small change to the NLI
   candidate scoring path — extract the relevant field/section before passing to the model.

4. If Strategy A and B still produce scores < 0.45 even for `should_support: true` pairs:
   a. Test one DeBERTa-based cross-encoder ONNX export in the same size class. Report score
      distributions under Strategy B (short-text, best case from above).
   b. If DeBERTa also fails on `should_support: true` pairs: write the definitive conclusion —
      NLI entailment is not the right task for this corpus. Recommend an alternative detection
      mechanism for Supports edges, informed by what the Informs heuristic approach does well.

---

## Output

A concise findings document covering:

1. Score distributions per strategy (table: pair ID, label, score-A, score-B, score-C)
2. **Decision**: extraction strategy to adopt, OR confirmation that NLI entailment is not viable
3. If NLI is not viable: a concrete proposal for non-NLI Supports detection (heuristic,
   similarity threshold, or structural signal — scoped to what could be implemented in one
   delivery session)
4. The 20-pair labeled set, formatted for future integration into the eval harness as hard
   ground truth scenarios

---

## Constraints and Prior Art

- **crt-023 (#329)**: Original model selection. MiniLM2 Q8 selected over FP32 and two other
  models. Eval used short entry descriptions — the known mismatch this spike investigates.
- **Current NLI max score**: ~0.32 on full body content. Threshold lowered to 0.45, still zero
  Supports edges written per tick.
- **Informs success pattern**: ASS-034's Informs detection uses structural signals, not NLI.
  4 edges written per tick. If NLI is not viable for Supports, this is the design template.
- **Model size constraint**: Stay within the ~85MB ONNX model class. The ONNX/Rayon inference
  pipeline (W1-2) accommodates any ONNX cross-encoder — swapping is a config change, not an
  infrastructure change.
- **Do not change the contradiction detection path.** If the model is swapped, validate that
  the new model still produces correct Contradicts scores on known contradiction pairs before
  shipping. The two functions share the same model — a swap affects both.
- **Scope boundary**: This spike does not design or implement the fix. It produces a decision
  and a recommendation. Implementation goes to a delivery session.
