# nan-018 — Eval Harness Strategic Upgrade: Tunability, Trust/Cost Visibility, and a Durable Primary Corpus

**Status**: SCOPE (pre-design)
**Date**: 2026-06-09
**Working number**: nan-018 (GH #716)

## Position in the Chain

```
nan-018 (THIS — eval harness upgrade)              [the INSTRUMENT — has no upstream gate]
   builds the dials: tunability + trust/cost metrics + primary corpus + drift guard
        ↓ ships (cargo install)
ass-073 (REWRITTEN — measurement spike) → runs the evaluation nan-018 unlocks on the upgraded harness
        ↓
ass-074 → consumes / extends the measurement
        ↓
crt-053 (retrieval behavior) → consumes the findings; AC-05/AC-11 gate on real numbers   [HOLD]
```

**nan-018 is the instrument; everything that measures with it is downstream.** ass-073 as originally scoped **cannot run** — it assumed tuning dials existed in the eval harness that do **not** exist (the crt-014 status-penalty constants are compile-time `const`s; there is no trust/negative metric; there is no cost metric). A spike with no instrument to measure with cannot produce findings. **ass-073 will be REWRITTEN after nan-018 ships** to perform only the evaluation nan-018 unlocks. The chain is therefore **nan-018 → (rewritten) ass-073 → ass-074 → crt-053**, and **ass-073 is no longer an input to nan-018**: nan-018 does not wait on it, does not design ahead of it, and does not draw its acceptance spec from it. nan-018's requirements are established by the scoping work in **this** document. Separating instrument (nan-018) from experiment (the downstream spikes + crt-053) is the point — you cannot tell whether a metric moved because behavior or instrument changed if you build both in one feature.

## Problem Statement

The `unimatrix eval` harness (nan-007/nan-010, in-server per ADR-004 #—) measures **positive relevance** — P@K, MRR, Kendall-tau, latency, rank-changes, CC@k, ICD. That is half of what the product needs to verify its own retrieval quality. Three gaps make it unable to gate the next class of retrieval work and, more broadly, leave the product **half-blind to whether retrieval is getting better or merely different**:

1. **Status-penalty steepness is not tunable.** The crt-014 topology penalties (`ORPHAN_PENALTY=0.75`, `CLEAN_REPLACEMENT_PENALTY=0.40`, `PARTIAL_SUPERSESSION_PENALTY=0.60`, `DEAD_END_PENALTY=0.65`, `FALLBACK_PENALTY=0.70`, `HOP_DECAY_FACTOR=0.60`, clamp `[0.10, 0.40]`) are **compile-time `const`s in `unimatrix-engine/src/graph.rs:41–59`, not `UnimatrixConfig` fields** (verified — grep of `config.rs` for penalty/orphan/dead_end returns nothing). A profile TOML cannot sweep them, so crt-053's Q8 (steepness target) cannot be answered with data.

2. **The harness cannot assert a trust/negative property.** Its metrics reward the *presence* of ground-truth IDs. There is **no "forbidden-ID-absent" metric and no "ID-A-must-rank-below-ID-B" assertion** (verified — metric set is `{P@K, MRR, Kendall-tau, rank_changes, CC@k, ICD, latency}`). So "the stale source is absent" / "a deprecated entry ranks below the weakest active" — the core of status-trust retrieval — is not expressible. Because Q8 is a **sweep**, its success criterion must be measurable **in the same instrument**; routing the trust assertion to a separate suite cannot correlate "this steepness → trust holds AND P@5 didn't regress."

3. **Cost is not observable.** The harness reports latency, not the noise an agent pays to read. The product's stated retrieval objective is **relevance/cost balance**; the cost half is currently unmeasurable.

Beyond crt-053: an eval that only sees positive relevance cannot detect **trust erosion** (stale knowledge creeping up the rankings as the deprecated population grows monotonically) or **cost growth**. Verifiable self-improvement of retrieval requires an instrument that measures trust and cost, not just relevance. **This feature is that instrument.**

A second, compounding problem the project has repeatedly hit: **curated eval data goes stale.** Continued schema/shape evolution (topic_source, cycle_stamp, new edge types, confidence dimensions) invalidates production snapshots, so a curated set does not stay usable. Without a durability strategy, a "primary harness dataset" is a perpetual re-authoring tax that never stabilizes.

**Why now:** crt-053 (status-signal-aware retrieval) is on HOLD because its acceptance criteria (AC-05 steepness, AC-11 relevance/cost bound) are written *as if* the harness can gate them, and it cannot. The downstream measurement spike (ass-073, to be rewritten) is itself **blocked** — it has no instrument: it assumed dials in the harness that do not exist. nan-018 builds those dials so the spike becomes runnable. The deprecated-entry population grows every release; the gap widens in both `context_search` and `context_briefing`.

## Strategic Framing (goal:self-learning)

The vision claims the product improves from usage. **Improvement you cannot verify is hope.** This feature makes "retrieval got better, not just different" a *measurable* claim, and establishes a **primary reference corpus** that is institutional memory of what good retrieval looks like — gating every future retrieval change on trust + relevance + cost, not a one-off A/B. It is self-learning applied to the search engine itself.

## Goals

1. **Config-expose the retrieval tuning levers** (Tunability). Thread the crt-014 status-penalty constants (and hop-decay factor/clamp; `MAX_TRAVERSAL_DEPTH` if a lever) into `UnimatrixConfig` and through `graph_penalty`, so they are sweepable by a profile TOML. Defaults unchanged; exposure is additive (no behavior change at default values).

2. **Add the trust/negative metric class** (the load-bearing gap). Express, **inside the eval harness** so it participates in A/B sweeps and the regression check:
   - **forbidden-set absence** — assert specified IDs (e.g. the stale source) are absent from results;
   - **relative-rank** — assert ID-A ranks below ID-B (deprecated below weakest active).
   This is a new metric *class* alongside positive relevance, reusable for any future correctness property (quarantine, contradiction suppression), not just crt-053.

3. **Cost visibility** (the relevance/cost objective's missing half) — **build the dial, token-weighted.** Cost is **token-weighted, not result-count**: the same k can carry wildly different token loads (a 50-token snippet vs a 500-token one), and the cost an agent actually pays is *tokens-read*. Define **cost = Σ(per-result token-proxy)**, with **set-size (k) as a SECONDARY axis, not the primary**. The explicit token-weighted metric is the thing being built; a precision+k proxy survives **only** as an explicitly-justified architecture call in design (never a deferral), and even then it does not displace the token-weighted definition as the primary. You cannot proxy-away a cost term that has never been measurable. (Resolved in design — see Open Questions / OQ-1.)

4. **Fixture-corpus support + property-based ground truth.** Add the ability to author small, hand-authored **fixture entry-graphs** (correction chains A→B→C→head, dangling deprecated, superseded-but-Active, deprecated-but-connected) with `expected` assertions expressed as **properties/relationships** (redirect-to-head, absence, rank-below), not literal ID lists. Property-based assertions survive shape changes that literal-ID assertions do not (generalizes crt-013 #703 "assert outcomes, never constants").

5. **The curated primary corpus + the two-corpus model.** Deliver the canonical fixture corpus as the **primary harness** (durable). Document the production snapshot as the **realism layer** (ephemeral; re-snapshot when shape drifts). The fixture corpus is the stable spine for trust/correctness; the snapshot supplies realistic P@5/MRR baselines.

6. **Drift guard — the mechanical version stamp** (release-model-patterned). The fixture corpus carries a **retrieval-shape hash** (plus the migration number for human legibility); the harness **fails-loud / warns** when the running schema's retrieval shape diverges from the corpus's stamped hash. Makes staleness loud instead of silent, even when the protocol trigger (Goal 8) is missed. **The hash's inputs are explicitly enumerated** — entry columns, edge types, confidence dimensions, and **embedding dimensionality / model-id** — and that enumeration is load-bearing twice over: it is the single definition of "shape" the Goal-8 protocol trigger keys on (OQ-4/OQ-5 are unified, not two drift definitions), and it is where the OQ-3 embed-model question is settled. If embedding dimensionality / model-id is in the hash, the durable reference is protected against ONNX embed-model drift and embed-at-load is safe; if not, a frozen vector sidecar is required instead. **Binding: the durable reference must not silently become embed-model-dependent.**

7. **Documentation for dev-team utilization** (Band 1 + Band 2 — first-class, not a tail-end "update docs"):
   - **Band 1 (normal):** `docs/testing/eval-harness.md` updated with the new capabilities; standard delivery/retro Unimatrix knowledge (ADRs for the architecture decisions — trust-metric placement, two-corpus model, penalty-config exposure, fixture-as-primary).
   - **Band 2 (special — keeps the asset alive):** a **fixture-corpus authoring guide**; a **schema-migration runbook** for the corpus + its assertions; the **two-corpus model** documented (when to use which, how to re-snapshot); a **config-knob reference** for the newly-exposed levers (meaning, ranges, defaults, effect).
   - **Assumptions/limits documented:** cost is **token-weighted** (cost = Σ per-result token-proxy, k secondary), with any precision+k proxy noted only if the architect made that explicit, justified call; trust assertions are fixture-based; the snapshot is ephemeral; **eval execution is deliberately NOT wired into the dev workflow as a quality gate — that is a separate future design.**

8. **Forward-discipline so future schema work keeps the corpus valid** (Band 3 — the linchpin; choreography + knowledge, patterned on existing project mechanisms). Three complementary layers:
   - **Protocol recommendation (deterministic; architect-authored, recommendation-only)** — nan-018 delivers **one document recommending a protocol trigger; it edits no `.claude/protocols/` file.** The recommended trigger, patterned on the `[CONDITIONAL] uni-docs` step, fires when **"your change alters the retrieval-shape hash"** (OQ-5; coupled to the Goal-6 hash, not an enumerated list — deterministic, no delivery-leader judgment). The recommendation describes how the design and delivery/bugfix protocols *would* carry the conditional eval-corpus-migration step. **A later uni-zero session ratifies and applies it; nan-018 touches no protocol file.**
   - **Mechanical guard** — the Goal-6 version stamp / retrieval-shape hash (catches drift even if the (future) trigger is missed). This is the live, code-level guard nan-018 actually ships.
   - **Unimatrix knowledge (surfacable)** — a `convention` ("schema/shape change ⇒ corpus migration") and `procedure` entries (how to migrate, how to author a scenario), surfaced to agents.
   Only the version-stamp (code) and the knowledge entries (retro) ship inside nan-018. The protocol layer is a **handoff recommendation** for separate ratification — no forward reference to a non-existent corpus arises because nan-018 makes no protocol edits at all.

## Non-Goals

1. **Wiring eval execution into the dev workflow as a quality gate.** Running evals as a standing gate that decides whether a retrieval change ships — CI-on-every-PR, automated regression policy, blocking-vs-advisory, ownership of failures — is a **separate future design** with its own process trade-offs. nan-018's Band-3 protocol trigger is **asset maintenance** (keep the corpus valid + validate it loads), explicitly NOT execution-gating. The line: nan-018 may run the corpus *once* to validate a migration; it does not make eval *results* a standing decision gate.
2. **Answering crt-053's Q5/Q8 themselves.** nan-018 builds the instrument; the **rewritten ass-073 / ass-074** take the measurements; crt-053 consumes them.
3. **Building crt-053's retrieval behavior** (leak fixes, redirect policy, #406, #585). That is crt-053.
4. **Re-tuning fusion weights or the confidence formula.** ASS-037 (#3984) is authority. nan-018 exposes status levers for *measurement*; it does not change deployed defaults.
5. **Changing the PPR algorithm / `personalized_pagerank` / positive-edge set.** Config exposure is additive; algorithms unchanged.
6. **Reviving NLI scoring** (`w_nli=0.00`, `nli_enabled=false` stand).
7. **A new crate.** Eval lives in `unimatrix-server` (ADR-004). nan-018 extends the existing module tree.
8. **Authoring an exhaustive production-scale scenario suite.** The primary fixture corpus is curated and small by design; breadth grows over time, not in one feature.

## Background Research

- **Eval harness today** (`unimatrix eval`, nan-007/nan-010, `docs/testing/eval-harness.md`): `scenarios` (query_log → JSONL, hand-authored `expected`), `run --configs a.toml,b.toml --k N` (in-process A/B; first = baseline), `report` (Markdown + zero-regression + distribution gate). Profile TOML deserializes as `UnimatrixConfig` overrides (`eval/profile/validation.rs`). Replay rebuilds `TypedGraphState` from the snapshot and calls the full `SearchService.search()`; retrieval mode is per-scenario.
- **Verified gaps (ass-073 scoping):** penalty constants are engine `const`s not config (`graph.rs:41–59`); no negative/trust metric in the metric set; latency-only cost; `determine_ground_truth` prefers `expected` over baseline (so property-based `expected` is honored once authored).
- **Staleness precedents:** ASS-039 (#4000) burned on null-`expected` scenarios measuring self-consistency; #4886 premise-drift; #500 snapshot-drift caution. The two-corpus + property-assertion design is the response.
- **Existing forward-discipline patterns to mirror:** the `[CONDITIONAL] uni-docs` step in the delivery protocol (conditional-on-criteria); `/uni-release` mechanical mirror-and-diff (mechanical guard). Band 3 patterns on both rather than inventing a new mechanism.
- **Architecture authority:** ADR-004 (eval lives in `unimatrix-server`, module tree, single-binary). Ships via `cargo install` from main — no npm release.

## Acceptance Criteria

- **AC-01 (Tunability):** the crt-014 status-penalty levers are `UnimatrixConfig` fields threaded through `graph_penalty`; a profile TOML sweep of a steepness value produces the expected A/B delta; **default values reproduce current behavior bit-for-bit** (exposure is additive).
- **AC-02 (Trust metric — absence):** the harness can assert, per scenario, that a specified forbidden ID set is **absent** from results, surfaced in `report` and counted in the regression check.
- **AC-03 (Trust metric — relative rank):** the harness can assert ID-A ranks below ID-B, surfaced and gated likewise.
- **AC-04 (Trust in the sweep):** a steepness sweep can report trust outcomes (AC-02/AC-03) **alongside** P@5/MRR in the same run, so "steepness X → trust holds AND relevance did not regress" is a single correlated result.
- **AC-05 (Fixture corpus):** a fixture entry-graph (the five status shapes) can be authored, loaded, and searched; `expected` assertions are **property/relationship-based**, not literal ID lists.
- **AC-06 (Primary corpus delivered):** the canonical fixture corpus ships in-repo, version-controlled, covering at minimum multi-correction chain, dangling chain, superseded-Active, and deprecated-connected shapes.
- **AC-07 (Two-corpus model):** the production-snapshot path still produces realistic P@5/MRR baselines; docs state fixture = primary/durable, snapshot = realism/ephemeral.
- **AC-08 (Drift guard):** the fixture corpus carries a **retrieval-shape hash** stamp (plus migration number for legibility), computed over an **explicitly enumerated** input set — entry columns, edge types, confidence dimensions, and embedding dimensionality / model-id; the harness **fails-loud or warns** on a hash mismatch between the running schema's retrieval shape and the corpus stamp (tested by simulating a mismatch). If embedding model-id / dimensionality is included in the hash inputs, embed-at-load fixtures are protected against ONNX embed-model drift (OQ-3 branch b); otherwise a frozen vector sidecar is required (OQ-3 branch a). The spec states which branch is taken.
- **AC-09 (Cost — build the dial, token-weighted):** an explicit **token-weighted** cost metric is added to the harness and surfaced in `report`, defined as **cost = Σ(per-result token-proxy)** — the tokens an agent actually pays to read the set — with **result-set-size (k) as a SECONDARY axis, not the primary** (same k, different token load → different cost). Narrowing to a documented precision+k proxy is permitted **only** if the architect makes that call explicitly in design with stated justification; even then it does not displace the token-weighted definition as primary, and it is **not** a deferral to a downstream spike (that spike cannot measure cost-of-noise until this metric exists). Whichever is chosen is stated explicitly, not left ambiguous; the default and lean is the explicit token-weighted metric.
- **AC-10 (Band 1 docs):** `docs/testing/eval-harness.md` updated; ADRs stored in Unimatrix for the architecture decisions.
- **AC-11 (Band 2 docs):** fixture-corpus authoring guide, schema-migration runbook, two-corpus model, and config-knob reference exist and are sufficient for a dev (human or agent) to author/migrate/sweep without reverse-engineering code.
- **AC-12 (Band 3 forward-discipline):** (a) nan-018 delivers a **documented protocol recommendation** for later uni-zero review — the recommended conditional eval-corpus-migration trigger (patterned on `[CONDITIONAL] uni-docs`, firing when "your change alters the retrieval-shape hash") — and **makes no protocol changes** to any `.claude/protocols/` file; (b) a Unimatrix `convention` couples schema-shape change to corpus migration and is surfacable in briefing; (c) `procedure` entries document the how. Layers (b) and (c) ship inside nan-018; layer (a) is a recommendation handed off for separate ratification.
- **AC-13 (Boundary):** **nan-018 introduces no protocol or workflow changes at all** — no eval-execution-as-workflow-gate, and **no edits to any `.claude/protocols/` file**; the entire Band-3 protocol layer is a recommendation handed off for separate ratification. The deferred-separate-design boundary is documented (Goal 7 assumptions). The recommended Band-3 trigger is asset-maintenance only (keep the corpus valid), explicitly NOT execution-gating.
- **AC-14 (Proof by use):** nan-018's own validation is that **a correlated steepness sweep is demonstrably runnable on the fixture corpus** — a sweep of an exposed steepness lever (AC-01) reports trust outcomes (AC-02/AC-03) alongside P@5/MRR (AC-04) and the cost metric (AC-09) in one run. This proves the instrument is real and the downstream spikes (rewritten ass-073, then ass-074) have something to measure with. The downstream spikes consume the instrument to answer crt-053's Q5/Q8; nan-018 only demonstrates the sweep executes end-to-end on its delivered corpus.

## Constraints

- C-01: Eval lives in `unimatrix-server` (ADR-004); extend the module tree, no new crate.
- C-02: Config exposure is **additive** — deployed defaults and behavior unchanged at default values (ASS-037 authority; this feature does not re-tune).
- C-03: Trust metrics live **in the harness** (not only the Python integration suite) because Q8 is a sweep needing its success criterion in-instrument.
- C-04: Property-based ground truth only for the fixture corpus (crt-013 #703); no literal-ID `expected` in the primary set.
- C-05: Single edge language JS/TS — eval CLI/harness is internal Rust tooling, not a client surface (Python integration suite remains the internal correctness harness; nan-018 does not change that boundary).
- C-06: Ships via `cargo install` from main — no npm release, no packaging churn (this is the "use eval as-is" concern resolved: enhancement rides the existing dev-binary path). *(Was C-07; renumbered after the former C-06 was dissolved.)*
- **C-07 — DISSOLVED:** the former "Band-3 protocol edits land with/after the capability ships" constraint is **moot and removed** — nan-018 makes **no protocol edits** (the Band-3 protocol layer is a recommendation-only deliverable handed off for separate ratification), so there is no protocol-vs-corpus sequencing constraint to enforce.

## Dependencies

- **nan-018 has no upstream feature/spike gate.** Its requirements are established by this scope; all premises are verified against current main (penalty `const`s at `graph.rs:41–59`; no penalty/orphan/hop_decay fields in engine `config.rs`; metric set lacks any forbidden-ID-absent / rank-below assertion; cost is latency-only; eval lives at `crates/unimatrix-server/src/eval/`). Design proceeds immediately.
- **nan-007/nan-010** — the existing eval harness this extends; `docs/testing/eval-harness.md`.
- **ADR-004** — eval-in-server architecture.
- **crt-014** (`graph.rs` topology penalties) — the constants being exposed.
- **ASS-037 (#3984)** — fixed formula authority (do not re-tune).
- **Downstream consumers (NOT inputs):**
  - **ass-073 (to be REWRITTEN)** — its original scope cannot run (no instrument). Once nan-018 ships, ass-073 is rewritten to perform only the evaluation nan-018 unlocks (steepness/trust/cost sweep on the fixture corpus).
  - **ass-074** — consumes / extends that measurement.
  - **crt-053** — consumes the findings; stays HOLD until the downstream spikes return numbers.

## Open Questions

These five questions previously routed to ass-073. They can no longer be answered by a spike that cannot run — the spike has no instrument, and the very measurement that would settle the empirical ones is what nan-018 must build first. **All five are now RESOLVED** (the human has ratified the product/protocol-facing answers). Each is recorded below with its resolution and owner: **architecture** (architect documents and applies in design/spec) or **human-later** (architect documents the recommendation; the human ratifies and applies it in a later uni-zero session). Per the **"build the dial"** principle, the empirical answers build the capability rather than narrow away a measurement never yet taken.

- **OQ-1 (Cost metric — explicit vs. precision+k proxy):** **RESOLVED. Owner architecture.** Build the **explicit token-weighted cost metric** — **cost = Σ(per-result token-proxy)**, with **set-size (k) as a SECONDARY axis, not the primary** (the same k can carry wildly different token loads; the cost an agent pays is tokens-read). Proxy-narrowing to precision+k survives only as an explicitly-justified architecture call in design, never as a deferral, and never displaces the token-weighted definition as primary. Sets Goal 3 / AC-09.

- **OQ-2 (Steepness exposure shape):** **RESOLVED. Owner architecture.** Expose the crt-014 penalty **constants individually PLUS an optional single-multiplier overlay**. Rationale: the sweep may find one penalty type needs adjusting, not all uniformly — individual exposure preserves that discovery; the instrument must not pre-narrow the experiment. The single multiplier is a convenience overlay, not a replacement for per-constant access. Sets AC-01's surface.

- **OQ-3 (Corpus vector-sidecar pairing):** **RESOLVED. Owner architecture** (architect's call), with a **binding product caveat**: the primary fixture corpus is a **durable yardstick**. Embed-at-load is the lower-tax lean, but it makes the reference baseline sensitive to **ONNX embed-model drift** — a second staleness vector beyond schema shape. The architect must choose one of: **(a)** pin determinism with a **frozen vector sidecar**; OR **(b)** ensure OQ-4's drift guard **also catches embed-model / embedding-dimensionality changes**, in which case embed-at-load is safe. **Binding constraint: the durable reference must not silently become embed-model-dependent.** Coupled to OQ-4 (the resolution of whether embedding model-id / dimensionality is in the hash decides which branch is taken). Affects AC-05/AC-06/AC-08 and the authoring guide.

- **OQ-4 (Version-stamp granularity):** **RESOLVED. Owner architecture** (the linchpin). Stamp the corpus with a **retrieval-shape hash, plus the migration number for human legibility**. Added requirement: **the hash's inputs are explicitly enumerated** — which entry columns, edge types, confidence dimensions, and **embedding dimensionality / model-id** feed it. This enumeration is load-bearing: it is what the OQ-5 protocol recommendation documents, and it is where OQ-3's embed-model question is resolved (the spec states explicitly whether embedding dimensionality / model-id is in the hash — if it is, OQ-3 branch (b) holds and embed-at-load is safe). **OQ-4 and OQ-5 are coupled and must be designed together.** Sets AC-08.

- **OQ-5 (Protocol-trigger criteria):** **RESOLVED. Owner human-later** (architect documents the recommended predicate; the human ratifies and applies it in a later uni-zero session — **OQ-5 is no longer a gate** and nan-018 makes **no protocol-file edits**). The ratified recommended predicate is: **the Band-3 conditional fires when "your change alters the retrieval-shape hash"** — coupled to OQ-4, **not** an enumerated list. Rationale: an enumerated trigger ("any change to entry columns…") is judgment-prone (does a display-only column count?) and over-broad, and an over-broad trigger gets ignored (false-positive fatigue → the gate rots, defeating Band 3's purpose). Tying it to the hash is **deterministic** (the hash moves or it doesn't — no delivery-leader judgment), **precise** (only shape-affecting changes fire), and **unified** (OQ-4's mechanical guard and OQ-5's protocol trigger become ONE definition of "shape," not two that can drift). The enumerated list (entry columns / edge types / confidence dims / embedding dimensionality+model-id) becomes **the documentation of what feeds the hash**, not the trigger itself. Sets AC-12.

**Human input:** all five OQs are resolved. The product/protocol-facing answers (OQ-3 durability caveat, OQ-5 trigger predicate, the recommendation-only scope of the Band-3 protocol layer) are **human-ratified**. OQ-1, OQ-2, OQ-4 are architect-owned and need no further human gate to begin design. OQ-5's recommended predicate is architect-to-document and **human-to-apply-later** in a separate uni-zero session — nan-018 itself touches no protocol file.

## Delivery Sequencing

This is a **wide** feature: config exposure + a new metric class (trust) + a token-weighted cost metric + a hand-authored fixture corpus + a drift guard (retrieval-shape hash) + three documentation bands + a protocol recommendation. It must be **waved deliberately, not delivered as an all-or-nothing monolith.**

- **Wave 1 — the instrument core (AC-01…AC-09) is the load-bearing spine.** Tunability levers, the trust metric class, the token-weighted cost metric, the fixture/primary corpus, the two-corpus model, and the drift guard. This is what the downstream sweep depends on; it must ship to unlock the rewritten ass-073.
- **Wave 2 (deferrable if delivery gets heavy) — docs + Band-3 forward-discipline (AC-10…AC-13).** Band 1/2 docs, the Unimatrix `convention`/`procedure` knowledge, and the architect-authored protocol **recommendation**. These **do not gate the downstream sweep** and can land as a later wave without blocking measurement.

Guidance for synthesis/delivery, recorded here so it is not lost: protect AC-14 (proof-by-use end-to-end sweep) as the Wave-1 exit; let the documentation/forward-discipline band trail if the core runs heavy.

## Tracking

GH Issue: **#716** (`goal:self-learning`, `enhancement`). Chain: **nan-018 (#716)** → (rewritten) ass-073 (measurement) → ass-074 → crt-053 (behavior, HOLD). ass-073's original scope cannot run and is superseded; it is rewritten to consume the instrument after nan-018 ships — it is **not** an input to nan-018. Eval-execution-as-workflow-gate explicitly deferred to a separate future design. Architecture authority ADR-004; formula authority ASS-037 (#3984); harness docs `docs/testing/eval-harness.md`.
