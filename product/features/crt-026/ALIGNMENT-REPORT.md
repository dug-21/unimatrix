# Alignment Report: crt-026

> Reviewed: 2026-03-22
> Artifacts reviewed:
>   - product/features/crt-026/architecture/ARCHITECTURE.md
>   - product/features/crt-026/specification/SPECIFICATION.md
>   - product/features/crt-026/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-026/SCOPE.md
> Scope risk source: product/features/crt-026/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | WARN | Pipeline positioning diverges from product vision text; see V-1 |
| Milestone Fit | PASS | Wave 1A WA-2, correct milestone; dependencies (WA-0, WA-1) complete |
| Scope Gaps | PASS | All SCOPE.md deliverables addressed in source docs |
| Scope Additions | PASS | No out-of-scope items added |
| Architecture Consistency | WARN | `phase_explicit_norm=0.0` placeholder added to `compute_fused_score` creates permanent dead-code term; see V-2 |
| Risk Completeness | PASS | All scope risks traced; 14 risk entries, 40 test scenarios |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | AC-07 dropped | SCOPE.md lists AC-07 (explicit phase affinity boost). Spec drops it with documented rationale: `w_phase_explicit=0.0`, `phase_category_weight` mapping deferred to W3-1 (OQ-03 resolved). SR-04 in scope risk assessment flagged this; spec writer resolved it cleanly. Acceptable. |
| Simplification | Pipeline positioning | SCOPE.md §Constraints describes boost as "outside `compute_fused_score`... avoids touching `InferenceConfig::validate`". Architecture/Spec resolves OQ-01 to integrate INSIDE `compute_fused_score`. This is a documented resolved decision (ADR-001), not a silent scope change. The simpler path (post-pipeline) was explicitly rejected in favour of W3-1 compatibility. Acceptable with the vision caveat noted at V-1 below. |
| Simplification | `phase_explicit_norm` always 0.0 | SCOPE.md §Component 5 reserved `w_phase_explicit` and `w_phase_histogram` as separate terms. Architecture ships `phase_explicit_norm: f64 = 0.0` as a placeholder field — always zero, never populated. This is a deliberate deferral, documented in ADR-003. See V-2 for the concern about long-lived dead-code. |

---

## Variances Requiring Approval

### WARN V-1: Pipeline positioning diverges from product vision description of WA-2

**What**: The product vision WA-2 section (PRODUCT-VISION.md line 230) describes the complete ranking pipeline after this feature as:
```
HNSW(k=20) → NLI re-rank → co-access boost → category affinity boost → top-k
```
This explicitly places "category affinity boost" as a **post-pipeline** step — after `co-access boost`, before `top-k`. The same vision document's WA-0 shipped formula section (line 189) states `sum=0.95, 0.05 headroom for WA-2`, framing WA-2 as an additive step that uses the reserved headroom rather than modifying the fused score function.

The architecture (ADR-001) resolves OQ-01 to integrate the histogram term **inside** `compute_fused_score` as a first-class dimension. This means the histogram boost participates in the pre-penalty fused score, making `status_penalty` interact with it multiplicatively rather than additively. The vision text's pipeline diagram is now inaccurate.

**Why it matters**: The vision describes `compute_fused_score` as W3-1's feature vector interface. Integrating the boost inside the function is architecturally correct for that purpose — it gives W3-1 a named, learnable dimension alongside the other six. However, the product vision's explicit pipeline diagram (post-pipeline positioning) and its WA-0 headroom framing (additive step outside the fused score) are both contradicted by the implementation approach. A human reading the vision document will find the pipeline description inconsistent with what ships.

There is also a scoring behavior difference: with inside-`compute_fused_score` placement, `status_penalty` applies to the boost (`(base + boost) * penalty`). With post-pipeline placement the vision describes, it would be `base * penalty + boost` — a deprecated entry matching the session histogram would receive the full boost regardless of penalty. The architecture's choice is safer (deprecated entries are still penalized), but it diverges from the vision's stated formula.

**Recommendation**: Accept the architectural decision (it is demonstrably safer and W3-1-compatible). Update PRODUCT-VISION.md's WA-2 pipeline diagram to read:
```
HNSW(k=20) → NLI re-rank → compute_fused_score (with histogram term) → status_penalty → top-k
```
and update the WA-0 headroom note to clarify that the 0.05 headroom is consumed inside `compute_fused_score` (not as a post-pipeline step). This is a documentation correction, not a code change.

---

### WARN V-2: `phase_explicit_norm = 0.0` is a permanent dead-code field inside `compute_fused_score`

**What**: The architecture ships `phase_explicit_norm: f64` on `FusedScoreInputs` and `w_phase_explicit: f64` on `FusionWeights`, both always `0.0` in crt-026. The term `weights.w_phase_explicit * inputs.phase_explicit_norm` is added to `compute_fused_score` and evaluates to `0.0 * 0.0 = 0.0` for every candidate on every query until W3-1 populates it. This is a multiplication-by-zero term hardwired into the hot path scoring loop.

**Why it matters**: `compute_fused_score` is described as a pure function — W3-1's feature vector interface. Adding a structurally zero term is not harmful to correctness, but it introduces a field that rustc cannot warn about as dead code (it is used — it is just always zero). Future contributors may misread it as a live term. If W3-1 is delayed or deprioritized, this field persists indefinitely in an always-zero state. RISK-TEST-STRATEGY.md R-07 explicitly calls out the risk of this placeholder being removed as dead code.

The SCOPE.md constraint (OQ-03 resolved) explicitly defers the explicit phase term to W3-1 because "a static mapping would couple ranking to SM vocabulary." This is the correct call. The question is whether reserving a live-but-zero field inside the scoring loop is the right mechanism for that deferral, versus a code comment at the call site noting the W3-1 extension point.

**Recommendation**: Accept as-is if W3-1 is on the near roadmap (Wave 3). The placeholder approach is the ADR-003 decision and is tracked by R-07 in the risk strategy. If W3-1 timeline is uncertain, consider whether a stub comment (removed field, doc comment at extension point) would be less confusing than a live zero-valued field. This is a code maintainability judgment call, not a correctness issue. No blocking action required.

---

## Detailed Findings

### Vision Alignment

**"Intelligence pipeline is a learned function"** (PRODUCT-VISION.md §Story, line 17): The architecture integrates `phase_histogram_norm` as a first-class `FusedScoreInputs` dimension with a named weight in `FusionWeights`. This directly advances the vision: W3-1 initializes from `w_phase_histogram=0.005` and refines from real usage. The field is stable, named, and learnable. This is a strong alignment with the vision's statement that `compute_fused_score` is W3-1's feature vector interface.

**"Session-conditioned, self-improving relevance function"** (PRODUCT-VISION.md §Story, line 17): crt-026 adds the first implicit session-context signal to the ranking pipeline. The histogram accumulates without agent cooperation — it is a behavioral signal, not a declared one. This is precisely what the vision describes as the step toward session-conditioned relevance without requiring WA-4's proactive machinery.

**"Config-driven from the start"** (PRODUCT-VISION.md WA-2, line 252): Both `w_phase_explicit` and `w_phase_histogram` are placed in `InferenceConfig` under `[inference]` with `#[serde(default)]` pattern, following the established `default_w_*` convention. This directly satisfies the vision's explicit requirement: "AFFINITY_WEIGHT constants are config-driven from the start — hardcoding them would repeat the pattern this whole wave is designed to eliminate."

**Pipeline positioning divergence** (V-1 above): The vision's WA-2 section shows the boost as a post-pipeline step. The architecture places it inside `compute_fused_score`. As noted in V-1, the architectural decision is sound, but the vision document description is now inaccurate.

**WA-2 two-term formula**: Product vision specifies two separate terms (explicit phase 0.015, histogram 0.005) both applying when both signals are present. crt-026 ships only the histogram term at 0.005; the explicit phase term is deferred at 0.0. SCOPE.md OQ-03 documents this resolution with clear rationale (phase vocabulary is opaque; static mapping would couple to SM vocabulary; W3-1 will learn the relationship). The deferral is justified and internally consistent. The `w_phase_explicit=0.0` default means the vision's explicit-phase behavior is not delivered by crt-026 — this is a known, accepted scope boundary.

**UDS injection summary** (PRODUCT-VISION.md WA-2, line 256): Vision states `"Recent session activity: decision × 3, pattern × 2 (design phase signal)"` — note the trailing `(design phase signal)` qualifier in the vision text. The specification's FR-12 and architecture Component 8 specify the format as `"Recent session activity: decision × 3, pattern × 2"` — omitting the `(design phase signal)` suffix. This is a minor format deviation. Since `current_phase` is opaque and the phase term is deferred, omitting the phase qualifier from the CompactPayload block is consistent and sensible. No action required.

---

### Milestone Fit

crt-026 is correctly positioned as Wave 1A WA-2. It depends on:
- WA-0 (crt-024, `compute_fused_score`, COMPLETE)
- WA-1 (crt-025, `SessionState.current_phase`, COMPLETE)

It explicitly does not implement:
- WA-3 (MissedRetrieval — the training signal, separate feature)
- WA-4a/WA-4b (proactive injection / briefing, separate features)

The feature does not reach into Wave 2 (security, container) or Wave 3 (GNN) territory beyond reserving named fields for W3-1 initialization. It is appropriately scoped to deliver one WA-2 deliverable — the implicit histogram signal — without pulling forward WA-2's second term (explicit phase) or unblocking WA-4's proactive delivery beyond noting the forward-compatibility flag (OQ-C/ADR-002).

The architecture's OQ-C note (WA-4a will likely need `Arc<SessionRegistry>` on `SearchService`, reopening ADR-002) is an appropriate forward-compatibility flag, not a scope addition.

---

### Architecture Review

**Component 1 (SessionState + SessionRegistry methods)**: Clean, minimal. `record_category_store` is synchronous, lock-held, microseconds — consistent with existing `record_injection` contract. `get_category_histogram` returns a clone, preventing external mutation of session state. Correct.

**Component 2 (context_store handler)**: Placement after duplicate guard, before confidence seeding is exactly right and matches AC-02. The `if let Some(ref sid)` guard pattern is the established crt-025 SR-07 pattern.

**Components 3 & 4 (ServiceSearchParams + context_search threading)**: Pre-resolution before `await` follows the crt-025 SR-07 snapshot pattern. Empty-map-to-None mapping (`if h.is_empty() { None } else { Some(h) }`) is the primary guard against the R-09 division-by-zero failure mode. The architecture's choice to carry `category_histogram: Option<HashMap<String, u32>>` rather than `Arc<SessionRegistry>` keeps `SearchService` dependency-free of session infrastructure — consistent with ADR-002.

**Component 5 (compute_fused_score)**: The `effective()` method's NLI-absent re-normalization denominator is explicitly documented to exclude the phase fields (ARCHITECTURE.md line 399-401). This is the correct treatment of R-06. The architecture specifies: "both new fields returned unchanged (pass-through) in both NLI-active and NLI-absent paths." This must be verified in implementation — the risk is real (#2964 pattern) and the documentation of the requirement is clear.

**Component 7 (UDS handle_context_search)**: OQ-B resolved: `sanitize_session_id` is confirmed applied before histogram pre-resolution (lines 796-803). The UDS path carries `session_id` from `HookRequest::ContextSearch.session_id` — not from `audit_ctx`. This source difference is correctly documented and the pre-resolution block is placed after the sanitize check.

**ADR-004 (no weight rebalancing)**: The six-weight sum (`0.95`) is NOT modified. `w_phase_histogram=0.005` is added outside the six-term sum constraint, bringing the effective default total to `0.955`. `InferenceConfig::validate()` is confirmed to check only the six-term sum (`w_sim + w_nli + w_conf + w_coac + w_util + w_prov <= 1.0`), so `0.955` passes cleanly. The doc-comment on `FusionWeights` must be updated — this is captured as a required action in the architecture (OQ-A).

**WA-4a forward-compatibility (OQ-C)**: Flagged correctly as a forward-compatibility risk in ADR-002 and the downstream integration section. No code change needed in crt-026; WA-4a must re-evaluate and supersede ADR-002. This is appropriate milestone discipline.

---

### Specification Review

**AC-07 dropped**: Specification explicitly drops AC-07 (explicit phase affinity boost) with full rationale. The acceptance criteria section opens with "All AC-IDs flow from SCOPE.md. AC-07 is explicitly dropped." The SCOPE.md SR-04 risk item identified the ambiguity; the spec writer resolved it. The resolution is complete and documented.

**AC-12 concreteness**: The scope risk assessment SR-01 flagged the risk that AC-12 would be vague ("ranks higher" without a numerical floor). The specification defines AC-12 with a concrete score delta floor: `≥ w_phase_histogram * 1.0 = 0.005` with `p=1.0` concentration. NFR-06 mandates ≥60% histogram concentration for test fixtures. SR-01 is fully resolved.

**OQ-A resolved in architecture**: Specification FR-11 notes that `InferenceConfig::validate()` must accept `sum=0.955` cleanly, and the architecture OQ-A confirms this via code-level analysis. The open question in the spec is correctly delegated to the architect and has a confirmed answer.

**UDS histogram summary format**: Spec FR-12 omits the `(design phase signal)` suffix present in the vision's example. As noted under Vision Alignment, this is consistent with the deferral of the explicit phase term and is acceptable.

**Constraint C-06** (application order): Explicitly states `final_score = compute_fused_score(&inputs, &weights) * status_penalty`. The histogram boost participates in the pre-penalty fused score. This is the correct, safer ordering (V-1 elaborates on why it differs from the vision's pipeline diagram but is the right call).

**Open questions for architect**: Four questions (OQ-A through OQ-D) remain in the specification, delegated to the architect. All four are resolved in ARCHITECTURE.md. The delegation is appropriate — these are implementation-level confirmations, not design-level unknowns.

---

### Risk Strategy Review

**Coverage**: 14 risk entries, 40 required test scenarios. All nine scope risks from SCOPE-RISK-ASSESSMENT.md are traced to the risk register. The traceability table at the end of RISK-TEST-STRATEGY.md maps each SR-* to its R-* resolution.

**Non-negotiable gate tests**: Seven gate-blocking tests are named. All map to acceptance criteria in the specification.

**R-06 (FusionWeights::effective() denominator)**: This is the highest architectural regression risk. The risk strategy correctly identifies it (historical entry #2964 cited), requires an explicit `effective(false)` test with the new fields, and asserts the re-normalization denominator must enumerate exactly the five core terms. This is adequate coverage.

**R-01 (0.005 weight too small to detect)**: Correctly identified as Critical/High priority. The resolution — manufactured histogram concentration (p=1.0 for exact delta; ≥60% for detectable delta) with a numerical floor assertion — is sound. Three specific test scenarios are defined.

**Security section**: Appropriately scoped. `session_id` sanitization is confirmed pre-existing. Histogram key injection blast radius is correctly assessed as limited to a HashMap key under an unexpected name — no SQL, no path traversal, no cross-session effect. CompactPayload injection risk is bounded by top-5 cap and `MAX_INJECTION_BYTES` budget.

**R-13 (pre-resolution after await)**: Identified as a real class of bug (entry #1274 cited — force-set race in session registry). Correctly categorized as a code-review check rather than an automatable unit test. Optional stress test is specified. Coverage is proportionate.

**Knowledge stewardship in risk doc**: The risk document queried Unimatrix before writing (entries #2758, #2800, #2964, #1274, #1611 cited). This is correct practice.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` topic=`vision` category=`pattern` for vision alignment patterns — found entries #2298 (config key semantic divergence) and #2063 (single-file topology vs split-file vision language). Neither directly applies to crt-026's specific alignment concerns.
- Queried: `mcp__unimatrix__context_search` for "WA-2 affinity boost compute_fused_score integration" — found #3156 (WA-2 post-pipeline vs. FusedScoreInputs pattern), #3161 (ADR-001 decision), #3164 (ADR-004 no rebalancing), #3163 (ADR-003 explicit phase deferral). These confirm the ADR decisions are stored and the alignment concern at V-1 (pipeline positioning divergence from vision text) is a known, deliberate departure.
- Stored: nothing novel — V-1 (product vision pipeline diagram now inaccurate after ADR-001) is a crt-026-specific documentation drift instance. The pattern (architectural decision superseding vision diagram wording) may generalize as a recurring type if it appears in WA-3 or WA-4 as well; will store after confirming recurrence.
