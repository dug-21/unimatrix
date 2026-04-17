# Alignment Report: vnc-013

> Reviewed: 2026-04-17
> Artifacts reviewed:
>   - product/features/vnc-013/architecture/ARCHITECTURE.md
>   - product/features/vnc-013/specification/SPECIFICATION.md
>   - product/features/vnc-013/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope: product/features/vnc-013/SCOPE.md (modified — diff reviewed)
> Scope risk: product/features/vnc-013/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly advances domain-agnostic platform goal and closes a Critical Gap |
| Milestone Fit | PASS | Correctly positioned in Vinculum phase; supports Wave 2 multi-project readiness |
| Scope Gaps | PASS | All SCOPE.md goals and ACs (including post-ASS-051 additions) addressed in all three source docs |
| Scope Additions | WARN | Source docs add four open questions for the spec writer (OQ-A through OQ-D in ARCHITECTURE.md) that have no corresponding SCOPE.md resolution — see details |
| Architecture Consistency | PASS | Layered design, blast-radius table, six ADRs, no conflicts across documents |
| Risk Completeness | PASS | 13 risks mapped, all scope risks traced, gate prerequisites explicit |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | Architecture OQ-A: `DEFAULT_HOOK_SOURCE_DOMAIN` constant placement | ARCHITECTURE.md introduces three options (observation.rs, new module, engine crate); SCOPE.md does not require the spec writer to resolve this — it defers to the spec writer. SPECIFICATION.md resolves it (FR-06 uses the constant) but placement is left open. Low impact — does not affect AC coverage. |
| Addition | Architecture OQ-B: `tool_input` promotion for `extract_event_topic_signal()` | ARCHITECTURE.md asks spec writer to confirm whether `tool_input` also needs promotion. SPECIFICATION.md (FR-04.7) resolves this as top-level based on ASS-049 confirmation, but notes spec writer must verify. AC-11 covers the test. |
| Addition | Architecture OQ-C: Gemini `AfterTool` response field name | ARCHITECTURE.md explicitly defers to implementer to confirm or document degraded mode. SPECIFICATION.md resolves as graceful degradation (C-09 / SR-02). No AC tests the positive case if confirmed. Documented risk in RISK-TEST-STRATEGY R-11. |
| Addition | Architecture OQ-D: `extract_observation_fields()` Gemini `SubagentStop` arm behavior | ARCHITECTURE.md asks spec writer to confirm the wildcard arm behavior for Gemini `PostToolUse` records. SPECIFICATION.md does not explicitly address this. No AC covers it. Low severity — wildcard arm already handles this correctly per the architecture analysis. |
| Simplification | SCOPE.md Constraint 8 (Codex "documentation-only") expanded to full code paths | SCOPE.md was updated from "documentation-only Codex config" to "Codex code paths built but not live-tested." All three source documents reflect the expanded scope (AC-17, AC-18, AC-19, FR-03). The SCOPE.md modification and source documents are consistent with each other. This is a legitimate scope refinement from ASS-051. |

---

## Variances Requiring Approval

None. All checks pass or are classified WARN (awareness items only, none requiring approval).

The four architecture open questions (OQ-A through OQ-D) are deferred-resolution items
appropriate for the implementation brief phase, not vision or scope variances. They are
captured as WARN items below for the implementer's awareness.

---

## Detailed Findings

### Vision Alignment

The product vision identifies "HookType enum tied to Claude Code events" as a Critical
Gap, marked Fixed by col-023 (W1-5). vnc-013 extends that fix to the three remaining
hardcoded `source_domain = "claude-code"` sites and the `build_request()` dispatch
boundary — the last unmigrated Claude Code coupling at the hook ingest boundary.

The vision's domain-agnostic platform direction ("Configurable for any workflow-centric
domain") requires that all LLM clients can participate equally. vnc-013 makes Gemini CLI
a first-class participant and builds forward-compatible Codex CLI paths. This is direct
vision execution, not tangential work.

The vision's "Single binary" and "Zero infrastructure" non-negotiables are preserved:
no new services, no schema changes, no new crate dependencies (NFR-07).

The vision's security cross-cutting concerns are addressed: the `mcp_context.tool_name`
injection risk (noted in RISK-TEST-STRATEGY Security Risks section) is correctly analyzed
as a cycle state corruption risk (not code execution), and the existing
`contains("context_cycle")` guard is flagged as permissive with a recommendation for
stricter equality matching. This is the correct treatment.

**Verdict**: PASS. The feature is a direct Critical Gap closure aligned with the
domain-agnostic platform vision.

---

### Milestone Fit

vnc-013 is a Vinculum phase feature (MCP server / hook boundary). The Vinculum phase
is complete in Wave 0 terms (vnc-001 through vnc-004 shipped). vnc-013 is a correctness
and extensibility fix to the hook ingest layer — appropriate for a late Vinculum patch
that unblocks Wave 2 multi-project deployment (W2-3 OAuth requires agent_id from
multiple LLM clients to be correctly attributed).

Wave 2 prerequisites include HTTP transport (W2-2) and OAuth (W2-3), which depend on
agent identity being stable and correctly sourced. vnc-013's `provider` field and
`source_domain` derivation are foundational for correct attribution across multiple
clients — shipping this before W2-3 is the correct sequencing.

The feature does not reach into Wave 3 capabilities (no GNN, no synthesis). Effort
matches the SCOPE.md framing: pure boundary instrumentation, six files, no new
infrastructure.

**Verdict**: PASS.

---

### Architecture Review

The four-layer design (wire protocol, normalization, source_domain derivation, reference
configs) is coherent and consistently represented across ARCHITECTURE.md, SPECIFICATION.md,
and RISK-TEST-STRATEGY.md.

Six ADRs are listed in the architecture. All decisions are present and traceable to
acceptance criteria:

| ADR | Decision | AC |
|-----|----------|----|
| ADR-001 | Claude Code names as canonical | AC-01, AC-08 |
| ADR-002 | Explicit `provider` field (not inference post-normalization) | AC-05, AC-06 |
| ADR-003 | Named `mcp_context` field on `HookInput` | AC-14 |
| ADR-004 | Approach A (registry-with-fallback) | AC-07(b), AC-07(c) |
| ADR-005 | Provider gate for rework detection | AC-04, AC-12 |
| ADR-006 | `--provider codex-cli` mandatory in Codex config | AC-19 |

The blast-radius table in ARCHITECTURE.md C-11 correctly enumerates six files. The
Specification (C-11) independently confirms the same six files. The RISK-TEST-STRATEGY
R-09 traces AC coverage to each file. The three documents are internally consistent on
blast radius.

**One minor inconsistency**: ARCHITECTURE.md lists `domain/mod.rs` in C-11 as "No
changes required" but includes it in the blast-radius table. SPECIFICATION.md's C-11
also includes it but marks it "No changes required — registry is insulated by
normalization." This is not a coverage gap; it is correct to include it as explicit
confirmation of no-change. No issue.

The `build_cycle_event_or_fallthrough()` call-site coupling risk (promotion adapter
must pass the cloned/mutated input, not the original) is correctly identified in
RISK-TEST-STRATEGY Integration Risks and in ARCHITECTURE.md Layer 2. The
`debug_assert!(event.provider.is_some())` canary mentioned in R-02 as ADR-002
requirement is not explicitly present in SPECIFICATION.md FR-02 or FR-04 — it appears
only in RISK-TEST-STRATEGY. This is a WARN: the canary is a good defensive measure
but the spec does not require it. Implementer may omit it without violating a spec
requirement.

**Verdict**: PASS with one WARN item (debug_assert canary).

---

### Specification Review

SPECIFICATION.md covers all 20 ACs from SCOPE.md with verification methods. All SCOPE.md
goals (1–9, including the ASS-051 additions in Goals §9) map to functional requirements:

| SCOPE Goal | FR |
|------------|-----|
| Goal 1: Canonical event taxonomy | FR-01 |
| Goal 2: Normalization at ingest boundary | FR-01, FR-04 |
| Goal 3: Dynamic source_domain | FR-06 |
| Goal 4: `provider` field on HookInput/ImplantEvent | FR-02 |
| Goal 5: Gemini CLI arms in build_request() | FR-04 |
| Goal 6: Gemini mcp_context field handling | FR-04.3 |
| Goal 7: Reference .gemini/settings.json | FR-09.1 |
| Goal 8: No downstream branching on provider-specific names | FR-01.6, NFR-02 |
| Goal 9 (ASS-051 addition): guard assertion in extract_observation_fields() | FR-08 |

Non-functional requirements (NFR-01 through NFR-08) all have corresponding SCOPE
constraints. No NFR appears without a scope anchor. No scope constraint is orphaned
from an NFR.

Open questions from original SCOPE.md (OQ 1–8) are all resolved in SPECIFICATION.md
with explicit resolution statements. The SPECIFICATION.md "Open Questions" section
correctly states "None."

**One gap**: SPECIFICATION.md FR-01.1 defines the function signature as returning
`(&str, &str)` while ARCHITECTURE.md Layer 2 specifies `(&'static str, &'static str)`.
The SCOPE.md AC-01 only specifies the behavior, not the lifetime annotation. This is
a minor technical inconsistency between the spec and architecture that will be resolved
at implementation; it does not affect correctness. Flagged as WARN.

**Verdict**: PASS with one WARN item (lifetime annotation inconsistency).

---

### Risk Strategy Review

The 13-risk register is complete and well-calibrated. Priority assignments are
consistent with the feature's blast radius:

- R-01 (mcp_context promotion silent fallthrough) as Critical: correct. This is the
  single point where Gemini `context_cycle` interception can fail silently.
- R-03 (Codex mislabel without --provider flag) as High with High likelihood: correct.
  The Codex code path is built speculatively and the `--provider` flag is documentation-
  only protection.
- R-04 (Approach A fallback regression) as High with Low likelihood: calibration is
  appropriate — the mechanism is simple but the blast radius of a regression is broad.

All nine scope risks from SCOPE-RISK-ASSESSMENT.md are traced to architecture risks
in the Scope Risk Traceability table. No scope risk is unresolved or unaddressed.

**One gap**: The security risk item (permissive `contains("context_cycle")` check)
in the Security Risks section recommends stricter equality matching but there is no
corresponding acceptance criterion or risk register entry to enforce this at gate time.
The SCOPE.md does not require it. This creates a situation where the implementer can
ship with the permissive check without violating any AC, and the security improvement
is lost. This is a WARN — it could be addressed by adding a note to the implementation
brief or converting the recommendation to an AC.

The gate prerequisite on AC-14 (R-01 unit tests must be green before other Gemini
BeforeTool ACs are attempted) is correctly formalized in both RISK-TEST-STRATEGY and
SPECIFICATION.md C-12. This is good process discipline.

**Verdict**: PASS with one WARN item (permissive `contains` check has no enforcement
path).

---

## WARN Items Summary (for implementer awareness)

1. **debug_assert canary not in SPECIFICATION.md**: RISK-TEST-STRATEGY R-02 references
   an ADR-002 requirement for `debug_assert!(event.provider.is_some())` in `listener.rs`,
   but SPECIFICATION.md does not include this requirement in any FR. Implementer
   should add it as a defensive measure. No approval needed; informational only.

2. **Lifetime annotation inconsistency**: FR-01.1 in SPECIFICATION.md gives
   `-> (&str, &str)` while ARCHITECTURE.md Layer 2 gives `-> (&'static str, &'static str)`.
   The architecture version is correct (pure static mapping, no borrowed returns).
   Implementer should follow ARCHITECTURE.md. No approval needed.

3. **Permissive `contains("context_cycle")` check has no enforcement path**: The
   security risk note recommends stricter equality checking but no AC enforces it.
   Recommend adding this to the implementation brief as a concrete recommendation
   to the architect/implementer. No approval needed; informational only.

4. **Architecture OQ-D unresolved**: `extract_observation_fields()` wildcard arm
   behavior for Gemini `PostToolUse` (from normalized `AfterTool`) is noted as "spec
   writer should confirm" in ARCHITECTURE.md but has no explicit SPECIFICATION.md
   treatment and no AC. The architecture analysis concludes the wildcard arm handles
   it correctly — the gap is documentation, not behavior. Informational only.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for `vision alignment review patterns scope variance` —
  Found entries #2298 (config key semantic divergence pattern), #3337 (architecture
  diagram informal headers diverge from spec — relevant pattern), #3426 (formatter
  underestimates regression risk). Entry #3337 is the closest match: it documents the
  pattern where architecture diagrams use informal strings that diverge from spec. In
  this review, the lifetime annotation inconsistency between ARCHITECTURE.md and
  SPECIFICATION.md is an instance of this pattern. The finding is feature-specific and
  does not generalize beyond confirming the existing pattern.
- Stored: nothing novel to store — the variances identified are feature-specific
  (lifetime annotation discrepancy, unresolved architecture open questions for spec
  writer). These do not represent a new cross-feature alignment pattern. The
  SCOPE.md evolution pattern (zero session refinements incorporated into SCOPE.md
  before source documents are produced) is expected workflow, not a misalignment.
