# FINDINGS (INTERNAL TRACK): Value communication for Unimatrix — personas, proof, and the agent(s)

**Spike**: ass-089
**Track**: INTERNAL (codebase + Unimatrix state). External personas/terminology handled in parallel; synthesis reconciles.
**Date**: 2026-07-02
**Approach**: investigation + evaluation (feasibility assessment, not implementation)
**Confidence**: directional (recommendation-grade, grounded in the real `capability` corpus, goal entries, ADR-004, PRODUCT-VISION.md, the bugfix-872 retro chain, and the agent/skill definitions)

**Grounding surveyed**
- `capability` corpus: SL1–SL7 + SLN1/SLN3 (self-learning), PD1–PD4 + PDN1 + PD-ROLLUP (proactive-delivery), C0–C16 + N1–N6 (personal-cloud), DA1 (domain-agnostic), SL-ROLLUP + SL-METRIC (curve/keystone). Status field is the truth source: `missing | partial | proven | claimed`.
- Goal entries: #4671 (root vision), #5219 self-learning, #4673 proactive-delivery, #4946 personal-cloud, #4678 domain-agnostic.
- ADR-004 (#1257) content boundary; PRODUCT-VISION.md.
- bugfix-872 seed artifact = two correction chains: #5280 (deprecated) → #5398 (active, extended via `context_correct` with full provenance) and #5329 (deprecated) → #5394 (REF-timer convention). Supersession + provenance visible in the graph (#5398 Prerequisite→#5394, Supports←#5399).
- Skill definitions `uni-capability` (the firewall, encoded) and `uni-retro` (the served-knowledge + correction loop, encoded).

---

## Findings

### Q A2: "Who are the enablement personas (evaluators; under-utilizing existing users) and what value are they currently *not* extracting?"

**Answer.** Four enablement personas, all inside the human-stated audience (teams running structured agentic workflows — software delivery and research — building products that need memory). Each mapped to the specific `proven` capabilities and idle skills/modes they are not exercising.

1. **The Evaluator (technical lead deciding to integrate).** Has felt agents relitigate/forget; is deciding whether Unimatrix earns the integration cost. **Not extracting:** the distinction between "a vector DB behind MCP" and a *self-learning + proactive* engine. They evaluate on install/first-run, where the value (the compounding curve) is not yet visible. Blind to SL2 (#5221 "useful knowledge surfaces more; misleading recedes" — self-tuning from usage), the proactive surface PD1/PD2 (#5362/#5368 — knowledge pushed before asked), and correction-with-provenance (SL7 #5390, correction chains).

2. **The Under-utilizing operator (installed, using it as a passive store).** Agents call `context_search`/`context_store`; the deployment behaves like RAG. **Not extracting, concretely:**
   - **Proactive delivery (PD1–PD4)** — they only *pull*; briefings/phase-conditioned injection are idle. Half the engine (the proactive surface the vision names as the differentiator) is dark.
   - **Correction with provenance** — they `deprecate + store` (or duplicate) instead of `context_correct`, so provenance chains never form and SL2's "misleading recedes" never engages (this is exactly what the bugfix-872 chain #5280→#5398 demonstrates done right).
   - **Retro (`uni-retro`)** — never run post-merge, so the dynamic layer never grows; the "every delivery makes it smarter" loop is broken at the source.
   - **Capability map (`uni-capability`)** — goals never decomposed, so the goal-driven "what's proven / what's left" view and honest status never exist for them.
   - **Typed edges (SL4 #5223, SL5 #5224)** — flat storage means graph-surfacing never fires; they get vector search only.

3. **The integrity-conscious evaluator (wrong-knowledge-served is expensive).** Regulated build, or research where a bad citation propagates. **Not extracting:** the integrity NFR set — hash-chain/audit (Architectural Principles 1–2), N-series (N1–N6), poison resistance SLN1 (#5229), graph-consistency-under-correction SLN3 (#5230). They would value the firewall itself but have never seen it surfaced.

4. **The research lead (running autonomous research spikes).** Named explicitly in the audience. **Not extracting:** graph-first retrieval orthogonal to vector search (DA validated via ASS-057), runtime taxonomy discovery DA1 (#5282), and the provisional-knowledge discipline (research stays *inert* in retrieval via `Motivates` not `Informs`, per `uni-capability`) — the property that makes an autonomous research loop safe.

**Evidence.** Capability names/statuses read directly from the corpus; skill behaviors read from `uni-retro` (Phase 1 served-knowledge reporting; Phase 2 correction/extraction) and `uni-capability` (decomposition + firewall). The "not extracted" set is the intersection of *proven-but-under-used* capabilities and *idle skills/modes*.

**Recommendation.** Structure the enablement asset around the two under-used engine surfaces the personas share: (1) the **proactive surface** (PD1–PD4 — stop pulling, start being handed knowledge) and (2) the **correction/retro loop** (`context_correct` + `uni-retro` — how the deployment compounds). Lead each persona section with the specific `proven` capability they are leaving on the table, named in the capability corpus's own outcome words. Do not lead with integrity NFRs except for persona 3.

---

### Q A3: "Where do awareness and enablement personas overlap vs diverge — does that overlap justify one agent or two?"

**Answer (internal-grounded; synthesis reconciles with external).**

**Overlap (large).** Per the human, the awareness target and the enablement target are the *same audience* — often the *same person at different funnel stages* (unaware → evaluating → using-but-under-utilizing). They share the entire honest spine: the `capability` corpus, the goal entries + vision, the real artifacts, the firewall, and the redaction boundary. Critically, **awareness posts are DERIVED FROM the enablement asset** (SCOPE line 22): the durable enablement doc is the source; posts are extracts of a single proven capability + one proof image.

**Divergence (bounded, along five axes):** audience stage (top-of-funnel vs evaluating/using), format (short thesis + one image vs durable instructional asset), truth-bar (awareness may hook on one `proven` capability; enablement must be comprehensive *and honestly mark `partial`*), cadence (opportunistic vs durable/versioned), and derivation direction (derived vs primary).

**Does the overlap justify one agent or two?** The overlap is in the load-bearing part — the *grounding and the firewall* — and the divergence is only in *audience/format/truth-bar/cadence*, i.e. thin presentation layers over one shared truth core. Forking the grounding across two independent agents is the specific hazard: marketing claims would drift from what enablement honestly admits is `partial` (e.g. PD-ROLLUP #5366 is `partial`; SL-ROLLUP #5369 is `claimed`). That drift is the "structurally present, behaviorally absent" self-refutation in the storefront.

**Internal lean: one agent, two modes over a single shared grounding/firewall core** (equivalently: one grounding module + two thin generators). This is the internal-grounded view; the external track may weight persona/terminology differently, and synthesis reconciles. See D1 for the full contract.

**Evidence.** Human audience statement (authoritative); SCOPE derivation direction (line 22); capability statuses (PD-ROLLUP `partial`, SL-ROLLUP `claimed`) that make a *single* honest source non-negotiable.

**Recommendation.** Treat awareness and enablement as **two modes of one agent sharing one grounding + firewall core** — not two independent agents. The overlap is in the truth core (must not fork); the divergence is presentation (cheap to parameterize).

---

### Q C1: "Which real system artifacts best *show* value... Rank by persuasive clarity. Use the bugfix-872 retro as the seed proof artifact."

**Answer.** Persuasive clarity = differentiation strength × readability. Ranked:

1. **The retro's served-knowledge narrative** ("N entries served across M sessions; #X retrieved in phase Y shaped the diagnosis"). The `uni-retro` Phase-4 format already produces this outcome-phrased. Highest composite: accessible to a non-expert, honest, and it *is* the seed. Shows the compounding loop concretely.
2. **The correction chain with provenance (the bugfix-872 seed: #5280→#5398 and #5329→#5394).** Maximally *differentiating* — it shows the product's signature behavior: a prior entry *extended/superseded with full provenance rather than duplicated*, and a deprecated entry visibly retired. It is the integrity product operating with integrity. But it is the most technical and the hardest to redact (see C2), so its readability is low top-of-funnel. Best used as the **proof-detail inside artifact #1**, not standalone.
3. **The capability-status view** (`context_graph` over a goal → proven/partial/missing/claimed). Uniquely persuasive for the *honesty* thesis: a product that publishes its own `partial`/`claimed` status is credible. It renders the firewall visible. Needs framing so `partial`/`claimed` reads as integrity, not weakness.
4. **A briefing / proactive-injection capture** (PD1/PD2: "entering phase X the agent was handed these 3 entries without asking"). Shows the proactive surface — the vision's differentiator. Truth-bar-limited: PD-ROLLUP is `partial` and PD2's injection path is called out as untested (#5368), so it can only be shown honestly on the specific proven instance.
5. **Aggregate served-knowledge counts / usage-scored ranking (SL2).** Numbers without a story. Lowest standalone clarity; use only as supporting evidence beneath #1–#3.

**Composite recommendation:** the best single proof artifact is **"a retro that contains a correction chain"** — #1 as the readable wrapper, #2 as the differentiating proof-detail inside it. This is exactly the bugfix-872 shape.

**Evidence.** `uni-retro` Phase-4 return format (served-knowledge line, outcome-phrased); the bugfix-872 chain read from #5280/#5398/#5329/#5394 (supersession + provenance edges present); `uni-capability` "Report what's left" op (the status view); PD-ROLLUP/PD2 statuses bounding artifact #4.

**Recommendation.** Rank the retro-with-correction-chain #1 as the default proof artifact; make the capability-status view the #2 recurring artifact (it is cheap to regenerate and it *is* the firewall shown). Reserve raw counts for support only.

---

### Q C2: "How are they captured and presented **safely**... What is the redaction / synthetic-example boundary?"

**Answer. The boundary: show real STRUCTURE from this project's own (tier-1) artifacts; redact or synthesize CONTENT; never source from another project; label any synthetic example as synthetic.**

The bugfix-872 entries make the risk concrete: they carry internal file paths (`packages/unimatrix/lib/hook-client/mcp-bridge`), env-var names (`UNIMATRIX_HOOK_MCP_STALE_MS`), issue/PR/CI-matrix specifics, size-budget internals, and security-mechanism detail (SLN1-class). None of that is a secret (Architectural Principle 8 / N2: no secret is ever in a DB), but it is *internal mechanism* that must not go outward.

**Three-tier source rule:**
- **Tier 1 — this project's own knowledge (Unimatrix dogfooding Unimatrix).** The *default and safest* proof source. Show with a light redaction pass: strip internal paths, issue/PR numbers, env-var/CI specifics, and any unshipped-feature or security-mechanism detail. Reduce to **outcome altitude** (this is also D3 — outcome, not mechanism).
- **Tier 2 — another project's / customer deployment's knowledge.** **Never shown verbatim.** A customer's `knowledge.db` holds their proprietary decisions. At most, show *shape* (counts, status distribution, edge topology) with all content redacted, and only with that customer's explicit consent. Default: do not touch.
- **Tier 3 — synthetic examples.** For anything not safely showable from Tier 1, construct a representative synthetic example (fabricated decision text over a real structural pattern). It **must be labelled synthetic** — presenting a fabricated artifact as real is the exact self-refutation the whole spike guards against; the honesty firewall applies to the *artifact*, not only the *claim*.

**Capture mechanism** (feeds the agent's grounding contract): a redaction pass that (a) asserts the source slug is this project, (b) strips paths / issue-numbers / env-var names / secret-shaped tokens / unshipped + security-internal detail, (c) reduces to outcome-altitude text, (d) flags anything ambiguous for human sign-off. **The human-in-loop step (D5) is the terminal redaction gate.**

**Evidence.** bugfix-872 entry contents (the internal tokens above); Architectural Principle 8 / N2 #5160 (no secret in any DB — so secrets are never the leak vector, internal mechanism is); the 1-client:1-project isolation posture (N3 #5356 "writes never mis-routed across projects") which extends to *outward presentation* — cross-project content must never surface.

**Recommendation.** Encode the three-tier rule as a hard precondition in the agent definition: Tier-1-only sourcing by default; Tier-2 forbidden without consent; Tier-3 must be labelled. Ship the redaction pass as part of the grounding module (shared spine), not per-mode.

---

### Q D1: "One agent (two modes) or two coupled agents? Define the shared spine... vs the distinct parts..."

**Answer. One agent, two modes over a single shared grounding + firewall core.** Decision driver: the truth core (capability spine + firewall + redaction) must be single-sourced; forking it is the drift hazard (A3).

**Shared spine (single-sourced, must not fork):**
- The `capability` corpus with its `status` field — the truth source.
- Goal entries (#4671, #5219, #4673, #4946, #4678) + PRODUCT-VISION.md — positioning.
- Real Tier-1 artifacts (retros, correction chains, capability-status views).
- The **firewall** (D2): market `proven`; describe `partial` honestly; never `claimed`.
- The **translation discipline** (D3): outcome altitude, drawn from `uni-capability` authoring rules.
- The **redaction boundary** (C2).

**Distinct parts (thin, per-mode):**

| Dimension | Awareness mode | Enablement mode |
|---|---|---|
| Audience | top-of-funnel, unaware of the problem | evaluating / under-utilizing (A2) |
| Format | short thesis (one claim) + one proof image | durable, sectioned instructional asset |
| Truth-bar | one `proven` capability as hook (must still be true) | comprehensive; must honestly mark `partial` |
| Cadence | opportunistic / frequent | durable, versioned, post-release (D5) |
| Derivation | **derived FROM** the enablement asset | **primary** source |
| Output | draft post + proof-artifact spec | draft enablement-doc sections |

**I/O contract (both modes):** IN = `context_search`/`context_get` over `capability`/`goal` (READ-ONLY, like this spike) + a redacted Tier-1 artifact set. OUT = a *draft* (never published) plus a per-claim citation to the backing capability ID and status. Enablement mode is the primary generator; awareness mode extracts hooks from enablement output.

**Evidence.** SCOPE lines 24–26 (shared honest spine, different audience/format/truth-bar); derivation direction line 22; capability statuses making one truth source non-negotiable.

**Recommendation.** Build **one agent with an awareness mode and an enablement mode** sharing one grounding module. Do not build two independent agents — that duplicates and risks forking the firewall. (Internal-grounded; synthesis reconciles with external track.)

---

### Q D2: "Grounding contract: how does generation stay anchored to `proven` capabilities so it cannot overclaim (the firewall, encoded in the agent definition)?"

**Answer.** Reuse the firewall that already exists in `uni-capability`, verbatim, as the *generation* contract. The capability skill's rule — "status advances to `proven` ONLY on attached behavioral, real-artifact evidence" — has a direct generation analogue: **generation may re-phrase capability status, never upgrade it.** Encode in the agent definition:

1. **Query-before-claim.** Every value claim must resolve to a `capability` entry (`context_search category=capability`) and READ its `status`. Unimatrix access is **READ-ONLY** (as in this spike) — the agent never writes.
2. **Status gate.**
   - `proven` → may be marketed as a delivered outcome.
   - `partial` → may be *described*, but MUST be framed as in-progress/honest ("we can X; Y is not yet proven"). Canonical example: PD-ROLLUP #5366 (`partial`).
   - `claimed` → **NEVER marketed.** Asserted, no behavioral test. Canonical example: SL-ROLLUP #5369 (`claimed`, blocked on SL-METRIC) — see Out-of-Scope: this blocks the single most natural marketing thesis.
   - `missing` → never mentioned as a capability.
3. **Artifact-backed.** Every proof image traces to a real Tier-1 artifact ID through the C2 redaction pass. No fabricated-as-real artifact.
4. **No synthesis beyond the corpus.** The agent may rephrase a capability's `name`/`why` (already outcome-phrased, layman-readable) but may not invent capabilities or upgrade status. It *inherits* status; it cannot *assert* status.
5. **Human-in-loop as terminal firewall (D5).** Drafts only; nothing publishes without human review — outward claims are irreversible.

**Evidence.** `uni-capability` "The firewall (load-bearing)" section; live statuses PD-ROLLUP `partial` / SL-ROLLUP `claimed` proving the gate has real teeth; READ-ONLY access model of this spike.

**Recommendation.** Encode the status gate as a literal rule table in the agent definition (proven→market, partial→hedge, claimed/missing→forbid) with a mandatory per-claim capability-ID citation, and keep the agent READ-ONLY in Unimatrix. The firewall is a *pre-generation query*, not a post-hoc review.

---

### Q D3: "Translation discipline: how are agent/workflow/skill definitions (choreography — the static layer) translated to *outcomes* rather than mechanisms in output?"

**Answer.** The `capability` corpus is *already* the translated layer, and the translation rule already exists — reuse both.

- **The capability `name` field IS the outcome vocabulary.** Every `proven` capability is a pre-translated outcome statement: PD2 #5368 "while working, relevant knowledge is pushed without the agent asking" — not "the injection hook fires on PreToolUse." SL2 #5221 "useful knowledge surfaces more; misleading recedes" — not "usage votes reweight PPR." Generation draws value statements from capability `name`s, not from protocol/skill mechanism.
- **The rule is `uni-capability`'s "Outcome altitude" authoring rule:** "Name the *outcome a user/operator experiences*, never the implementation." Import it as the output-writing contract.
- **The vision models the same split:** static layer (agent/workflow/skill defs) vs dynamic layer (knowledge). Unimatrix's value is *managing the dynamic layer*, so output must speak dynamic-layer outcomes, not static choreography. The README already writes in this register ("agents stopped relitigating decisions"); enablement output extends that register.
- **Anti-pattern to firewall against:** "our swarm protocol has a retro skill that runs `context_cycle_review`" (mechanism) instead of "every delivery makes the system measurably smarter" (outcome). Mechanisms (`context_cycle_review`, PPR, edges, waves, gates, hooks, MCP tool names) never appear in output *except* as optional proof-detail for a technical evaluator.
- **Concretely:** a mechanism→outcome lookup in the agent definition, seeded from the capability corpus; every sentence passes a "mechanism or outcome?" check and mechanism is demoted to an optional appendix.

**Evidence.** `uni-capability` "Authoring rules → Outcome altitude" and "Mechanism vs curve — do not conflate"; capability `name`s (PD2/SL2) already at outcome altitude; PRODUCT-VISION.md static-vs-dynamic mental model; README register.

**Recommendation.** Do not re-derive a translation style — bind output altitude to the capability `name` field and import the "Outcome altitude" rule verbatim. Treat any MCP-tool / protocol / skill mechanism in draft output as a lint failure, allowable only in a clearly-marked technical appendix.

---

### Q D4: "Content-surface boundary: where does this sit relative to README (what-is), CLAUDE.md (agent rules), and onboarding skills — per ADR-004 / knowledge #1257? Confirm the fourth surface."

**Answer. Confirmed: a FOURTH, external, value/education surface — distinct from all three ADR-004 surfaces.**

Per ADR-004 (#1257): README owns *what-is + install/configure/operate* (external reference); CLAUDE.md owns *contributor/agent behavioral rules* (internal); onboarding skills (`uni-init`, `uni-seed`) own *per-repo config + orientation walkthrough* (adopter setup).

The value/education surface answers a question none of them own: **"why is it worth it (awareness) / how do I extract the *most* value from what I already run (enablement)."** It is external like README, but:
- **Register differs:** README is neutral/complete *reference/operation*; the value surface is *persuasion* (awareness) and *value-maximization* (enablement) — selective and argumentative, not reference.
- **Source differs:** README is derived from the product spec; the value surface is derived from the `capability` corpus + real Tier-1 artifacts.

**Boundary guards (to prevent blur):**
- MUST NOT restate README install/operate content — cross-reference only (ADR-004's cross-reference rule).
- MUST NOT contain CLAUDE.md internal rules or protocol choreography (the D3 firewall keeps mechanism out).
- MUST NOT duplicate onboarding walkthroughs — `uni-seed` owns orientation; enablement is value-maximization for an *already-oriented* user, a layer *above* onboarding.
- Needs its **own owner** (the proposed agent), distinct from `uni-docs` (which per ADR-004 owns README only).

**Flag:** ADR-004 currently enumerates three surfaces. Adding a fourth without amending the ADR invites silent collision with README. Recommend a follow-on ADR amendment naming the fourth surface and its owner — a design/vision act, not this spike (see Out-of-Scope).

**Evidence.** ADR-004 (#1257) decision + consequences (README owned by uni-docs; onboarding owned by init/seed); README head confirms it is what-is/install/operate.

**Recommendation.** Confirm and document the value/education surface as a fourth, external surface owned by the new agent; distinguish it from README by *register* (persuasion/enablement vs reference) and *source* (capability corpus + artifacts vs spec). Amend ADR-004 in a downstream design session before build.

---

### Q D5: "Workflow: cadence/trigger (post-release is a natural batching moment) and the human-in-loop step (drafts only; never auto-publish)."

**Answer.**

**Trigger / cadence — post-release batching.** A release (`uni-release`) is precisely when new honest proof material appears: capabilities flip to `proven`, and new retros + correction chains exist. Run the agent **post-release**, batching (a) newly-`proven` capabilities → candidate awareness hooks + enablement-doc deltas, and (b) the release's retro + any correction chains → candidate proof artifacts (C1). Tying generation to real *deltas* prevents over-claiming stale material and keeps the firewall (D2) fed with fresh, status-fresh input. Secondary trigger: on-demand (human requests a post about a specific proven capability).

**Human-in-loop — drafts only, never auto-publish (non-negotiable).** Outward claims are irreversible; a wrong claim on the marketing surface is the self-refutation the spike exists to prevent. The human is the **terminal firewall gate**, with two checks: (1) *claim-accuracy* — does the draft's claim match the cited capability `status`? and (2) *redaction/safety* — the C2 boundary (no leaked content; synthetic labelled). No publishing credentials live in the agent; it emits drafts and citations, the human posts. This mirrors the capability firewall, where status→`proven` is a gated human/guardian act, never automatic.

**Evidence.** `uni-release` as the release moment; `uni-capability` firewall as a gated (never automatic) act; `uni-retro` runs post-merge and is the source of both served-knowledge narratives and correction chains — aligning the value agent's cadence with an existing post-merge/post-release rhythm.

**Recommendation.** Trigger post-release, batching newly-proven capabilities + the release's retro/correction chains. Emit drafts + per-claim citations only; require a two-part human gate (claim-accuracy, redaction) before any publish. Keep publishing credentials out of the agent.

---

### Q GOAL-MAPPING (SCOPE Constraints open question): recommend (a) new strategic goal `adoption`/`go-to-market`, (b) explicit non-goal convention, or (c) anchor to the vision root — to INFORM the vision session, not decide.

**Answer / recommendation: (b) explicit non-goal convention.** Track adoption / value-communication under a distinct label (e.g. `growth:*` or `meta:adoption`) explicitly separate from `goal:*`, documented as orthogonal to the four product-delivery goals. This is a vision-session call; the finding informs it.

**Rationale, internal-grounded:**
- The four goals are all **product-delivery** goals that decompose into `capability` nodes with behavioral `done_when` tests over the governed surface. Adoption/GTM has **no behavioral `done_when` over the codebase** ("we published a post" is not a behavioral proof). Making it a fifth strategic goal **(a)** would corrupt the `uni-capability` firewall — the whole capability map assumes "proven only on behavioral real-artifact evidence," which is meaningless for GTM — and would force every feature to weigh a non-product goal.
- **(c) anchor to the vision root** — the root vision (#4671) is an *engine property* (self-learning knowledge). GTM is a business function, not an engine property; anchoring it to the root dilutes the root's meaning.
- **(b)** fits the project's established posture: monetization/business concerns have been deliberately kept out of the goal set (monetization-undecided; avoid overstating defensive structure; "consistency is NOT a goal"). A distinct non-`goal:*` label tracks the work without corrupting capability/goal semantics or forcing behavioral-proof machinery onto marketing.

**Evidence.** `uni-capability` firewall + `done_when` behavioral requirement (GTM cannot satisfy it); the four goals' capability decompositions (all product-delivery); root vision #4671 scope; project posture on keeping monetization/business out of goals.

---

## Unanswered Questions

None of my assigned internal-track questions are unanswerable at directional confidence. Two carry caveats that belong to the external track / a downstream session:
- **A3 / D1** give the *internal-grounded* one-agent-two-modes lean; the final one-vs-two call must be reconciled with the external track's persona/terminology weighting in synthesis.
- **GOAL-MAPPING** is explicitly a vision-session decision; this finding informs, does not decide.

---

## Out-of-Scope Discoveries

- **The firewall blocks the most natural marketing thesis (material).** SL-ROLLUP #5369 "every deployment gets measurably smarter the more it is used" is `claimed`, not `proven` — blocked on the SL-METRIC keystone (#5373). Under the D2 firewall this headline promise **cannot be marketed** today. Awareness messaging must hook on a `proven` capability (e.g. SL2, PD2-on-its-proven-instance, the correction-with-provenance chain) until SL-METRIC lands and SL-ROLLUP clears its bar. Worth surfacing to the vision session; may motivate prioritizing SL-METRIC if the "gets smarter" claim is strategically central to GTM.
- **ADR-004 amendment needed (design/vision act).** ADR-004 (#1257) enumerates three surfaces; the confirmed fourth value/education surface should be added to the ADR with a named owner before the agent is built, to prevent silent README collision. New spike/design item, one line: "extend ADR-004 to name the fourth (value/education) surface and its owner."
- **Proof-artifact tooling gap.** The best proof artifact (C1: retro-with-correction-chain, and the capability-status view) has no existing *safe-export/redaction* renderer. The C2 redaction pass is currently manual; a small capture/redaction utility (Tier-1-scoped, outcome-altitude reducer) would be a natural first delivery under this agent. Carry-forward, not pursued here.

---

## Recommendations Summary

- **A2**: Enablement personas = Evaluator, Under-utilizing operator, Integrity-conscious evaluator, Research lead — all inside the stated audience; the value they miss is concentrated in the **proactive surface (PD1–PD4)** and the **correction/retro loop** (`context_correct` + `uni-retro`); structure the enablement asset around those two under-used surfaces.
- **A3**: Overlap is in the truth core (capability spine + firewall), divergence only in presentation; internal lean = **two modes of one agent over a shared grounding/firewall core** (synthesis reconciles with external).
- **C1**: Rank proof artifacts: (1) retro served-knowledge narrative, (2) correction chain with provenance [best used *inside* #1], (3) capability-status view, (4) proactive-injection capture [truth-bar-limited], (5) aggregate counts [support only]. Default artifact = **"a retro containing a correction chain"** (the bugfix-872 shape).
- **C2**: Boundary = **show real STRUCTURE from Tier-1 (own-project) artifacts; redact/synthesize CONTENT; never source Tier-2 (other projects); label synthetic as synthetic**; ship a redaction pass with human as terminal gate.
- **D1**: Build **one agent, two modes** over a single shared spine (capability corpus + goals + artifacts + firewall + translation + redaction); modes differ only in audience/format/truth-bar/cadence/derivation.
- **D2**: Encode the `uni-capability` firewall as a pre-generation **status gate** — `proven`→market, `partial`→hedge honestly, `claimed`/`missing`→forbid — with a mandatory per-claim capability-ID citation; keep the agent READ-ONLY in Unimatrix.
- **D3**: Bind output altitude to the capability `name` field and import the "Outcome altitude" rule verbatim; treat any MCP-tool/protocol/skill mechanism in output as a lint failure (appendix-only).
- **D4**: Confirmed — a **fourth, external, value/education surface**, distinguished from README by register (persuasion/enablement vs reference) and source (capability corpus + artifacts vs spec); needs its own owner; amend ADR-004 downstream.
- **D5**: Trigger **post-release**, batching newly-proven capabilities + the release's retro/correction chains; **drafts only, never auto-publish**; two-part human gate (claim-accuracy + redaction); no publishing credentials in the agent.
- **GOAL-MAPPING**: Recommend **(b) explicit non-goal convention** (a distinct `growth:*`/`meta` label, not a fifth `goal:*`) — GTM has no behavioral `done_when` and would corrupt the capability firewall; informs the vision session, does not decide.
