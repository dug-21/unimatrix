# Alignment Report: crt-028

> Reviewed: 2026-03-23
> Artifacts reviewed:
>   - product/features/crt-028/architecture/ARCHITECTURE.md
>   - product/features/crt-028/specification/SPECIFICATION.md
>   - product/features/crt-028/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-028/SCOPE.md
> Scope risk source: product/features/crt-028/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly implements WA-5 from the product vision roadmap |
| Milestone Fit | PASS | Wave 1A feature; no future-milestone capabilities pulled in |
| Scope Gaps | WARN | One open question (OQ-SPEC-1) crosses from RISK-TEST-STRATEGY into spec territory without SCOPE.md resolution; minor header format divergence between SCOPE.md and SPECIFICATION.md |
| Scope Additions | PASS | No capabilities added beyond what SCOPE.md requests |
| Architecture Consistency | PASS | Architecture is fully consistent with specification; all open questions from SCOPE.md are settled |
| Risk Completeness | PASS | 13 risks identified and mapped; all SCOPE-RISK-ASSESSMENT risks traced with resolutions |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | OQ-SPEC-1: assistant-turn-with-no-text-blocks behavior | SCOPE.md (D-2) is silent on this case. RISK-TEST-STRATEGY identifies it as an open question requiring a spec addition to FR-02.4. The spec itself does not include the required extension. Tester cannot implement R-10 scenarios without this resolution. |
| Gap | Output header format: SCOPE.md uses `"--- Recent Context ---"` / `"--- Unimatrix Knowledge ---"`; SPECIFICATION.md uses `"=== Recent conversation (last N exchanges) ==="` / `"=== End recent conversation ==="` | SCOPE.md Background Research and PRODUCT-VISION.md WA-5 both specify the `===` format. SPECIFICATION.md correctly uses `===`. ARCHITECTURE.md uses both `---` headers (in the data flow diagram) and `===` format (in the SR-04 resolution and output example). The `---` headers in ARCHITECTURE.md's data-flow illustration are informal, but RISK-TEST-STRATEGY R-12 scenarios reference `"--- Recent Context ---"` and `"--- Unimatrix Knowledge ---"` as the expected header strings, contradicting the spec's `===` headers. This is a testability gap, not a functional gap, but the tester will write assertions against the wrong strings. |
| Simplification | GH #354 and #355 bundled with WA-5 | SCOPE.md explicitly includes D-9 for both. Acceptable: they are bug fixes left open from crt-027, not new scope. Rationale is documented. |

---

## Variances Requiring Approval

### VARIANCE 1 — Header string mismatch between SPECIFICATION.md and RISK-TEST-STRATEGY.md

1. **What**: SPECIFICATION.md FR-02 defines output section headers as `=== Recent conversation (last N exchanges) ===` and `=== End recent conversation ===`. ARCHITECTURE.md SR-04 resolution and output format example use the same `===` convention. However, RISK-TEST-STRATEGY.md R-12 test scenarios explicitly assert against `"--- Recent Context ---"` and `"--- Unimatrix Knowledge ---"` — a different header format using `---` delimiters.

2. **Why it matters**: The tester implements R-12 scenario assertions based on RISK-TEST-STRATEGY verbatim. If RISK-TEST-STRATEGY says to assert `"--- Recent Context ---"`, the test will fail against the implementation that emits `"=== Recent conversation ==="`. Either the spec is wrong, or the risk strategy is wrong. The product vision (PRODUCT-VISION.md WA-5) shows the `===` format, supporting the spec. The discrepancy is not caught until test-writing time or CI failure — a silent scope confusion.

3. **Recommendation**: Update RISK-TEST-STRATEGY.md R-12 test scenarios to reference the `===` header strings matching SPECIFICATION.md FR-02.1/FR-02.5. The `---` headers in ARCHITECTURE.md's data-flow diagram are informal and do not need changing, but should be annotated as illustrative-only to prevent future confusion. Tester must not use RISK-TEST-STRATEGY R-12 assertions verbatim until corrected.

---

### WARN 1 — OQ-SPEC-1 resolution is in RISK-TEST-STRATEGY but not in SPECIFICATION.md

1. **What**: RISK-TEST-STRATEGY identifies OQ-SPEC-1 ("assistant turn with no text blocks — only tool_use and thinking") as requiring a spec addition to FR-02.4. The risk strategy explicitly states: "Required spec addition: FR-02.4 should be extended with..." and provides the exact text. SPECIFICATION.md FR-02.4 does not include this extension. The test for R-10 is explicitly blocked on this spec clarification.

2. **Why it matters**: The specification is the delivery contract. If the resolution lives only in the risk strategy, the implementer and spec author may not see it. When the implementer writes `build_exchange_pairs`, they may choose either behavior — emit or suppress — without knowing the spec mandates "emit if ToolPair present, suppress if both empty." The tester cannot write conformant R-10 scenarios without the spec clause.

3. **Recommendation**: Add the OQ-SPEC-1 resolution to SPECIFICATION.md FR-02.4 before delivery begins. The RISK-TEST-STRATEGY already provides the exact wording. This is a low-effort fix that prevents an ambiguity-driven implementation divergence. Classify as WARN (not VARIANCE) because the resolution text exists and is unambiguous — it just needs to be in the right document.

---

## Detailed Findings

### Vision Alignment

crt-028 is explicitly named in PRODUCT-VISION.md under Wave 1A as "WA-5: PreCompact Transcript Restoration (ASS-028 Recommendation 2)." The vision description matches the feature scope precisely:

- Vision: "Read the transcript file locally (no server round-trip) before sending the CompactPayload request" — matches SCOPE.md D-1 and D-7.
- Vision: Output format `=== Recent conversation (last N exchanges) ===` — matches SPECIFICATION.md FR-02.1/FR-02.5.
- Vision: "Separate injection limit for PreCompact (recommended: 3000 bytes)" — matches SCOPE.md D-4, SPECIFICATION.md FR-04.1, and ARCHITECTURE.md constants table.
- Vision: "Fully independent — ships in any order relative to WA-1 through WA-4" — matches SCOPE.md §Dependencies (crt-027 only).

The bundled security fixes (GH #354, GH #355) are a crt-027 leftover cleanup; they do not conflict with the vision and are consistent with the product vision's security gap remediation posture.

**Verdict**: PASS — full vision-to-scope-to-architecture-to-specification traceability.

---

### Milestone Fit

crt-028 targets Wave 1A (Adaptive Intelligence Pipeline). No Wave 2 capabilities (HTTP transport, OAuth, container packaging) are pulled in. No Wave 3 capabilities (GNN training, session-conditioned learned function) are anticipated beyond the acknowledged OQ-1 deferral (session-injection affinity at compaction — explicitly noted as "not blocking delivery"). The feature explicitly avoids server schema changes, new crates, and wire protocol changes. Effort is self-contained within the hook process.

The sole external dependency (crt-027) is a Wave 1A predecessor, not a future-wave item.

**Verdict**: PASS.

---

### Architecture Review

The architecture document is thorough and internally consistent. Key observations:

- All three open questions from SCOPE.md (OQ-1, OQ-2, OQ-3) are resolved in ARCHITECTURE.md with explicit design decisions (ADR-001 through ADR-004 referenced, OQ-1/OQ-2/OQ-3 settled sections present).
- The tail-bytes strategy (ADR-001) directly addresses SCOPE-RISK-ASSESSMENT SR-02. The 4× multiplier rationale is quantified.
- The graceful degradation contract (ADR-003) is structurally enforced via the `Option<String>` return type and `and_then` call site — not merely documented.
- The GH #354 fix (ADR-004) uses a named helper `sanitize_observation_source` rather than an inline match — the doc comment designates it as the sole write gate, providing forward-enforcement documentation for R-07.
- The SR-04 (empty briefing + non-empty transcript) case is explicitly handled in `prepend_transcript` with all four case branches enumerated.
- The crt-027 symbol consumption table is precise — four symbols identified with exact locations (SR-06 / R-13 mitigation).

The only architectural item that does not fully satisfy the SCOPE-RISK-ASSESSMENT is the ARCHITECTURE.md data-flow diagram using `"--- Recent Context ---"` and `"--- Unimatrix Knowledge ---"` as section labels, while the SR-04 resolution section correctly uses the `===` format. This is the root cause of the RISK-TEST-STRATEGY header mismatch (VARIANCE 1 above).

**Verdict**: PASS on substance; the header label inconsistency is an editorial error confined to the data-flow illustration and carries no implementation risk beyond the R-12 test authoring issue.

---

### Specification Review

The specification is complete and traceable. All 15 acceptance criteria from SCOPE.md (AC-01 through AC-15) map to functional requirements in the specification:

| SCOPE.md AC | SPECIFICATION.md FR |
|-------------|---------------------|
| AC-01 | FR-01.1, FR-05.1 |
| AC-02 | FR-02.2, FR-02.3 |
| AC-03 | FR-03.1 through FR-03.5 |
| AC-04 | FR-03.6 |
| AC-05 | FR-04.1, FR-04.2 |
| AC-06 | FR-06.1 |
| AC-07 | FR-06.2 |
| AC-08 | FR-06.3, FR-06.4 |
| AC-09 | FR-06.5 |
| AC-10 | FR-04.1 |
| AC-11 | FR-07.1 through FR-07.6 |
| AC-12 | FR-08.1, FR-08.2 |
| AC-13 | FR-08.3 |
| AC-14 | FR-05.4 |
| AC-15 | NFR-01, FR-06.6 |

The specification adds NFR-02 (no tokio runtime in hook process) and NFR-03 (no stderr on graceful degradation) beyond the SCOPE.md acceptance criteria. These are consistent with the existing hook process constraints stated in SCOPE.md §Constraints and are not scope additions — they are implementation guardrails formalizing known constraints.

**Gap**: FR-02.4 does not include the OQ-SPEC-1 resolution. This is the WARN flagged above.

**Verdict**: PASS with WARN on OQ-SPEC-1.

---

### Risk Strategy Review

The RISK-TEST-STRATEGY is well-structured and traces all seven SCOPE-RISK-ASSESSMENT risks to architecture risks and resolutions:

| SCOPE-RISK-ASSESSMENT | RISK-TEST-STRATEGY | Resolution |
|-----------------------|-------------------|------------|
| SR-01 (JSONL format change) | R-04 | ADR-001/002 fail-open design; adjacent-record edge cases tested |
| SR-02 (sync I/O budget) | R-02 | ADR-001 tail-bytes 12 KB cap; TAIL_MULTIPLIER tuning knob |
| SR-03 (MAX_PRECOMPACT_BYTES not configurable) | — | Accepted; FR-04.4 adds config.toml forward pointer |
| SR-04 (empty briefing output format) | R-12 | prepend_transcript() four-case explicit logic |
| SR-05 (GH #354 under-reviewed) | R-07 | Named helper with dedicated unit test + integration test |
| SR-06 (crt-027 API change) | R-13 | Compile-time dependency; cargo check gate |
| SR-07 (degradation miscoped) | R-01 | Structural enforcement via Option<String> + and_then |

The RISK-TEST-STRATEGY also adds new risks not in SCOPE-RISK-ASSESSMENT: R-05 (reversal ordering), R-06 (UTF-8 boundary), R-08 (quarantine post-filter), R-09 (key-param fallback sensitive fields), R-10 (OQ-SPEC-1 tool-only turns), R-11 (path traversal), R-13 (crt-027 symbol rename). These are appropriate additions from the tester's perspective and do not represent scope additions.

R-09 (key-param fallback selecting sensitive fields) identifies a production limitation and recommends a field denylist (`api_key`, `token`, `secret`, `password`). This is documented as a follow-up, not in-scope. The recommendation is sound and the deferral is explicitly noted.

The R-10/OQ-SPEC-1 scenario is marked blocked on spec clarification — correctly identified as a prerequisite, but the spec has not been updated. This is the same WARN as above.

The R-12 test scenario assertions use `"--- Recent Context ---"` and `"--- Unimatrix Knowledge ---"` which contradicts SPECIFICATION.md FR-02. This is VARIANCE 1.

**Verdict**: PASS on coverage completeness and risk identification. The R-12 header string issue and OQ-SPEC-1 gap are called out as VARIANCE 1 and WARN 1 respectively.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found entries #2298 (config key semantic divergence) and #2063 (file-topology vs vision language). Neither pattern applies to crt-028, which has a clean scope-to-vision mapping. The header-string mismatch found here (VARIANCE 1) is a new pattern type: cross-document header format divergence between architecture diagrams and test strategy assertions.
- Stored: see below (storing the cross-document header divergence pattern as it generalizes to any feature where architecture uses informal ASCII diagrams that testers may treat as authoritative).
