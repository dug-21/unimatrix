# Alignment Report: vnc-049

> Reviewed: 2026-09-15
> Artifacts reviewed:
>   - product/features/vnc-049/architecture/ARCHITECTURE.md
>   - product/features/vnc-049/specification/SPECIFICATION.md
>   - product/features/vnc-049/RISK-TEST-STRATEGY.md
> Scope source: product/features/vnc-049/SCOPE.md, SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md; goals #5731 (personal-cloud), #5734 (trusted-identity), #5681 (integrity), #5732 (domain-agnostic)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | C18 is the named next client for personal-cloud North-star (#5731); attribution-at-ingest strengthens Knowledge Integrity (#5681) |
| Milestone Fit | PASS | Targets C18 exactly; AC-07 builds forward-compat seam only, no premature E1 enforcement — correct milestone discipline |
| Scope Gaps | PASS | All 7 ACs traced to FRs, components, ADRs, and risks; no SCOPE item unaddressed |
| Scope Additions | PASS (WARN resolved) | Schema migration reframe (ADR-001) is necessary and in-outcome; write-time stamp DECIDED **opencode-only** (2026-09-15) — non-opencode rows stay NULL, existing behavior preserved byte-for-byte (OQ-2/R-05 resolved) |
| Architecture Consistency | PASS | ADR-001..009 internally consistent; code-line re-measurement correctly overturns SCOPE's raw-line carve premise (background.rs untouched) |
| Risk Completeness | PASS | SR-01..12 fully mapped to R-01..17 + behavioral-outcome lens; AC-06 end-path gating test well-specified |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | AC-01→FR-01/02/03/12; AC-02→FR-04/05; AC-03→FR-06; AC-04→FR-07; AC-05→FR-09/10; AC-06→FR-08; AC-07→FR-11 — all covered |
| Addition | Schema migration (source_domain + model_id columns; next sequential version resolved against CURRENT_SCHEMA_VERSION at delivery — 32 today) | Not in SCOPE's "provider flag + domain pack" framing. Necessary: ARCHITECTURE §System Overview establishes source_domain is not persisted today and is read-derived from event_type; since OpenCode emits the same canonical event names as claude-code, a domain pack alone cannot distinguish them. Outcome (AC-03/AC-06 stored-record attribution) is unchanged from SCOPE — only the mechanism deepened. Aligns with and strengthens the integrity goal ("every entry attributed"). |
| Addition (RESOLVED) | Write-time source_domain stamp — opencode-only | DECIDED 2026-09-15 (human ruling): ADR-001 stamps source_domain at write ONLY for `provider == "opencode"`; `claude-code`/`gemini-cli`/`codex-cli` rows stay NULL and read back via today's read-derived resolution — existing behavior preserved byte-for-byte, zero T-SEC-12/13 change. Generalizing to all harnesses is a different outcome deferred to its own cycle (integrity #5681 / C11). OQ-2 / R-05 closed. |
| Simplification | background.rs / listener.rs carve deferred | ADR-005: no ad-hoc mid-delivery monolith split; new-module-with-thin-wiring + a scheduled decomposition issue. Rationale documented; consistent with PL-10. |

## Variances Requiring Approval

No FAIL or hard VARIANCE. The one WARN-level item has been **RESOLVED by human ruling (2026-09-15)**:

1. **What**: ADR-001's write-time source_domain stamp could have generalized attribution semantics to existing gemini-cli/codex-cli rows.
2. **Why it mattered**: Touches Knowledge Integrity (#5681) — an unrequested behavior change to shipped observation attribution, with T-SEC-12/13 exposure.
3. **RESOLUTION (DECIDED — opencode-only)**: The write-time stamp applies only to `provider == "opencode"`; all other providers stay NULL and read back via today's read-derived resolution — existing behavior preserved byte-for-byte, zero T-SEC-12/13 change. Generalization is a different outcome that would earn its own cycle with a T-SEC re-baseline and an explicit intended-semantic-change assertion. OQ-2 / R-05 closed; no open variance remains.

## Detailed Findings

### Vision Alignment
The `personal-cloud` goal (#5731) names C18 directly: "OpenCode (C18) is the prioritized next client — a plugin-architecture harness fronting many backend models, chosen as the vehicle for bringing local models into the dev workflow." SCOPE's Goal 4 (local-model distinguishability via per-model source_domain) is the direct expression of that North-star clause. The feature also advances `domain-agnostic` (#5732: multi-LLM harness support tracking demand — OpenCode is the fourth harness). The AC-06 raison d'être (distinguish ollama/qwen3 from cloud) is the exact local-model on-ramp the vision cites.

Attribution-at-ingest (ADR-001/002) aligns with — and strengthens — Knowledge Integrity (#5681, "every entry attributed"): persisting source_domain/model_id at write is more faithful than lossy read-time derivation, and it is backward compatible (legacy NULL rows keep the registry/DEFAULT read path; migration is additive, honoring Architectural Principle 2). Graceful degradation (Principle 5) is honored strongly: ADR-009 makes PreCompact's experimental leg and bus-derived Stop/SessionStart fail-safe and documented, and NFR-02 preserves transport fail-open. No architectural principle is contradicted (hash chain, in-memory hot path, single binary, no-secrets are unaffected; capability-check seam is preserved via AC-07, not undermined).

### Milestone Fit
C18 is the intended capability and the feature targets it precisely. AC-07 builds the E1 trusted-identity seam viability only — no enforcement, no PL-4 fail-closed flip, no MCP-proxy shim. The `trusted-identity` goal (#5734) confirms this is correct: it names opencode/C18 as "the first real demand-pull consumer" and defers enforcement to future discovery under #5734. Keeping the `external_identity` seam viable without building on it is the disciplined choice — it avoids building future-milestone capability while not foreclosing it (SR-12 guards against a ceremonial N=1 seam). The C18 ledger guardrail (C18 stays `partial` until AC-06 local-vs-cloud distinctness is behaviorally demonstrated end-path, regardless of E2 in-cycle vs fast-follow) is honest milestone accounting that removes a defer-and-declare-victory incentive.

### Architecture Review
ADR-001..009 are internally consistent and each traces to a scope risk. The load-bearing correction — code-line re-measurement (tests/comments excluded) overturning SCOPE's raw-line carve premise, showing background.rs's observations INSERT is inside its test module and production write lives in listener.rs — is sound and correctly reframes the ADR-005 modularity decision (background.rs not carved; listener.rs edited via thin wiring + scheduled decomposition). The three-coupled-touchpoints pattern (#5737: provider arm both normalizers + explicit source_domain resolution + model carrier) is honored in C2/C3/C7/C4 with parity-corpus coverage (C8). ADR-003 (E2 in-cycle) is within SCOPE's granted architect latitude ("architect-sized — IN by default") and rides the same migration/plumbing as AC-03, which is efficient rather than scope-expanding.

### Specification Review
FRs are testable and AC-traced. The Ubiquitous Language section correctly separates provider (harness identity, always "opencode") from source_domain (attribution) from model-id (backend model) — the distinction the whole feature rests on. AC-06 is specified as a behavioral end-path assertion (plugin→wire→attribution→store→query with a negative assertion and anti-tautology guard), matching SR-01/SR-10 and the anti-proxy lessons. NOT-in-scope exclusions match SCOPE exactly (SubagentStart injection, full E1, command-hook mirror, whole-module carve, active-governance). No requirement exceeds SCOPE.

### Risk Strategy Review
Every scope risk SR-01..12 maps to an architecture risk R-01..17 with a resolution and coverage requirement (Scope Risk Traceability table). The behavioral-outcome coverage section binds each SCOPE-lens entry point to a scenario that must drive the real user entry point, not a seam — directly enforcing the anti-tautology/path-divergence posture the SCOPE-RISK-ASSESSMENT demanded. The AC-06 end-path test is correctly designated the C18 `proven` gate. R-05 (existing-provider generalization) is present and routed, which is what surfaces the one WARN above. Coverage is complete for the stated scope.

## Knowledge Stewardship
- Queried: /uni-query-patterns (context_search topic vision) for vision-alignment patterns -- nearest hits were config-key semantic divergence (#2298) and architecture-diagram/spec divergence (#3337/#3337); no prior recurring pattern specific to harness-onboarding mechanism reframes.
- Stored: nothing novel to store -- the "architecture reframes a SCOPE-stated attribution mechanism to ingest-persistence and generalizes beyond the target harness" observation is N=1 (vnc-049 only) and does not yet generalize; the three-coupled-touchpoints harness-onboarding pattern is already captured as #5737. Re-evaluate for storage if a second harness-onboarding feature repeats the reframe-and-generalize shape.
