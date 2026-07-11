# SCOPE — ass-098: A pipeline-independent, corpus-grounded relevance oracle — the topical-relevance ODOMETER for "did retrieval return the best answer to an arbitrary question?"

Origin: interactive human scoping session, 2026-07-10 (uni-zero); refined post-review same day —
per-surface evaluation added (G1/G4), briefing reframed as a replay-based goal-relevance surface
(via `cycle_events.goal`), judge fixed to Claude Sonnet via `claude -p`, graded-metric extension
pre-approved.

**Revision — 2026-07-11 (query-source reset).** A live probe proved the historical `query_log` has
**0 `context_search` calls** (3,105 `uds` vs 1 `mcp`-probe); the earlier per-surface plan of mining
the log for real user questions is a **documented dead-end**. The primary query source is now
**synthetic reverse-QA** (sample entry E → generate query Q that E answers → `context_search(Q)`),
scored by **known-item recall@k/MRR** + **graded nDCG@5**. The prior `uds` odometer number (0.615) —
which measured whole UserPromptSubmit turns, not `context_search` — is **retracted** (#951). The
`uds` and `cycle_events.goal` surfaces are demoted to optional lesser cross-checks.

Builds directly on **ass-097**
(`product/research/ass-097/FINDINGS.md`), which proved that snapshot-MRR on our corpus is a
**near-degenerate, self-referential comparator** — because its ground truth WAS the pipeline's own
logged results (`query_log.result_entry_ids`, confirmed at `eval/scenarios/extract.rs`). ass-097's
G6 verdict was GO with a re-scoped instrument and named the missing piece: an **honest,
corpus-grounded ground-truth oracle that is independent of the retrieval pipeline**. ass-098 builds
and validates that oracle layer. **It does not build a new harness** — the harness already computes
the metrics; what is missing is a trustworthy, pipeline-independent source of truth to feed them.

The load-bearing lesson carried forward: the relevance oracle MUST be independent of the retrieval
pipeline. Scoring retrieval against its own logged output measures self-consistency, not quality
(the ass-097 failure mode). Every design choice below exists to honor that independence.

## Framing

Keep three measurement jobs sharply distinct — the spike must never conflate them:

- **Comparator** (dev-time A/B): snapshot → P@5/MRR on a frozen shape. Legitimate, working, but only
  meaningful *relative* to a fixed configuration. ass-097 showed it is near-degenerate on our corpus.
- **Floor-proof / consistency floor** (the nan-018 fixture property-assertion class —
  `redirect_to_head` / `forbidden_absent` / `rank_below`): a **cheap, deterministic, absolute**
  pass/fail that retrieval honors what the corpus records *about itself* (status/currency invariants).
  This is the FLOOR. It is complementary to — not a substitute for — what ass-098 builds.
- **Odometer** (this spike): a **costed, graded, absolute** topical-relevance metric answering *"did
  retrieval return the best answer to an arbitrary user question?"* — real-world topical relevance,
  not corpus-self-consistency. It must be comparable across corpus states so "does a larger corpus
  perform better or worse" is a valid read.

**The pipeline-independent-oracle principle (stated up front, non-negotiable):** the relevance
judgment feeding the metric must come from a source that never saw, and does not derive from, the
pipeline's own ranking. In ass-098 that source is an **LLM-judge**, calibrated against a **human
golden set**, plus an **objective known-item anchor** (the corpus entry a query was built to answer).
Note the query-source reset (2026-07-11): `query_log` holds **zero** real `context_search` queries
(see G1), so queries are **not** mined from the log — they are **synthesized by reverse-QA** (pick an
entry E, generate a query Q that E answers, search Q). Independence is *strengthened*, not weakened:
the ground-truth target E is chosen independently of the pipeline, and the judge never sees the
pipeline's ranking.

The floor (nan-018 fixture assertions) and the odometer (ass-098) are **both** the `done_when` of
**SL-METRIC #5572**, not either/or: the floor proves retrieval doesn't violate self-recorded
invariants; the odometer proves it answers arbitrary questions well, and that this is improving (or
at least holding under corpus growth).

## Goal (answerable questions)

Empirical-first: construct the synthetic reverse-QA query bank, build a throwaway judge, calibrate it
against humans, compute the first absolute numbers (known-item + nDCG@5), and *prove they discriminate*
— before recommending anything. The empirical core (G1–G5) is deliberately tight; G6–G7 are
directional design; G8 is the call.

- **G1 — Query-bank construction via synthetic reverse-QA + the `query_log` dead-end (empirical).**
  The original plan — mine `query_log` for real user questions — is **empirically dead** for the
  surface that matters. Document the finding, then build the query bank synthetically.

  **The documented `query_log` finding (grounded by a live probe, 2026-07-11).** `context_search`
  *does* log a clean request→response pair — probe row `query_id=5498`, `source="mcp"`,
  `retrieval_mode="flexible"`, `result_entry_ids=[4895,4085,2806,3549,3524]` = exactly the top-5
  returned — so **`context_search(query) → top-k` is the correct, logged, gradeable unit**
  (`mcp/tools.rs:848-854`). **But the historical log has zero real `context_search` traffic:** the
  prod DB (`~/.unimatrix/0d62f3bf1bf46a0a/unimatrix.db`) is `source` split **`uds`/strict = 3,105 vs
  `mcp`/flexible = 1**, and that single `mcp` row is the probe. Agents here only **auto-inject** (the
  `uds` UserPromptSubmit/SubagentStart hook path, `listener.rs:1757-1765`); **nobody explicitly
  searches.** The `uds` rows feed **whole conversational prompts** as the search string — a separate,
  lesser surface (conversational-turn relevance), **not `context_search` quality**. This is why the
  prior G4 `uds` number (0.615) was **retracted** (correction posted to #951): it measured retrieval
  on whole UserPromptSubmit turns, not on `context_search`. (Two facts retained from earlier grounding,
  now demoted to context: `uds` can't split UserPromptSubmit vs SubagentStart without a 1-line change;
  `context_briefing` uses a different algorithm and logs nothing — see Prior art.)

  **The reset — build the bank by SYNTHETIC REVERSE-QA through `context_search`:**
  1. **Sample entries E** from the paired snapshot, spread across category/status. Exclude
     deprecated/quarantined **as targets** (they should recede, not be the sought answer) — justify the
     handling; they remain in the corpus as distractors.
  2. **Generate a query Q per E** with the `claude -p` generator: a focused, realistic developer
     **search** query that E *answers* — phrased as someone searching for the KNOWLEDGE/PROBLEM E
     addresses, **not a paraphrase of E's wording**. **Leakage control:** instruct against lexical
     copying; optionally verify low token-overlap(Q, E) and regenerate on failure.
  3. **Run `context_search(Q)`** on the same paired snapshot → top-k. (Paired-snapshot discipline:
     E is sampled from, and Q is searched against, the *same* DB state.)

  **Caveats to carry (state explicitly):**
  - **Synthetic, not real usage.** Reverse-QA queries stand in for `context_search` traffic that does
    not yet exist. A true real-usage odometer still needs actual `context_search` adoption — an
    **unanswered/future item** (see OPEN), not something this bank resolves.
  - **Known-item non-uniqueness.** E is *a* correct answer by construction, but not necessarily the
    *only* good one — a different-but-equally-good entry must not be penalized. This is exactly why the
    graded-relevance score (G4) complements the objective known-item score.
  - **Optional real-input cross-check.** The one source of *genuine* human-authored queries available
    offline is `cycle_events.goal` (`db.rs:537`, keyed by feature_cycle) — usable as a small,
    goal-relevance sanity check on the synthetic bank (replay through `IndexBriefingService`), never
    the primary source and never pooled with the `context_search` number (different construct).

  **Premise gate:** if a low-leakage synthetic bank that *discriminates* (G4) cannot be built, the
  approach is at risk — say so plainly.
- **G2 — LLM-judge oracle PoC (empirical).** Build a **throwaway** LLM-judge that, given a query and
  the top-k retrieved entries, emits **graded best-answer relevance** per entry. Deliver: the
  rubric/prompt, the graded output schema, a stability read (same input → same grade across repeats),
  and a **cost/latency profile** (LLM-judge spend is FUNDED but must be reported and bounded). Judge
  model = **Claude Sonnet** (approved). Grounded provider correction: there is **no reusable outbound
  LLM code seam** in the repo — the "Claude / Codex / Gemini integrations" are **inbound** event
  hooks (those CLIs report lifecycle events *into* Unimatrix; `KNOWN_PROVIDERS` at `uds/hook.rs:158`
  is source attribution), not LLM callers; no SDK, API key, or HTTP client exists. **But** the spike
  runs inside a Claude Code environment, so the lowest-friction path is a **`claude -p` CLI shellout
  from the scratch script** (subscription-covered, no API key, report usage) — *not* a net-new API
  dependency. Fallback if the CLI path is unavailable at execution: Anthropic API + an
  `ANTHROPIC_API_KEY` in `.env`, capped at the stated budget.
- **G3 — Human golden-set calibration (empirical).** Hand-judge a small golden set (~50–150 queries)
  and measure LLM-judge↔human agreement (κ / correlation). Where does the judge diverge, and does the
  divergence have a pattern? Deliver a defensible statement: **is the judge trustworthy enough to
  scale**, and in what regime? Without this anchor the metric is unfalsifiable — the judge is trusted
  only where it tracks the human.
- **G4 — The success metric + discrimination test (empirical — the crux).** On the reverse-QA bank,
  run `context_search(Q)` → top-k on the paired snapshot and score **two complementary ways** — the
  first absolute topical-relevance number(s):
  - **Known-item (objective, no judge).** E is the answer by construction: is E in the top-k, and at
    what rank → **recall@k / MRR on the known target**. This reuses the existing binary seam unchanged
    — `expected=[E_id]` via `determine_ground_truth` (`eval/runner/metrics.rs:26`), **zero new code**.
  - **Graded relevance (the odometer).** The independent `claude -p` judge grades **all** top-k for
    relevance to Q → **nDCG@5**. This catches what known-item mis-scores: a *different-but-equally-good*
    entry (known-item unfairly penalizes) and *target-plus-junk* (known-item can't see). This needs the
    **throwaway graded metric** (nDCG@5 over `Vec<(u64, grade)>`), which is **pre-approved**.

  Report **both**; they complement — known-item is the cheap objective anchor, nDCG@5 is the graded
  odometer. Then run the **discrimination test** ass-097 could not construct: deliberately degrade
  retrieval (shuffle / truncate / inject distractors into the top-k) and confirm **both metrics drop**;
  hold or improve and confirm they don't — the positive/negative control proving the metric measures
  retrieval quality, not noise. (Optional, clearly separated: the `uds` whole-prompt surface and the
  `cycle_events.goal` real-input cross-check from G1 may be reported as distinct lesser numbers, never
  pooled with the headline `context_search` reverse-QA number.)
- **G5 — Scale-comparability via subsampling (empirical + directional).** The corpus can't grow
  during the spike, so test cross-size comparability by measuring the odometer on **corpus subsets of
  increasing size**. Does the metric behave coherently as corpus size grows? Design the
  **difficulty-normalization** the metric needs so cross-size reads are honest — a larger corpus is
  *harder* (more distractors), so holding the metric under growth is itself a win, and a naive
  cross-size comparison is confounded by difficulty. Deliver: the subsample study + a difficulty-aware
  comparison design.
- **G6 — crt-feature gate protocol (directional).** Once trustworthy, how is the odometer used as a
  **gate** to validate the impact of a crt-type feature (before/after the change, does the metric
  move)? Design the gate protocol and the **minimum discriminating power** required to trust a gate
  verdict (effect size vs judge noise vs sample size). What movement is real vs within judge jitter?
- **G7 — Product-capability shape (directional).** What would it take to ship this as a dev-facing
  product feature — *"evaluate your own corpus"* — fitting the personal-cloud, per-project,
  independent-config-and-db model (the multi-project goal)? Sketch the surface, per-project fit,
  corpus-agnostic boundaries, and where an internal gate instrument and a shippable feature would
  pull apart (if they do).
- **G8 — Go/no-go + recommended build shape (directional).** Given G1–G7: is there a trustworthy,
  pipeline-independent topical-relevance odometer we can build on the corpus we have now? Recommend
  the build shape (judge model, rubric, calibration cadence, metric form, where it lives) — or report
  the blocking gap.

## Breadth

`code+ecosystem`.

- **Internal (code):** the `context_search` path — the gradeable `query → top-k` unit
  (`mcp/tools.rs:848-854`) replayed via `EvalServiceLayer` (`eval/profile/layer.rs`) on a paired
  snapshot; the corpus entries sampled as reverse-QA targets E; the eval harness metric layer
  (`unimatrix-server/src/eval/runner/metrics.rs`) and its GT-injection seam (`determine_ground_truth`
  + `ScenarioRecord.expected` — reused unchanged for the known-item `expected=[E]` score); the
  `query_log` schema (`unimatrix-store/src/migration.rs:264-275`) — documented dead-end, 0 `mcp` rows;
  and the provider seam a judge sits beside (`unimatrix-embed`: `EmbeddingProvider` /
  `CrossEncoderProvider`, all local ONNX — no generative LLM, so the judge shells out to `claude -p`).
- **Ecosystem:** LLM-as-judge / RAG-eval prior art — **reproduce, don't re-derive**. RAGAS-style
  graded relevance / answer-relevance, TREC pooling and pooled-judgment methodology, nDCG and known-
  item retrieval, and judge-calibration practice (LLM↔human agreement, κ, position/verbosity bias).

## Approach

- `measurement` + `proof-of-concept` for the empirical goals (G1–G5): construct the synthetic
  reverse-QA query bank, build a **throwaway** judge PoC, run it, calibrate it against a human golden
  set, compute the first known-item + nDCG@5 numbers, and run the degrade/hold discrimination controls
  + the subsample scale study.
- `investigation` for the directional goals (G6–G7): design the crt-gate protocol and the
  product-capability shape — no build required.

## Confidence required

- `empirical` for G1–G5 — real query set, a working judge, a real agreement statistic, a real first
  number, and real discrimination + subsample evidence.
- `directional` for G5's difficulty-normalization design, G6 (gate protocol), G7 (product shape), and
  G8 (go/no-go + build shape) — defensible designs and a recommendation, not validated builds.

## Target outputs

- **G1** — the **synthetic reverse-QA query bank** (entries E sampled across category/status +
  generated queries Q, with leakage-control evidence) **and** the documented `query_log` dead-end
  finding (probe row 5498; `uds` 3,105 vs `mcp` 1; `context_search` is the correct gradeable unit but
  has zero history; `uds` whole-prompt is a lesser surface). Include the known-item-non-uniqueness +
  synthetic-query caveats and the premise-gate verdict.
- **G2** — a **working throwaway LLM-judge PoC** + its rubric/prompt + graded output schema +
  stability read + **cost/latency profile** against a stated spike budget.
- **G3** — the **calibration result**: LLM-judge↔human agreement stat on the ~50–150-query golden set,
  a divergence characterization, and the trust-to-scale verdict.
- **G4** — the **first absolute topical-relevance numbers** on the reverse-QA `context_search` unit:
  **known-item recall@k / MRR** (objective, E-as-target) **and nDCG@5** (graded, judge over all top-k),
  reported together, + the **discrimination-test result** (degrade → both drop; hold → hold). Any
  `uds` / `cycle_events.goal` cross-check numbers reported separately, never pooled.
- **G5** — the **subsample scale study** (odometer vs corpus-subset size) + the
  **difficulty-normalization design** for honest cross-size reads.
- **G6** — the **crt-feature gate protocol** design + its minimum-discriminating-power threshold.
- **G7** — the **product-capability sketch** (surface, per-project fit, boundaries, gate-vs-product
  tension).
- **G8** — **go/no-go + recommended build shape**.

## Constraints

**Hard (fixed):**
- **Research only — no committed product code, no PR.** The judge PoC is **throwaway/scratch**, like
  ass-097's harness. No capability-status writes (uni-zero applies those after reviewing findings).
- **NEVER commit `query_log` content or snapshots.** `query_log` is sensitive agent-interaction
  history (per `snapshot.rs` content-sensitivity warning NFR-07/C-12 and `retention.rs` activity-data
  classification) — scratch dir only, treat exactly as ass-097 treated snapshots.
- **`query_log` mining is a documented DEAD-END** (0 `mcp` rows; see G1) — it is **not** the query
  source. The gradeable unit is **`context_search(Q) → top-k` on a paired snapshot**, with Q from the
  synthetic reverse-QA bank. `query_log`'s `result_entry_ids` (pipeline output) remain **never** a
  relevance signal — the ass-097 self-GT trap this spike exists to escape.
- **The metric is scored against pipeline-independent ground truth, never against pipeline logs.**
  Known-item uses `expected=[E]` (E chosen independently of the ranker) via the `determine_ground_truth`
  hard-label channel; graded relevance uses the independent `claude -p` judge over the top-k. The
  self-referential path is bypassed by construction.
- **Paired-snapshot discipline** (the #500 KB-drift trap): entries E are sampled from, and each query
  Q is searched against, the **same** snapshot DB state — otherwise E's presence/rank measures KB
  drift, not retrieval. Never pass the live DB to `eval run` (the live-DB path guard). (Note: a probe
  `context_search` run against the *live* DB writes an `mcp` `query_log` row — as the row-5498 probe
  did — so run the bank against the snapshot for reproducibility and to avoid polluting the log.)
- **LLM-judge spend is bounded to a stated spike budget** even though funded — report the cost/latency
  profile and stop at the budget. Budget ceiling applies to the API fallback path (**$75**); the
  primary `claude -p` CLI path is subscription-covered (no per-call spend to cap) but usage is still
  reported.
- **Judge = Claude Sonnet via the `claude -p` CLI shellout** (primary) — subscription-covered, no
  `.env` key, called from the throwaway scratch script. There is **no reusable outbound LLM code
  seam**: the repo's Claude/Codex/Gemini integrations are inbound event hooks (provider attribution),
  not LLM callers; embeddings + NLI are local ONNX only. Fallback only if the CLI path is unavailable:
  Anthropic API with `ANTHROPIC_API_KEY` in `.env` (never in code), capped at the $75 ceiling.
- **The graded metric extension (throwaway nDCG@k over `Vec<(u64, grade)>`) is pre-approved** as a
  contingency if binary P@k/MRR proves too coarse (G4) — it remains scratch code, never committed.

**Hypothesis (challengeable positions to TEST — not givens):**
- The **LLM-judge agrees with human judgment closely enough to trust at scale** (G3 may show it does
  not, or only in a narrow regime).
- **Synthetic reverse-QA queries are representative of real `context_search` usage.** Generated
  developer queries stand in for traffic that does not yet exist — they may be easier, cleaner, or
  differently-distributed than real searches. This is a genuine limitation to test (leakage control +
  the discrimination control in G4 bound it), and a real-usage odometer ultimately needs actual
  `context_search` adoption (OPEN).
- **Known-item scoring is fair** — E is *a* correct answer but maybe not the only one; the graded
  nDCG@5 (judge over all top-k) is the guard against known-item's non-uniqueness penalty (G4 tests
  that the two agree/complement).
- **nDCG/P@k-against-the-judge actually discriminates** retrieval quality (G4's degrade/hold controls
  test it; a metric that doesn't move under deliberate degradation is unfit).
- The metric is **comparable across corpus sizes** via subsampling (G5 — difficulty confound may break
  naive comparison).
- **One instrument can serve BOTH an internal crt-gate AND a shippable product feature** without the
  two designs pulling apart (G6/G7 — they may diverge).

## Dependencies

Builds on:
- **ass-097** — the self-GT failure diagnosis; the *reason* the oracle must be pipeline-independent,
  and the source of the Comparator/Floor/Odometer framing this spike inherits.
- **nan-018** — the complementary fixture property-assertion **floor** (the cheap deterministic half
  of SL-METRIC #5572); ass-098 is the costed graded **odometer** half.
- **ass-039** — behavioral ground truth (1,761 scenarios) as prior art on non-pipeline truth sources.

If **go**, ass-098 unblocks: the **odometer half of SL-METRIC #5572's `done_when`** (the trusted
live-corpus quality verdict, ass-097's open `done_when(2)`); a **crt-feature impact gate**; and a
candidate **product capability** ("evaluate your own corpus").

## Prior art

- `product/research/ass-097/FINDINGS.md` — snapshot-MRR is near-degenerate/self-referential on our
  corpus; GO with a re-scoped instrument; the missing piece = a pipeline-independent oracle.
- `docs/testing/eval-harness.md` — the D1–D4 offline flow, the `eval scenarios` → `expected` vs
  `baseline` GT model, metric definitions (P@k/MRR/CC@k/ICD — **no nDCG/MAP**), the #500 paired-
  snapshot lesson, and the "eval is NOT a standing quality gate" boundary.
- `docs/testing/eval-fixture-authoring.md` + `docs/testing/eval-two-corpus-model.md` — the fixture
  (durable/trust) vs snapshot (realism/ephemeral) split; note the fixture loader **rejects literal-id
  `expected`** (crt-013 #703), so the oracle's literal-id hard labels ride the **snapshot / query-log
  scenario path**, not the fixture corpus.
- Grounded code seams: `query_log` schema (`unimatrix-store/src/migration.rs:264-275`, + `phase`
  via v16→v17 `:572-595`; struct `unimatrix-store/src/query_log.rs:32-44`); GT-injection
  (`eval/runner/metrics.rs:26` `determine_ground_truth`; `eval/scenarios/types.rs`
  `ScenarioRecord.expected`); current soft-GT source (`eval/scenarios/extract.rs` `expected: None` →
  `baseline.entry_ids`); provider (`unimatrix-embed` `EmbeddingProvider` / `CrossEncoderProvider`,
  local ONNX only — no outbound LLM).
- **Empirical `query_log` probe (2026-07-11) — the query-source reset evidence.** Live `context_search`
  on the prod DB (`~/.unimatrix/0d62f3bf1bf46a0a/unimatrix.db`) produced probe row **`query_id=5498`**:
  `source="mcp"`, `retrieval_mode="flexible"`, `result_entry_ids=[4895,4085,2806,3549,3524]` = exactly
  the top-5 — proving `context_search` logs a clean request→response pair (the gradeable unit). But the
  historical `source` split is **`uds`/strict = 3,105 vs `mcp`/flexible = 1** (the probe) — **zero real
  `context_search` history.** The prior `uds` odometer number (0.615) measured whole UserPromptSubmit
  turns, not `context_search`; **retracted, correction posted to #951**.
- **Surface grounding (now demoted to context):** the two production `query_log` writers are
  `mcp/tools.rs:848-854` (`context_search`, `source="mcp"`/`"flexible"`) and `uds/listener.rs:1757-1765`
  (`handle_context_search`, hardcoded `source="uds"`/`"strict"`/`phase=NULL` for **all** hook events;
  the `source: None` vs `Some("SubagentStart")` discriminator is live at `listener.rs:1616` but
  discarded at insert). Provider attribution is inbound-only (`uds/hook.rs:158` `KNOWN_PROVIDERS`).
- **Briefing alternate-source grounding** (a **third** surface via replay, not a query-log gap):
  `context_briefing` (`tools.rs:1868`) uses `IndexBriefingService` and writes **no** `query_log` row;
  its input `task` (`tools.rs:433`, the primary query) is **persisted nowhere**; audit records
  `operation="context_briefing"` with `target_ids` but no query (`tools.rs:2179-2195`,
  `audit.rs:46-67`); usage is a **no-op at `access_weight=0`** (`usage.rs:321-327`). The
  pipeline-independent input that IS recoverable is the session/cycle **goal text**:
  `cycle_events.goal` via `get_cycle_start_goal` (`db.rs:537`), keyed by feature_cycle. Historical
  input⊕returned pairing is only a session-scoped `audit_log → sessions (db.rs:928) → cycle_events`
  join, and its returned ids (and `goal_clusters.entry_ids_json`, `db.rs:1229`) are pipeline output —
  usable only for the replay input, never as GT.
- Capability map: **SL-METRIC #5572** (keystone — the floor+odometer `done_when`), **SL-REUSE #5577**
  (the longitudinal reuse signal), self-learning goal **#5684**.
- The **multi-project independent config+db** goal — the per-project, self-serve fit for the G7
  product-capability shape.

## Tracking

GH Issue: **TBD** — created on scope approval.

---

## OPEN — needs human

**Both prior OPEN items are RESOLVED** (human decisions, 2026-07-10):
- *Metric form* — throwaway graded nDCG@k extension **pre-approved** as a contingency if binary
  P@k/MRR proves too coarse (G4 decides on evidence). Folded into G4 + Constraints.
- *Judge provider + budget* — model = **Claude Sonnet**; path = **`claude -p` CLI shellout**
  (subscription-covered, no key); API fallback capped at **$75**. Corrected the "no LLM seam" claim:
  the repo's Claude/Codex integrations are inbound event hooks, not reusable outbound callers.

**Residual (non-blocking) — for downstream / human awareness, not a gate on starting the spike:**
1. **The reverse-QA bank is SYNTHETIC — a real-usage odometer still needs actual `context_search`
   adoption.** With zero real `context_search` history (0 `mcp` rows), the spike measures retrieval on
   *generated* developer queries, which validates the instrument but not real-world question-relevance
   distribution. Closing this needs a product shift — agents (or humans) actually using `context_search`
   so the log accrues real queries — which is itself a signal G7's product-capability shape should
   drive. Flagged as the primary unanswered/future item.
2. **The `uds` bucket cannot split UserPromptSubmit vs SubagentStart** without a 1-line change (stop
   hardcoding `source="uds"` at `listener.rs:1763`; thread the live discriminator at `listener.rs:1616`)
   — out of research scope. Relevant only if the lesser `uds` whole-prompt surface is reported.
3. **The briefing goal-relevance cross-check** (replay of `cycle_events.goal`) is an *optional*
   real-input sanity check, not the primary source; a faithful-input briefing odometer would require a
   delivery-side change to **persist the briefing `task`** at invocation granularity — a product
   decision, out of research scope.
