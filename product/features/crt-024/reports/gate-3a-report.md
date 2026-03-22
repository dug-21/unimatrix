# Gate 3a Report: crt-024

> Gate: 3a (Component Design Review)
> Date: 2026-03-21
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | Component boundaries, interfaces, and ADRs match exactly |
| Specification coverage | PASS | All FRs, NFRs, and ACs have pseudocode coverage; no scope additions |
| Risk coverage | PASS | All 17 risks (R-01 through R-16, R-NEW) map to named test scenarios |
| Interface consistency | PASS | Shared types in OVERVIEW.md used consistently across all pseudocode files |
| Knowledge stewardship compliance | PASS | All active-storage agents have Stored/Declined; pseudocode agents have Queried |

---

## Detailed Findings

### Architecture Alignment

**Status**: PASS

**Evidence**:

1. **Component boundaries**: ARCHITECTURE.md names two components that change — `SearchService` (`search.rs`) and `InferenceConfig` (`infra/config.rs`). Pseudocode is decomposed exactly as `inference-config.md` (Wave 1) and three Wave 2 files (`score-structs.md`, `compute-fused-score.md`, `search-service.md`). OVERVIEW.md explicitly maps components to files.

2. **Formula fidelity**: The canonical six-term formula in ARCHITECTURE.md §The Canonical Fused Scoring Formula is reproduced identically in `compute-fused-score.md`. The function body is exactly: `w_sim*sim + w_nli*nli + w_conf*conf + w_coac*coac_norm + w_util*util_norm + w_prov*prov_norm`. Status penalty applied outside, matching ADR-004.

3. **ADR adherence**:
   - ADR-001 (six-term formula): pseudocode uses six terms throughout — confirmed.
   - ADR-002 (`apply_nli_sort` removal): `search-service.md` deletes the function, changes `try_nli_rerank` to return `Option<Vec<NliScores>>` — confirmed.
   - ADR-003 (default weights): `score-structs.md` and `inference-config.md` both specify w_nli=0.35, w_sim=0.25, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05, sum=0.95 — confirmed.
   - ADR-004 (formula as pure function): `compute_fused_score` is defined as `pub(crate) fn` with no side effects, no async, no I/O — confirmed.

4. **Integration surface**: All interface points from ARCHITECTURE.md §Integration Surface are reflected. `MAX_CO_ACCESS_BOOST` is imported from `unimatrix_engine::coaccess`, not duplicated. `rerank_score` retained. `NliScores.entailment` cast from f32 to f64 at call site.

5. **Pipeline step ordering**: ARCHITECTURE.md §Data Flow specifies Step 6c boost_map prefetch before Step 7 NLI scoring. `search-service.md` implements this exactly, with `await` on boost_map before NLI scoring begins, and both resolved before the scoring loop.

6. **BriefingService out of scope**: `OVERVIEW.md` §What Is NOT Changed lists `BriefingService` and `MAX_BRIEFING_CO_ACCESS_BOOST` as explicitly untouched — matches ARCHITECTURE.md §System Overview.

7. **No new dependencies**: Pseudocode adds only an import (`MAX_CO_ACCESS_BOOST`), no new crate dependencies — consistent with ARCHITECTURE.md §Technology Decisions.

8. **EvalServiceLayer wiring**: `search-service.md` §`SearchService::new` Change provides two implementation options (A and B) and mandates FR-14/AC-15 compliance — consistent with ARCHITECTURE.md §EvalServiceLayer Integration.

**No deviations found.**

---

### Specification Coverage

**Status**: PASS

**Evidence**:

Coverage of all functional requirements:

| FR | Requirement | Pseudocode Coverage |
|----|-------------|---------------------|
| FR-01 | Six-term fused formula | `compute-fused-score.md` — exact formula body |
| FR-02 | Six weight fields in InferenceConfig | `inference-config.md` — six fields with serde(default) |
| FR-03 | Weight validation at startup | `inference-config.md` — per-field [0,1] + sum≤1.0 checks |
| FR-04 | co-access normalization, no constant duplication | `score-structs.md` + `search-service.md` — import only |
| FR-05 | Utility delta shift-and-scale | `score-structs.md` §Normalization Helper Notes; `search-service.md` scoring loop |
| FR-06 | Provenance boost normalization with guard | `search-service.md` scoring loop; guard: `if PROVENANCE_BOOST == 0.0 { 0.0 }` |
| FR-07 | NLI absence re-normalization, five-weight denominator | `score-structs.md` §`FusionWeights::effective` |
| FR-08 | Single pipeline pass, step order | `search-service.md` §Pipeline step ordering |
| FR-09 | WA-2 extension contract | `OVERVIEW.md` + `score-structs.md` + `compute-fused-score.md` — WA-2 extension comments on every extensible struct/function |
| FR-10 | ScoredEntry.final_score semantics | `search-service.md` §Step 11 |
| FR-11 | apply_nli_sort disposition | `search-service.md` §apply_nli_sort Removal — removed, tests migrated |
| FR-12 | rerank_score retention | `search-service.md` §Fallback Path — explicitly retained |
| FR-13 | BriefingService untouched | `OVERVIEW.md` §What Is NOT Changed |
| FR-14 | EvalServiceLayer config wiring | `search-service.md` §SearchService::new Change |

Coverage of non-functional requirements:

| NFR | Requirement | Pseudocode Coverage |
|-----|-------------|---------------------|
| NFR-01 | No latency regression | Single pass replaces two sort passes — documented in `search-service.md` |
| NFR-02 | Score range [0,1] | Shift-and-scale + PROVENANCE_BOOST guard + weight sum validation enforce the range by construction |
| NFR-03 | Determinism | `search-service.md` §Sort — stable sort documented; `compute-fused-score.md` §Error Handling notes determinism |
| NFR-04 | No engine crate changes | `OVERVIEW.md` §What Is NOT Changed |
| NFR-05 | Config backward compatibility | `inference-config.md` — `#[serde(default)]` on all six fields; Default impl provided |

**No scope additions detected.** Pseudocode does not implement any feature outside the six signals, the two changed files, and the pipeline rewrite. WA-2 is referenced only as a documented extension point with `w_phase = 0.0` default — not implemented.

**One structural note (WARN-level, not FAIL)**: AC-16 (eval harness gate run) is a process gate, not a code deliverable. The pseudocode correctly does not attempt to represent it. The test plan OVERVIEW.md references it as a Stage 3c responsibility. This is correct.

---

### Risk Coverage

**Status**: PASS

**Evidence**:

All risks from RISK-TEST-STRATEGY.md map to named test scenarios in the test plans:

| Risk | Priority | Test Plan Coverage | Verdict |
|------|----------|--------------------|---------|
| R-01 (util_norm shift-and-scale) | Critical | `compute-fused-score.md`: 3 boundary tests + non-negative score test | PASS |
| R-02 (zero-denominator guard) | High | `score-structs.md`: zero-denom guard, single-nonzero, complement test | PASS |
| R-03 (PROVENANCE_BOOST guard) | High | `compute-fused-score.md`: 3 prov_norm tests + is_finite property test | PASS |
| R-04 (regression test churn) | Critical | `search-service.md`: pre-merge audit + per-test update requirement + AC-11 named test | PASS |
| R-05 (apply_nli_sort migration) | Critical | `search-service.md`: 5 named successor tests, one-to-one mapping table | PASS |
| R-06 (W3-1 training signal) | High | `compute-fused-score.md`: AC-11, Constraint 9, Constraint 10 as named unit tests | PASS |
| R-07 (boost_map sequencing) | High | `search-service.md`: integration test in `test_lifecycle.py`; code review gate | PASS |
| R-08 (MAX_CO_ACCESS_BOOST duplication) | High | `search-service.md`: coac_norm boundary test using imported constant; grep gate | PASS |
| R-09 (spurious re-normalization in NLI-enabled path) | High | `score-structs.md`: NLI-active unchanged test + headroom preserved test | PASS |
| R-10 (try_nli_rerank return type) | High | `search-service.md`: compile gate + 3 retained fallback tests + new success test | PASS |
| R-11 (util_delta negative range) | High | `compute-fused-score.md`: Ineffective entry util_norm=0.0; fused_score >= 0.0 | PASS |
| R-12 (weight validation bypass) | Med | `inference-config.md`: direct InferenceConfig construction tests | PASS |
| R-13 (config backward compat) | Med | `inference-config.md`: partial TOML test + missing-fields default test | PASS |
| R-14 (status_penalty inside formula) | Med | `compute-fused-score.md`: compile-time struct shape + penalty-as-multiplier test | PASS |
| R-15 (NliScores index alignment) | Med | `search-service.md`: deliberate alignment test (low-sim/high-NLI vs high-sim/low-NLI) | PASS |
| R-16 (struct extensibility) | Low | `score-structs.md`: named-field compilation test | PASS |
| R-NEW (EvalServiceLayer wiring) | High | `search-service.md`: 3 tests (sim-only profile, default-weights profile, differential) | PASS |

Integration risks from RISK-TEST-STRATEGY.md §Integration Risks:

| IR | Coverage |
|----|---------|
| IR-01 (boost_map before NLI scoring) | Code review gate + test_lifecycle.py integration test |
| IR-02 (FusedScoreInputs shared reference) | Architecture note: boost_map read-only; covered by R-07 test |
| IR-03 (BriefingService isolation) | grep audit gate in `search-service.md` §IR-03 |
| IR-04 (rerank_score callable) | cargo check gate + FR-12 retention |

Security risks from RISK-TEST-STRATEGY.md §Security Risks:

| SeR | Coverage |
|-----|---------|
| SeR-01 (untrusted weight config) | `inference-config.md` validation tests — reject at startup, structured error |
| SeR-02 (NaN propagation) | PROVENANCE_BOOST guard + is_finite property test in `compute-fused-score.md` |
| SeR-03 (weight sum bypass on reload) | Noted as future risk only; no hot-reload in current architecture |

**All risks have at least one named test scenario. Critical risks have multiple tests each.**

---

### Interface Consistency

**Status**: PASS

**Evidence**:

1. **`FusedScoreInputs`**: Defined in `OVERVIEW.md` with six fields (similarity, nli_entailment, confidence, coac_norm, util_norm, prov_norm). Used in `score-structs.md`, `compute-fused-score.md` (function parameter), and `search-service.md` (construction in scoring loop). Field names match exactly across all files.

2. **`FusionWeights`**: Defined in `OVERVIEW.md` with six weight fields plus `effective(nli_available: bool) -> FusionWeights` method. Implemented in `score-structs.md`. Called in `compute-fused-score.md` (parameter type) and `search-service.md` (call site `self.fusion_weights.effective(nli_available)`). Consistent across all files.

3. **`compute_fused_score` signature**: Declared in `compute-fused-score.md` as `pub(crate) fn compute_fused_score(inputs: &FusedScoreInputs, weights: &FusionWeights) -> f64`. Called in `search-service.md` as `compute_fused_score(&inputs, &effective_weights)`. Types match.

4. **Default values**: `OVERVIEW.md` §`FusionWeights` defaults, `inference-config.md` default functions, and `score-structs.md` struct comments all agree: w_sim=0.25, w_nli=0.35, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05. No divergence.

5. **Normalization formulas**: `OVERVIEW.md` §`FusedScoreInputs` normalization table, `score-structs.md` §Normalization Helper Notes, and `search-service.md` scoring loop all specify identical formulas:
   - `coac_norm = raw / MAX_CO_ACCESS_BOOST`
   - `util_norm = (raw_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)`
   - `prov_norm = if PROVENANCE_BOOST == 0.0 { 0.0 } else { raw_prov / PROVENANCE_BOOST }`

6. **`try_nli_rerank` signature change**: `search-service.md` changes the return type to `Option<Vec<NliScores>>` and removes `penalty_map` and `top_k` parameters. The OVERVIEW.md §Sequencing Constraints acknowledges ADR-002. No conflict between pseudocode files.

7. **`InferenceConfig` → `FusionWeights` data flow**: `OVERVIEW.md` §Data Flow shows `InferenceConfig.{w_*}` feeding into `SearchService.fusion_weights`. `inference-config.md` adds the fields. `score-structs.md` defines `FusionWeights::from_config(cfg: &InferenceConfig)`. `search-service.md` stores the `fusion_weights` field and documents construction. Chain is complete and consistent.

**No contradictions between pseudocode files found.**

---

### Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

Active-storage agents (architect, risk-strategist):

| Agent | Report | Stewardship Block | Content |
|-------|--------|-------------------|---------|
| crt-024-agent-1-architect | `agents/crt-024-agent-1-architect-report.md` | Present (implicit via ADRs Stored section) | Stored: 4 ADRs (#2969–#2972) in Unimatrix |
| crt-024-agent-3-risk | `agents/crt-024-agent-3-risk-report.md` | Present | `Queried:` 4 searches documented. `Stored: nothing novel to store` with reason: patterns first-observed here, will store after second feature |

Read-only agents (pseudocode, spec, test-plan):

| Agent | Report | Stewardship Block | Content |
|-------|--------|-------------------|---------|
| crt-024-agent-2-spec | `agents/crt-024-agent-2-spec-report.md` | Present | `Queried:` documented with 6 specific entries found |
| crt-024-agent-1-pseudocode | `agents/crt-024-agent-1-pseudocode-report.md` | Present | `Queried:` 2 searches with specific entries listed |
| crt-024-agent-2-testplan | `agents/crt-024-agent-2-testplan-report.md` | Present | `Queried:` 3 searches with specific entries listed. `Stored: nothing novel to store` with reason |

All five design-phase agent reports have `Knowledge Stewardship` sections. The architect report uses "ADRs Stored in Unimatrix" as its store section rather than the canonical `Stored:` label — this is a minor formatting variance but the content (4 entries with IDs) demonstrates compliance. The risk and test-plan agents both provide explicit reasons for not storing ("first-observed here, will store after second feature" and "recommend storing after Stage 3c"). No block is present without a reason.

**One minor variance (WARN-level)**: The architect agent's stewardship block uses "ADRs Stored in Unimatrix" as the section label rather than `## Knowledge Stewardship` with `Stored:` entry format. Content is present and substantive — this is a formatting deviation, not a substantive gap.

---

## Rework Required

None. All checks passed.

---

## Additional Notes

1. **Open question OQ-1 (SearchService::new signature)** is appropriately left for the implementation agent. Both options (A and B) are documented in `search-service.md` with the critical invariant (FR-14 compliance) stated. The implementation agent has clear guidance.

2. **Open question OQ-3 (confidence_weight dead code)** is noted for the implementation agent. Not a design gap — it is a code cleanup consideration that will become visible only when the actual source is modified.

3. **R-04 (regression test churn)** is addressed as a process gate (pre-merge audit) rather than a unit test. This is the correct approach — the count of deleted tests is not verifiable in pseudocode, only at Stage 3b/3c.

4. **NaN handling in NliScores.entailment** (open question OQ-1 in testplan report): the test plan identifies this and names the test (`test_fused_score_nan_nli_defaults_to_zero`). The implementation approach is left open — either cast and check or check before cast. This is an appropriate implementation-time decision. Risk R-05 (apply_nli_sort migration) explicitly covers this behavior.

5. **AC-11 numerical values**: The pseudocode uses sim=0.8 for the AC-11 check (in `compute-fused-score.md`) while the architecture uses sim=0.5 and the test plan uses sim=0.8. The architecture's AC-11 regression test assertion block at the top of ARCHITECTURE.md uses sim=0.5; the AC-11 specification text uses sim=0.8. This is a pre-existing discrepancy in the source documents, not introduced by pseudocode. The mathematical outcome (score_A > score_B) holds under both sim values with default weights. The implementation agent should use the SPECIFICATION.md AC-11 values (sim=0.8) as the authoritative test inputs.

## Knowledge Stewardship

- Queried: none (gate validator role does not query patterns before assessing)
- Stored: nothing novel to store — gate-3a results are feature-specific and belong in this report, not in Unimatrix
