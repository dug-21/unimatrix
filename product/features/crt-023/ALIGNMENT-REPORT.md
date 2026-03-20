# Alignment Report: crt-023

> Reviewed: 2026-03-20
> Artifacts reviewed:
>   - product/features/crt-023/architecture/ARCHITECTURE.md
>   - product/features/crt-023/specification/SPECIFICATION.md
>   - product/features/crt-023/RISK-TEST-STRATEGY.md
>   - product/features/crt-023/architecture/ADR-001 through ADR-007
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-023/SCOPE.md
> Scope risk: product/features/crt-023/SCOPE-RISK-ASSESSMENT.md
>
> **Revision note (2026-03-20)**: VARIANCE 1 resolved. ADR-007 added, FR-22b and AC-25 added
> to SPECIFICATION.md, ARCHITECTURE.md updated. All VARIANCE items closed. Final status: PASS.

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | All W1-4 vision requirements addressed |
| Milestone Fit | PASS | Correctly targets W1 Intelligence Foundation; no premature W2/W3 scope |
| Scope Gaps | PASS | VARIANCE 1 resolved: `nli_auto_quarantine_threshold` added (FR-22b, AC-25, ADR-007, entry #2716) |
| Scope Additions | WARN | Three additions beyond SCOPE.md: `nli_post_store_k` config field (D-04), `nli_model_name` string selector (D-03 resolution), `wait_for_nli_ready` method in EvalServiceLayer (ADR-006) |
| Architecture Consistency | PASS | All ADRs resolve open questions; consistent with existing patterns |
| Risk Completeness | PASS | 22 risks catalogued; all SCOPE-RISK-ASSESSMENT.md items traced |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | NLI-derived auto-quarantine higher-threshold requirement | SCOPE.md §Constraints-5: "NLI-derived auto-quarantine requires a higher confidence threshold than the existing manual-correction path." The spec (FR-22, AC-13) addresses the edge-creation rate cap but does not define a separate auto-quarantine confidence threshold for NLI-derived edges vs manual-correction edges. The vision (W1-4) also states this requirement. No FR, AC, or ADR specifies the threshold differential. |
| Addition | `nli_post_store_k` config field (D-04) | Not in SCOPE.md §Goals-8 field list (which lists `max_contradicts_per_tick` as the last field). Introduced as a resolved decision D-04 to decouple post-store neighbor count from search re-ranking candidate count. Rationale is documented in SCOPE.md §Resolved Decisions and carried through spec (FR-11, AC-19). Low risk; justified. |
| Addition | `nli_model_name: Option<String>` config field (D-03) | SCOPE.md §Goals-8 does not list `nli_model_name`; it lists `nli_model_path`. D-03 adds config-string model selection. Justified by the 3-profile eval requirement and the need to swap models without code changes. Documented in SCOPE.md §Resolved Decisions D-03 and carried through spec (FR-05, FR-11, AC-21). |
| Addition | `EvalServiceLayer::wait_for_nli_ready()` method (ADR-006) | Not in SCOPE.md scope. Introduced as a consequence of ADR-006's skip-not-fail behavior for eval CI environments. Implemented inside `EvalServiceLayer`, not a new MCP tool or public API. Low risk; required to fulfil NFR-12. |
| Simplification | SCOPE.md §Non-Goals: "automated CI gate for eval results" | Spec (FR-26 to FR-29) faithfully implements human-reviewed gate. No automation added. |

---

## Variances Requiring Approval

### VARIANCE 1 — NLI-derived auto-quarantine threshold not specified

1. **What**: SCOPE.md §Constraints-5 states: "NLI-derived auto-quarantine should require a higher confidence threshold than the existing manual-correction path." The product vision (W1-4) echoes this: "NLI-derived auto-quarantine should require a higher confidence threshold." Neither the SPECIFICATION.md nor any ADR defines this differential threshold. The spec defines `nli_contradiction_threshold` (default 0.6) as the edge-creation gate, and the circuit breaker (`max_contradicts_per_tick`) as the volume rate-limiter, but is silent on the auto-quarantine trigger threshold for NLI-origin edges versus manually-corrected entries.

2. **Why it matters**: The auto-quarantine feedback loop (NLI creates Contradicts edges → topology penalty → auto-quarantine threshold crossed) is the highest-impact failure mode for search quality (R-10 in the risk strategy). The vision and scope intended a second line of defence at the auto-quarantine step — NLI edges should have a higher bar to trigger quarantine than a human correction would. Without this differential, a miscalibrated NLI batch writing edges at the circuit-breaker cap (10 edges) has the same quarantine triggering potential as a deliberate human action.

3. **Recommendation**: The spec writer or architect should either (a) add an explicit `nli_auto_quarantine_threshold` config field (with a value meaningfully higher than the existing auto-quarantine threshold for manual corrections), or (b) explicitly document in the specification that the threshold differential is deferred to a follow-on feature and describe what mitigates the risk in the interim (circuit breaker + existing auto-quarantine hold-on-error pattern from entry #1544 are the current safeguards). Either outcome requires a human decision. This cannot be left implicitly uncovered given that both the vision and scope documents state the requirement.

---

## Detailed Findings

### Vision Alignment

The seven W1-4 vision requirements enumerated in the spawn prompt are evaluated individually.

**1. NLI model integrity via SHA-256 hash pinning (Critical security requirement)**

PASS. Vision states: "[Critical] ONNX model integrity must be verified at load time via SHA-256 hash pinned in config — a replaced model file is an undetectable model-poisoning attack vector. Hash mismatch transitions `NliServiceHandle` to `Failed` state."

Addressed fully. ARCHITECTURE.md §Component 2: "SHA-256 hash verification before `Session::builder().commit_from_file()` (ADR-003)." SPECIFICATION.md FR-09 and NFR-09 specify the requirement precisely. AC-06 covers verification: log must contain "security" and "hash mismatch", server continues on fallback. ADR-003 documents the per-config-file hash binding decision and the implementation order (hash check before session construction). R-05 in the risk strategy identifies production deployments omitting the hash as Critical risk and mandates a warning log even when `nli_model_sha256 = None`.

One nuance: `nli_model_sha256` is `Option<String>` with no default value, meaning production deployments can omit it. The risk strategy (R-05, scenario 1) requires a `tracing::warn!` when the field is absent, which is stronger than the SCOPE.md requirement. This is a proportionate addition.

**2. Post-store detection must be fire-and-forget (not on MCP hot path)**

PASS. Vision: "post-store, fire-and-forget — not on the MCP hot path."

Addressed at every level. SCOPE.md §Goals-4 specifies fire-and-forget. ARCHITECTURE.md §Component 4: "fire-and-forget a tokio task." SPECIFICATION.md FR-18: "must spawn a fire-and-forget tokio task (not blocking the MCP response)." NFR-02: "Post-store NLI detection runs fire-and-forget and must not add latency to the `context_store` MCP response." ADR-004 documents the embedding handoff contract for the move into the spawned task. No deviation from vision intent detected.

**3. Bootstrap edge promotion path (W1-1 bootstrap edges)**

PASS. Vision: "Processes any `bootstrap_only=1` Contradicts edges from W1-1. Confirmed → DELETE+INSERT with `source='nli'`, `bootstrap_only=0`. Refuted → DELETE only. W1-1 shipped zero such rows; the path is implemented as a future-proof first-tick background task."

Addressed fully. ARCHITECTURE.md §Component 5 and §Component Interactions specify the exact logic. SPECIFICATION.md FR-23, FR-24, FR-25 cover the promotion task, the durable idempotency marker, and the NLI-readiness deferral respectively. ADR-005 resolves SR-07 with the `COUNTERS` table `bootstrap_nli_promotion_done` key, including transaction scope and zero-row handling. The architecture correctly notes that W1-1 produced zero bootstrap rows (AC-08 was unresolved in crt-021) and handles both zero-row and non-zero-row cases.

**4. Circuit breaker on NLI → auto-quarantine feedback loop**

WARN (see VARIANCE 1 above). Vision: "This feedback loop must have a rate limit: cap the number of `Contradicts` edges created per tick. NLI-derived auto-quarantine should require a higher confidence threshold than the existing manual-correction path."

The rate-limit half is fully addressed: `max_contradicts_per_tick` config field, AC-13, FR-22, R-09, R-10. The per-call versus per-tick semantic ambiguity (SR-06) is resolved in the spec (FR-22, AC-23). However, the second half — the higher auto-quarantine confidence threshold for NLI-derived edges — is not carried into any FR, AC, NFR, or ADR. This is the gap identified in VARIANCE 1.

**5. Graceful degradation when model absent/hash-invalid**

PASS. Vision: "If the NLI model file is absent, hash-invalid, or fails to load, the server starts successfully and falls back to the cosine-similarity heuristic with a logged warning."

Addressed comprehensively. ARCHITECTURE.md §System Overview states: "NLI absence, hash mismatch, or load failure never prevents server startup — graceful degradation to cosine similarity is the invariant." SPECIFICATION.md NFR-03: "Server startup must succeed and all MCP tools must be functional regardless of NLI model presence, hash validity, or model loading outcome." AC-14 covers all three degradation conditions. FR-15 covers the search fallback path. SCOPE.md Constraint 4: "NLI absence must not prevent server startup" — carried through exactly.

**6. Gate condition: W1-3 eval harness results must demonstrate measurable improvement before production deployment**

PASS. Vision: "Gate condition: W1-3 eval harness results show measurable improvement on a representative query set." Also: "No model ships without eval results."

Addressed precisely. SPECIFICATION.md FR-26 to FR-29 define the eval gate mechanism. AC-09 specifies the gate condition (aggregate P@K or MRR for candidate >= baseline, zero-regression section empty or all regressions approved). AC-18 covers the `EvalServiceLayer` stub fill-in. FR-29 documents the waiver condition (D-01: zero query history) and requires AC-01 to pass regardless of waiver. ADR-006 resolves the missing-model behavior for eval runs. SCOPE.md §Constraints-9: "Eval gate is blocking" is faithfully carried through as a non-negotiable constraint in the spec.

**7. NLI confidence scores stored in metadata column for W3-1 GNN edge features**

PASS. Vision: "NLI confidence score stored in `metadata` column for W3-1 GNN edge features."

Addressed at all levels. SCOPE.md §Goals-4: "NLI confidence stored in `metadata` column for W3-1." ARCHITECTURE.md §GRAPH_EDGES Write Path: `metadata = '{"nli_entailment": <f32>, "nli_contradiction": <f32>}'`. SPECIFICATION.md FR-19 and AC-11 specify the metadata JSON format precisely. SPECIFICATION.md §Domain Models `NliEdge` write contract names both keys. The metadata is stored for both post-store detection and bootstrap promotion writes. This unlocks the W3-1 dependency explicitly noted in SCOPE.md §Non-Goals.

---

### Milestone Fit

PASS. crt-023 is correctly scoped to Wave 1 — Intelligence Foundation. It depends on W1-1 (GRAPH_EDGES), W1-2 (RayonPool), and W1-3 (eval harness), all of which are marked COMPLETE in PRODUCT-VISION.md. It delivers search re-ranking and semantic contradiction detection, which are squarely W1 capabilities.

No W2 or W3 capabilities are introduced. The feature explicitly defers:
- W2-4 (GGUF separate rayon pool) — noted as a Non-Goal in both SCOPE.md and SPECIFICATION.md
- W3-1 (GNN training) — crt-023 produces the input features W3-1 needs but does not train the GNN
- New `unimatrix-onnx` crate — deferred to before W3-1 per crt-022a consultation

The `NliDebertaV3Small` enum variant is implemented unconditionally (ADR-003) as a future-proof hook. This is a minor forward-looking addition, but it is a code scaffold (~10 lines), not a feature expansion, and is explicitly noted as having unconfirmed ONNX availability. This does not constitute a milestone violation.

---

### Architecture Review

PASS. The architecture is coherent and well-reasoned.

**Pattern consistency**: `NliProvider` mirrors `OnnxProvider` (Mutex<Session> + Tokenizer, ADR-001). `NliServiceHandle` mirrors `EmbedServiceHandle` (Loading→Ready|Failed→Retrying state machine). This consistency reduces implementation risk and cognitive overhead.

**SR-02/SR-03 resolution (ADR-001)**: The pool floor raise to 6 when NLI is enabled, combined with `spawn_with_timeout(MCP_HANDLER_TIMEOUT)` fallback and the `max_contradicts_per_tick` cap on post-store tasks, is a credible mitigation. The ADR quantifies the hold-time analysis (20 pairs × 200ms = 4s worst case) and documents the session-pool alternative that was rejected. This is appropriate architectural rigour for the highest-risk design decision.

**SR-09 resolution (ADR-004)**: The embedding handoff contract is explicit: move after HNSW insert, not before. Four edge cases are documented (duplicate detection, embedding failure, HNSW insert failure, NLI not ready). The adapted/normalized embedding is correctly identified as the right vector for HNSW neighbor queries.

**SR-07 resolution (ADR-005)**: The `COUNTERS` table `bootstrap_nli_promotion_done` key approach is correct and idempotent. The transaction scope (marker in same transaction as last batch) is specified. The zero-row case completes and sets the marker.

**SR-06 resolution (FR-22)**: The `max_contradicts_per_tick` per-call semantic is resolved and documented. The config field name is retained for compatibility; implementation comments must note the per-call meaning (AC-23).

**One architecture note**: ARCHITECTURE.md §Component 3 (search pipeline) specifies the status penalty step as applying "before NLI scoring" with entries receiving a `score = base_score * status_penalty` multiplier. ADR-002 states: "Status penalty is applied as a post-NLI multiplicative modifier only if the search pipeline requires it" — this wording is ambiguous and in mild tension with the pipeline diagram in ARCHITECTURE.md which shows penalty applied before NLI scoring. SPECIFICATION.md FR-14 shows the pipeline with status filter/penalty before NLI scoring, consistent with ARCHITECTURE.md. ADR-002's text says "before NLI scoring (so entries with severe penalties may have their NLI scores depressed by a multiplicative factor before the sort)." Risk R-17 specifically flags this as a test requirement. This is internally consistent but the ADR wording ("post-NLI multiplicative modifier only if the search pipeline requires it") may confuse implementers. This is a documentation clarity issue, not a functional gap.

---

### Specification Review

PASS. The specification is thorough, well-structured, and faithfully translates SCOPE.md into implementable FRs and ACs.

**AC count**: 24 ACs (18 from SCOPE.md, plus AC-19 through AC-24 covering D-01 through D-04 and SR-06). Each resolved decision in SCOPE.md has a corresponding AC. This is good practice.

**SR-04 addressed**: SCOPE-RISK-ASSESSMENT.md SR-04 flagged that the eval gate waiver needs a minimum evidence bar. FR-29 and AC-22 address this: the waiver requires AC-01 (NLI inference call demonstrated by test suite) to pass regardless of waiver. This directly addresses the scope risk recommendation.

**SR-06 addressed**: The `max_contradicts_per_tick` per-call vs per-tick ambiguity is resolved in FR-22 and AC-23, with an explicit note that the config field name is retained for compatibility. This directly addresses the scope risk recommendation.

**Constraints faithfully carried**: All 10 SCOPE.md constraints are present in the specification constraints section with no omissions or weakening.

**Non-Goals faithfully carried**: All 15 SCOPE.md Non-Goals are reflected in SPECIFICATION.md §NOT in Scope. No scope creep detected in the specification.

**Gap noted**: The NLI-derived auto-quarantine threshold differential (SCOPE.md §Constraints-5, second sentence) does not appear in any FR, AC, or NFR. See VARIANCE 1.

---

### Risk Strategy Review

PASS. The risk strategy is the most thorough of the three source documents.

**Coverage**: 22 risks across Critical (5), High (12), Medium (5), Low (0). All 9 SCOPE-RISK-ASSESSMENT.md items are traced in the §Scope Risk Traceability table, with explicit Architecture Risk mappings and resolution notes.

**Non-negotiable tests identified**: Six tests explicitly called out as blocking for feature ship: R-01 (pool saturation load test), R-03 (stable sort tie-breaking), R-05 (hash mismatch → Failed + security log), R-09 (cap across both edge types), R-10 (miscalibration cascade end-to-end), R-13 (mutex poison at get_provider() boundary). These align with the highest-severity vision security requirements.

**R-05 extends beyond SCOPE.md**: The risk strategy adds a scenario (R-05, scenario 1) requiring a `tracing::warn!` when `nli_model_sha256` is absent. SCOPE.md and the vision only require the hash mismatch path to log "security"/"hash mismatch". The warn-on-absent requirement is a beneficial addition that helps operators discover misconfigured production deployments. It is consistent with the spirit of the [Critical] security requirement.

**R-20 (bootstrap vs post-store conflict)**: Correctly identifies the `INSERT OR IGNORE` idempotency gap where a post-store NLI write for a bootstrap edge silently ignores itself. The risk strategy correctly requires the intended resolution to be explicitly documented (either the post-store task handles it, or bootstrap promotion is guaranteed to run first). This is a valid implementation-time concern.

**Integration risks**: The §Integration Risks section captures the write pool contention interaction, the EvalServiceLayer 60s wait impact on existing tests, and the GRAPH_EDGES write path race at startup. These are implementation-level concerns that are appropriately documented.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found entries #2298 (config key semantic divergence pattern) and #2063 (single-file topology vs vision language). Neither is directly applicable to crt-023; crt-023's alignment is strong.
- Stored: nothing novel to store — the VARIANCE found (NLI auto-quarantine threshold gap) is crt-023-specific and not a generalizable misalignment pattern. The gap arises from a SCOPE.md §Constraints sentence that was partially carried (rate-limit half) but not fully carried (threshold-differential half). This is not the same as the config semantic divergence pattern or the scope-gap pattern in the stored entries. If this same pattern (partial constraint carry-through) appears in future features, it would warrant storing.
