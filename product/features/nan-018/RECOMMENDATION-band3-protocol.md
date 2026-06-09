# RECOMMENDATION — Band-3 Conditional Eval-Corpus-Migration Protocol Trigger

**Status**: RECOMMENDATION (handoff for separate uni-zero ratification — NOT applied)
**Feature**: nan-018 (#716)
**Wave**: 2 (deferrable; zero code coupling to Wave-1)
**Authority**: ADR-005 (#4893) Band-3 boundary; ADR-002 (#4895) retrieval-shape hash
**Satisfies**: AC-12(a), AC-13 (boundary)

---

## 0. Read-this-first boundary statement

This document is a **recommendation handed off for a SEPARATE, later uni-zero ratification
session**. It is **not** an applied change.

- **nan-018 edits NO `.claude/protocols/` file.** `git diff --name-only origin/main -- .claude/protocols/`
  is empty for this feature, by design. This is a HARD GATE (AC-13).
- **nan-018 wires NO eval-execution-as-gate.** No CI hook, no PR hook, and no protocol step makes
  eval *results* a standing decision gate. That is a **separate future design** and an explicit
  non-goal (SCOPE Non-Goal #1).
- The trigger described below is **asset-maintenance only** — it keeps the fixture corpus valid
  (re-stamp + revalidate assertions + confirm the corpus loads). It is **explicitly NOT
  execution-gating**: it never makes "did the eval pass?" a condition for shipping a change.

A later uni-zero session is the owner that ratifies and applies this. Until then, the corpus is
protected solely by the **mechanical guard** (the retrieval-shape hash drift guard, ADR-002), which
is the live, code-level backstop nan-018 actually ships. The protocol layer is the recommendation;
the mechanical guard is the shipped reality.

---

## 1. Problem this would address

Curated eval data goes stale. Schema/shape evolution (new entry columns, edge types, confidence
dimensions, or a changed embedding model-id/dimensionality) silently invalidates the fixture
corpus — the durable yardstick whose numbers feed the downstream spike chain (rewritten ass-073 →
ass-074 → crt-053 ACs). A yardstick that silently changes length is worthless.

nan-018 ships a **mechanical guard** (ADR-002): at eval start the running schema's retrieval-shape
hash is compared to the corpus stamp, and a mismatch on the primary/fixture corpus is a hard error.
That guard catches drift **at eval time**. The gap this recommendation closes is **earlier, at
authoring time**: a contributor who alters the retrieval shape should be reminded to re-stamp and
migrate the corpus *as part of their change*, rather than discovering the break later when someone
next runs the eval. The mechanical guard is the backstop; this conditional protocol step is the
forward reminder.

---

## 2. Recommended trigger predicate (OQ-5, human-ratified)

> **The conditional fires when "your change alters the retrieval-shape hash."**

Key properties — these are the reasons this predicate was chosen over an enumerated list:

- **Coupled to the ADR-002 hash, NOT an enumerated list.** The same hash definition that powers the
  drift guard is the trigger predicate. OQ-4 (the hash) and OQ-5 (the trigger) are **ONE definition
  of "shape"**, not two definitions that can drift apart.
- **Deterministic.** The hash moves or it does not — there is no delivery-leader judgment call
  ("does a display-only column count?"). The ADR-002 manifest already names which inputs are in
  scope, so the determinism is structural.
- **Precise.** Only shape-affecting changes fire. An enumerated trigger ("any change to entry
  columns…") is over-broad and judgment-prone; an over-broad trigger produces false-positive fatigue
  and gets ignored — the gate rots, defeating Band 3's purpose.

**What feeds the hash (documentation, not the trigger):** the ADR-002 enumerated input set — entry
columns (retrieval-relevant, display-only excluded), edge types (the `RelationType` retrieval set),
confidence dimensions, and embedding identity (`model_id` + `dimension` + `embedding_model_sha256`).
This enumeration is *documentation of what moves the hash*, not a separate enumerated trigger list.
Because embedding identity is a first-class hash input (ADR-002 OQ-3 branch (b)), an ONNX
embed-model upgrade also moves the hash and therefore also fires this conditional — embed-model drift
and schema drift are one trigger.

---

## 3. Recommended action when the trigger fires (asset-maintenance only)

When a change alters the retrieval-shape hash, the conditional step would direct the contributor to:

1. **Re-stamp** the fixture corpus manifest: recompute `shape_hash`, bump `migration_number` (human
   legibility), and bump `manifest_version` **only if the hash input-set itself changed** (a new
   input class, not a new value of an existing class).
2. **Migrate the corpus + revalidate assertions** per the Band-2 schema-migration runbook
   (`docs.md` Band-2 #2): update property/relationship assertions that the shape change affects.
3. **Validate the corpus loads** — a single one-time migration-validation run of the corpus to
   confirm it loads and the drift guard now passes against the new shape.

That one-time migration-validation run is **allowed** (it validates the *asset*, not a quality
verdict). A **standing decision gate** on eval results is **not** allowed (Non-Goal #1) — the step
verifies the corpus is valid and loads; it never makes "did P@5 regress?" a ship condition.

---

## 4. How the protocols WOULD carry this (illustrative only — DO NOT APPLY here)

Patterned on the existing `[CONDITIONAL] uni-docs` step in
`.claude/protocols/uni/uni-delivery-protocol.md` (Phase 4: "documentation update (if trigger
criteria met)"). A future ratification session might add an analogous conditional to the **delivery**
and **bugfix** protocols, for example:

```
[CONDITIONAL] uni-eval-corpus-migration — re-stamp + migrate the fixture corpus
              (if the change alters the retrieval-shape hash, per ADR-002)
```

- **Where**: alongside the existing `[CONDITIONAL] uni-docs` step (same conditional-on-criteria slot),
  in the delivery protocol and the bugfix protocol.
- **Condition**: the ADR-002 retrieval-shape hash differs from the corpus stamp (deterministic — the
  guard already computes this).
- **Action**: §3 above (re-stamp + migrate + validate-loads). Asset-maintenance only.
- **Design protocol**: a parallel note in the design protocol so a feature whose architecture
  *plans* a shape change carries the corpus-migration expectation forward into delivery.

This block is **illustrative**. nan-018 does not write it into any protocol file. The exact wording,
slot, and which protocols carry it are for the ratification session to decide.

---

## 5. Explicit deferred-separate-design boundary

| Concern | nan-018 (this feature) | Deferred to a SEPARATE design |
|---|---|---|
| Retrieval-shape hash + mechanical drift guard | **Ships** (ADR-002, code) | — |
| `convention` + `procedure` knowledge entries | **Ships** (Unimatrix, this wave) | — |
| Band-3 conditional protocol step | **Recommendation only** (this doc) | Ratify + apply in a later uni-zero session |
| Eval-execution-as-quality-gate (CI/PR, blocking vs advisory, ownership of failures) | **Not built** | A distinct future design with its own process trade-offs (Non-Goal #1) |

The two deferred items are **different** and must not be conflated:

1. **This recommendation** (the asset-maintenance conditional) is a small, bounded handoff — ratify
   the predicate, choose the slot, write the conditional. It is recommendation-only here purely
   because editing a protocol is outside nan-018's boundary (AC-13), not because it is large.
2. **Eval-execution-as-gate** (making eval *results* decide whether a change ships) is a genuinely
   larger, unscoped design — CI-on-every-PR, regression policy, blocking-vs-advisory, failure
   ownership. nan-018 takes no position on it and this recommendation does **not** advance it.

The recommended conditional concerns only **corpus validity** (a precondition for trustworthy
measurement); it never concerns the **quality verdict** of any measurement.

---

## 6. Companion artifacts shipped inside nan-018 (for the ratifier's reference)

The ratification session can rely on these already being in place:

- **Mechanical guard** — the retrieval-shape hash + drift guard (ADR-002), live in `eval/shape/`.
  This is the durable backstop that protects the corpus even if this conditional is never ratified.
- **Unimatrix `convention`** — "schema/shape change ⇒ corpus migration", surfacable in briefing
  (tags `["nan-018","convention"]`). This is the knowledge-layer expression of the same predicate.
- **Unimatrix `procedure`(s)** — how to migrate the corpus (re-stamp / bump numbers / revalidate
  assertions) and how to author a fixture scenario (tags `["nan-018","procedure"]`).
- **Band-2 docs** — fixture-corpus authoring guide, schema-migration runbook, two-corpus model,
  config-knob reference (`docs.md`).

---

## 7. Handoff checklist for the ratification session

- [ ] Ratify the trigger predicate: "your change alters the retrieval-shape hash" (coupled to the
      ADR-002 hash, not an enumerated list).
- [ ] Choose the slot: alongside `[CONDITIONAL] uni-docs` in the delivery and bugfix protocols
      (and a forward note in the design protocol).
- [ ] Confirm the action is asset-maintenance only (§3) and add the conditional step text.
- [ ] Confirm NO eval-execution-as-quality-gate is introduced (that remains a separate design).
- [ ] Cross-reference the shipped mechanical guard (ADR-002) and the `convention`/`procedure`
      Unimatrix entries so the protocol step, the guard, and the knowledge layer stay one definition.
