# Scope Risk Assessment: crt-023

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `cross-encoder/nli-deberta-v3-small` ONNX export may be unavailable on HuggingFace Hub at implementation time — D-03 3-profile eval degrades to 2-profile | Med | Med | Architect should verify deberta ONNX availability as first implementation step; document fallback in ADR before design is finalised |
| SR-02 | Shared rayon pool saturation: NLI re-ranking (20 pairs × 50–200ms) competing with embedding inference on the same pool (4–8 threads) may push MCP handler latency above acceptable bounds under concurrent load | High | Med | Pool sizing must be an explicit ADR. Evidence: entries #735, #1628, #1688 all trace MCP instability to pool/spawn_blocking contention; NLI adds the first multi-pair batched workload to this pool |
| SR-03 | `Mutex<Session>` serialises NLI inference: re-ranking 20 pairs runs sequentially through one session, not in parallel. At 200ms/pair worst-case, one search call holds the mutex for ~4s, blocking all concurrent NLI requests | High | Med | Architect must decide: accept serialisation (simplest, consistent with ADR-001 entry #67) or introduce a session pool. Document decision as ADR |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | AC-09 eval gate is waived for zero-query-history deployments (D-01). If the only available knowledge base at gate-check time has no query history, the gate produces no evidence — feature ships unvalidated for quality improvement | Med | Med | Spec writer must define a minimum evidence bar for gate waiver: e.g., require at least N hand-authored eval scenarios when query log is empty |
| SR-05 | D-02 replaces `rerank_score` entirely with NLI entailment score. If NLI scores are poorly calibrated for short terse entries (e.g., ADRs with `[adr, cortical]` tags), search ranking could regress relative to the blended formula — with no easy rollback path short of `nli_enabled=false` | Med | Low | Spec writer should require per-scenario regression data in eval report; architect should document rollback path (config toggle) explicitly |
| SR-06 | `max_contradicts_per_tick` is defined as a per-call cap (AC-13) but the open question in SCOPE.md §Goals-6 and §Constraints-5 conflates "per tick" and "per store call". Ambiguity could lead to over- or under-rate-limiting | Low | Med | Spec writer must clarify cap semantics: per-`context_store` call vs per background tick; the two locations need consistent terminology |

## Integration Risks

| Risk ID | Risk | Likelihood | Severity | Recommendation |
|---------|------|------------|----------|----------------|
| SR-07 | Bootstrap edge promotion task (AC-12) must handle zero-row case (current production state) AND non-zero-row case (future deployments or restored backups). W1-1 left AC-08 unresolved — if a non-zero-row database is ever encountered, a bug in the DELETE+INSERT path could silently drop valid NLI-promoted edges or leave stale `bootstrap_only=1` rows | Med | Med | Architect should specify idempotency mechanism explicitly (e.g., a `bootstrap_promotion_done` flag in COUNTERS) rather than relying on row absence as the guard |
| SR-08 | `EvalServiceLayer::from_profile()` stub requires NLI model file to be locally present or downloadable during eval runs. CI/eval environments without HuggingFace network access cannot run the NLI-enabled candidate profile, reducing the 3-profile comparison to 1 profile | Med | Low | Spec writer should add a constraint: eval environments must either pre-cache the model or the eval CLI must support `--skip-profile-on-missing-model` to produce partial results rather than failing |
| SR-09 | Post-store fire-and-forget task reuses the embedding already computed during `context_store`. If the embedding is not threaded through to the spawned task (e.g., moved into the insert path before the spawn), the task either recomputes it (latency + pool pressure) or fails to retrieve it (incorrect neighbours) | Low | High | Architect must specify the embedding handoff contract: how the already-computed embedding reaches the spawned NLI task without re-embed |

## Assumptions

- **§Goals-1 / §Background-Model Selection**: Assumes `cross-encoder/nli-MiniLM2-L6-H768` ONNX export is available and Apache 2.0 licensed. The scope states this is "confirmed available" — architect should treat this as confirmed but verify hash at download time.
- **§Background-W1-1**: Assumes zero `bootstrap_only=1` rows on all production databases. If this is wrong (e.g., a database migrated through a different path), the bootstrap promotion task encounters a non-trivial workload on first startup tick.
- **§Background-W1-2**: Assumes the rayon pool from crt-022 is live and sized at 4–8 threads. Pool sizing adequacy for NLI workload is an open question in SCOPE.md §Open Questions-1 — the scope does not resolve it.
- **§Goals-9 / §Constraints-9**: Assumes a snapshot with real query history is available for eval gate. D-01 waives the gate when it is not, making the quality improvement claim unverifiable at ship time for some deployments.

## Design Recommendations

- **SR-02, SR-03**: Pool sizing and session concurrency are the highest-risk design decisions. Architect should benchmark 20-pair NLI batch latency on the target hardware before committing to pool size and single-session vs session-pool approach.
- **SR-07**: Specify the bootstrap promotion idempotency mechanism in the architecture before implementation — do not leave it as "runs once on first tick" without a durable completion marker.
- **SR-09**: Define the embedding handoff contract explicitly in the architecture; this is a subtle ownership/lifetime issue in async Rust that is easy to get wrong silently.
- **SR-04**: Spec writer should add a minimum evidence requirement for the D-01 gate waiver to prevent the eval gate from being vacuously passed.
