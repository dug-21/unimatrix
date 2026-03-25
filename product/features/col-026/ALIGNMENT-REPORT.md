# Alignment Report: col-026

> Reviewed: 2026-03-25
> Artifacts reviewed:
>   - product/features/col-026/architecture/ARCHITECTURE.md
>   - product/features/col-026/specification/SPECIFICATION.md
>   - product/features/col-026/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> SCOPE source: product/features/col-026/SCOPE.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances the observability/learning-loop pillars of the Collective phase |
| Milestone Fit | PASS | Belongs to Wave 1A / Collective intelligence layer; no future-milestone capabilities introduced |
| Scope Gaps | WARN | Two minor items from SCOPE.md underspecified in source docs (see below) |
| Scope Additions | WARN | One addition in ARCHITECTURE.md not present in SCOPE.md (`pass_number` field on PhaseStats); one minor divergence in knowledge-reuse field naming |
| Architecture Consistency | PASS | Three-layer design is coherent; all SCOPE-RISK-ASSESSMENT recommendations addressed |
| Risk Completeness | PASS | Risk register is comprehensive; all 8 scope risks traced; 13 risks across Critical/High/Med/Low |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | FR-14 threshold audit scope is narrowed in SPECIFICATION | SCOPE goal 6 says "Replace threshold language in **all** findings with baseline framing." SPECIFICATION §FR-14 enumerates only 3 files; ARCHITECTURE.md §Component 5 enumerates 9 specific lines. SCOPE.md does not constrain the audit scope, so the narrowed enumeration could miss sites outside those files. No evidence the enumeration is exhaustive vs. the full `detection/` directory. |
| Gap | SR-04 golden-output snapshot test — verifiable exit criterion | SCOPE-RISK-ASSESSMENT SR-04 recommendation says "Spec must define section ordering as an explicit acceptance criterion with a **golden-output fixture** (or snapshot test) covering the full rendered output." SPECIFICATION AC-11 and NFR-05 address ordering but stop short of a golden-output snapshot fixture with a known expected string. The AC verifies ordering by relative position checks, not a byte-level snapshot. |
| Addition | `pass_number: u32` field on `PhaseStats` struct | ARCHITECTURE.md §Component 2 includes a `pass_number: u32` field (1-indexed, per-row) that does not appear in SPECIFICATION §Domain Models `PhaseStats` definition. SCOPE.md does not mention per-pass tracking at row granularity. The SPECIFICATION defines `pass_breakdown: Vec<(u64, u64)>` for multi-pass data instead. These are divergent representations of the same concept — architecture and spec disagree on the struct shape. |
| Addition | `total_served` vs `total_stored` field naming divergence | SCOPE goal 4 / AC-12 says the knowledge reuse section should show "total entries served" and "total stored". SPECIFICATION FR-13 adds `total_served: u64` to `FeatureKnowledgeReuse`. ARCHITECTURE.md §Component 4 names the analogous field `total_stored: u64` (meaning entries stored during the cycle, not served). These are different quantities with swapped names between documents. |
| Simplification | SR-06 `No phase information captured` detection granularity | SCOPE-RISK-ASSESSMENT SR-06 asks the architect to decide between per-cycle check vs. cross-cycle check. ARCHITECTURE.md §SR-06 resolves this as "simple per-cycle check." This simplification is documented with rationale and is consistent with existing `phase_narrative = None` behavior. Acceptable. |

---

## Variances Requiring Approval

### VARIANCE 1 — PhaseStats struct field mismatch between ARCHITECTURE and SPECIFICATION

**What**: `ARCHITECTURE.md §Component 2` defines `PhaseStats` with a `pass_number: u32` field (1-indexed pass number for this particular row, distinguishing pass 1 from pass 2 of a reworked phase). `SPECIFICATION.md §Domain Models` omits `pass_number` entirely, instead using `pass_breakdown: Vec<(u64, u64)>` for per-pass duration/record data. The architecture also lacks the `pass_breakdown` field. These are two different struct representations for the same concept. Implementation agents will receive conflicting instructions from the two documents.

**Why it matters**: This is a struct-shape disagreement between the two authoritative design documents. An implementation agent following ARCHITECTURE.md will build a different struct than one following SPECIFICATION.md. The mismatch will surface as a compile conflict or a test failure when the formatter accesses a field that does not exist.

**Recommendation**: The spec author must reconcile. Both fields carry useful information:
- `pass_number` per row is needed to label Phase Timeline rows correctly (e.g., `implementation/1`, `implementation/2`).
- `pass_breakdown` is needed for the Rework annotation (FR-07 requires per-pass duration and record counts).
The resolved struct should include both or an equivalent representation. The specification should be the authoritative shape; the implementation brief author must align ARCHITECTURE.md before delivery begins.

---

### VARIANCE 2 — `total_served` / `total_stored` field naming divergence

**What**: SPECIFICATION §FR-13 adds a field `total_served: u64` to `FeatureKnowledgeReuse` (defined as "all distinct entry IDs served across all sessions for this cycle"). ARCHITECTURE.md §Component 4 defines a field `total_stored: u64` (defined as "entries created during this cycle"). These are distinct quantities — one counts what was served to agents, the other counts what agents stored. The names are swapped relative to the concepts in SCOPE.md goals and AC-12. The rendered Knowledge Reuse section format in ARCHITECTURE.md shows `**Stored this cycle**: {total_stored}` and separately shows `**Total served**: {delivery_count}`. The SPECIFICATION shows `**Total served**: {N}  |  **Stored this cycle**: {M}` mapping served→N and stored→M, where N is `total_served` (the new field) and M is `total_stored` (the existing `delivery_count`-adjacent field).

**Why it matters**: The knowledge reuse computation adds one new field in the spec (`total_served`) and a different new field in the architecture (`total_stored`). An implementation agent will add one or both fields incorrectly. If both are added with the architecture's naming, the spec's `total_served` is missing; if named per the spec, the architecture's `total_stored` is missing. The rendered output will either double-count or omit one of the two quantities.

**Recommendation**: Clarify canonical field names before implementation begins. The SPECIFICATION is the authoritative contract; the implementation brief should reference spec field names. Suggested canonical names:
- `total_served: u64` — distinct entry IDs served (new, from spec)
- `total_stored: u64` — entries created during this cycle (new, from architecture)
Both fields are needed and serve different purposes. Both should appear in the spec's `FeatureKnowledgeReuse` definition before delivery starts.

---

## Detailed Findings

### Vision Alignment

col-026 directly advances two of the product vision's stated priorities:

1. **Learning loop / intelligence pipeline**: The product vision states: "The function learns. Every session makes it better." `context_cycle_review` is the primary mechanism by which the system's retrospective analysis reaches agents and humans. A more accurate, goal-aware, phase-structured report means the feedback loop produces higher-quality signal. col-026 connects `cycle_events` goal text (col-025) and time-window attribution (col-024) to the report surface — completing the intended chain from event capture to human-readable outcome.

2. **Attribution and provenance**: The vision states "hash-chained corrections, immutable audit log, trust-attributed provenance — tamper-evident from first write to last." col-026 surfaces the attribution path (which of three fallback paths produced observations) in the report header, making the provenance of retrospective data visible to agents and humans. This is a direct expression of the vision's transparency principle.

3. **Domain agnosticism**: The product vision's Critical Gaps table lists removal of dev-workflow-specific language as a high-priority concern. col-026's AC-13 (removing threshold/allowlist language from detection output) and AC-19 (fixing the compile_cycles recommendation) are concrete steps toward that goal, consistent with the config externalization work in Wave 0.

No vision principles are contradicted.

---

### Milestone Fit

col-026 is a Collective phase (`col-` prefix) feature enhancing the observability and retrospective tooling that supports orchestration workflows. It targets the ongoing Wave 1A / Collective intelligence layer work, specifically building on the shipped foundations of:

- col-024 (cycle_events-first attribution — Wave 1A)
- col-025 (goal signal — Wave 1A)

The feature introduces no Wave 2 deployment capabilities (no container, OAuth, HTTP transport), no Wave 3 ML capabilities (no GNN, no GGUF inference), and no Wave 1 intelligence capabilities (no ranking pipeline changes). It is firmly a Wave 1A observability enhancement. Milestone discipline is clean.

SCOPE.md §Constraints explicitly states: "Schema is at v16 (col-025). No schema migration is required or permitted in col-026." Both ARCHITECTURE.md §System Overview ("No schema migration. No new MCP tools. Schema remains at v16.") and SPECIFICATION.md NFR-06 repeat this constraint. No milestone overreach detected.

---

### Architecture Review

The three-layer decomposition (formatter-only → struct extensions → new types + computation) is a sound incremental design that directly addresses the blast-radius concern raised in SR-04. Key strengths:

- All five SCOPE-RISK-ASSESSMENT recommendations that were addressed to the architect are resolved: SR-01 (ADR-002 mandates `cycle_ts_to_obs_millis()`), SR-02 (ADR-003 mandates batch IN-clause), SR-03 (ADR-001 adopts `Option<bool>`), SR-07 (integration surface pinned), SR-08 (construction sites enumerated).
- The `is_in_progress: Option<bool>` three-state semantic (ADR-001) resolves the historical retro semantic corruption risk correctly.
- The batch IN-clause design for `feature_cycle` lookup (Component 4) correctly prevents the N+1 pattern.
- SR-06 resolution (simple per-cycle check) is documented and reasonable.

One issue requires attention: the `pass_number` field in ARCHITECTURE.md §Component 2 does not appear in SPECIFICATION §Domain Models. This is VARIANCE 1 above.

The integration surface pins for col-024 and col-025 (§Integration Points) are well-specified with exact function signatures, file paths, and line references. This is good practice for in-flight dependencies.

---

### Specification Review

The specification is thorough and traceable. All 19 acceptance criteria (AC-01 through AC-19) map cleanly to SCOPE.md goals:

| SCOPE goal | Spec coverage |
|-----------|--------------|
| Goal 1 — surface goal, cycle type, attribution | FR-01, FR-02, FR-03, FR-04, AC-01–AC-04 |
| Goal 2 — Phase Timeline table | FR-06, FR-07, FR-08, AC-06, AC-07 |
| Goal 3 — per-finding phase annotation | FR-09, AC-08 |
| Goal 4 — fix knowledge reuse metric (GH#320) | FR-13, AC-12 |
| Goal 5 — What Went Well section | FR-11, AC-10 |
| Goal 6 — replace threshold language | FR-14, AC-13 |
| Goal 7 — burst notation for evidence | FR-10, AC-09 |
| Goal 8 — session profile enhancement | FR-15, FR-16, AC-14, AC-15 |
| Goal 9 — in-progress indicator | FR-05, AC-05 |
| Goal 10 — header rebrand | FR-01, AC-01 |
| Goal 11 — permission-friction investigation | FR-19, AC-19 |

No SCOPE goals are unaddressed.

Two observations on specification quality:

1. The metric direction table in ARCHITECTURE.md (§Component 5, What Went Well) lists 10 metrics. SPECIFICATION §FR-11 lists 16 metrics. The spec table is a superset — it adds `context_load_before_first_write_kb`, `file_breadth`, `mutation_spread`, `cold_restart_count`, `task_rework_count`, `edit_bloat_kb`. This expansion beyond the architecture is minor and acceptable (all metrics are already present in the detection system), but the implementation brief should resolve which table is canonical. R-06 in RISK-TEST-STRATEGY.md requires all 16 (spec count) to be tested, so the spec table is clearly intended as authoritative.

2. FR-14 threshold audit scope (3 files in spec vs. 9 lines across ~4 files in architecture) — this is the Gap flagged above. The spec's narrower enumeration may produce an incomplete fix at gate time.

---

### Risk Strategy Review

The risk-test strategy is the strongest of the three source documents. All findings:

- 13 risks identified across 4 priority tiers, which is proportionate for a formatter overhaul of this scope.
- All 8 SCOPE-RISK-ASSESSMENT risks are explicitly traced in the Scope Risk Traceability table with architecture and spec resolutions identified.
- Coverage summary: 22 Critical scenarios, 18 High scenarios, 5 Med scenarios, 2 Low scenarios. Total 47 test scenarios.
- Three failure mode responses are explicitly defined (PhaseStats error, DB error for goal, all paths empty).
- Four security risks are identified and addressed (goal markdown injection, gate_outcome_text injection, entry title table-breaking, IN-clause SQL injection).
- R-10 (hotspot phase annotation uses highest-count phase for multi-phase findings) correctly addresses the ARCHITECTURE.md description ("earliest evidence timestamp" approach) by specifying the highest-count phase as the winner — this is a spec improvement over the architecture and is documented.

One note on R-03 scenario 8: the risk document correctly identifies that naive `contains("pass")` would match the word "compass." The test scenario documents this as a known fragility but does not require a fix. The spec's GateResult derivation (§Domain Models) uses `contains("pass")` — the implementation should consider word-boundary matching. This is not a blocking issue at design time but should be noted in the implementation brief.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found entry #3426 "Formatter overhaul features consistently underestimate section-order regression risk — golden-output test required" (directly relevant, already captured from prior work; confirms the SR-04 gap finding in this report is a known recurring pattern).
- Stored: nothing novel to store — the recurring pattern (scope additions in struct definitions between architecture and spec documents for formatter overhauls) is feature-specific in its detail. The golden-output regression pattern is already captured as #3426. No generalizable new pattern emerged beyond what is already stored.
