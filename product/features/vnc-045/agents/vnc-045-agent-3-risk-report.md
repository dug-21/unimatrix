# Agent Report: vnc-045-agent-3-risk (architecture-risk mode)

**Deliverable**: `product/features/vnc-045/RISK-TEST-STRATEGY.md` — REVISED for the REDUCED (mechanism-only) scope. `protected_tags` deferred in full.

## Reduced risk summary
- **Critical: 0** (prior Critical R-01 five-site per-slug threading VOIDED-BY-DEFERRAL).
- **High: 3** — R-01 forbidden-surface / derived-state blast radius (invariance + read-freshness); R-02 replace atomicity + colon-less degrade-to-add; R-03 audit completeness (primary retrofit-hard control, ADR-009).
- **Med: 2** — R-05 lifecycle guards (refuse quarantined / allow deprecated, re-implemented in-op); R-06 namespace-derivation & tag-parse edge cases.
- **Low-Med: 3** — R-04 value-opacity / no-validator regression; R-07 live-control wiring (`check_write_rate` + `audit_write_count_since` op-list); R-08 injection / over-broad `LIKE` DELETE.
- Total 8 risks (was 12). Risk profile materially reduced with the deferral.

## Confirmation: no deferred material carries test requirements
- VOIDED-BY-DEFERRAL in traceability: SR-03, SR-04, SR-06 (five-site threading + daemon divergence), SR-07 (validator; residual sliver "no validator shipped" under R-04), SR-08 (cadence guard), SR-09 (`merge_configs`), SR-10 (config parity).
- No scenario requires a validator, allow-list, `ProtectedTagsConfig`, `min_trust_level`, cadence guard, or per-slug config threading.
- The two preserved seams (value-opacity interception point, `Write`-gate trust location) are asserted as seams ONLY — no `evaluate(tag)` rejection path and no trust accept/reject difference is tested.
- ADR-003/005/006/007 (DEFERRED) generated zero scenarios.

## Test-seam constraint carried forward (#5468)
Handler `#[tool]` fn not unit-constructible → orchestration + audit proofs at `StoreTagService` + store-primitive + `audit_log` read-back seams; route/format proofs in Stage-3c integration. Stated in the header and Integration Risks.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` (context_search) for atomic-replace/partial-write posture, audit `{}`-sentinel + fire-and-forget settle + handler non-constructibility, `Outcome` serde, and cross-feature risk patterns (category=pattern). Findings: #4420 (non-transactional multi-write posture → R-02 likelihood), #267/#92 (`insert_in_txn` / chain atomicity → positive R-02 precedent), #5468 (audit `{}` sentinel + settle + handler non-constructibility → R-01/R-03 seams), #4366 (`Outcome` serde variant-string → R-03), #4388/#4389 (`session_id` before `tokio::spawn` → R-03), #4377 (fire-and-forget audit test strategy → R-03). Prior threading evidence (#3216/#5269/#5427) is VOIDED with the deferred config surface.
- Stored: nothing novel — instantiated patterns (audit `{}` sentinel; handler non-constructibility test-seam; DELETE+INSERT-as-one-tx) are already captured (#5468, #267); feature-specific risks live in the strategy doc, not Unimatrix. No cross-feature pattern (category=pattern) matched.
