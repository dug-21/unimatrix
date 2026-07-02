# ass-089 — Value communication for Unimatrix: personas, message, and the agent(s) that generate both

## Problem Statement

The README (and the product surfaces generally) describe **what Unimatrix is**, not **what it is
worth** or **how to get the most value from it**. The realization that triggered this spike: a routine
retrospective (bugfix-872) is itself a dense, honest demonstration of the product's value in action —
knowledge served shaped the diagnosis (#5280/#5303), a correction extended a pattern with full
provenance rather than duplicating it (#5280→#5398), and the system caught its own process gap
(#5329→#5394). The most credible marketing for a knowledge-integrity product is the system's own
honest operation, shown — not claimed.

Two **different but related** functions fall out of this, and the spike must treat them as distinct:

1. **Awareness / marketing content** — lightweight external posts whose job is to *generate dialogue
   and pique interest*, not to educate exhaustively. Shape: a short thesis ("knowledge curation —
   TBD better term — is a missing primitive for agentic workflows") paired with a **demonstrating
   artifact** (e.g. a retro screenshot) as the proof image. Audience: top-of-funnel; prospects who
   don't yet know they have this problem.
2. **Value enablement content** — instructional material on *how to extract the most value*, grounded
   in the real capability set + the agent/workflow/skill definitions. Audience: prospects evaluating,
   and existing users under-utilizing. This is the durable asset; awareness posts can be derived from
   it, but its truth-bar and depth are higher.

These likely need **different-but-related solutions** (shared honest spine, different audience/format/
truth-bar). The spike decides whether that is one agent with two modes or two coupled agents.

## Why It Matters (vision alignment)

Unimatrix's thesis is trustworthy, attributed, self-improving knowledge. That makes value-communication
a **firewall problem, not just a copywriting problem**: an overclaiming marketing surface would be
self-refuting for *this* product (the "structurally present, behaviorally absent" failure, in our own
storefront). Any generation must be grounded in **proven** capabilities and **real** artifacts — it may
market `proven`, describe `partial` honestly, and never market `claimed`. The capability map (the
`capability` corpus) is the natural honest spine: each `proven` capability is already an outcome-phrased,
layman-readable, behaviorally-true statement.

Note: this work advances **adoption / go-to-market**, which is *orthogonal* to the four product-delivery
goals (self-learning, proactive-delivery, personal-cloud, domain-agnostic). It does not map cleanly to
any of them — flagged as an open question below.

## Bounded Questions

**A. Personas.**
- A1. Who are the awareness personas (who feels the pain of agents forgetting / relitigating / repeating
  mistakes)? Enumerate 3–5 with the pain each holds and the trigger that would make a post resonate.
- A2. Who are the enablement personas (evaluators; under-utilizing existing users) and what value are
  they currently *not* extracting?
- A3. Where do awareness and enablement personas overlap vs diverge — does that overlap justify one
  agent or two?

**B. Message & terminology.**
- B1. What value framing converts interest for the awareness persona — is "knowledge curation" the right
  primitive name, or is there a sharper term? Propose candidates with rationale.
- B2. What is the smallest honest thesis for an awareness post (one claim + one proof artifact) that
  generates dialogue without over-explaining?

**C. Proof artifacts (the demonstrating image).**
- C1. Which real system artifacts best *show* value (retros, served-knowledge counts, correction chains
  with provenance, briefings, capability-status views)? Rank by persuasive clarity.
- C2. How are they captured and presented **safely** — no leaking proprietary/internal content, secrets,
  or another project's knowledge? What is the redaction / synthetic-example boundary?

**D. The two-function architecture (feasibility).**
- D1. One agent (two modes) or two coupled agents? Define the shared spine (capability map + goals +
  artifacts) vs the distinct parts (audience, format, truth-bar, cadence).
- D2. Grounding contract: how does generation stay anchored to `proven` capabilities so it cannot
  overclaim (the firewall, encoded in the agent definition)?
- D3. Translation discipline: how are agent/workflow/skill definitions (choreography — the static layer)
  translated to *outcomes* rather than mechanisms in output?
- D4. Content-surface boundary: where does this sit relative to README (what-is), CLAUDE.md (agent rules),
  and onboarding skills (agent orientation) — per ADR-004 / knowledge #1257? It is a fourth, external,
  value/education surface — confirm the boundary.
- D5. Workflow: cadence/trigger (post-release is a natural batching moment) and the human-in-loop step
  (drafts only; never auto-publish — outward-facing/irreversible).

## Expected Output (FINDINGS.md)

A **recommendation + feasibility assessment**, not an implementation:
1. A persona set (awareness + enablement) with pains and triggers.
2. The recommended value thesis and a terminology recommendation for the "knowledge curation" primitive.
3. A ranked list of proof artifacts + a safe-capture/redaction approach.
4. A decision: one agent (two modes) vs two coupled agents, with each one's I/O contract, its grounding/
   firewall mechanism, and its trigger/cadence + human-in-loop workflow — shaped for hand-off to a
   design session.
5. A recommendation on the goal/label question (below).

## Constraints & Prior Art

- **Firewall (non-negotiable):** ground in `proven` capabilities + real artifacts; never market `claimed`.
- **Human-in-loop:** the agent drafts; the human curates and posts. Never auto-publish.
- **Content boundary:** respect ADR-004 (#1257) — do not blur into README/CLAUDE.md/onboarding.
- **Spine already exists:** the `capability` corpus (proven, outcome-phrased) + the `goal` entries +
  PRODUCT-VISION.md are the honest positioning source. The bugfix-872 retro is the seed proof artifact.
- **Open question — goal mapping:** this is an adoption/go-to-market concern with no matching product
  goal. Recommend whether to (a) introduce a new strategic goal (e.g. `adoption` / `go-to-market`),
  (b) adopt an explicit non-goal convention for growth work, or (c) anchor it to the vision root. This
  is a vision-session decision the findings should inform, not make.

## Out of Scope

- Writing actual marketing copy or the enablement doc (that is the downstream function this spike scopes).
- Building the agent(s)/skill (design + delivery sessions).
- Channel/distribution strategy and analytics (a separate go-to-market concern).
