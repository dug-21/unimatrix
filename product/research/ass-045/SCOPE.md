# ASS-045: Monetization Strategy — Commercial Model for a Compliance-Ready AI Platform

**Date**: 2026-04-10 (reframed from original licensing-only scope)
**Tier**: 1 (prerequisite for W2-0; feeds all Wave 2 delivery items)
**Feeds**: W2-0 (product bifurcation and packaging), W2-3 (enterprise positioning), all Wave 2 items
**Related open issue**: #333 (License evaluation: BSL-1.1 feasibility)
**Prior art**: ASS-048 findings (Q5 — BSL procurement risk, Q3 — SOC 2 Type I as Wave 2 target)

---

## Question

What is the right commercial model for Unimatrix — a product built on a genuine OSS foundation, targeting SOC 2 Type I readiness in Wave 2, and aspiring to ISO/IEC 42001 certification as a long-term enterprise differentiator?

The original question (which license for BSL-1.1?) was too narrow. ASS-048 found that BSL creates moderate procurement friction and that SOC 2 is the right compliance anchor for enterprise buyers. The real question is: how does this product make money, how does compliance become a commercial asset rather than just a cost, and what does the licensing instrument need to be to support that model?

---

## Why It Matters

The commercial model decision drives everything downstream: what goes in OSS vs. enterprise, what license the enterprise code carries, how the codebase is structured, what gets built in Wave 2, and how the product is sold. Making this decision at the licensing-instrument layer (BSL vs. FSL vs. Apache + commercial) without first deciding the commercial model produces the wrong answer. A subscription model, a usage-based model, and a self-hosted perpetual license all imply different license instruments and different OSS/enterprise boundary positions.

SOC 2 compliance is not just a checkbox — it is the primary gate for enterprise procurement approval. The spike must evaluate how compliance becomes a commercial motion, not just a technical implementation.

ISO/IEC 42001 (AI management system standard) is confirmed post-Wave 2 by ASS-048. This spike evaluates what it requires and what Wave 2 must not foreclose — not whether to achieve it in Wave 2.

---

## Goal Questions

### 1. What commercial model fits the product and market?

Evaluate options for how Unimatrix generates revenue:

**Open-core (OSS community tier + paid enterprise tier)**:
- OSS tier: MIT/Apache, STDIO, single-project, local daemon. Everything shipped through Wave 1A. Freely distributable.
- Enterprise tier: HTTPS, OAuth, multi-agent, multi-project, admin console, compliance features. Requires a commercial license.
- Revenue model: perpetual license, annual subscription, or per-seat subscription?
- This is the assumed model. Challenge it: is it actually the right fit for a developer tool targeting enterprise AI teams?

**Managed SaaS / hosted service**:
- Unimatrix operated as a cloud service. Enterprise buys usage, not a binary.
- Revenue model: usage-based (per agent, per call, per repo-month) or subscription tier?
- Implications: infrastructure cost, multi-tenant isolation requirements, data residency concerns for enterprise buyers. Does this conflict with the "air-gap deployable" hard constraint in W2-1?

**Self-hosted enterprise license only** (no SaaS):
- Customer deploys Unimatrix in their own environment. Revenue is license fees + support contracts.
- This is the implied Wave 2 model. Is it sufficient at the target market stage, or does it create a long sales cycle that a small team cannot sustain?

**Developer tools-as-a-platform** (SDK + marketplace):
- OSS core. Enterprise SDK / integration tier. Third-party integrations generate licensing revenue.
- Premature for Wave 2? Or worth designing toward?

For each model: who are the comparable companies, what traction signals exist, what does the sales motion look like, and what does it imply for the OSS/enterprise boundary?

### 2. What belongs in each tier, and why?

There are now three tiers, not two. The boundary decisions are commercial strategy decisions — they determine what the developer community gets for free and what drives conversion to the paid product.

**Tier model (working hypothesis — researcher should validate):**

| Feature | Local OSS | Developer cloud (MIT) | Enterprise (commercial) |
|---------|-----------|----------------------|------------------------|
| STDIO transport | ✓ | — | — |
| HTTPS transport | — | ✓ | ✓ |
| Static token auth | — | ✓ | — |
| OAuth 2.1 | — | — | ✓ |
| Single project | ✓ | ✓ | ✓ |
| Multi-project | — | — | ✓ |
| Multi-agent RBAC | — | — | ✓ |
| Three-role model (Admin/Operator/Auditor) | — | — | ✓ |
| Structured audit log | — | — | ✓ |
| Admin console | — | — | ✓ |
| Control plane DB | — | — | ✓ |
| Docker image | — | ✓ (MIT) | ✓ (commercial) |

**The commercial boundary is OAuth + multi-user + compliance.** HTTPS transport itself is MIT — it's the auth layer above it that is commercial. Evaluate whether this boundary is coherent and defensible, or whether any cell in the table above should move.

Specific tensions to evaluate:
- **Audit log**: enterprise-only? Or should a local OSS audit log exist for debugging, and the structured/exportable compliance audit log is enterprise?
- **Admin console**: enterprise-only is the right call if all admin operations are already available via API. Confirm.
- **Multi-project for developer cloud**: a developer working on multiple repos from Codespaces might want multi-project in the MIT tier. Does allowing this undercut enterprise, or does enterprise multi-project mean something different (multi-user access control per project)?

The boundary must tell a coherent story: *the engine is free, the enterprise deployment platform is not.*

### 3. What license instrument supports the chosen commercial model?

This question is answerable only after §1 and §2 are settled. The license instrument enforces the model — it is not the strategy itself.

Evaluate against the chosen commercial model:

**BSL-1.1**: Time-limited commercial restriction, converts to open-source on Change Date. Pros: community eventually gets full source. Cons: ASS-048 Q5 — moderate procurement friction due to HashiCorp precedent, non-OSI recognized.

**Functional Source License (FSL)**: Full source available, commercial use restricted for 2 years, then Apache 2.0. Simpler, more readable than BSL. Used by GitButler, Turso. Less precedent in enterprise infrastructure tools.

**Apache 2.0 OSS + named commercial license**: Core is genuinely open (Apache 2.0). Enterprise features require a separate commercial license agreement. Clean dual-license model. More active license enforcement required (no automatic conversion, no source-available safety valve).

**SSPL**: Use freely, but serving the software to others (as a SaaS) requires open-sourcing your full stack. Relevant if SaaS is a concern. Not relevant for self-hosted enterprise tier.

For each option: what happens at enterprise procurement review? Does the legal team see it as a known quantity, a red flag, or a negotiation surface? What does the grant language need to say to permit developer evaluation, internal CI/CD use, and academic deployment without a commercial license?

**ASS-048 Q5 input**: Grant must include "internal software development, AI agent pipelines, CI/CD use is permitted regardless of commercial relationship" language. Dual-license path (source-available + commercial contract) may be the cleanest enterprise procurement story.

### 4. How does SOC 2 compliance become a commercial asset?

ASS-048 confirmed SOC 2 Type I as the Wave 2 compliance target. Wave 2's three-role RBAC, structured audit log, OAuth 2.1, and HTTPS together constitute a SOC 2 Type I control set.

Evaluate:
- What is the commercial value of SOC 2 Type I attestation for enterprise sales? Does it unblock procurement, or is Type II the actual gate?
- What does the path from Wave 2 (Type I readiness) to SOC 2 Type II audit look like? Type II requires 12 months of operation post-controls — when does that clock start, and what does it cost?
- Should SOC 2 Type II certification be a named deliverable in the post-Wave 2 roadmap, with explicit budget and timeline?
- How do comparable companies use SOC 2 as a sales motion? (Trust landing page, security review automation via Vanta/Drata, automated evidence collection from audit log?)
- What does "SOC 2 ready" mean in marketing terms vs. in legal/procurement terms? Do not overstate the claim before Type II is achieved.

### 5. ISO/IEC 42001 as a Commercial Differentiator

Wave 2 is designed to be a 42001 enabler — meaning it establishes the architectural and audit foundations that make 42001 certification achievable in a future phase. The question for this spike is not *what Wave 2 needs to implement* (that is ASS-042's job), but whether **42001 certification is a commercially valuable differentiator** that should be named in the product positioning and pricing strategy.

Evaluate:
- In the enterprise AI tooling market right now (2026), is ISO/IEC 42001 certification a named buyer requirement, an emerging concern, or largely unknown at the procurement level? Who is asking for it — CISOs, legal, procurement, AI governance teams?
- What is the realistic timeline for a company our stage to pursue 42001 certification? What does it cost and require organizationally?
- Is "designed for 42001 certification from inception" a credible and differentiated positioning claim? Are any comparable developer tools making it?
- Does 42001 certification unlock specific enterprise buyer segments (regulated industries, government, financial services) that SOC 2 alone does not?
- How should the commercial model reflect 42001 as a future milestone? Is it a tier upgrade ("42001-certified enterprise"), a separate SKU, or simply a trust/compliance narrative that supports the existing enterprise tier pricing?

The monetization strategy (tiers, pricing, license instrument) should not change based on which phase delivers 42001. This question asks whether 42001 belongs in the commercial story at all, and how prominently.

### 6. Codebase split — implementation consequence of the commercial model

The three-tier model sharpens the codebase split question. HTTPS transport is MIT (developer cloud tier), so it belongs in MIT crates — not commercial ones. The commercial crate adds only what the enterprise tier requires: OAuth middleware, control plane DB, RBAC enforcement, audit log, admin API, admin console.

Once §1–3 are settled, evaluate three split approaches relative to the chosen model:

**Option A — Feature-flagged mono-repo**: Enterprise features behind `#[cfg(feature = "enterprise")]`. OSS build excludes enterprise features by flag. Mixed license headers per file.

**Option B — OSS core + commercial overlay crate**: `crates/unimatrix-enterprise/` (commercial license) depends on the MIT core crates. MIT core has no knowledge of the enterprise crate. Enterprise crate adds transport, auth, control plane, admin API, admin console.

**Option C — Mono-repo with enterprise subdirectory**: Same repo, `crates/unimatrix-enterprise/` is the sole commercially-licensed crate. Clean dependency DAG. MIT core independently publishable to crates.io.

Evaluate each for: legal clarity, contributor experience (CLA requirements), CI complexity, `cargo publish` compatibility, and how well it supports the chosen commercial model's distribution artifact plan.

---

## Breadth

`industry + literature`

Primary sources: comparable open-core company case studies, enterprise procurement guidance, SOC 2 Trust Services Criteria documentation, ISO/IEC 42001 standard, BSL/FSL license text and legal analysis, enterprise SaaS pricing research.

This spike does not access the Unimatrix codebase. Codebase split implementation details are input to subsequent delivery scoping, not to this spike.

---

## Approach

`evaluation + literature`

- Evaluate commercial models against comparable companies (HashiCorp pre-BSL, CockroachDB, Sentry, GitLab, Sourcegraph, Tailscale, Grafana) — how did they structure OSS/enterprise boundaries and what drove their decisions?
- Read primary SOC 2 Trust Services Criteria and ISO/IEC 42001 documentation
- Produce ranked recommendations for §1–§5 with explicit rationale and evidence

---

## Confidence Required

`directional` — a well-evidenced recommendation for each question. No proof-of-concept required. Recommendations must be backed by primary sources and comparable company evidence, not opinion.

---

## Target Outputs

1. **Commercial model recommendation** — chosen model with explicit rationale and trade-offs; comparable companies; what it implies for the OSS/enterprise boundary
2. **OSS vs. enterprise boundary recommendation** — feature-by-feature decision with the commercial rationale for each gate
3. **License instrument recommendation** — chosen instrument with grant language draft; why it fits the commercial model; procurement risk assessment
4. **SOC 2 commercial motion plan** — how to translate Wave 2 controls into a sales asset; Type I → Type II roadmap; tooling recommendation (Vanta, Drata, manual)
5. **ISO/IEC 42001 design constraints** — what Wave 2 must not foreclose; which control domains require architectural decisions now vs. which are policy-layer additions later; positioning assessment
6. **Codebase split recommendation** — chosen option, conditional on commercial model and license instrument

---

## Constraints

**Hard**:
- MIT/Apache OSS tier must remain genuinely open — nothing about the split restricts OSS users
- The split must not require forking the repository — both OSS and enterprise build from the same source tree
- MIT core crates must be independently publishable to crates.io without enterprise code
- "Air-gap deployable" is a hard constraint on W2-1 (container) — any SaaS model must not conflict with this requirement for enterprise customers

**Hypothesis** (subject to challenge by this research):
- Open-core is the right commercial model
- Self-hosted enterprise license is sufficient (SaaS not needed in Wave 2)
- BSL-1.1 is the right license instrument
- SOC 2 Type I is the right Wave 2 compliance target (ASS-048 confirms this, but the researcher should validate it as a commercial gate, not just a technical checkbox)

---

## Prior Art

- **ASS-048 Q5**: BSL creates moderate procurement friction; Additional Use Grant must permit internal developer tool use; dual-licensing path may be cleaner for enterprise contracts
- **ASS-048 Q3**: SOC 2 Type II is confirmed as the correct primary target; Wave 2 targets Type I readiness; ISO/IEC 42001 is post-Wave 2
- **WAVE2-ROADMAP.md**: Working hypotheses §1 (OSS/Enterprise bifurcation), §7 (Admin Console as primary management surface)
- **GH Issue #333**: License evaluation — prior discussion context
