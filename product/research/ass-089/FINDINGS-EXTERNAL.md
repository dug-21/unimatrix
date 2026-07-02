# FINDINGS (EXTERNAL TRACK): Value communication for Unimatrix — personas, terminology, claim

**Spike**: ass-089 · **Track**: External / positioning (owns A1, B1, B2 CLAIM/framing side) · **Date**: 2026-07-02
**Approach**: investigation/literature of the external ecosystem (agent-memory & context-engineering vocabulary, persona archetypes). No Unimatrix access.
**Confidence**: directional (positioning research from primary/industry sources; no PoC)

---

## A1 — Awareness personas

The pain is real and named across the 2025–2026 market: "why AI agents forget," the "doom loop," "institutional memory as a bottleneck." Five personas, ranked by how sharply they feel *Unimatrix's specific* differentiator (trust/provenance/self-correction). Human confirmed the same audience is BOTH awareness and enablement target.

**P1 — Agent/AI application engineer (builds agent products needing memory).** *Highest resonance.*
- **Pain**: Already bolted on a memory layer (Mem0/Zep/LangMem/RAG) and still gets stale, flat, unattributed recall — "you cannot know whether a memory is stale because it was never trustworthy or has since changed." Can't tell *why* a fact is trusted or *who* decided it.
- **Trigger**: A recalled item that carries provenance and *corrects itself* with the correction attributed — the thing memory-layer vendors don't show. "Memory is a trust problem" lands hardest here.

**P2 — Staff/tech lead running coding agents on a real codebase.** *High resonance.*
- **Pain**: The "doom loop" — agent relitigates a settled architectural decision, repeats a corrected mistake, has lost the *rationale*. "Each session starts without memory of previous corrections… design decisions carry rationale that may be forgotten." Lives the exact forget/relitigate/repeat triad.
- **Trigger**: A real artifact where the system served a past decision *with rationale and provenance* and an agent *extended* it rather than re-arguing or duplicating.

**P3 — Emerging context/platform engineer (owns "what the agent knows").** *High resonance; fastest-growing role.*
- **Pain**: Now formally owns the context layer (Gartner, dbt name this role in 2026) but the toolbox is ad-hoc — hand-written `PROJECT_STATE.md`, `CLAUDE.md`, prompt-stuffing — no governance, no trust signal, no way to catch stale/contradictory knowledge before an agent acts on it.
- **Trigger**: A post that *names the missing primitive* — governed, attributed knowledge as a first-class thing — and shows the system catching its own stale/contradictory entry.

**P4 — Applied research / R&D lead running agentic experiments.** *Medium-high; the "research" audience.*
- **Pain**: Ruled-out hypotheses and findings evaporate between sessions; the team re-runs disproved experiments because there's no attributed institutional record of "what we tried and why it failed."
- **Trigger**: Accumulated, attributed lessons visibly stopping a repeat dead-end — memory as an anti-amnesia ledger for *reasoning*, not chat.

**P5 — Engineering leadership worried about institutional amnesia.** *Medium; broadest, least differentiated.*
- **Pain**: "Organizations lose knowledge because they document the outcome, not the reasoning" — decisions live in Slack/hallways; agents amplify amnesia by starting at zero. Market frame: "institutional memory is a bottleneck for AI agents."
- **Trigger**: The institutional-memory framing (already circulating, e.g. Forbes) + a real artifact showing decisions captured with rationale, not just outcomes.

**Recommendation**: Lead awareness on **P1 + P2**; use **P3** for "name the primitive" posts, **P4** for the research door, **P5** as amplification only (closest to generic KM). One post = one persona; the B2 claim must be pointed at one at a time.

---

## B1 — Terminology & message primitive

**Industry vocabulary map (connect our brand term to one of these):**

| Term | Denotes | Owners | Fit |
|---|---|---|---|
| **agent memory / memory layer** | persistent recall, mostly conversational + working state | Mem0 ("universal memory layer," ~47k stars, $24M Series A), Zep, Letta, LangMem | Highest search volume; connotes *conversational recall*, not curated knowledge. Partial. |
| **context engineering** | discipline of curating/governing the context window; memory/RAG/KG are components | Anthropic, Gartner, Fowler, dbt | The umbrella. Anthropic: *"strategies for curating and maintaining the optimal set of tokens."* |
| **knowledge base / RAG / "LLM Wiki"** | retrieval over a corpus; compiled structured knowledge | broad | Fits the structured-knowledge half; misses memory/self-correction. |
| **institutional / organizational / corporate memory** | decisions+rationale preserved vs turnover | Forbes, Catalect, KM vendors | Strong for P5; enterprise search term. |
| **knowledge curation** | activity of selecting/maintaining knowledge | generic; Anthropic uses "curating" as a verb | The internal candidate — verdict below. |

**Verdict on "knowledge curation":** Directionally aligned (echoes Anthropic's "curating and maintaining") but weak as a *primitive name*: (1) it's an **activity, not an artifact** — prospects don't search it, can't point at it; (2) it **undersells the two differentiators** (provenance/attribution + self-correction); (3) it **collides with generic content curation**. Keep "curate" as a verb in body copy; not the primitive noun.

**Verdict on "engram":** Real neuroscience term (Semon, 1904 — the physical memory trace). It **connects cleanly to the concept of memory** — conceptually fine. But it **fails decisively as a brand/primitive because the name is saturated in Unimatrix's exact niche.** One search surfaces ~8–9 distinct AI-memory products/repos plus an academic framework named "Engram," several with near-identical positioning:
- `engram.tools` — "Shared Memory for AI Coding Agents"
- `engram.so` — "extracts structured knowledge — typed, ranked, organized… so agents understand what they know" (≈ Unimatrix's pitch)
- `engram-memory/engram` — "universal memory layer… surfaces contradictions before any agent acts on stale information" (≈ the self-correction pitch)
- `openengram.ai`, `engram-ai.dev`, `Gentleman-Programming/engram`, `kael-bit/engram-rs`, `softmaxdata/engram`, `Agentscreator/engram-memory`
- arXiv 2511.12960 **"ENGRAM: Effective, Lightweight Memory Orchestration for Conversational Agents"**

Consequences: (a) **zero differentiation** — signals "generic agent memory," the crowded category Unimatrix wants to stand apart from; (b) **direct name collisions** with live competitors (SEO, trademark, "which Engram?"); (c) **anchors Unimatrix to conversational recall**, the opposite of its differentiator. It obscures the *brand*, not the concept. **Do not adopt "engram."** It is the single worst themed choice — simultaneously generic and taken by direct competitors.

**Candidate primitive names + strategy — two-part naming**: a *connective term* prospects search (discovery) + a *distinctive qualifier* carrying the differentiator. Unimatrix isn't "memory" (recall of state) — it's curated, attributed, *self-correcting knowledge*. Name toward "knowledge," connect through "memory / context engineering." Ranked:
1. **"Trustworthy knowledge layer for agents" / "attributed memory"** — leads with the differentiator; searchable connective; refutes the flat-facts pack.
2. **"Self-correcting knowledge base"** — connects to knowledge base/RAG + the capability competitors don't show. Honest only if self-correction is `proven` (see B2 firewall).
3. **"Institutional memory for agents"** — connects to enterprise-amnesia vocabulary (P5); weaker for P1/P2.
4. A Unimatrix-universe themed brand word used as the product name, always paired on first use with a plain connective ("[Term] — trustworthy memory for agents"). **Not "engram."**

**Recommendation**: Adopt **"attributed / trustworthy knowledge memory"** as the plain-English primitive, always paired with a searchable connective ("agent memory" / "context engineering") on first use. Retire "knowledge curation" as the primitive noun; reject "engram." The primitive's job: say the one true differentiated thing — *memory that carries provenance and corrects itself*.

---

## B2 — Smallest honest thesis (CLAIM/framing side; internal track ranks the artifact)

**The claim** (reframe the category — the market fixes "agents forget" by accumulating flat facts):

> **"Agent memory is a trust problem, not a storage problem. Recall without provenance is just faster misinformation."**

Grounded sub-claim (provable core): *Knowledge served to an agent should carry its provenance and be able to correct itself — with the correction attributed — instead of overwriting silently or accumulating unverifiable facts.*

**Why it generates dialogue without over-explaining**: mildly contrarian against "just add a memory layer," so P1/P2/P3 argue or agree (both drive engagement); it names the differentiator *in the claim*, so no architecture explainer is needed; the artifact carries the proof, the words carry the frame.

**Artifact shape the claim demands** (specific ranking = internal track's call): a **real self-correction / provenance chain from the system's own operation** — one knowledge item extended or corrected *with full attribution* (the retro's correction chain is the seed). Load-bearing only if the image shows the differentiator competitors can't — attributed self-correction — not a generic "we have memory" screenshot. One claim, one image, no diagram.

**Post skeleton**: Claim (one sentence, memory→trust) · Proof (one image of a real attributed correction/provenance chain) · no explainer. The tension between claim and image is the invitation to dialogue.

**FIREWALL FLAG (hand to internal track):** Market only `proven`. "Self-correcting *with attribution*" is demonstrated by the retro and is safe. **Avoid "self-improving" / "self-learning" on the awareness surface unless the capability map marks it `proven`** — autonomous self-improvement is the exact aspirational overclaim the firewall forbids, and for a trust product it is self-refuting. Use "self-correcting" (provable), not "self-improving" (aspirational). Internal track must confirm the paired capability is `proven` before this ships.

---

## Unanswered Questions
- **Which specific artifact ranks highest** — internal track (C1). I specify only the shape (attributed self-correction chain).
- **Is "self-correction with attribution" marked `proven`** in the capability map — internal track must confirm; the firewall flag is contingent on it.
- **Trademark/domain availability** for any brand term not cleared; only established that "engram" collides with direct competitors. A name shortlist needs formal availability check.

## Out-of-Scope Discoveries
- **The trust/provenance wedge is unclaimed as a *primary* position.** Incumbents (Mem0/Zep/Letta) lead on scale, recall, temporal graphs; MemOS mentions "provenance and versioning" as a feature, not a banner. Unimatrix's honest differentiator (attributed, self-correcting knowledge) is a lane no incumbent *leads* with. Warrants a dedicated positioning/GTM spike.
- **The "context/platform engineer" role is being formally defined now** (Gartner, dbt, 2026). Unimatrix could position as *the tool this new role owns* — an enablement angle. Flag for the enablement track.

## Recommendations Summary
- **A1**: Five personas; lead awareness on **P1 (agent/app engineer)** + **P2 (tech lead running coding agents)**; **P3** for "name the primitive," **P4** for research, **P5** amplification only. One post = one persona.
- **B1**: **Reject "engram"** (saturated by ~9 competing products in the identical niche; generic + colliding). Retire "knowledge curation" as the primitive noun. Adopt **two-part naming**: "attributed/trustworthy knowledge memory" + searchable connective ("agent memory"/"context engineering") on first use.
- **B2**: Claim = **"Agent memory is a trust problem, not a storage problem — recall without provenance is just faster misinformation,"** paired with **one real attributed self-correction/provenance artifact**, no explainer. **Firewall**: say "self-correcting," never "self-improving," unless the capability map proves it.

**Note on one secondary source**: a "~70% repetition of corrected errors" figure attributed to DeepMind appears only in a vendor blog (getunblocked.com); I could not verify it against a primary paper — do not cite it as fact. All other findings rest on primary/industry sources (Anthropic, Gartner, Forbes, Mem0/Zep/Letta docs, the named GitHub repos, arXiv). Sources are 2025–2026 and current.
