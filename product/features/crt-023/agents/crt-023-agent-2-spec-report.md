# Agent Report: crt-023-agent-2-spec

## Output

- SPECIFICATION.md written to: `product/features/crt-023/specification/SPECIFICATION.md`

## Counts

- **Functional Requirements**: 29 (FR-01 through FR-29)
- **Non-Functional Requirements**: 12 (NFR-01 through NFR-12)
- **Acceptance Criteria**: 24 (AC-01 through AC-24; AC-01–AC-18 from SCOPE.md expanded, AC-19–AC-24 new)

## Key Decisions Made

**D-01 (Eval gate waiver)**: Incorporated as FR-29 and AC-22. Added a minimum evidence
bar: test suite AC-01 (at least one successful NLI inference call) must pass regardless
of waiver. Addresses SR-04 from risk assessment.

**D-02 (NLI replaces rerank_score)**: Specified as FR-14 and AC-20. The entailment score
sorts the candidate set; `rerank_score` is retained in code for the fallback path but is
not called when NLI is active. Added a note that the architect must document this as an
ADR (replacement vs blend is a material scoring decision).

**D-03 (Config-driven model selection)**: Added `nli_model_name: Option<String>` to the
config (FR-05, FR-11, AC-21). This field resolves to an `NliModel` enum variant at
startup. `nli_model_path` overrides the cache path when set. Both can coexist. Eval run
requires 3 profiles when deberta ONNX is available; degrades to 2 profiles when not
(FR-27). Unrecognized model name strings fail startup validation (AC-17).

**D-04 (Separate nli_post_store_k)**: FR-11 and AC-19 specify `nli_post_store_k`
(default 10) as a distinct config field from `nli_top_k` (default 20). Each service
reads its own field; they do not share the value.

**SR-06 (max_contradicts_per_tick semantics)**: FR-22 and AC-23 clarify the cap is
per-`context_store` call (one fire-and-forget task invocation), not globally per
background tick. Config field name retained for compatibility with SCOPE.md and product
vision references.

**SR-07 (Bootstrap promotion idempotency)**: FR-24 and AC-24 specify a durable marker
in the `COUNTERS` table (`bootstrap_nli_promotion_done` key) rather than relying on
row absence as a guard.

**SR-04 (Eval gate minimum evidence under waiver)**: AC-22 requires the test suite
AC-01 to pass regardless of waiver. This prevents vacuous gate passage.

## Open Questions for Architect

Five open questions are documented in the specification (OQ-01 through OQ-05):

- **OQ-01**: Pool sizing and `Mutex<Session>` concurrency (highest risk — SR-02, SR-03)
- **OQ-02**: Embedding handoff contract from `context_store` to fire-and-forget task (SR-09)
- **OQ-03**: COUNTERS table key mechanism for bootstrap promotion idempotency (SR-07)
- **OQ-04**: Eval CLI behavior when model file is absent from eval environment (SR-08)
- **OQ-05**: Deberta ONNX availability verification (SR-01)

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "NLI cross-encoder search reranking ONNX provider pattern" -- ADR-001 (entry #67/#19) Mutex<Session> for ONNX inference is the established pattern; ADR-002 lazy-init and lesson #685 degraded-mode requirement confirmed.
- Queried: `/uni-query-patterns` for "circuit breaker contradiction auto-quarantine rate limit edges" -- entries #1544 and #1542 directly applicable to circuit breaker and bootstrap promotion designs.
- Queried: `/uni-query-patterns` for "EmbedServiceHandle state machine loading ready failed retry" -- degraded-mode and lazy-init patterns confirmed; NliServiceHandle is a novel component with no prior pattern.
- No generalizable patterns identified at specification stage. Patterns from architect ADR decisions will be stored after architecture phase.
