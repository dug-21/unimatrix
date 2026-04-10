# FINDINGS: Enterprise Security Requirements

**Spike**: ass-048
**Date**: 2026-04-10
**Approach**: evaluation + literature
**Confidence**: directional

---

## Findings

### Q: What authentication model do enterprise security teams prefer for machine-to-machine AI agent calls?

**Answer**: OAuth 2.0 client credentials is the dominant standard for M2M authentication in modern enterprise environments, but the picture is nuanced. The MCP specification (the emerging standard for AI agent-to-server communication) adopted OAuth 2.1 as of June 2025. For highly regulated verticals (financial services, healthcare, government), mTLS is either layered on top of OAuth or mandated as the transport-layer mutual identity mechanism. A two-layer pattern — mTLS for transport-layer identity + OAuth tokens for application-layer authorization — is the enterprise gold standard in regulated sectors. Plain API keys are legacy and fail enterprise security reviews.

**Evidence**:

*OAuth 2.0 client credentials as baseline:*
- Scalekit (primary source, 2024): "Fine-grained permissions for server-to-server communication through scopes"; "stateless token management performs better across distributed systems." Explicitly recommends OAuth client credentials as the standard M2M pattern for most SaaS environments. (https://www.scalekit.com/blog/oauth-client-credentials-vs-mtls)
- MCP Authorization Spec (June 2025, official protocol spec): Mandates OAuth 2.1. MCP servers must act as OAuth 2.1 resource servers only, delegating token issuance to external authorization servers. The client credentials grant appeared in early drafts, was removed, and "is coming back" in draft extensions for agent-to-agent scenarios. (https://stackoverflow.blog/2026/01/21/is-that-allowed-authentication-and-authorization-in-model-context-protocol/)
- MCP spec November 2025 update: Introduces "Identity Assertion Authorization Grant" for service-to-service scenarios and "Enterprise-Managed Authorization" enabling corporate IdP token issuance behind the scenes. (https://aaronparecki.com/2025/11/25/1/mcp-authorization-spec-update)

*mTLS for regulated industries:*
- Scalekit: "A CISO should prioritize mTLS when operating in highly regulated industries like finance, healthcare, or government where mutual identity assurance is a mandatory compliance requirement."
- Open Banking and Financial Grade API (FAPI) specifications combine both: mTLS for transport identity + OAuth for application authorization. This is the reference pattern for regulated-sector enterprises.
- NIST 800-53 Rev 5 IA-9 (Service Identification and Authentication) and IA-3 (Device Identification) require mutual identification of services — mTLS satisfies both. (https://csf.tools/reference/nist-sp-800-53/r5/ia/ia-2/)

*Infrastructure-asserted identity as emerging pattern:*
- Aembit (2025): advocates "infrastructure-asserted identity" — authenticating workloads through their runtime environment (AWS IAM roles, Kubernetes service accounts) combined with short-lived scoped tokens. Not yet dominant in enterprise patterns, but gaining traction for cloud-native workloads. (https://aembit.io/blog/mcp-oauth-2-1-pkce-and-the-future-of-ai-authorization/)

*SAML is not for M2M:*
- SAML is an XML-based federation protocol designed for browser-based human user SSO. It is not appropriate for M2M. Enterprises using SAML for human SSO do not expect SAML in M2M contexts. (https://securityboulevard.com/2025/08/sso-federation-protocols-a-guide-to-saml-oauth-2-0-and-oidc/)

**Assessment of hypothesis "OAuth 2.0 client credentials is the right auth model"**:
Directionally correct for general enterprise environments. The MCP spec's convergence on OAuth 2.1 validates this direction. However, the hypothesis is incomplete: OAuth client credentials alone is not sufficient without PKCE, audience claims, token expiry, and scope controls. mTLS support is expected in regulated verticals. For a self-hosted MCP server, the deployment model matters: enterprises routinely terminate TLS at a proxy (Kong, AWS ALB, Nginx) and pass plain HTTP to upstream services. Unimatrix must support both TLS-direct and proxy-terminated deployment.

**Assessment of hypothesis "HTTPS transport is required vs. deployment model where TLS terminates at a proxy"**:
Confirmed reasonable. Proxy-terminated TLS is standard enterprise practice. Unimatrix should not require end-to-end application-layer TLS if an operator configures a TLS-terminating proxy — but must document this deployment option.

**Recommendation**: Implement OAuth 2.1 with client credentials grant as the primary M2M authentication mechanism. Support short-lived scoped tokens with configurable expiry, audience claims, and scope validation. Document support for deployment behind a TLS-terminating proxy. Do not implement application-layer mTLS in Wave 2 — it is high implementation cost, primarily required by regulated verticals that are not early adopters, and can be added later.

---

### Q: What access control granularity is expected in enterprise AI tooling?

**Answer**: The Admin/Operator two-role model is insufficient for enterprise deployment approval. Enterprise buyers consistently require at minimum three roles: Admin (full control), Operator/User (standard usage), and Read-Only/Auditor (view-only for compliance reviews). SOC 2 CC6.3 explicitly requires separation of duties and role-based least-privilege access. The absence of an auditor role creates a direct compliance gap against SOC 2, ISO 27001, and NIST 800-53.

**Evidence**:

*Three-role minimum from industry practice:*
- EnterpriseReady (canonical enterprise SaaS readiness reference): Explicitly identifies "auditor role with read-only access to review system activity, while administrators retain full privileges" as a required pattern for enterprise SaaS. (https://www.enterpriseready.io/features/role-based-access-control/)
- Sourcegraph Cody Enterprise: SAML/OIDC/OAuth for authentication plus RBAC. Separates Site Admin from standard users. Supports per-repository permission granularity via code host permission sync. (https://sourcegraph.com/docs/admin/config/authorization-and-authentication)
- GitHub Copilot Enterprise: Holds SOC 2 Type II, ISO 27001:2013, CSA STAR Level 2, TISAX. These certifications imply role hierarchy and access control structure. (https://techcommunity.microsoft.com/blog/azuredevcommunityblog/demystifying-github-copilot-security-controls-easing-concerns-for-organizational/4468193)
- Enterprise AI tools (Augment Code survey, 2025): "Implement granular RBAC restricting AI tool access to specific repositories, services, or data classifications, with regular access reviews... separation of duties is critical for SOC 2." (https://www.augmentcode.com/guides/7-soc-2-ready-ai-coding-tools-for-enterprise-security)
- Microsoft 365: Introduced dedicated "AI Administrator" role in 2025, distinct from general IT admin, for Copilot governance. Signals enterprise expectation for AI-specific admin roles. (https://www.refactored.pro/blog/2025/7/13/ai-governance-rbac)

*SOC 2 CC6.3 requirement:*
- SOC 2 CC6.3 requires "access to be based on roles, least privilege, and duty segregation." Without a read-only role, all non-admin users have write capabilities — this cannot satisfy duty segregation requirements for audit and compliance functions. (https://secureframe.com/hub/soc-2/common-criteria)

*ISO 27001:2022 Annex A:*
- Annex A 5.16 requires lifecycle management of all identities including service accounts. Annex A 8.2 requires privileged access (admin accounts) to be distinctly logged and monitored. Elevated privilege access carries explicit segregation requirements. (https://hightable.io/iso-27001-annex-a-8-2-privileged-access-rights/)

*NIST 800-53 AC-2:*
- Requires organizations to define types of accounts (individual, group, service, temporary, emergency, etc.) with distinct authorization levels. AC-2 supplemental guidance for privileged accounts requires "additional scrutiny by organizational personnel," implying admin accounts must be distinct from standard user accounts. (https://csf.tools/reference/nist-sp-800-53/r5/ac/ac-2/)

*ABAC vs. RBAC question:*
- ABAC is emerging for enterprise AI systems ("data classification, masking, encryption, and attribute-based access controls"), but is a Phase 2+ maturity expectation. Not required for Wave 2. (https://www.enkryptai.com/blog/enterprise-ai-security-framework-2025-securing-llms-rag-and-agentic-ai)

**Assessment of hypothesis "Admin XOR Operator (two-role model) is sufficient"**:
**Contradicted.** The two-role model fails at the first enterprise security review. SOC 2 CC6.3 requires duty segregation. The minimum viable RBAC for enterprise approval is three roles.

**Recommendation**: Extend to three roles before enterprise positioning: **Admin** (full control including user management and configuration), **Operator** (standard read/write operations, current behavior), **Auditor** (read-only: can search, read entries, view status, but cannot store/correct/deprecate/enroll/quarantine). The Auditor role is the critical gap. Custom roles are a nice-to-have for enterprise tier but not Wave 2-blocking.

---

### Q: What compliance frameworks are relevant, and which controls do they mandate?

**Answer**: SOC 2 Type II is the correct primary target — it is the most commonly required certification by US enterprise buyers. ISO 27001 is the European equivalent required for multinational adoption. NIST 800-53/FedRAMP is a government-sector requirement and out of scope for Wave 2. ISO/IEC 42001 (AI management system) is an emerging certification gaining procurement traction. The Wave 2-achievable controls are access management (SOC 2 CC6), audit logging (CC7), and encrypted transit.

**Evidence**:

*SOC 2 Type II:*
- "SOC 2 adoption surged 40% in 2024 as companies rushed to meet client demands." "Over 60% of businesses say they're more likely to partner with a startup that has SOC 2." (https://trycomp.ai/soc-2-for-ai-companies)
- Security is the mandatory Trust Services Criterion. Availability, Processing Integrity, Confidentiality, and Privacy are optional additions elected by the service provider. (https://trycomp.ai/soc-2-compliance-requirements)
- Key SOC 2 controls relevant to Unimatrix:
  - **CC6.1**: Logical access controls to prevent security events (satisfied by: token-based auth, HTTPS)
  - **CC6.2**: Identity verification, access revocation (satisfied by: token expiry, credential revocation)
  - **CC6.3**: Role-based access, least privilege, duty segregation (requires: three-role model with Auditor)
  - **CC7.1–7.3**: Monitoring for vulnerabilities and irregular activity (requires: structured audit log)
- SOC 2 Type II requires a 12-month audit window demonstrating controls operate effectively over time. Type I (point-in-time) is feasible in Wave 2; Type II requires 12 months of operation post-controls-implementation.
- (https://linfordco.com/blog/trust-services-critieria-principles-soc-2/, https://secureframe.com/hub/soc-2/common-criteria)

*ISO 27001:2022:*
- Controls relevant to Unimatrix:
  - **Annex A 5.15**: Access Control — policies defining who accesses what
  - **Annex A 5.16**: Identity and Access Management — lifecycle for all identities including service accounts
  - **Annex A 8.2**: Privileged Access Rights — admin accounts require distinct logging and oversight
  - **Annex A 8.15**: Logging — "logs recording activities, exceptions, faults and other relevant events should be produced, stored, protected, and analysed." Privileged access logging is explicitly required. (https://hightable.io/iso-27001-annex-a-8-15-logging/)
- ISO 27001 certification is a 2+ year effort requiring formal ISMS. Not achievable in Wave 2. However, designing controls to be ISO 27001-compatible now avoids rework.

*NIST 800-53 Rev 5:*
- Relevant control families: IA (Identification/Authentication), AC (Access Control), AU (Audit and Accountability).
- Key controls: IA-5 (authenticator management — credential rotation and expiry), AC-2 (account management with role types), AC-3 (access enforcement), AC-6 (least privilege), AU-2 through AU-12 (event logging, log review, retention).
- FedRAMP Moderate ATO requires 325+ controls. Google's Gemini achieved FedRAMP High in March 2025; Claude achieved it in April 2025 via AWS and Google Cloud. This is achievable for AI tools but requires a dedicated multi-year security program. (https://brocyber.com/aifedramp)

*ISO/IEC 42001 (AI Management System):*
- Published 2023. "76% of organizations plan to pursue ISO 42001 soon" (CSA 2025). Strongly aligned with EU AI Act. Increasingly required by enterprise buyers alongside SOC 2 for AI-specific products. (https://isaca.org/resources/news-and-trends/isaca-now-blog/2025/iso-42001-balancing-ai-speed-safety)
- Not achievable in Wave 2, but documenting AI risk management processes now is Wave 2-compatible groundwork.

**Compliance gap analysis:**

| Control Area | Wave 2 Achievable | Post-Wave 2 |
|---|---|---|
| TLS in transit (HTTPS) | Yes | — |
| Token-based auth (OAuth 2.1) | Yes | — |
| Three-role RBAC (Admin/Operator/Auditor) | Yes | Custom roles, ABAC |
| Structured audit log | Yes | SIEM integration, retention policies |
| Credential rotation / token expiry | Yes | Certificate rotation (mTLS) |
| SOC 2 Type I readiness (controls designed) | Yes | Type II (12-month audit period) |
| ISO 27001 certification | No | Yes (2+ years) |
| FedRAMP Moderate | No | Yes (dedicated program) |
| ISO/IEC 42001 certification | No | Yes (2+ years) |

**Assessment of hypothesis "SOC 2 compliance is the right compliance target"**:
**Confirmed.** SOC 2 Type II is the correct primary target for US enterprise buyers. Design for SOC 2 Type I readiness in Wave 2 (controls exist and are documented); plan for Type II after 12 months of operation.

**Recommendation**: Design all Wave 2 security controls to be SOC 2 Type I-ready: three-role RBAC, structured audit log, token-based auth with expiry, HTTPS enforcement. Do not attempt ISO 27001 or FedRAMP in Wave 2. Track ISO/IEC 42001 as a future differentiator as enterprise AI procurement matures.

---

### Q: What AI-specific security risks should inform Unimatrix's security model?

**Answer**: Unimatrix as a multi-agent knowledge and context storage system faces a specific cluster of AI-native risks not covered by general web security: RAG/vector store poisoning, credential harvesting from stored context, indirect prompt injection via retrieved entries, and excessive agency from over-permissioned agent write access. These map to OWASP LLM 2025 LLM08, LLM01, LLM06, LLM02; and to MITRE ATLAS techniques including RAG Poisoning, False RAG Entry Injection, Retrieval Content Crafting, and RAG Credential Harvesting.

**Evidence**:

*OWASP LLM Top 10 2025 — risks directly relevant to Unimatrix:*

The 2025 edition was published at https://genai.owasp.org/resource/owasp-top-10-for-llm-applications-2025/ and is the authoritative reference.

- **LLM08:2025 — Vector and Embedding Weaknesses**: "Systems using RAG pipelines are particularly vulnerable to weaknesses in how vectors and embeddings are generated, stored, or retrieved." Specific threats: unauthorized access exposing sensitive embeddings; embedding inversion attacks recovering original data from vectors. Mitigations: "Enforce strict logical and access partitioning in vector databases, with fine-grained access controls for users" and "Audit and validate all data sources regularly." This is the highest-priority risk for Unimatrix: the vector store and embedding pipeline are direct attack surfaces.

- **LLM01:2025 — Prompt Injection (indirect)**: Indirect injection is the primary concern for Unimatrix. An agent that stores text into Unimatrix, which is later retrieved and injected into another agent's context, is a classic indirect injection vector. "Indirect injections happen when a model processes external sources that lead to failure." Hidden instructions in stored knowledge entries can bypass agent safeguards when retrieved at query time.

- **LLM06:2025 — Excessive Agency**: "Covers excessive functionality, excessive permissions, and excessive autonomy." Agents with write access to Unimatrix (context_store, context_correct, context_deprecate) should not have that capability unless their role explicitly permits it. The Auditor role directly mitigates this.

- **LLM02:2025 — Sensitive Information Disclosure**: Context stored in Unimatrix can contain sensitive architectural decisions, code patterns, API surface details, and agent instructions. Unauthorized read access to the context store is a data exfiltration risk.

Source: https://www.confident-ai.com/blog/owasp-top-10-2025-for-llm-applications-risks-and-mitigation-techniques

*MITRE ATLAS techniques targeting RAG/knowledge systems (v5.4.0, February 2026 — 16 tactics, 84 techniques, 56 sub-techniques):*

Zenity Labs collaboration (October 2025) added 8 new techniques and 4 sub-techniques specifically for AI agents and GenAI systems. Most relevant to Unimatrix:

- **RAG Poisoning**: "Inject malicious content into data indexed by a RAG system to contaminate future retrieval." An adversary who gains write access to Unimatrix (or submits a malicious entry via an agent with write capability) can persistently corrupt context retrieved by other agents. Mitigation: write authorization controls, content provenance tracking, input validation.

- **False RAG Entry Injection**: "Introduce false entries into a victim's retrieval-augmented generation database." Similar to RAG Poisoning but focused on fabricated entries. Direct threat to Unimatrix's knowledge integrity.

- **Retrieval Content Crafting**: Creating content designed to influence future agent behavior through RAG retrieval. An attacker who understands the retrieval mechanism (embedding similarity) can craft entries that will be semantically retrieved in specific contexts to steer agent behavior.

- **RAG Credential Harvesting**: "Using an LLM to search for and collect credentials that were inadvertently ingested into a RAG database." If API keys, tokens, or other secrets are stored in Unimatrix entries (e.g., in code examples or pattern descriptions), retrieval can exfiltrate them.

- **Gather RAG-Indexed Targets**: Reconnaissance step — identifying data sources in retrieval-augmented systems for targeting. Precedes poisoning attacks.

Sources: https://zenity.io/blog/current-events/zenity-labs-and-mitre-atlas-collaborate-to-advances-ai-agent-security-with-the-first-release-of, https://labs.zenity.io/p/techniques-from-zenitys-genai-attacks-matrix-incorporated-into-mitre-atlas-to-track-emerging-ai-thr

*NIST AI RMF Agentic Profile (Cloud Security Alliance, 2025):*
- **Tool-Use Risk Inventory (AG-MP.1)**: Every tool available to an agent requires classification by consequence scope, reversibility, and authentication requirements. Read-only tools are lower risk than write/execute tools. Unimatrix's 12 MCP tools require a risk classification: context_store/correct/deprecate/quarantine/enroll are high-consequence write tools; context_search/lookup/get/briefing are low-consequence read tools. This distinction should be enforced by RBAC scopes.
- **Delegation Chain Monitoring (AG-MS.3)**: Multi-agent deployments require monitoring of authority expansion. Unimatrix should log which agent made which tool call and flag unusual patterns (e.g., a normally read-only agent attempting a write operation).
- Source: https://labs.cloudsecurityalliance.org/agentic/agentic-nist-ai-rmf-profile-v1/

*Enkrypt AI Enterprise AI Security Framework (2025):*
- For RAG systems: "Data classification, masking, encryption, and attribute-based access controls; provenance tracking for all retrieved content; poisoning detection mechanisms to identify tampered knowledge bases."
- For agentic AI: "Scoped API tokens with least-privilege principles; intent checks + human approvals for sensitive actions; detailed audit trails linking prompts, data sources, and tool calls."
- Source: https://www.enkryptai.com/blog/enterprise-ai-security-framework-2025-securing-llms-rag-and-agentic-ai

**Risk register for Unimatrix — prioritized:**

| Risk | Framework | Severity | Wave 2 Mitigation |
|---|---|---|---|
| RAG poisoning via write access | LLM08, ATLAS RAG Poisoning | High | RBAC write authorization, per-tool scoping on tokens |
| Credential harvesting from stored context | ATLAS RAG Credential Harvesting | High | Sensitive data warning on ingest; output filtering |
| Indirect prompt injection via stored entries | LLM01 (indirect) | High | Content sanitization on store; output validation |
| Excessive agency — over-permissioned agents | LLM06 | High | Auditor role; read-only scopes on tokens |
| Sensitive info disclosure via unauthorized read | LLM02 | Medium | RBAC read scoping; token audience claims |
| False entry injection — data integrity | ATLAS False RAG Entry | Medium | Entry provenance (agent attribution); audit log |
| Retrieval content crafting — behavioral manipulation | ATLAS Retrieval Crafting | Medium | Contradiction detection (existing crt-003 is a partial mitigation) |
| Embedding inversion — vector data recovery | LLM08 | Low–Medium | Access control on vector store; HTTPS |
| Unbounded tool consumption — DoS | LLM10 | Low | Rate limiting per agent/token |

**Recommendation**: Four highest-priority AI-specific mitigations for Wave 2: (1) per-tool write authorization in RBAC so Auditor and scoped Operator tokens cannot call mutating tools; (2) structured audit log attributing each tool call to the authenticated agent/token identity; (3) sensitive content ingestion policy — documented prohibition on storing raw credentials, API keys, or secrets in Unimatrix entries, enforced by documentation and optionally by pattern scanning; (4) rate limiting per token to prevent LLM10 unbounded consumption. The existing contradiction detection (crt-003) is a partial mitigation against Retrieval Content Crafting — this is a defensible security claim for enterprise documentation.

---

### Q: What is the enterprise reception risk of BSL-1.1 licensing from a procurement perspective?

**Answer**: BSL-1.1 creates moderate-to-high procurement friction with enterprise legal and procurement teams, primarily because it is not OSI-recognized open source (triggering additional legal review in organizations with open-source-only procurement policies), the "Additional Use Grant" language requires case-by-case legal interpretation, and the Terraform/HashiCorp 2023 precedent created active institutional wariness toward BSL in enterprise procurement teams. Severity depends entirely on the Additional Use Grant wording in Unimatrix's specific BSL-1.1 implementation.

**Evidence**:

*BSL core characteristics relevant to procurement:*
- BSL-1.1 is source-available but explicitly not open source. OSI does not recognize it. The Linux Foundation in October 2023 described BSL as "the defining representation of this threat to open source."
- Core restriction: "The BSL prohibits the licensed code from being used in production — without explicit approval from the licensor." (https://fossa.com/blog/business-source-license-requirements-provisions-history/)
- The permissiveness of any specific BSL deployment depends entirely on the "Additional Use Grant" language added by the licensor.

*The Terraform/OpenTofu precedent — documented enterprise legal reaction:*
- August 10, 2023: HashiCorp moved Terraform from MPL-2.0 to BSL-1.1. Community backlash was documented: 33,000+ GitHub stars for OpenTofu manifesto within a month, 140+ companies and 700 individuals pledging support.
- OpenTofu manifesto explicitly cited enterprise procurement concerns: "The BUSL and the additional use grant written by the HashiCorp team are vague." "Every company, vendor, and developer using Terraform has to wonder whether what they are doing could be construed as competitive with HashiCorp's offerings." (https://opentofu.org/manifesto/)
- By Q4 2024, 38% of Terraform users were evaluating or migrating to OpenTofu (Spacelift Survey). Enterprise adoption directly reversed based on license risk.
- The Linux Foundation accepted OpenTofu in September 2023, signaling formal institutional rejection of BSL for infrastructure tooling.
- Source: https://dev.to/terraformmonkey/terraform-licensing-the-2023-change-still-shaping-your-2025-strategy-4mfb

*BSL vs. Apache 2.0 vs. SSPL for procurement:*

| Dimension | BSL 1.1 | Apache 2.0 | SSPL |
|---|---|---|---|
| OSI-recognized open source | No | Yes | No |
| Production use | Restricted unless granted | Unrestricted | Restricted (hosted service) |
| Passes enterprise open-source-only policy | Fails in strict policies | Yes | Fails |
| Legal review burden | High (case-by-case grant) | Low (well-understood) | High |
| Time-limited | Yes (4-year conversion) | No (perpetual) | No (perpetual) |
| Procurement ambiguity | High (grant language varies per vendor) | None | High |

Source: https://fossa.com/blog/business-source-license-requirements-provisions-history/

*The "Additional Use Grant" is the deciding variable:*
- MariaDB's BSL grant is broad (production use generally permitted; restrictions target competitive hosted database services only). HashiCorp's was narrow and ambiguous (triggered the controversy).
- If Unimatrix's Additional Use Grant explicitly permits: internal enterprise use, CI/CD pipeline integration, use as a developer tool by any company regardless of competitive relationship — procurement friction drops significantly.
- If the Additional Use Grant is narrow or silent on these use cases, enterprise legal teams will require a separate commercial license negotiation before approving procurement, adding 2–4 weeks of legal cycle time and creating deal-loss risk.

*Standard enterprise procurement policy behavior:*
- Organizations with "open source only" procurement policies (common in financial services, some government agencies) will automatically reject BSL unless a commercial license addendum is provided.
- Organizations with "source-available acceptable" policies will proceed if the Additional Use Grant is clearly permissive.
- Apache 2.0 passes all standard enterprise procurement policies without legal review. BSL requires legal review in all cases, regardless of the grant's permissiveness.

*4-year automatic conversion:*
- BSL converts to open source (typically GPL or Apache) after 4 years per BSL-1.1 §2. Provides long-term certainty but does not reduce near-term procurement friction.

**Assessment of BSL-1.1 risk for Unimatrix specifically:**
Unimatrix is a developer tool and MCP server, not a database or cloud hosting service. Its competitive surface (tools "competitive with Unimatrix" per BSL terms) is narrower than HashiCorp's was. This reduces ambiguity somewhat. However, the Terraform precedent has made enterprise procurement teams actively cautious about BSL regardless of specific terms. A security review that surfaces BSL (rather than Apache 2.0 or MIT) adds friction even when the use case is clearly permissible.

Risk rating: **Moderate procurement friction** for self-hosted enterprise deployment. Risk increases if Unimatrix is positioned as a platform other vendors build on (could trigger "competitive" interpretations). Risk is lower for end-user internal deployment where no resale is involved.

**Recommendation (fed to ASS-045)**: The procurement risk of BSL-1.1 is real but manageable if the Additional Use Grant is broadly permissive for internal developer tool use. Recommend the grant include explicit language: "Production use for internal software development, including use within AI agent pipelines, CI/CD systems, and IDE integrations, is permitted regardless of commercial relationship with the licensor." Consider a dual-licensing model (BSL-1.1 for source visibility + commercial license for enterprise contracts) as a risk mitigation path. Apache 2.0 would eliminate all procurement friction but removes the commercial protection BSL provides against competitive SaaS resellers.

---

## Unanswered Questions

1. **OpenAI enterprise RBAC primary source**: The OpenAI Help Center RBAC article returned 403. The three-role model (Owner/Admin/Reader) is industry-known but was not directly verified from primary source. ASS-042 should verify against the actual OpenAI platform documentation.

2. **NIST 800-53 IA-9 full text**: Specific requirements for non-person entity authentication under NIST 800-53 Rev 5 IA-9 were not directly retrieved. This control is directly applicable to Unimatrix's M2M auth model and should be read before implementing token validation logic.

3. **Quantitative enterprise BSL rejection rates**: No quantitative data on how often BSL-licensed tools are rejected at procurement vs. flagged for review vs. approved. The OpenTofu statistics provide directional evidence but not precise procurement failure rates. ASS-045 should attempt to source this.

4. **Unimatrix's actual BSL-1.1 Additional Use Grant text**: This spike does not have access to the actual grant language. The procurement risk assessment depends critically on that text. ASS-045 must review it.

5. **FedRAMP applicability to on-premises-deployed tools**: FedRAMP technically applies to cloud services, not on-premises deployments. If Unimatrix is deployed on-premises by government customers, DISA STIGs and FISMA may apply instead. Out of scope for Wave 2 but warrants a dedicated spike if government market is targeted.

---

## Out-of-Scope Discoveries

1. **MCP Authorization Spec convergence on enterprise OAuth delegation** (November 2025 spec): The MCP protocol now includes an official "Enterprise-Managed Authorization" extension that integrates with corporate IdPs for transparent agent authorization — bypassing the OAuth redirect entirely. This may significantly reduce auth implementation burden for Unimatrix if adopted. Warrants a follow-on spike on MCP protocol evolution and enterprise auth delegation patterns.

2. **ISO/IEC 42001 becoming a procurement differentiator**: 76% of organizations plan to pursue ISO 42001 (CSA 2025). As enterprise buyers begin requiring AI-specific governance certifications, ISO 42001 may become a Wave 3+ procurement requirement for AI developer tools. Not a Wave 2 constraint, but should be in product roadmap.

3. **EU AI Act enforcement begins August 2, 2026**: If Unimatrix has European customers, the EU AI Act becomes legally binding in approximately 4 months. Developer tools that interact with AI systems may fall under specific obligations depending on risk classification. Warrants a separate spike for European market strategy.

4. **OWASP LLM07:2025 — System Prompt Leakage**: New 2025 entry addressing exposure of internal system prompts. Unimatrix stores agent operating instructions and context. If agents store system prompts as knowledge entries, a leakage risk exists if read access is not properly scoped. Not addressed in the current tool surface design.

5. **Unimatrix's contradiction detection (crt-003) is an existing MITRE ATLAS mitigation**: The existing contradiction detection logic is functionally a mitigation against ATLAS "Retrieval Content Crafting" attacks. This is a defensible security claim that could be positioned in enterprise security documentation and in SOC 2 control evidence.

---

## Recommendations Summary

- **Q1 (Auth model)**: Implement OAuth 2.1 with client credentials grant for M2M. Support TLS-terminating proxy deployment. Do not implement application-layer mTLS in Wave 2. Validate token expiry, scope, and audience claims.
- **Q2 (RBAC)**: Extend to three roles before enterprise positioning: Admin, Operator, Auditor (read-only). The two-role model fails SOC 2 CC6.3 duty segregation requirements. Auditor role is the blocking gap.
- **Q3 (Compliance)**: SOC 2 Type II is confirmed as the right primary target. Design Wave 2 controls for SOC 2 Type I readiness: three-role RBAC, structured audit log, token auth with expiry, HTTPS enforcement. Do not pursue ISO 27001, FedRAMP, or ISO 42001 in Wave 2.
- **Q4 (AI-specific risks)**: Top four Wave 2 mitigations: (1) per-tool write authorization in RBAC; (2) audit log with agent attribution per tool call; (3) sensitive content ingestion policy (no raw credentials in context store); (4) rate limiting per token. Existing contradiction detection (crt-003) is an existing ATLAS-mitigation asset.
- **Q5 (BSL procurement)**: BSL-1.1 creates moderate procurement friction. Risk is manageable if the Additional Use Grant is broadly permissive for internal developer tool use. Recommend explicit "internal developer tool use is permitted" language in the grant, and a commercial license path for enterprises that need it (feed to ASS-045).
