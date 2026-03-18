# Alignment Report: alc-003

> Reviewed: 2026-03-18
> Artifacts reviewed:
>   - product/features/alc-003/architecture/ARCHITECTURE.md
>   - product/features/alc-003/specification/SPECIFICATION.md
>   - product/features/alc-003/RISK-TEST-STRATEGY.md
> Scope sources:
>   - product/features/alc-003/SCOPE.md
>   - product/features/alc-003/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | VARIANCE | One Critical security requirement from the vision is inverted by the scope and all three source docs |
| Milestone Fit | PASS | W0-2 milestone; feature correctly positioned and scoped for that wave |
| Scope Gaps | PASS | All SCOPE.md goals addressed in source documents |
| Scope Additions | WARN | Architecture adds one open question (store-level permissive param cleanup) beyond explicit scope |
| Architecture Consistency | PASS | Architecture is internally consistent and resolves all scope risks |
| Risk Completeness | PASS | Risk register is thorough; all scope risks traceable to mitigations |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | None identified | All SCOPE.md goals (1–8) and acceptance criteria (AC-01–AC-10) are covered across the three source documents |
| Addition | Store-level `permissive` param removal (ARCHITECTURE.md §Open Questions #1) | Architecture raises but does not resolve whether `unimatrix-store/src/registry.rs` `permissive` parameter is removed in this feature. SCOPE.md only mentions `PERMISSIVE_AUTO_ENROLL` const removal and server-side call sites. The store cleanup is presented as a recommendation ("clean the store-level signature too"), not a committed deliverable. Low risk since SR-09 / R-09 already cover the blast-radius concern. |
| Simplification | `PERMISSIVE_AUTO_ENROLL` — deleted entirely rather than converted to runtime env var | Vision W0-2 says "convert from compile-time const to env-var (default false)". SCOPE.md and source docs delete it entirely with no env var replacement. This simplification is rational (the env var path would be a half-done state per SCOPE-RISK-ASSESSMENT.md §Assumptions) and is acceptable given the design rationale. Documented here for awareness. |

---

## Variances Requiring Approval

### VARIANCE-01 — Critical: Server Startup Posture Inverts W0-2 Critical Security Requirement

**What**: The product vision W0-2 [Critical] security requirement states:

> "`UNIMATRIX_SESSION_AGENT` must not fall back to a privileged default if unset — the unset case must produce `[Read, Search]` only, never `[Read, Write, Search]`. Writing without explicit identity configuration must be a deliberate opt-in."

All three source documents — SCOPE.md, ARCHITECTURE.md, and SPECIFICATION.md — implement the opposite behavior: if `UNIMATRIX_SESSION_AGENT` is unset, the server **refuses to start entirely** (non-zero exit, no tool calls served). There is no fallback to `[Read, Search]`. There is no degraded access mode.

SCOPE.md Goals §1 and §4, SPECIFICATION.md FR-02, and all acceptance criteria (AC-04) are unanimous and consistent with each other — but they contradict the vision's Critical requirement, which assumes the server starts and operates in a read-only mode when unconfigured.

The two behaviors are mutually exclusive:
- Vision says: unset → start with `[Read, Search]`
- Source docs say: unset → refuse to start

**Why it matters**: This is a [Critical]-classified item in the product vision's W0-2 security requirements. The vision's stated rationale is "Writing without explicit identity configuration must be a deliberate opt-in" — a correct security principle. However, the SCOPE.md design rationale also has a sound argument: "An unauthenticated deployment has no operational use case for local STDIO." Both positions are defensible; they represent different points on the security trade-off between availability (allow read-only access) and hardening (force configuration before any access). The design choice in SCOPE.md is arguably stricter than the vision requirement, but it directly contradicts the written text of a [Critical] item.

Additionally, SCOPE-RISK-ASSESSMENT.md §Assumptions flags a related inconsistency within SCOPE.md itself: "Proposed Approach item 1 assumes `read_session_agent_env()` returns `None` (not `Err`) when absent, causing fall-through to `permissive=false`. But Goals §1 and AC-04 state the server must refuse to start." The source documents resolve this consistently in favor of startup refusal, but the vision has not been updated to match.

**Recommendation**: Human decision required. Two options:

Option A — Accept the variance (prefer fail-fast): The startup-refusal posture is accepted as a deliberate tightening of the vision requirement. Update the vision's W0-2 [Critical] item to read: "if `UNIMATRIX_SESSION_AGENT` is unset, the server must refuse to start." This is the stricter and arguably cleaner security posture for a local STDIO deployment with no unauthenticated use case.

Option B — Conform to vision (prefer degraded mode): Revert to the vision's design. When `UNIMATRIX_SESSION_AGENT` is unset, the server starts and operates with `[Read, Search]` only. Write attempts return `CapabilityDenied`. The spec and architecture must be revised; AC-04 must be replaced with a degraded-mode acceptance criterion.

The three source documents are internally consistent with each other. If Option A is chosen, only the vision requires updating. If Option B is chosen, SCOPE.md, SPECIFICATION.md, ARCHITECTURE.md, and RISK-TEST-STRATEGY.md all require revision.

---

## Detailed Findings

### Vision Alignment

The feature is correctly positioned as W0-2. The strategic intent — making the capability system non-vestigial, separating authentication from attribution, and building the identity seam for OAuth — is faithfully reflected across all three source documents.

The single material deviation is VARIANCE-01 (startup posture), documented above.

The vision's secondary W0-2 security requirements are all addressed:

- **[High] Env var validation against `[a-zA-Z0-9_-]{1,64}`, reject protected names**: Addressed by FR-04 in the specification, `SessionIdentitySource::resolve()` in the architecture, and R-05 in the risk strategy. Case-insensitive protected name check (`"HUMAN"`, `"System"` etc.) is explicitly covered in AC-07 and R-05 test scenarios.

- **[High] Unknown callers rejected with structured error, not silent fallback**: Addressed. Post-alc-003, unknown callers use session capabilities (no registry lookup, no silent downgrade). When the session lacks a capability, `CapabilityDenied` is returned as a structured MCP error. The architecture's `require_cap()` redesign confirms this (ARCHITECTURE.md §7).

- **[Medium] `UNIMATRIX_SESSION_AGENT` is non-secret attribution, not a credential**: Addressed. The specification's security posture section (SPECIFICATION.md §Constraints / Security Posture) explicitly documents this, stating the value "must not be treated as a secret." The risk strategy's security risks section covers the impersonation surface and its limits.

The vision's reconciliation note on ADR #1839 ("resolve whether W0-2 supersedes, extends, or layers on top of #1839 before implementation") is addressed. SCOPE.md, SPECIFICATION.md, and ARCHITECTURE.md all contain a dedicated ADR #1839 reconciliation section concluding that the two are sequential stages, with #1839 deferred. ARCHITECTURE.md documents this as ADR-004 (to be stored in Unimatrix). The architect is explicitly instructed to update ADR #1839 status in Unimatrix.

The vision's W0-2 scope note ("capability defaults for a configured session agent belong in W0-3 config... not hardcoded here") is a visible tension: SCOPE.md explicitly hardcodes `[Read, Write, Search]` as the session agent capability set, acknowledging this as a W0-3 deferral. The `SESSION_AGENT_DEFAULT_CAPS` constant and W0-3 forward-compatibility notes throughout all three source documents demonstrate awareness of this boundary. This is an accepted, documented simplification.

### Milestone Fit

The feature is explicitly labeled W0-2 in all source documents and maps directly to the W0-2 roadmap entry. The feature does not build W0-3, W1, or W2 capabilities. The `SessionIdentitySource` abstraction is designed as a seam for W2-2/W2-3, not as a premature implementation of OAuth. The `JwtClaims` variant is present but `#[allow(dead_code)]` and explicitly untested in alc-003 scope — this is appropriate milestone discipline.

The prerequisite features (vnc-005 daemon mode, nxs-011 sqlx migration) are correctly identified as complete.

### Architecture Review

The architecture is well-structured and internally consistent. Key assessments:

**`SessionIdentitySource` abstraction (SR-04, vision forward-path to OAuth)**: The enum-not-trait decision is sound for a fixed variant set. The W2-2 replacement path is precisely documented in ARCHITECTURE.md §W2-2 OAuth Replacement Seam. The critical note that W2-2 HTTP mode requires `session_agent: Option<SessionAgent>` per-connection (not per-server-construction) is present and correctly flags the `UnimatrixServer` field model as the true seam risk (not just the enum). This aligns with the vision's "capability check is identical — same service layer, different transport" principle.

**Non-Negotiable #2 (audit log complete)**: The startup audit event (`session_agent_enrolled`) is specified in the architecture (§Startup audit record). The MCP `initialize`-event logging addresses SR-01 and SR-07. Audit attribution for all four paths (empty, whitespace, named specialist, absent param) is covered by R-06 test scenarios. The audit log completeness requirement is satisfied.

**Non-Negotiable #3 (capability checks at service layer, not transport layer)**: The architecture explicitly moves capability resolution to the service layer (`require_cap()` in `server.rs` reading from `session_agent.capabilities`). No capability check occurs at transport layer. The per-call `agent_id` has zero effect on capability resolution. This is correctly aligned.

**Non-Negotiable #5 (no secret material in DBs)**: `UNIMATRIX_SESSION_AGENT` is a plain identifier stored in `AGENT_REGISTRY` as an `agent_id`. This is attribution data, not credential data. The specification explicitly states it must not be treated as a secret. This is correctly aligned.

**`enroll_session_agent()` bypasses AgentRegistry guard (R-05)**: The architecture acknowledges that `enroll_session_agent()` calls `store.agent_enroll()` directly, bypassing the `AgentRegistry` protected-agent check. The mitigation is that validation occurs in `SessionIdentitySource::resolve()` before `enroll_session_agent()` is called. The risk strategy tests this explicitly (R-05 scenarios 1–3). This design is acceptable but the architecture should note (and the delivery team should verify) that `store.agent_enroll()` itself does NOT enforce the protected-agent guard — the guard lives only in `AgentRegistry::enroll_agent()` and `SessionIdentitySource::resolve()`. The risk strategy's integration boundary note for `infra/registry.rs → unimatrix-store/src/registry.rs` covers this.

**Single binary / Zero infrastructure principles**: No new binary, no new service, no new infrastructure dependency. The change is additive to the existing server binary via an env var read and a startup code path. Correctly aligned.

**Breaking change posture**: The specification declares alc-003 a breaking change for all existing deployments. The migration path (one action: add env var to `settings.json`) is clearly documented. This is consistent with the vision's security-first posture and does not violate Single binary or Zero infrastructure principles — it adds a required configuration step, not a new component.

### Specification Review

The specification is complete and resolves the SR-05 contradiction (AC-03 vs Goals §5) that was flagged in the scope risk assessment. SPECIFICATION.md AC-03 clearly states: "(4) No registry lookup occurs during the call (verifiable by mocking or instrumentation)" and capabilities come from the session. This is fully consistent with FR-07 and Goals §5.

The specification correctly exposes `SESSION_AGENT_DEFAULT_CAPS` as a named constant at the W0-3 seam (FR-12), resolving SR-03. The constant is defined in `session_identity.rs` and passed as a parameter to `enroll_session_agent()` — not inlined in `main.rs` or startup logic.

NFR-07 (test isolation) and the test workflow (Workflow 3) provide clear guidance for resolving the test blast-radius impact. The pre-flight measurement approach from ADR-005 is reflected in AC-10's sequencing requirement.

One minor observation: the specification's dependency table lists `identity.rs` as the location for `SessionIdentitySource`, but the architecture places it in `mcp/session_identity.rs` (a new file). The specification's dependency table says "Replace `extract_agent_id()` with `resolve_call_identity()` returning `CallIdentity`. Add `SessionIdentitySource` trait/enum." This is functionally consistent but the file naming differs slightly. Not a variance — the architecture's new-file approach is cleaner and the specification does not mandate that `SessionIdentitySource` stay in `identity.rs`.

### Risk Strategy Review

The risk register is comprehensive (14 risks, covering Critical through Low severity). All SCOPE-RISK-ASSESSMENT.md items (SR-01 through SR-08) are traceable to risk register entries and mitigations. The traceability table at the end of the document confirms full coverage.

The pre-flight blast radius measurement (R-01, ADR-005) as the mandated first commit is correct sequencing and directly addresses the most likely implementation failure mode.

The security risks section correctly characterizes the untrusted input surface for both `UNIMATRIX_SESSION_AGENT` (operator-controlled) and `agent_id` (LLM-controlled), with appropriate severity differentiation. The SQL injection defense-in-depth note (regex eliminates SQL metacharacters before parameterized queries) is accurate.

R-04's refinement of the SR-04 risk is an improvement: the architecture's `SessionIdentitySource` enum is correctly identified as a necessary but insufficient seam — the `UnimatrixServer` field model (startup-time construction vs per-connection assignment for HTTP) is the true seam concern for W2-2 compatibility. The risk strategy recommends verifying that `main.rs` resolves identity before passing it to `UnimatrixServer::new()`, which is the correct call-site discipline for W2-2 compatibility.

The test environment pollution risk (R-13) is well-handled: subprocess spawn pattern for startup-refusal tests, `#[serial]` or drop-guard pattern for in-process env var mutations. This is consistent with the project's test isolation conventions.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` (topic: vision) for vision alignment patterns — one result (#2063: analytics.db topology pattern for nxs-011) which is not relevant to alc-003. No prior vision alignment patterns on record for security feature reviews.
- Stored: nothing novel to store at this time. VARIANCE-01 (startup posture contradicts a vision [Critical] item) is feature-specific and depends on a human decision that will either update the vision or revise the scope. Once resolved, if the startup-refusal posture is adopted as a vision principle for STDIO deployments, a pattern entry of the form "STDIO authentication: fail-fast on missing identity is stricter and preferred over degraded access" would be worth storing.
