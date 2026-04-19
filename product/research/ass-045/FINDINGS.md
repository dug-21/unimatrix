# FINDINGS: Monetization Strategy — Commercial Model for a Compliance-Ready AI Platform

**Spike**: ASS-045
**Date**: 2026-04-10
**Approach**: evaluation + literature
**Confidence**: directional

---

## Q1: What commercial model fits the product and market?

**Answer**: Open-core with self-hosted enterprise annual subscription is the right model. SaaS in Wave 2 is not viable given the air-gap constraint and team size. The challenged hypothesis — that open-core is right — holds, but with specific structural guidance from comparable failures.

**Evidence**:

Four models were evaluated against the product's current position:

**Open-core (OSS community + self-hosted enterprise subscription)**: This is the validated path for developer infrastructure tools targeting enterprise AI teams. Comparable companies: GitLab (Community Edition free → Premium $29/seat/yr → Ultimate $99/seat/yr), Grafana (AGPL core → commercial Enterprise tier with auth, reporting, advanced data sources), Tailscale (free personal → Starter $6/seat/mo → Premium $18/seat/mo → Enterprise custom). All three follow the same structural pattern: permissive or copyleft OSS core, commercial tier gated on compliance and multi-tenancy features. The Confluent data point is relevant: 35% of customers spending over $100K annually started with the free tier (Monetizely analysis). This validates that the OSS funnel drives enterprise pipeline for developer infrastructure.

**Managed SaaS / hosted service**: Blocked by the air-gap constraint in W2-1. Enterprise AI teams in regulated industries require on-premise or private-cloud deployment. SaaS conflicts directly with the SCOPE.md hard constraint. A small team cannot sustain the infrastructure and multi-tenant isolation engineering a SaaS requires while simultaneously delivering Wave 2. SaaS as a future phase (Wave 3+) is worth designing toward but must not drive Wave 2 commercial architecture.

**Self-hosted enterprise license only (no SaaS)**: The risk is a long sales cycle with no organic adoption funnel. HashiCorp's data shows 89% of enterprise revenue traced back through the open-source funnel. Eliminating the funnel is a bet against the mechanism that built comparable companies. CockroachDB's 2024 pivot abandoned open-core — but they had an established user base before closing the funnel. They retired the free Core product, made Enterprise free for companies under $10M ARR, and required commercial license above that threshold (SD Times, August 2024). This is a post-scale move, not a Wave 2 move.

**Developer tools-as-a-platform / SDK + marketplace**: Premature for Wave 2. Requires a substantial developer ecosystem that does not yet exist.

**Revenue model within open-core**: Annual subscription per seat is the cleaner model than perpetual license for self-hosted enterprise. Per-seat subscription aligns incentives, creates predictable recurring revenue, and is the dominant model for comparable developer tools. Usage-based pricing (per agent call, per context request) achieves 29% higher growth rates than seat-based in OpenView data, but requires telemetry infrastructure not in scope for Wave 2. Per-seat annual subscription is the correct Wave 2 instrument.

**The CockroachDB lesson**: Their 2024 move to a single enterprise model reveals a failure mode of open-core: the OSS tier was deliberately half-featured rather than genuinely useful, creating support burden without conversion. The lesson is not "abandon open-core" — it is "the OSS tier must be genuinely useful, not deliberately crippled."

**Recommendation**: Open-core with annual per-seat subscription for enterprise self-hosted tier. OSS tier (MIT, genuine community value). Enterprise tier gated on multi-user, compliance, and administrative features. SaaS deferred post-Wave 2.

---

## Q2: What belongs in each tier, and why?

**Answer**: The three-tier table in SCOPE.md is largely correct but two cells need adjustment. The commercial boundary is coherent: the engine is free; the enterprise deployment platform is not.

**Evidence**:

GitLab's tier structure provides the clearest comparable: Community Edition (free, all core SCM/CI/CD features), Premium ($29/seat — SAML SSO, audit log streaming, advanced RBAC, group-level admin), Ultimate ($99/seat — compliance management, security scanning, advanced governance). Per GitLab docs, most audit events are available only in Premium and Ultimate; audit event streaming to external destinations requires Ultimate. Sentry gates SAML SSO, audit controls, and HIPAA BAA at the Business ($80/mo) cloud tier. These comparable companies confirm: multi-user governance, compliance audit, and admin are the commercial gates; core functionality is not.

**Cell-by-cell analysis against the SCOPE.md table**:

| Feature | Local OSS | Developer Cloud (MIT) | Enterprise (commercial) | Verdict |
|---------|-----------|----------------------|------------------------|---------|
| STDIO transport | MIT | — | — | Correct. Local, single-agent use. |
| HTTPS transport | — | MIT | MIT (used by enterprise too) | Correct. HTTPS is infrastructure, not the commercial gate — the auth layer above it is. |
| Static token auth | — | MIT | — | Correct. Lightweight credential for individual developer use. |
| OAuth 2.1 | — | — | Commercial | Correct. OAuth 2.1 implies enterprise IdP integration, PKCE, token rotation — enterprise deployment concerns. |
| Single project | MIT | MIT | MIT | Correct. Single-project use must be free across all tiers. |
| Multi-project | — | — | Commercial | **Adjusted**: A developer working across multiple repos from Codespaces wants multi-project in MIT. Allow multi-project in developer cloud for single-user use (MIT). Enterprise multi-project means per-project RBAC — genuinely different and genuinely commercial. |
| Multi-agent RBAC | — | — | Commercial | Correct. RBAC is the enterprise governance primitive. |
| Three-role model (Admin/Operator/Auditor) | — | — | Commercial | Correct. Exists specifically to satisfy SOC 2 CC6.3 duty segregation requirements. No value in single-user deployment. |
| Structured audit log | — | — | Commercial | **Adjusted**: A local OSS debug log (unstructured, file-only) should exist for debugging. The structured, exportable, retention-policy-enforced compliance audit log is commercial. |
| Admin console | — | — | Commercial | Correct, conditional: all admin operations must be available via API in the OSS tier. If not, the console becomes a usability gate rather than a value gate, generating community friction. |
| Control plane DB | — | — | Commercial | Correct. OSS deployment uses embedded config, not a separate control plane. |
| Docker image | — | MIT | Commercial | Correct. Enterprise Docker image differs only in licensed feature inclusion and commercial license assertion. |

**Adjusted boundary story**: "The local OSS engine is free for individual agents. The developer cloud tier is free for individual developers using HTTPS with static token auth, including multi-project use for solo users. Enterprise is the multi-user, RBAC-enforced, SOC 2-ready deployment platform."

**Recommendation**: Accept the SCOPE.md tier table with two adjustments: (1) allow multi-project in developer cloud MIT for single-user use; (2) include a local debug audit log in the MIT tier — the compliance audit log (exportable, with retention policy) is commercial.

---

## Q3: What license instrument supports the chosen commercial model?

**Answer**: Apache 2.0 (or MIT) on core crates + named commercial license agreement on the enterprise crate. Not BSL-1.1. Not FSL. The dual-license model is the cleanest procurement story and the only one that satisfies the hard constraints.

**Evidence**:

**BSL-1.1**: BSL creates exactly the procurement friction ASS-048 identified. The FOSSA analysis confirms: every BSL implementation is effectively a new license because of Additional Use Grant variability. Enterprise Open Source Program Offices (OSPOs) must individually review each BSL implementation — they cannot apply a blanket approval. The HashiCorp post-mortem (Fintan Ryan, Medium, post-IBM acquisition analysis) is instructive: new customer growth dropped to 1.5% QoQ immediately after the BSL announcement. The HashiCorp FAQ on "competitive use" definitions creates a legal opinion burden for any developer tool buyer who is also in the AI tooling space — they must get a legal opinion on whether their use constitutes a competitive offering. BSL works as an anti-cloud-provider-strip-mining instrument (MongoDB's use case) for established products. It does not work as a trust-building instrument for a new product.

**FSL-1.1**: FSL is cleaner than BSL — no Additional Use Grant variability, standardized terms, 2-year conversion to Apache 2.0 or MIT (vs. BSL's 4-year GPL conversion). Armin Ronacher (Flask/Werkzeug, Sentry contributor) argues FSL is superior to AGPL for single-vendor commercial projects because enterprise legal teams are wary of GPL-family licenses (lucumr.pocoo.org, September 2024). Sentry uses FSL-1.1-Apache-2.0 on the web app code. However: FSL's 2-year sunset converting to Apache 2.0 means the enterprise overlay becomes freely usable after 2 years. For a product where the enterprise features are architectural (RBAC, OAuth, audit log) rather than just code, this may be acceptable — but it limits long-term protection of enterprise tier investment. More importantly, FSL applies to the entire product. The SCOPE.md hard constraint requires MIT core crates. FSL cannot apply to MIT crates. FSL could apply only to the enterprise overlay crate, but then it is a hybrid model that is less familiar to procurement than a standard commercial license agreement.

**SSPL**: Irrelevant. SSPL prevents cloud providers from offering the software as a managed service without open-sourcing their entire stack. Unimatrix has no SaaS offering in Wave 2. SSPL solves a problem that does not exist here.

**Apache 2.0 + named commercial license agreement**: The cleanest model for these constraints. MIT (or Apache 2.0 for patent protection) core crates are genuinely open, independently publishable to crates.io, and pre-approved by enterprise OSPOs — zero procurement friction. The enterprise overlay crate carries a named commercial license: a contract, not a source-available license. Enterprise buyers evaluate commercial software contracts routinely; this is a known quantity. The legal team sees a standard commercial software agreement, not a novel license requiring individual review. Source access under the commercial contract satisfies the "enterprise buyers can audit the code they license" requirement.

**Draft evaluation grant language for commercial license** (per ASS-048 Q5 requirement): The commercial license must include an explicit evaluation and internal-use grant:

> "Licensee may use the Software for: (a) internal software development and testing; (b) AI agent orchestration pipelines operated for Licensee's own benefit; (c) CI/CD pipeline automation for Licensee's internal software projects; and (d) evaluation and proof-of-concept deployments. The foregoing uses do not require payment of a commercial license fee, provided Licensee does not offer the Software or its functionality to third parties as a service."

This removes the BSL-style ambiguity that makes procurement hostile.

**CLA vs DCO**: Open Core Ventures handbook recommends Developer Certificate of Origin (DCO) over CLAs. DCOs require no legal paperwork and do not create barriers for casual contributors. CLAs are needed only if the company wants to relicense contributions from the MIT tier into a commercial use. Since the enterprise overlay crate does not accept community contributions, CLAs are not required. DCO on MIT core crates is sufficient.

**Procurement risk assessment**:
- MIT/Apache 2.0 core crates: zero friction. Pre-approved at all enterprise OSPOs.
- Commercial license on enterprise crate: standard enterprise software procurement. No legal novelty.
- BSL on enterprise crate: moderate friction. OSPO individual review required. "Competitive use" ambiguity creates legal opinion burden.
- FSL on enterprise crate: lower friction than BSL, but 2-year sunset creates commercial uncertainty and lower long-term enterprise protection.

**Recommendation**: MIT (or Apache 2.0) on core crates + named commercial license agreement on `unimatrix-enterprise` crate. Draft commercial license with explicit evaluation grant permitting internal use, CI/CD, and AI pipeline use. No BSL. No FSL. Have a lawyer with enterprise SaaS licensing experience draft the final agreement.

---

## Q4: How does SOC 2 compliance become a commercial asset?

**Answer**: SOC 2 Type I is the correct Wave 2 target — the clock on the 6-12 month Type II observation period starts when Wave 2 controls go live. Type I opens mid-market doors; Type II seals enterprise deals. The commercial motion has three distinct phases. Drata is the correct tooling choice for a small team at this stage.

**Evidence**:

**Type I vs Type II as procurement gates**: DSalta analysis (2026 data) is explicit: "most enterprise security questionnaires explicitly request SOC 2 Type 2" and Type II becomes mandatory for "enterprise contracts over $100K annually," regulated industries (healthcare, finance, government), and companies handling sensitive customer data at scale. ESM Global Consulting confirms: "For many clients, Type I is an introduction. Type II seals the deal." Seed-stage VCs accept Type I; Series A+ investors expect Type II on a timeline. Enterprise perception: Type I demonstrates intent; Type II demonstrates sustained operational control. 29% of organizations have lost potential business due to absence of required compliance certification.

**Challenging the ASS-048 hypothesis on Type I sufficiency**: ASS-048 confirmed SOC 2 Type II as the correct primary target, and this research confirms it. Type I readiness is the correct Wave 2 implementation target — the controls must be built before they can be audited for operational effectiveness. However, achieving Type I is not the full enterprise procurement gate. The commercial framing must be honest: Type I opens conversations and passes mid-market security reviews, but enterprise contracts over $100K will require a Type II commitment date. This is a "begins the clock" not a "finishes the race."

**Commercial motion phases**:
1. **Wave 2 build**: "We are building SOC 2-ready controls" — accurate during build. Not a certification claim, but a roadmap signal enterprise buyers value.
2. **Wave 2 complete → Type I audit**: "SOC 2 Type I attested" — legitimate, third-party-audited claim. Opens enterprise conversations, passes mid-market security reviews, unlocks deals under $100K ARR.
3. **12-15 months post-Wave 2 → Type II**: Full enterprise gate opens. Deals over $100K, regulated industries, government procurement.

**Timeline and cost**:
- Type I: $15K–$50K total (auditor fees + internal labor + tooling). Achievable within 3 months of Wave 2 completion.
- Type II: $30K–$100K+ total. Requires 6-12 month observation period + 3-month audit = 9-15 months minimum after controls go live.
- The observation clock starts when controls go live in production, not when the audit begins.

**Tooling recommendation**:
- **Vanta**: $10K–$45K/year, 300+ integrations, easiest onboarding, user-friendly.
- **Drata**: $8K–$35K/year, 3-5 week implementation (fastest), 20-30% cheaper than Vanta, strong automation. **Recommended** for technical early-stage teams.
- **Secureframe**: $8K–$35K/year, superior multi-framework support (ISO 27001, PCI DSS beyond SOC 2). Better choice if ISO 27001 is in scope concurrently.
- **Manual**: $0 tooling cost but 200-500 hours internal engineering time. Not viable for a small team simultaneously shipping Wave 2.
- None of the three currently support ISO 42001 automation — will require manual evidence collection or specialist consultants.

**Marketing language discipline**: "SOC 2 Type I attested" (after audit). "SOC 2 Type II in progress" (during observation). "SOC 2 Type II attested" (after Type II). Never "SOC 2 compliant" (undefined term) or "SOC 2 ready" as a substitute for an actual report.

**Recommendation**: Use Drata for Type I → Type II path. Build a trust landing page (e.g., trust.unimatrix.ai) with controlled access for the SOC 2 report — this is the primary sales motion artifact. Start the Type II observation clock on the day Wave 2 controls go live in production.

---

## Q5: ISO/IEC 42001 as a Commercial Differentiator

**Answer**: ISO/IEC 42001 is an emerging but real commercial differentiator for 2026. It is not yet a universal named procurement requirement but is becoming one in regulated industries and EU-adjacent markets. "Designed for ISO 42001 from inception" is a credible and currently differentiated positioning claim. It belongs in commercial positioning and the product roadmap — not in tier structure.

**Evidence**:

**Current market status**: A June 2025 benchmark of 1,000 compliance professionals found 76% of organizations intend to use ISO 42001 (or equivalent) as their AI governance backbone (Secure Privacy, 2026). SAP certified key AI services in 2025. Cornerstone Galaxy certified December 2025. UiPath certified. Microsoft has ISO 42001 for Azure AI. The certification is operationally real — major enterprise AI vendors are actively acquiring it. KPMG positions it as a competitive differentiator through "leadership in ethical AI" and regulatory readiness, particularly for companies subject to EU AI Act.

**Who is requiring it**: Enterprise RFPs in 2026 increasingly ask for "AI governance proof." Regulated industries — financial services, healthcare, EU-jurisdiction companies — are driven by EU AI Act obligations (Act took effect August 2024, obligations phasing in through 2026). Cloud Security Alliance (January 2025) documents that ISO 42001 and NIST AI RMF simplify EU AI Act conformance. CISOs and AI governance teams are the named stakeholders. The trajectory: universal requirement for regulated industry procurement within 2-3 years. For North American enterprise buyers outside regulated industries, currently a differentiator.

**"Designed for 42001 from inception" as a positioning claim**: Credible and differentiated. No developer tools in the MCP server / AI agent context management space are currently making this claim. Seismic (AI revenue enablement) used ISO 42001 certification as a PR event in 2025. The claim is differentiated because architectural requirements (audit log, RBAC, bias/drift monitoring hooks) are non-trivial to retrofit — a product built with Wave 2 foundations has a genuine technical head start. This is a real differentiator, not marketing noise.

**Buyer segments ISO 42001 unlocks that SOC 2 alone does not**: (1) EU-regulated buyers where AI Act obligations create vendor governance requirements; (2) financial services buyers with AI model risk policies; (3) government/public sector buyers with AI ethics mandates. SOC 2 addresses information security; it says nothing about AI model risk, bias, or drift. Enterprise AI teams increasingly have both a CISO (SOC 2 concern) and an AI governance team (42001 concern). Holding both certifications covers both reviewers in the enterprise procurement process.

**Cost and timeline**: Startup/small company: $20K–$60K direct costs, plus 200–400 hours internal time (~$30K–$60K salary-equivalent). Total $50K–$120K. Timeline: 4–9 months. Annual surveillance: $8K–$15K/year. Requires a functioning SOC 2 or ISO 27001 foundation first — ISO 42001 builds on information security governance.

**What Wave 2 must not foreclose**:
1. **Audit log schema extensibility**: The audit log schema must be extensible to include AI system metadata (which model, which agent, which context version was used in a decision). A fixed audit log schema that cannot accommodate AI-specific fields would require an architectural migration before ISO 42001 certification. This is a schema design decision — add extension columns or a JSONB metadata field in Wave 2, even if empty.
2. **Auditable decision interfaces**: Agent pipeline decisions should flow through auditable interfaces rather than opaque internal calls. ISO 42001 requires demonstrable AI system traceability. Wave 2 must not use architectural patterns that make bias/drift monitoring hooks impossible to add without internal refactoring.
3. **ISO 27001 sequencing option**: ISO 42001 builds on ISO 27001 as a foundation. If the roadmap includes ISO 27001 (which overlaps significantly with SOC 2 Type II controls), sequencing it before ISO 42001 reduces total effort. Worth noting in the post-Wave 2 roadmap.

**Commercial model integration**: ISO 42001 supports the enterprise tier as a trust narrative — not a separate SKU or tier upgrade. The right framing: "Enterprise tier is SOC 2 Type II attested and ISO 42001 certified." It justifies pricing premium without creating a new tier.

**Recommendation**: Name ISO 42001 as a post-Wave 2 milestone on the product roadmap. Include "designed for ISO 42001 from inception" in enterprise tier positioning. Do not create a new tier for it. Budget $50K–$120K and 6–9 months for certification after SOC 2 Type II is complete. Ensure Wave 2 audit log schema includes an extensible metadata field for AI system attributes. Run ISO 42001 pursuit in parallel with SOC 2 Type II observation period where feasible — they address different workstreams.

---

## Q6: Codebase split — implementation consequence of the commercial model

**Answer**: Option C (mono-repo with `crates/unimatrix-enterprise/` as the sole commercially-licensed crate) is the correct choice. Option A (feature flags) fails legal clarity. Option B (external overlay) is architecturally equivalent to Option C but with more operational overhead.

**Evidence**:

**Option A — Feature-flagged mono-repo**: Ruled out. Files containing both MIT code and commercially-licensed code under `#[cfg(feature = "enterprise")]` are legally ambiguous — the license of the compiled output depends on build configuration, and the file header cannot accurately represent both. Goodwin Law's analysis of source-available licensing (September 2024) confirms that enterprise legal teams and OSPOs flag mixed-license file structures as risks requiring individual review. OSPO guidance consistently recommends commercial and open-source code be in clearly separated directories with unambiguous license headers. Feature flags are appropriate for behavioral configuration, not for license boundary enforcement. Additionally, the SCOPE.md hard constraint — "MIT core crates must be independently publishable to crates.io" — is violated if any enterprise-flagged code appears in the same crate file.

**Option B — OSS core + commercial overlay crate in a separate repository**: Architecturally sound but operationally costly. Open Core Ventures handbook explicitly warns against separate repositories: "changes need to be made in multiple places, duplicating efforts" and "separate repositories reduce monetization potential." For a small team, maintaining two repositories means CI/CD configuration, release pipelines, dependency management, and integration testing are all duplicated. If the enterprise crate depends on MIT core crates via published crates.io versions, the release cycle must sequence MIT release before every enterprise release — a forcing function that slows development.

**Option C — Mono-repo, `crates/unimatrix-enterprise/` as sole commercial crate**: Satisfies all SCOPE.md hard constraints:
- MIT core crates independently publishable to crates.io: dependency DAG is strictly `unimatrix-enterprise` → MIT crates, never the reverse. MIT crates have no knowledge of the enterprise crate.
- No repository fork required: OSS and enterprise build from the same source tree.
- Legal clarity: `crates/unimatrix-enterprise/` carries a distinct commercial license; all other crates carry MIT headers. No file-level ambiguity.
- CI complexity: single repo, conditional build targets. Enterprise build job runs only in private CI or against a license key check. Public CI builds only MIT crates.
- Contributor experience: DCO on OSS crates; enterprise crate does not accept external contributions.
- `cargo publish` compatibility: MIT core crates publish normally to crates.io. Enterprise crate is distributed via company artifact registry or Docker image — not published to crates.io. This is the standard pattern for commercial Rust software.

**License headers in Cargo.toml**:
- Core crates: `license = "MIT"` (or `"Apache-2.0"`)
- Enterprise crate: `license = "LicenseRef-Unimatrix-Commercial"` (SPDX custom identifier)
- The root workspace `Cargo.toml` must **not** set a workspace-level default license — per-crate override is required.

**Distribution**: Enterprise tier distributed as a Docker image containing the compiled `unimatrix-enterprise` binary linked against MIT core crates. The Docker image carries the commercial license assertion. OSS users receive MIT source and Docker images built from MIT-only crates.

**Recommendation**: Option C — mono-repo with `crates/unimatrix-enterprise/` as the sole commercially-licensed crate. DCO on MIT crates. Enterprise crate distributed via Docker image, not crates.io. SPDX license identifier per crate in Cargo.toml.

---

## Unanswered Questions

**1. Pricing calibration**: The range $30–$80/seat/year for the enterprise tier is directionally supported by GitLab ($29/$99), Tailscale ($6/$18), and general developer tool benchmarks, but not validated against buyers. A pricing validation interview with 5-10 target enterprise buyers is needed before committing to a published price.

**2. Vanta/Drata ISO 42001 support timeline**: Neither Vanta nor Drata currently supports ISO 42001 automation. Re-evaluate when the ISO 42001 pursuit timeline is confirmed — one or both may have added support by then.

**3. Commercial license agreement text**: This spike identified required grant language and structural terms, but did not produce a full commercial license agreement. A lawyer with enterprise SaaS licensing experience must draft the actual agreement. The evaluation grant language above is the negotiating input, not the final contract.

**4. EU AI Act jurisdictional scope for ISO 42001**: Whether Unimatrix needs ISO 42001 specifically for EU AI Act compliance depends on whether it is classified as a "high-risk AI system" under Annex III of the Act. This is a legal determination based on deployment context. Warrants legal advice before making EU market-specific compliance claims.

---

## Out-of-Scope Discoveries

**1. Usage-based pricing for AI agent tooling**: OpenView data shows usage-based models grow 29% faster than seat-based. As Unimatrix gains telemetry infrastructure in future waves, a usage-based pricing dimension (per context request or per agent session) may be worth introducing. Warrants a future spike when Wave 2 telemetry data is available.

**2. No established commercial player in the MCP context management server market**: Research found no direct comparable product in the MCP server space with an established commercial model. First-mover opportunity, but also a signal that the market may be too early for friction-free enterprise monetization. Worth monitoring.

**3. Replicated as a distribution mechanism**: Replicated.com provides air-gap enterprise software distribution, license key management, update channels, and entitlement checks. Directly relevant to the enterprise Docker image distribution challenge in Wave 2 and could eliminate significant custom engineering for license enforcement and update delivery. Warrants investigation as a Wave 2 distribution implementation option.

**4. Secureframe for multi-framework compliance**: If the product pursues ISO 27001 as an intermediate step between SOC 2 Type II and ISO 42001, Secureframe's multi-framework support makes it worth re-evaluating over Drata at that stage.

---

## Recommendations Summary

| # | Question | Recommendation |
|---|----------|---------------|
| Q1 | Commercial model | Open-core with annual per-seat subscription for enterprise self-hosted tier. OSS tier is MIT. SaaS deferred post-Wave 2. |
| Q2 | OSS/enterprise boundary | Accept SCOPE.md tier table with two adjustments: (1) allow multi-project in developer cloud (MIT) for single-user use; (2) include a local debug audit log in MIT tier — compliance audit log is commercial. |
| Q3 | License instrument | MIT/Apache 2.0 on core crates + named commercial license agreement on `unimatrix-enterprise`. **Not BSL-1.1. Not FSL.** Commercial license must include explicit evaluation grant permitting internal use, CI/CD, and AI pipeline use. |
| Q4 | SOC 2 commercial motion | Use Drata. Type I audit immediately post-Wave 2. Start Type II observation clock on day Wave 2 goes live. Target Type II 12-15 months post-Wave 2. Build trust landing page. |
| Q5 | ISO/IEC 42001 | Name as post-Wave 2 milestone. Use "designed for ISO 42001 from inception" in enterprise positioning. No new tier. Wave 2 must include extensible audit log schema for AI system metadata. |
| Q6 | Codebase split | **Option C** — mono-repo, `crates/unimatrix-enterprise/` as sole commercially-licensed crate. DCO on MIT crates. Enterprise crate distributed via Docker image, not crates.io. SPDX identifiers per crate. |
