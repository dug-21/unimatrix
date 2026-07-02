# FINDINGS: Value communication for Unimatrix — personas, message, and the agent(s) that generate both

**Spike**: ass-089
**Date**: 2026-07-02
**Approach**: investigation + evaluation (recommendation + feasibility assessment, not implementation) — synthesized from two tracks (external positioning/literature + internal codebase/Unimatrix state)
**Confidence**: directional

> **Synthesis note.** This file merges FINDINGS-EXTERNAL.md (A1, B1, B2 claim/framing) and FINDINGS-INTERNAL.md (A2, A3, C1–C2, D1–D5, goal-mapping). The two straddling questions — A3 (one agent vs two) and B2 (thesis + firewall) — are reconciled explicitly in their sections. No re-investigation was performed.

---

## Findings

### Q A1: "Who are the awareness personas (who feels the pain of agents forgetting / relitigating / repeating mistakes)? Enumerate 3–5 with the pain each holds and the trigger that would make a post resonate."

**Answer.** Five personas, ranked by how sharply they feel Unimatrix's *specific* differentiator (trust / provenance / self-correction). The pain is real and named across the 2025–2026 market ("why AI agents forget," the "doom loop," "institutional memory as a bottleneck"). The human confirmed the same audience is BOTH the awareness and the enablement target.

1. **P1 — Agent/AI application engineer** (builds agent products needing memory). *Highest resonance.*
   - Pain: already bolted on a memory layer (Mem0/Zep/LangMem/RAG) and still gets stale, flat, unattributed recall — can't tell *why* a fact is trusted or *who* decided it.
   - Trigger: a recalled item that carries provenance and *corrects itself* with the correction attributed. "Memory is a trust problem" lands hardest here.
2. **P2 — Staff/tech lead running coding agents on a real codebase.** *High resonance.*
   - Pain: the "doom loop" — the agent relitigates a settled decision, repeats a corrected mistake, has lost the *rationale*.
   - Trigger: a real artifact where the system served a past decision *with rationale and provenance* and an agent *extended* it rather than re-arguing or duplicating.
3. **P3 — Emerging context/platform engineer** (owns "what the agent knows"). *High resonance; fastest-growing role (Gartner, dbt name it in 2026).*
   - Pain: formally owns the context layer but the toolbox is ad-hoc (hand-written `PROJECT_STATE.md`, `CLAUDE.md`, prompt-stuffing) — no governance, no trust signal, no way to catch stale/contradictory knowledge before an agent acts.
   - Trigger: a post that *names the missing primitive* and shows the system catching its own stale/contradictory entry.
4. **P4 — Applied research / R&D lead** running agentic experiments. *Medium-high; the "research" door.*
   - Pain: ruled-out hypotheses evaporate between sessions; the team re-runs disproved experiments — no attributed record of what was tried and why it failed.
   - Trigger: accumulated, attributed lessons visibly stopping a repeat dead-end.
5. **P5 — Engineering leadership** worried about institutional amnesia. *Medium; broadest, least differentiated.*
   - Pain: decisions live in Slack/hallways; agents amplify amnesia by starting at zero.
   - Trigger: the institutional-memory framing (already circulating) + a real artifact showing decisions captured with rationale, not just outcomes.

**Evidence.** 2025–2026 primary/industry sources (Anthropic, Gartner, Forbes; Mem0/Zep/Letta docs; named GitHub repos; arXiv) plus the human's authoritative audience statement.

**Recommendation.** Lead awareness on **P1 + P2**; use **P3** for "name the primitive" posts, **P4** for the research door, **P5** as amplification only. **One post = one persona** — the B2 claim must be pointed at one persona at a time.

---

### Q A2: "Who are the enablement personas (evaluators; under-utilizing existing users) and what value are they currently *not* extracting?"

**Answer.** Four enablement personas, all inside the human-stated audience, each mapped to the specific `proven` capabilities and idle skills/modes they are not exercising:

1. **The Evaluator** (technical lead deciding to integrate). Not extracting: the distinction between "a vector DB behind MCP" and a *self-learning + proactive* engine. They evaluate at install/first-run, before the compounding curve is visible — blind to SL2 (#5221 "useful knowledge surfaces more; misleading recedes"), the proactive surface PD1/PD2 (#5362/#5368), and correction-with-provenance (SL7 #5390).
2. **The Under-utilizing operator** (installed, using it as a passive store — behaves like RAG). Not extracting: **proactive delivery PD1–PD4** (only pulls; briefings/phase-injection idle), **correction-with-provenance** (uses `deprecate + store` or duplicates instead of `context_correct`, so chains never form and SL2's "misleading recedes" never engages), **`uni-retro`** (never run post-merge, so the dynamic layer never grows), **`uni-capability`** (goals never decomposed), and **typed edges** (SL4 #5223 / SL5 #5224 — flat storage, so graph-surfacing never fires).
3. **The integrity-conscious evaluator** (wrong-knowledge-served is expensive — regulated build or research). Not extracting: the integrity NFR set — hash-chain/audit, N-series (N1–N6), poison resistance SLN1 (#5229), graph-consistency-under-correction SLN3 (#5230).
4. **The research lead** (running autonomous research spikes; named in the audience). Not extracting: graph-first retrieval orthogonal to vector search (DA, ASS-057), runtime taxonomy discovery DA1 (#5282), and the provisional-knowledge discipline (research stays *inert* via `Motivates` not `Informs`) that makes an autonomous research loop safe.

**Evidence.** Capability names/statuses read from the corpus; skill behaviors from `uni-retro` and `uni-capability`. The "not extracted" set = intersection of *proven-but-under-used* capabilities and *idle skills/modes*.

**Recommendation.** Structure the enablement asset around the two under-used engine surfaces the personas share: (1) the **proactive surface** (PD1–PD4 — stop pulling, start being handed knowledge) and (2) the **correction/retro loop** (`context_correct` + `uni-retro` — how the deployment compounds). Lead each persona section with the specific `proven` capability they leave on the table, named in the corpus's own outcome words. Lead with integrity NFRs only for persona 3.

---

### Q A3: "Where do awareness and enablement personas overlap vs diverge — does that overlap justify one agent or two?" *(straddling question — both tracks reconciled)*

**Converged answer: ONE agent, two modes over a single shared grounding/firewall core.** Both tracks lean the same way and the reconciliation is clean:

- **External track**: a *shared honest spine* with *different presentation* per audience (one post = one persona; awareness format differs from enablement).
- **Internal track**: *two modes of one agent over a shared grounding/firewall core* — the truth core (capability spine + firewall + redaction) must be single-sourced; only presentation forks.

These are the same decision. Both tracks agree the load-bearing part (grounding + firewall) is shared and the divergence is thin presentation.

**Overlap (large).** Per the human, awareness and enablement target the *same audience* — often the *same person at different funnel stages* (unaware → evaluating → using-but-under-utilizing). They share the entire honest spine: the `capability` corpus, goals + vision, real artifacts, the firewall, and the redaction boundary. Awareness posts are **derived FROM** the enablement asset (SCOPE line 22).

**Divergence (bounded, five axes).** Audience stage; format (short thesis + one image vs durable instructional asset); truth-bar (awareness hooks on one `proven` capability, enablement is comprehensive and must honestly mark `partial`); cadence (opportunistic vs durable/versioned); derivation direction (derived vs primary).

**Why one, not two.** The overlap sits in the load-bearing part — the grounding and firewall — while divergence is only in presentation. Forking the grounding across two independent agents is the specific hazard: marketing claims would drift from what enablement honestly admits is `partial` (PD-ROLLUP #5366 is `partial`; SL-ROLLUP #5369 is `claimed`). That drift is the "structurally present, behaviorally absent" self-refutation in our own storefront.

**Evidence.** Human audience statement (authoritative); SCOPE derivation direction (line 22); capability statuses (PD-ROLLUP `partial`, SL-ROLLUP `claimed`) making a single honest source non-negotiable; external persona work confirming one audience across funnel stages.

**Recommendation.** Build **one agent with an awareness mode and an enablement mode** sharing one grounding + firewall core. The overlap is in the truth core (must not fork); the divergence is presentation (cheap to parameterize). Full I/O contract in D1.

---

### Q B1: "What value framing converts interest for the awareness persona — is 'knowledge curation' the right primitive name, or is there a sharper term? Propose candidates with rationale."

**Answer.** Position by naming toward "knowledge" and connecting through "memory / context engineering." Unimatrix isn't "memory" (recall of state) — it's curated, attributed, *self-correcting knowledge*.

**Industry vocabulary map** (connect the brand term to one of these):

| Term | Denotes | Owners | Fit |
|---|---|---|---|
| agent memory / memory layer | persistent recall, mostly conversational + working state | Mem0, Zep, Letta, LangMem | Highest search volume; connotes *conversational recall*. Partial. |
| context engineering | discipline of curating/governing the context window | Anthropic, Gartner, Fowler, dbt | The umbrella term. |
| knowledge base / RAG | retrieval over a corpus | broad | Fits structured-knowledge half; misses self-correction. |
| institutional / organizational memory | decisions+rationale preserved vs turnover | Forbes, KM vendors | Strong for P5; enterprise search term. |
| knowledge curation | activity of selecting/maintaining knowledge | generic | Internal candidate — verdict below. |

**Verdict on "knowledge curation":** directionally aligned (echoes Anthropic's "curating and maintaining") but weak as a *primitive name*: (1) it's an **activity, not an artifact** — prospects don't search it; (2) it **undersells the two differentiators** (provenance/attribution + self-correction); (3) it **collides with generic content curation**. Keep "curate" as a verb in body copy; retire it as the primitive noun.

**Verdict on "engram" (rejected):** conceptually fine (real memory-trace term) but **fails decisively as a brand/primitive because the name is saturated in Unimatrix's exact niche** — ~8–9 distinct AI-memory products/repos plus an arXiv framework, several with near-identical positioning (`engram.tools` "Shared Memory for AI Coding Agents"; `engram.so`; `engram-memory/engram` "surfaces contradictions before any agent acts"; arXiv 2511.12960). Consequences: zero differentiation, direct competitor name collisions (SEO, trademark), and it anchors Unimatrix to conversational recall — the opposite of its differentiator. **Do not adopt "engram."**

**Candidate primitive names — two-part naming** (a *connective term* prospects search + a *distinctive qualifier* carrying the differentiator), ranked:
1. **"Trustworthy knowledge layer for agents" / "attributed memory"** — leads with the differentiator; searchable connective.
2. **"Self-correcting knowledge base"** — connects to RAG + the capability competitors don't show. *Honest only if self-correction is `proven`* (see B2 firewall — it is; see C1).
3. **"Institutional memory for agents"** — connects to enterprise-amnesia vocabulary (P5); weaker for P1/P2.
4. A Unimatrix-universe themed brand word as the product name, always paired on first use with a plain connective. **Not "engram."**

**Evidence.** Primary/industry sources (Anthropic, Gartner, Forbes, Mem0/Zep/Letta docs, named GitHub repos, arXiv), 2025–2026.

**Recommendation.** Adopt **"attributed / trustworthy knowledge memory"** as the plain-English primitive, always paired with a searchable connective ("agent memory" / "context engineering") on first use. Retire "knowledge curation" as the primitive noun; reject "engram." The primitive's job: say the one true differentiated thing — *memory that carries provenance and corrects itself*. A shortlist still needs a formal trademark/domain availability check (see Unanswered Questions).

---

### Q B2: "What is the smallest honest thesis for an awareness post (one claim + one proof artifact) that generates dialogue without over-explaining?" *(straddling question — external claim reconciled against internal firewall)*

**The claim (recommended):**

> **"Agent memory is a trust problem, not a storage problem. Recall without provenance is just faster misinformation."**

Grounded sub-claim (provable core): *knowledge served to an agent should carry its provenance and be able to correct itself — with the correction attributed — instead of overwriting silently or accumulating unverifiable facts.*

**Why it generates dialogue without over-explaining:** mildly contrarian against "just add a memory layer," so P1/P2/P3 argue or agree (both drive engagement); it names the differentiator *in the claim*, so no architecture explainer is needed; the artifact carries the proof, the words carry the frame. **Post skeleton:** Claim (one sentence, memory→trust) · Proof (one image of a real attributed correction/provenance chain) · no explainer. C1 ranks the exact artifact.

**FIREWALL RECONCILIATION — the proven-vs-claimed line (unmistakable, do not cross):**

The external claim is **SAFE**, but only because of *what it hooks on*. The two tracks resolve cleanly:

- **SAFE — hook here.** The claim hooks on **attributed self-correction / provenance**, which is **`proven`** — demonstrated by the bugfix-872 correction chain (#5280→#5398, extended via `context_correct` with full provenance; #5329→#5394), per internal C1. "Self-correcting *with attribution*" is a delivered, real-artifact-backed behavior. Marketing it is honest.
- **FORBIDDEN — never hook here.** The claim must **NOT** drift onto **self-improvement / self-learning** — "every deployment gets measurably smarter the more it is used" (SL-ROLLUP **#5369**) is **`claimed`, NOT `proven`**, blocked on the SL-METRIC keystone (**#5373**). Under the firewall this headline promise **cannot be marketed today.** It is also the exact aspirational overclaim that would be self-refuting for a trust product.

**The line for every downstream drafter:** use **"self-correcting"** (proven — #5398 chain) — **never "self-improving" / "self-learning" / "gets smarter"** (claimed — #5369, forbidden until SL-METRIC #5373 lands and SL-ROLLUP clears its bar). The distinction is not stylistic; it is the firewall. The paired proof artifact is `proven`; the tempting adjacent headline is `claimed`. Ship the first, never the second.

**Artifact shape the claim demands:** a **real self-correction / provenance chain from the system's own operation** — one knowledge item extended or corrected *with full attribution* (the bugfix-872 chain is the seed). Load-bearing only if the image shows the differentiator competitors can't (attributed self-correction), not a generic "we have memory" screenshot. One claim, one image, no diagram. (C1 ranks the specific artifact; C2 governs safe capture.)

**Evidence.** External positioning analysis (the contrarian claim tested against incumbent messaging); internal C1 confirming the paired capability (attributed self-correction) is `proven` via the bugfix-872 chain; internal D2 status read confirming SL-ROLLUP #5369 is `claimed` / blocked on #5373.

**Recommendation.** Ship the claim **"Agent memory is a trust problem, not a storage problem — recall without provenance is just faster misinformation,"** paired with **one real attributed self-correction/provenance artifact**, no explainer. Encode the proven-vs-claimed line as a hard firewall rule in the agent (D2): say "self-correcting," never "self-improving," until the capability map proves it.

---

### Q C1: "Which real system artifacts best *show* value (retros, served-knowledge counts, correction chains with provenance, briefings, capability-status views)? Rank by persuasive clarity."

**Answer.** Persuasive clarity = differentiation strength × readability. Ranked:

1. **The retro's served-knowledge narrative** ("N entries served across M sessions; #X retrieved in phase Y shaped the diagnosis"). `uni-retro` Phase-4 already produces this, outcome-phrased. Highest composite: accessible to a non-expert, honest, and it *is* the seed. Shows the compounding loop concretely.
2. **The correction chain with provenance** (the bugfix-872 seed: #5280→#5398 and #5329→#5394). Maximally *differentiating* — a prior entry extended/superseded with full provenance rather than duplicated, and a deprecated entry visibly retired. But it is the most technical and hardest to redact (C2), so its top-of-funnel readability is low. Best used as the **proof-detail inside artifact #1**, not standalone.
3. **The capability-status view** (`context_graph` over a goal → proven/partial/missing/claimed). Uniquely persuasive for the *honesty* thesis: a product that publishes its own `partial`/`claimed` status is credible. Renders the firewall visible. Needs framing so `partial`/`claimed` reads as integrity, not weakness.
4. **A briefing / proactive-injection capture** (PD1/PD2). Shows the proactive surface (the vision's differentiator). Truth-bar-limited: PD-ROLLUP is `partial` and PD2's injection path is untested (#5368), so it can only be shown honestly on the specific proven instance.
5. **Aggregate served-knowledge counts / usage-scored ranking (SL2).** Numbers without a story. Lowest standalone clarity; support only.

**Composite:** the best single proof artifact is **"a retro that contains a correction chain"** — #1 as the readable wrapper, #2 as the differentiating proof-detail inside it (the bugfix-872 shape). This is the artifact B2's claim hooks on — and it is `proven`.

**Evidence.** `uni-retro` Phase-4 return format; the bugfix-872 chain (#5280/#5398/#5329/#5394 — supersession + provenance edges present); `uni-capability` status view; PD-ROLLUP/PD2 statuses bounding artifact #4.

**Recommendation.** Rank **retro-with-correction-chain #1** as the default proof artifact; make the **capability-status view #2** recurring (cheap to regenerate; it *is* the firewall shown). Reserve raw counts for support only.

---

### Q C2: "How are they captured and presented **safely** — no leaking proprietary/internal content, secrets, or another project's knowledge? What is the redaction / synthetic-example boundary?"

**Answer. The boundary: show real STRUCTURE from this project's own (Tier-1) artifacts; redact or synthesize CONTENT; never source from another project; label any synthetic example as synthetic.**

The bugfix-872 entries make the risk concrete: they carry internal file paths (`packages/unimatrix/lib/hook-client/mcp-bridge`), env-var names (`UNIMATRIX_HOOK_MCP_STALE_MS`), issue/PR/CI specifics, size-budget internals, and security-mechanism detail. None is a *secret* (Architectural Principle 8 / N2: no secret is ever in a DB), but it is *internal mechanism* that must not go outward.

**Three-tier source rule:**
- **Tier 1 — this project's own knowledge (Unimatrix dogfooding Unimatrix).** Default and safest proof source. Show with a light redaction pass: strip internal paths, issue/PR numbers, env-var/CI specifics, unshipped-feature and security-mechanism detail. Reduce to **outcome altitude** (= D3).
- **Tier 2 — another project's / customer deployment's knowledge.** **Never shown verbatim.** At most show *shape* (counts, status distribution, edge topology) with all content redacted, and only with that customer's explicit consent. Default: do not touch.
- **Tier 3 — synthetic examples.** For anything not safely showable from Tier 1, construct a representative synthetic example (fabricated text over a real structural pattern). It **must be labelled synthetic** — presenting a fabricated artifact as real is the exact self-refutation the spike guards against; the honesty firewall applies to the *artifact*, not only the *claim*.

**Capture mechanism** (feeds the grounding contract): a redaction pass that (a) asserts the source slug is this project, (b) strips paths / issue-numbers / env-var names / secret-shaped tokens / unshipped + security-internal detail, (c) reduces to outcome-altitude text, (d) flags anything ambiguous for human sign-off. **The human-in-loop step (D5) is the terminal redaction gate.**

**Evidence.** bugfix-872 entry contents; Architectural Principle 8 / N2 #5160; the 1-client:1-project isolation posture (N3 #5356) extended to *outward* presentation.

**Recommendation.** Encode the three-tier rule as a hard precondition in the agent definition: Tier-1-only by default; Tier-2 forbidden without consent; Tier-3 must be labelled. Ship the redaction pass as part of the shared grounding module, not per-mode. (Note: this redaction pass has no existing tooling — see Out-of-Scope.)

---

### Q D1: "One agent (two modes) or two coupled agents? Define the shared spine vs the distinct parts."

**Answer. One agent, two modes over a single shared grounding + firewall core** (converged with A3). Decision driver: the truth core must be single-sourced; forking it is the drift hazard.

**Shared spine (single-sourced, must not fork):**
- The `capability` corpus with its `status` field — the truth source.
- Goal entries (#4671, #5219, #4673, #4946, #4678) + PRODUCT-VISION.md — positioning.
- Real Tier-1 artifacts (retros, correction chains, capability-status views).
- The **firewall** (D2): market `proven`; describe `partial` honestly; never `claimed`.
- The **translation discipline** (D3): outcome altitude.
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

**I/O contract (both modes):** IN = `context_search`/`context_get` over `capability`/`goal` (**READ-ONLY**, like this spike) + a redacted Tier-1 artifact set. OUT = a *draft* (never published) plus a per-claim citation to the backing capability ID and status. Enablement mode is the primary generator; awareness mode extracts hooks from enablement output.

**Evidence.** SCOPE lines 22–26; capability statuses making one truth source non-negotiable; both tracks converging on shared-spine/thin-presentation.

**Recommendation.** Build **one agent with an awareness mode and an enablement mode** sharing one grounding module. Do not build two independent agents — that duplicates and risks forking the firewall.

---

### Q D2: "Grounding contract: how does generation stay anchored to `proven` capabilities so it cannot overclaim (the firewall, encoded in the agent definition)?"

**Answer.** Reuse the firewall that already exists in `uni-capability`, verbatim, as the *generation* contract. Its rule — "status advances to `proven` ONLY on attached behavioral, real-artifact evidence" — has a direct generation analogue: **generation may re-phrase capability status, never upgrade it.** Encode in the agent definition:

1. **Query-before-claim.** Every value claim resolves to a `capability` entry (`context_search category=capability`) and READs its `status`. Unimatrix access is **READ-ONLY** — the agent never writes.
2. **Status gate.**
   - `proven` → may be marketed as a delivered outcome. *(Canonical safe hook: attributed self-correction, the #5398 chain — B2.)*
   - `partial` → may be *described*, but MUST be framed as in-progress/honest ("we can X; Y is not yet proven"). Canonical example: PD-ROLLUP #5366.
   - `claimed` → **NEVER marketed.** Canonical example: SL-ROLLUP **#5369** (`claimed`, blocked on SL-METRIC **#5373**) — this blocks the single most natural "gets smarter" marketing thesis (B2 firewall; Out-of-Scope).
   - `missing` → never mentioned as a capability.
3. **Artifact-backed.** Every proof image traces to a real Tier-1 artifact ID through the C2 redaction pass. No fabricated-as-real artifact.
4. **No synthesis beyond the corpus.** The agent may rephrase a capability's `name`/`why` but may not invent capabilities or upgrade status. It *inherits* status; it cannot *assert* status.
5. **Human-in-loop as terminal firewall (D5).** Drafts only; nothing publishes without human review.

**Evidence.** `uni-capability` "The firewall (load-bearing)" section; live statuses PD-ROLLUP `partial` / SL-ROLLUP `claimed` proving the gate has teeth; READ-ONLY access model of this spike.

**Recommendation.** Encode the status gate as a literal rule table (proven→market, partial→hedge, claimed/missing→forbid) with a mandatory per-claim capability-ID citation, and keep the agent READ-ONLY. The firewall is a *pre-generation query*, not a post-hoc review.

---

### Q D3: "Translation discipline: how are agent/workflow/skill definitions (choreography — the static layer) translated to *outcomes* rather than mechanisms in output?"

**Answer.** The `capability` corpus is *already* the translated layer, and the translation rule already exists — reuse both.

- **The capability `name` field IS the outcome vocabulary.** PD2 #5368 "while working, relevant knowledge is pushed without the agent asking" — not "the injection hook fires on PreToolUse." SL2 #5221 "useful knowledge surfaces more; misleading recedes" — not "usage votes reweight PPR." Generation draws value statements from capability `name`s, not from protocol/skill mechanism.
- **The rule is `uni-capability`'s "Outcome altitude" authoring rule:** "Name the *outcome a user/operator experiences*, never the implementation." Import it as the output-writing contract.
- **The vision models the same split:** static layer (agent/workflow/skill defs) vs dynamic layer (knowledge). Unimatrix's value is *managing the dynamic layer*, so output must speak dynamic-layer outcomes.
- **Anti-pattern to firewall against:** "our swarm protocol has a retro skill that runs `context_cycle_review`" (mechanism) instead of "every delivery makes the system measurably smarter" (outcome — *and note this specific outcome is `claimed`, so it is doubly forbidden; use a `proven` outcome instead*). Mechanisms (`context_cycle_review`, PPR, edges, waves, gates, hooks, MCP tool names) never appear in output except as optional proof-detail for a technical evaluator.

**Evidence.** `uni-capability` "Authoring rules → Outcome altitude" and "Mechanism vs curve"; capability `name`s already at outcome altitude; PRODUCT-VISION.md static-vs-dynamic model; README register.

**Recommendation.** Do not re-derive a translation style — bind output altitude to the capability `name` field and import the "Outcome altitude" rule verbatim. Treat any MCP-tool / protocol / skill mechanism in draft output as a lint failure, allowable only in a clearly-marked technical appendix.

---

### Q D4: "Content-surface boundary: where does this sit relative to README (what-is), CLAUDE.md (agent rules), and onboarding skills — per ADR-004 / knowledge #1257? Confirm the fourth surface."

**Answer. Confirmed: a FOURTH, external, value/education surface — distinct from all three ADR-004 surfaces.**

Per ADR-004 (#1257): README owns *what-is + install/configure/operate*; CLAUDE.md owns *contributor/agent behavioral rules*; onboarding skills (`uni-init`, `uni-seed`) own *per-repo config + orientation*. The value/education surface answers a question none of them own: **"why is it worth it (awareness) / how do I extract the *most* value from what I already run (enablement)."** It is external like README, but:
- **Register differs:** README is neutral/complete reference/operation; the value surface is *persuasion* (awareness) and *value-maximization* (enablement) — selective and argumentative.
- **Source differs:** README derives from the product spec; the value surface derives from the `capability` corpus + real Tier-1 artifacts.

**Boundary guards:** MUST NOT restate README install/operate (cross-reference only); MUST NOT contain CLAUDE.md internal rules or protocol choreography (the D3 firewall keeps mechanism out); MUST NOT duplicate onboarding walkthroughs (`uni-seed` owns orientation; enablement is value-maximization for an *already-oriented* user); needs its **own owner** (the proposed agent), distinct from `uni-docs` (owns README only).

**Evidence.** ADR-004 (#1257) decision + consequences; README head confirming what-is/install/operate.

**Recommendation.** Confirm the value/education surface as a fourth, external surface owned by the new agent; distinguish it from README by *register* and *source*. **Amend ADR-004 in a downstream design session before build** (see Out-of-Scope) — adding a fourth surface without amending the ADR invites silent README collision.

---

### Q D5: "Workflow: cadence/trigger (post-release is a natural batching moment) and the human-in-loop step (drafts only; never auto-publish)."

**Answer.**

**Trigger / cadence — post-release batching.** A release (`uni-release`) is exactly when new honest proof material appears: capabilities flip to `proven`, and new retros + correction chains exist. Run the agent **post-release**, batching (a) newly-`proven` capabilities → candidate awareness hooks + enablement-doc deltas, and (b) the release's retro + correction chains → candidate proof artifacts (C1). Tying generation to real *deltas* prevents over-claiming stale material and keeps the firewall fed with status-fresh input. Secondary trigger: on-demand (human requests a post about a specific proven capability).

**Human-in-loop — drafts only, never auto-publish (non-negotiable).** Outward claims are irreversible; a wrong claim on the marketing surface is the self-refutation the spike exists to prevent. The human is the **terminal firewall gate**, with two checks: (1) *claim-accuracy* — does the draft's claim match the cited capability `status`? (specifically: is it a `proven` hook, not a `claimed` one — B2/D2) and (2) *redaction/safety* — the C2 boundary. No publishing credentials live in the agent; it emits drafts + citations, the human posts.

**Evidence.** `uni-release` as the release moment; `uni-capability` firewall as a gated (never automatic) act; `uni-retro` runs post-merge and is the source of both served-knowledge narratives and correction chains.

**Recommendation.** Trigger **post-release**, batching newly-proven capabilities + the release's retro/correction chains. Emit drafts + per-claim citations only; require a **two-part human gate** (claim-accuracy incl. proven-vs-claimed check, redaction) before any publish. Keep publishing credentials out of the agent.

---

### Q GOAL-MAPPING (SCOPE Constraints open question): recommend (a) new strategic goal `adoption`/`go-to-market`, (b) explicit non-goal convention, or (c) anchor to the vision root — to INFORM the vision session, not decide.

**Answer / recommendation: (b) explicit non-goal convention.** Track adoption / value-communication under a distinct label (e.g. `growth:*` or `meta:adoption`) explicitly separate from `goal:*`, documented as orthogonal to the four product-delivery goals.

**Rationale (internal-grounded):**
- The four goals are all **product-delivery** goals that decompose into `capability` nodes with behavioral `done_when` tests over the governed surface. Adoption/GTM has **no behavioral `done_when` over the codebase** ("we published a post" is not behavioral proof). Making it a fifth strategic goal **(a)** would corrupt the `uni-capability` firewall (the map assumes "proven only on behavioral real-artifact evidence") and force every feature to weigh a non-product goal.
- **(c) anchor to the vision root** — the root vision (#4671) is an *engine property*; GTM is a business function, so anchoring dilutes the root's meaning.
- **(b)** fits the project's posture: monetization/business concerns are deliberately kept out of the goal set. A distinct non-`goal:*` label tracks the work without corrupting capability/goal semantics.

**Evidence.** `uni-capability` firewall + `done_when` requirement (GTM cannot satisfy it); the four goals' capability decompositions (all product-delivery); root vision #4671 scope; project posture on keeping monetization out of goals.

---

## Unanswered Questions

- **Which specific brand primitive to adopt** — B1 gives a ranked shortlist and a naming *strategy*, but the final name requires a **formal trademark / domain availability check** (external track cleared only that "engram" collides with direct competitors). *(external track)*
- **Final one-vs-two agent call** is answered as ONE agent (A3/D1 converged across both tracks); no residual disagreement remains. The design session still owns the concrete agent-definition build.
- **GOAL-MAPPING** is explicitly a **vision-session decision**; this finding recommends (b) to inform it, does not decide. *(internal track)*
- **The context/platform-engineer role (P3) is still being formally defined** (Gartner, dbt, 2026); positioning Unimatrix as "the tool this role owns" is promising but rests on a role whose definition is not yet settled. *(both tracks flag)*

---

## Out-of-Scope Discoveries

- **The trust/provenance wedge is unclaimed as a *primary* position.** Incumbents (Mem0/Zep/Letta) lead on scale, recall, temporal graphs; MemOS mentions "provenance and versioning" as a feature, not a banner. Unimatrix's honest differentiator (attributed, self-correcting knowledge) is a lane no incumbent *leads* with. Warrants a dedicated positioning/GTM spike. *(external)*
- **The "context/platform engineer" role is being formally defined now** (Gartner, dbt, 2026). Unimatrix could position as *the tool this new role owns* — an enablement angle. *(both tracks — deduplicated)*
- **The firewall blocks the most natural marketing thesis (material).** SL-ROLLUP #5369 "every deployment gets measurably smarter" is `claimed`, not `proven` — blocked on SL-METRIC keystone #5373. Awareness messaging must hook on a `proven` capability until SL-METRIC lands. May motivate prioritizing SL-METRIC if the "gets smarter" claim is strategically central to GTM. *(internal)*
- **ADR-004 amendment needed (design/vision act; carry-forward).** ADR-004 (#1257) enumerates three surfaces; the confirmed fourth value/education surface should be added with a named owner before the agent is built, to prevent silent README collision. One-line design item: "extend ADR-004 to name the fourth (value/education) surface and its owner." *(internal)*
- **Proof-artifact tooling gap (carry-forward).** The best proof artifact (C1: retro-with-correction-chain, and the capability-status view) has no existing *safe-export/redaction* renderer; the C2 redaction pass is currently **manual**. A small Tier-1-scoped, outcome-altitude redaction/capture utility would be a natural first delivery under this agent. *(internal)*
- **Unverified secondary figure — do not cite.** A "~70% repetition of corrected errors" figure attributed to DeepMind appears only in a vendor blog (getunblocked.com), unverifiable against a primary paper. Do not cite as fact. *(external)*

---

## Recommendations Summary

*(Shaped for hand-off to a design session — this is a recommendation + feasibility assessment, not an implementation.)*

- **A1 (awareness personas)**: Five personas; lead awareness on **P1 (agent/app engineer)** + **P2 (tech lead running coding agents)**; **P3** (context/platform engineer) for "name the primitive," **P4** (R&D lead) for the research door, **P5** (eng leadership) amplification only. **One post = one persona.**
- **A2 (enablement personas)**: Evaluator, Under-utilizing operator, Integrity-conscious evaluator, Research lead — all inside the stated audience. The missed value concentrates in the **proactive surface (PD1–PD4)** and the **correction/retro loop** (`context_correct` + `uni-retro`); structure the enablement asset around those two surfaces.
- **A3 + D1 (one vs two agents — converged)**: **One agent, two modes** (awareness + enablement) over a single shared grounding + firewall core. Overlap is the truth core (capability spine + firewall + redaction — must not fork); divergence is thin presentation (audience/format/truth-bar/cadence/derivation). READ-ONLY Unimatrix access; output is drafts + per-claim capability-ID citations.
- **B1 (terminology)**: **Reject "engram"** (saturated by ~9 competing products; generic + colliding). **Retire "knowledge curation"** as the primitive noun. Adopt **two-part naming**: "attributed / trustworthy knowledge memory" + a searchable connective ("agent memory" / "context engineering") on first use. Formal trademark/domain check still required.
- **B2 (thesis) + FIREWALL**: Ship the claim **"Agent memory is a trust problem, not a storage problem — recall without provenance is just faster misinformation,"** paired with **one real attributed self-correction/provenance artifact**, no explainer. **The proven-vs-claimed line is the firewall, not a style choice:** say **"self-correcting"** (proven — the #5398 correction chain), **never "self-improving"/"self-learning"/"gets smarter"** (SL-ROLLUP #5369 is `claimed`, blocked on SL-METRIC #5373 — forbidden until proven).
- **C1 (proof artifacts)**: Rank (1) retro served-knowledge narrative, (2) correction chain with provenance [best used *inside* #1], (3) capability-status view, (4) proactive-injection capture [truth-bar-limited], (5) aggregate counts [support only]. Default artifact = **"a retro containing a correction chain"** (the bugfix-872 shape) — this is the `proven` hook B2 relies on.
- **C2 (safe capture)**: **Show real STRUCTURE from Tier-1 (own-project) artifacts; redact/synthesize CONTENT; never source Tier-2 (other projects) without consent; label synthetic as synthetic.** Ship the redaction pass in the shared grounding module; human is the terminal gate. (Redaction tooling does not yet exist — see carry-forwards.)
- **D2 (firewall/grounding)**: Encode the `uni-capability` firewall as a **pre-generation status gate** — `proven`→market, `partial`→hedge honestly, `claimed`/`missing`→forbid — with a mandatory per-claim capability-ID citation; keep the agent READ-ONLY.
- **D3 (translation)**: Bind output altitude to the capability `name` field; import the "Outcome altitude" rule verbatim; treat any MCP-tool/protocol/skill mechanism in output as a lint failure (technical-appendix-only).
- **D4 (surface boundary)**: Confirmed — a **fourth, external, value/education surface**, distinguished from README by register (persuasion/enablement vs reference) and source (capability corpus + artifacts vs spec); needs its own owner. **Amend ADR-004 (#1257) downstream** before build.
- **D5 (workflow)**: Trigger **post-release**, batching newly-proven capabilities + the release's retro/correction chains. **Drafts only, never auto-publish**; **two-part human gate** (claim-accuracy incl. proven-vs-claimed check + redaction); no publishing credentials in the agent.
- **GOAL-MAPPING**: Recommend **(b) explicit non-goal convention** (a distinct `growth:*`/`meta` label, not a fifth `goal:*`) — GTM has no behavioral `done_when` and would corrupt the capability firewall. Informs the vision session; does not decide.
