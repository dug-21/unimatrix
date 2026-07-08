# LinkedIn Comms Strategy (Unimatrix, alpha)

> Working strategy asset for the value/education generator. Grounded in ass-089 (personas,
> message, firewall) + the live capability map. Scope: LinkedIn posts only. Terminology
> (decided): primitive noun = **Vincula** (brand), paired on first use with **"attributed
> memory"** — "Vincula — attributed memory for agents." Honest per firewall (attributed +
> self-correcting; never "gets smarter").

## 1. Goal

Top-of-funnel awareness that generates dialogue and pulls curious engineers to **try the alpha**.
Not sales, not conversion, not lead-gen. Success = conversation + a slow build of the right
followers + real feedback. Matches Doug's own register ("Interested in feedback / Give it a shot").

## 2. Guardrails (non-negotiable)

- **One post = one persona = one claim = one proof artifact.** No feature lists in a post.
- **Firewall — market `proven` only.** Say **"self-correcting"** (proven). **Never "self-improving" /
  "gets smarter" / "self-learning"** as the thesis — SL-ROLLUP is `asserted`, blocked on the
  SL-METRIC keystone. Forbidden until that proves out.
- **Voice** — Doug's VOICE.md. Plain, anti-hype, comma-chained. **Drafts only; Doug posts.**
- **Artifacts** — real, Tier-1 (this project's own), redacted to outcome altitude (no file paths,
  lib/protocol internals, raw IDs, PR#). Synthetic must be labeled synthetic.

## 3. The benefit inventory → post themes

Too many benefits for one post — so each proven benefit is one post. Each theme names the persona,
the claim, the proof artifact, and the **firewall scope** (the honest edge — what the claim must NOT
drift into).

| # | Theme | Persona | Claim (plain) | Proof artifact | Firewall scope |
|---|-------|---------|---------------|----------------|----------------|
| 1 | Memory is a trust problem | P1/P2 | recall you can't trust is faster errors; knowledge should carry where it came from + correct itself, attributed | retro w/ a real correction chain | say "self-correcting", not "gets smarter" |
| 2 | Auditable knowledge lifecycle | P3 / integrity eval | every entry hash-chained; every change attributed in an append-only log; trace how any fact evolved | audit-trail / supersession view | it's tamper-**recorded** (accidental/single-point), not tamper-evident vs a raw-DB adversary |
| 3 | Knowledge before you ask | P2 | on a real event, the relevant entry showed up without the agent searching | one proven injection instance (PD2) | the *instance* is proven; the "every phase, always" rollup (PD-ROLLUP) is partial — don't claim the rollup |
| 4 | Survives context compaction | P1/P2 | the window compacted and the agent kept its working knowledge instead of re-searching | a PreCompact restore capture (PD4) | proven; keep it to the restore behavior |
| 5 | Graph finds what vector search misses | P1 | retrieval walks a typed graph, not just nearest-neighbors — surfaces entries vector alone won't | before/after retrieval; the +0.0122 MRR expansion figure | cite the measured mechanism; not the "smarter over time" curve |
| 6 | Own your knowledge cloud, secure by default | P2/P5 | one container, one command, pinned-TLS, credential out-of-tree — a personal cloud w/out the ops tax | deploy walkthrough / bundle attach | proven floor; multi-LLM parity is north-star (don't imply Codex/Gemini are live) |
| 7 | N projects, one cloud, no bleed | platform/leadership | every project isolated — own db, vectors, hash chain — a write to A can't reach B | isolation shape | proven |
| 8 | Config, not a rebuild | P4 / domain teams | a different domain runs the same engine on config — categories, weights, domain packs | config overlay example | "config not rebuilt" is proven; "a whole different domain live in prod" (DA9) is partial — scope it |
| 9 | See what drove your delivery | P2/P3 | after a feature, see which decisions, patterns, lessons actually shaped it | the cycle-review artifact (redacted) | retro surfacing a *human* finding is honest; don't imply the engine auto-detects issues |
| 10 | A product that publishes its own status | P3 / integrity | we mark our own capabilities proven / partial / missing — honesty as the differentiator | capability-status view | frame partial as integrity, not weakness |

## 4. The arc (sequencing)

Lead with the wedge (the trust position no incumbent leads with), then the daily benefit, then
ownership, then reach.

- **Arc A — The wedge (differentiation):** #1 → #2 → #10. Plant the trust/honesty flag.
- **Arc B — The daily benefit (proactive surface):** #3 → #4 → #5. "Knowledge before you ask."
- **Arc C — Own it (personal cloud):** #6 → #7. Self-host / platform persona.
- **Arc D — Beyond code (domain-agnostic):** #8 → #9 (the rich retro artifact as the closer).

One persona per post; weight P1/P2 early (highest resonance per ass-089 A1). P5 amplification only.

## 5. Cadence

- **Steady drumbeat: ~1 post / week** off the theme backlog — sustainable solo, enough to build
  presence without burnout. ~10 themes ≈ a first quarter.
- **Post-release spikes (ass-089 D5):** when a release flips a capability to `proven` or produces a
  fresh retro/correction chain, that becomes a timely, real-proof post — it jumps the queue because
  the artifact is fresh and honest.
- Two triggers, one backlog: the weekly drumbeat pulls from the arc; releases inject timely posts.

## 6. README alignment (the second half of the ask)

Content-surface boundary (ass-089 D4): **README = what-is + install/use/operate (reference); LinkedIn
= why-it's-worth-it (persuasion).** They cross-reference, never duplicate. A post piques interest →
the README lands the reader and lets them install + verify the claim.

**Aligned / good:**
- Every post theme maps to a README "Core Capabilities" section — a curious reader can verify + install.
- Install/use is thorough: npm, container, build-from-source, wire-into-project, remote/bundle attach,
  first-use examples, tips, full config. The "provides the details to install/use" requirement is met.

**Gaps / risks to fix (docs work — flag for a docs/delivery pass, not this session):**
1. **Firewall drift in the README headline.** The README leads with "self-**improving** delivery,"
   "continuously more relevant," and a "Self-Learning" section — the exact `asserted` claim the post
   firewall forbids. If posts say "self-correcting" while the storefront says "self-improving," the
   README contradicts the honest posts. On a trust product that's the self-refutation to avoid.
   → light honesty pass: keep the proven mechanisms, soften "gets smarter / self-improving" to
   "designed to learn from usage (early)" until SL-METRIC proves it.
2. **"Contradiction Detection" section overclaims.** It reads as a present capability; the map has
   KI-CONTRADICT `missing` (NLI detection was removed). The section actually describes similarity
   `Supports`-edges + a Lambda density metric + `context_correct` resolution — not detection +
   suppression. → rename/reframe to what's true (no "detection" of served contradictions yet).
3. **Top-of-funnel on-ramp.** The README is comprehensive but heavy for LinkedIn traffic. → add a
   crisp "Try the alpha in 60 seconds" quickstart + a clear CTA a post can point at.
4. **Terminology.** "knowledge curation" as the primitive noun appears in the README; align it with
   whatever the naming exercise lands — README and posts must use the same noun.

## 7. Open dependencies

- **Naming exercise** — RESOLVED: **Vincula** + "attributed memory" connective. README syncs to it.
- **The generator** (design session) — automates §3–§5 with the four-module core (capability spine +
  firewall + redaction + voice). This strategy is its content plan.
- **README honesty pass** — items 6.1 / 6.2 are a docs/delivery act; this session only flags them.
