# SCOPE: ass-102 — Extensible-platform assessment + capability breakdown for `goal:platform`

**Goal advanced**: `goal:platform` — Extensible platform surface (Unimatrix #5689)
**Type**: assessment / design-research (read-only; no build)
**Grounding**: platform/plugin reframe (uni-zero, 2026-07-18); ass-100/101 (identity seam); domain-agnostic goal + capabilities; packaging conclusions (dug-21/jurati#12)

---

## The question

Unimatrix has adopted a **platform** framing (PRODUCT-VISION.md): a self-learning knowledge engine with a *presented, versioned, DevX-first extension contract*, on which domain solutions are built rather than baked in. The goal entry `#5689` is thin and emerging. This spike produces the **assessment and candidate capability breakdown** that turns it actionable.

**Primary question**: What is the current state of Unimatrix's extension surface (across the L1/L2/L3 layers below), and what **objectives + behaviorally-proven capability decomposition** would deliver a presented, versioned, DevX-first extension contract?

The three layers to assess:
- **L1 — domain config**: categories, confidence weights, server instructions, observation domain packs.
- **L2 — policy/auth seams**: the relying-party identity verifier (swappable trusted issuer), BearerValidator, observation ingestion sources — the seams where a solution supplies the policy Unimatrix declined (identity, cost, gates).
- **L3 — solution contract**: the memory + capability-map substrate over the MCP wire + client SDK; the two hard co-evolution seams (wire stability + SDK semver).

## Why it matters

- `goal:platform` cannot be sequenced or claimed without a capability breakdown — "what's proven vs what's left" is undefined today.
- The platform framing consolidates currently-**loose threads** (scattered seams: domain packs, BearerValidator, trusted-issuer verifier, observation ingestion, capability substrate, client SDK) into one contract — this spike is where that consolidation gets its evidence.
- The **enterprise extension story** (internal enterprise devs extend Unimatrix along clean lines) depends on this contract being real and presented, not implicit.
- DevX has no home in the current goals; this spike establishes the baseline it must improve against.

## What to explore (bounded)

1. **Seam inventory + state.** Enumerate the existing extension points across L1/L2/L3 (code + config + docs). For each: is it *presented* (documented, discoverable) or *implicit*? *Versioned* or not? What's its stability contract today?
2. **DevX baseline.** What does it actually take, today, to (a) add a domain pack (L1), (b) add a policy/auth seam consumer (L2), (c) stand up a new domain solution (L3)? Where are the sharp edges, undocumented steps, internals-reaching requirements?
3. **The identity seam as the reference L2.** Assess the ass-100/101 relying-party verifier + client-SDK identity plugin: is it *generic* (any issuer) or Jurati-shaped? What would make it a clean, presented L2 seam?
4. **Real L3 evidence.** Cross-reference the domain solutions that already exercise the engine — SDLC (Jurati), research (ASS-057, `uni-research-*`), environmental/data (`ndp-*` roster). Which seams do they actually use? Which are exercised vs assumed?
5. **Candidate capability decomposition.** Propose a candidate objectives list and capability breakdown (functional + nfr), with draft `done_when` phrasing, and a first-pass status read (proven / partial / missing / asserted) for each against today's codebase. Distinguish **claim-floor** (minimum to claim the goal) from **north-star** (curve).

## Expected output (FINDINGS.md)

- **(a) Seam inventory + state table** — every L1/L2/L3 extension point, presented-vs-implicit, versioned-vs-not, current stability contract.
- **(b) DevX gap assessment** — the concrete friction to extend at each layer today; the sharpest edges.
- **(c) Candidate capability decomposition** — proposed objectives + capabilities (functional + nfr) with draft `done_when` and first-pass status. Framed as *input for uni-zero to author* into the capability map (research synthesizes; uni-zero authors — the uni-capability firewall).
- **(d) Prove-core-seam-rest recommendation** — which capabilities are claim-floor (proven on the real domains: SDLC + research) vs which are seams to promote only on evidence. Explicitly hold the "no speculative plugin SDK" line.

## Constraints / prior art

- **No build.** Options + assessment + candidate decomposition that feed a uni-zero authoring pass; does not amend ADRs or ship code.
- **Do not author the capability nodes.** The spike *proposes* the breakdown; uni-zero authors the outcome-phrased capabilities (firewall discipline).
- **Prior art to build on**: goal `#5689`; the domain-agnostic goal + its capabilities (DA1–DA9, NX-*); ass-100/101 FINDINGS (identity/root-of-trust); the packaging thread (dug-21/jurati#12); jurati #1/#2 (what the Unimatrix package ships vs doesn't). Use the `uni-capability` skill for the decomposition format.
- **Out of scope**: designing a general plugin SDK; Jurati-internal shape; any single solution's roster/spine.
