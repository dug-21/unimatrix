# ASS-048: Enterprise Security Requirements

**Date**: 2026-04-09
**Tier**: 0 (gates ASS-042; feeds ASS-041, ASS-045)
**Feeds**: ASS-041 (auth model selection), ASS-042 (identity/RBAC model), ASS-045 (licensing reception risk)
**Researcher**: `uni-external-researcher` (breadth: industry + literature; approach: evaluation + literature)

---

## Question

What do enterprise security teams actually require from an AI development tool before they will approve it for internal deployment? Specifically: what authentication models, access control patterns, compliance frameworks, and AI-specific security controls are expected?

This spike is Tier 0 — it produces the requirements that ASS-041 (auth model) and ASS-042 (security architecture) are designed to satisfy. Getting this wrong means building a security model that enterprise buyers reject.

---

## Why It Matters

The working hypothesis is that OAuth 2.0 client credentials + a two-role Admin/Operator model is the right enterprise security design. But this is a product decision made without talking to CISOs. If enterprise security teams prefer mTLS, require SAML federation, expect attribute-based access control, or mandate SOC 2 Type II compliance before deployment, none of that is reflected in the current Wave 2 plan. Building the wrong security model is the most expensive mistake Wave 2 can make — it cannot be refactored without breaking the auth layer.

---

## Goal Questions

1. **What authentication model do enterprise security teams prefer for machine-to-machine AI agent calls?** (OAuth 2.0 client credentials, mTLS, SAML, API keys, or something else?) What is the evidence from industry practice, CISO guidance, and security framework recommendations?

2. **What access control granularity is expected in enterprise AI tooling?** Is Admin/Operator a sufficient two-role model, or do enterprises expect: read-only auditor roles, project-scoped admin, per-capability restrictions, or full ABAC? What precedents exist in comparable enterprise developer tools?

3. **What compliance frameworks are relevant, and which controls do they mandate?** Focus on: SOC 2 Type II, ISO 27001, NIST 800-53, FedRAMP (for government buyers). For each: what specific controls are required, and which are realistically achievable in a Wave 2 enterprise product vs. which require more maturity?

4. **What AI-specific security risks should inform Unimatrix's security model?** Evaluate: OWASP LLM Top 10, MITRE ATLAS (Adversarial Threat Landscape for AI Systems), NIST AI Risk Management Framework. What risks are specific to a multi-agent knowledge system (vs. general web security), and what controls mitigate them?

5. **What is the enterprise reception risk of BSL-1.1 licensing from a procurement perspective?** Enterprise security and legal teams review license compliance before approving vendor tools. Does BSL-1.1 create procurement friction? How do enterprise legal teams treat BSL vs. Apache 2.0 + commercial license vs. SSPL? (Feed to ASS-045.)

---

## Breadth

`industry + literature`

Primary sources: CISO guidance publications, security framework specifications (NIST, ISO, SOC 2 criteria), OWASP and MITRE documentation, enterprise AI security posture reports (Gartner, analyst firms), public case studies from comparable tools that achieved enterprise adoption.

This spike does not search the Unimatrix codebase. It evaluates the external world.

---

## Approach

`evaluation + literature`

- Read primary security framework documents (SOC 2 Trust Services Criteria, ISO/IEC 27001:2022, NIST 800-53 rev 5, OWASP LLM Top 10, MITRE ATLAS)
- Evaluate industry practice from comparable enterprise developer tools (how do tools like GitHub Copilot Enterprise, JetBrains AI, Sourcegraph Cody Enterprise, Tabnine Enterprise handle machine-to-machine auth and RBAC?)
- Synthesize requirements into a ranked list: must-have for enterprise approval, nice-to-have, deferred/post-Wave-2

---

## Confidence Required

`directional` — reach a well-evidenced recommendation for each question. Working proof-of-concept not required. Recommendations must be backed by primary sources, not secondary summaries.

---

## Target Outputs

1. **Auth model recommendation**: which authentication model enterprise security teams most commonly require for machine-to-machine AI agent calls, with evidence
2. **RBAC requirements matrix**: what access control granularity is expected, and what the minimum viable model looks like
3. **Compliance gap analysis**: for each relevant framework, which controls Unimatrix can satisfy in Wave 2 and which require later maturity
4. **AI-specific risk register**: the top risks from OWASP LLM Top 10 / MITRE ATLAS relevant to Unimatrix's threat model, with recommended mitigations
5. **BSL procurement risk assessment**: input to ASS-045 on whether BSL-1.1 creates legal/procurement friction with enterprise buyers

---

## Constraints

**Hard** (technically fixed — changing requires rewriting shipped code):
- Rust codebase
- SQLite per-repo isolation for data plane (schema v22)
- Existing MCP tool API surface

**Hypothesis** (subject to challenge by this research):
- OAuth 2.0 client credentials is the right auth model
- Admin XOR Operator (two-role model) is sufficient
- HTTPS transport is required (vs. a deployment model where TLS terminates at a proxy)
- SOC 2 compliance is the right compliance target

The researcher should explicitly evaluate and challenge each hypothesis above. A finding that contradicts a hypothesis is a success, not a failure.

---

## Prior Art

None — this is a Tier 0 spike with no upstream dependencies. The working hypotheses in `product/WAVE2-ROADMAP.md` (§ Working Hypotheses) provide context on what assumptions are being made. The researcher should treat those as the hypothesis set to validate or contradict.

---

## Dependencies

- **Unblocks**: ASS-041 §2 (auth model recommendation), ASS-042 §0 (role model derivation), ASS-045 §0 (licensing reception risk)
- **No prerequisites** — run immediately, Tier 0
