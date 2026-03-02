# Agent Report: col-010-agent-3-risk

Agent ID: col-010-agent-3-risk
Role: Risk Strategist (Architecture-Risk Mode)
Feature: col-010 — Session Lifecycle Persistence & Structured Retrospective
Date: 2026-03-02

---

## Task Completed

Produced `product/features/col-010/RISK-TEST-STRATEGY.md`.

---

## Artifacts Read

- `product/features/col-010/SCOPE.md`
- `product/features/col-010/SCOPE-RISK-ASSESSMENT.md` (14 scope risks SR-01 through SR-14)
- `product/features/col-010/architecture/ARCHITECTURE.md`
- `product/features/col-010/architecture/ADR-001-abandoned-session-status-variant.md`
- `product/features/col-010/architecture/ADR-002-injection-log-gc-cascade.md`
- `product/features/col-010/architecture/ADR-003-batch-injection-log-writes.md`
- `product/features/col-010/architecture/ADR-004-lesson-learned-fire-and-forget-embedding.md`
- `product/features/col-010/architecture/ADR-005-provenance-boost-query-time-constant.md`
- `product/features/col-010/architecture/ADR-006-p0-p1-component-split.md`
- `product/features/col-010/specification/SPECIFICATION.md`

---

## Risk Summary

| Priority | Risk Count |
|----------|-----------|
| Critical | 2 |
| High     | 4 |
| Medium   | 6 |
| Low      | 2 |
| **Total**| **14**    |

All 14 scope-level risks (SR-01 through SR-14) are traced in the Scope Risk Traceability table.

---

## Top Risks for Vision Guardian and Synthesizer Attention

### R-09 (Critical) — `detail_level = "summary"` Default Breaks Existing Callers

**Why it matters**: This is the highest-likelihood regression in the feature. Every existing caller of `context_retrospective` that does not pass `detail_level` will silently receive a 1-2KB summary instead of the previous 87KB full output. Tests asserting on `hotspots[].evidence` array contents fail. Agents iterating evidence arrays encounter empty fields. The specification (FR-10.8) mandates auditing existing tests BEFORE implementing P1 tiered output — this is a hard prerequisite gate, not an afterthought. The vision guardian should confirm: is this default change acceptable given existing agent dependencies on full retrospective output?

### R-01 (Critical) — Schema v5 Migration `next_log_id` Idempotency

**Why it matters**: The migration writes `next_log_id = 0` to COUNTERS in the same transaction as table creation. If a server restart occurs after table creation but before the schema version is written (or if `migrate_if_needed()` is called twice), the counter is reset to 0. This overwrites existing IDs, causing `insert_injection_log_batch` to overwrite live records. The architecture specifies a check-then-write guard, but its implementation and atomicity need explicit integration test coverage — specifically testing the partial-migration-restart scenario. The synthesizer should ensure this test scenario is in the P0 test plan as a blocking gate.

### R-03 (High) — `total_injections` Accuracy Under Fire-and-Forget Writes

**Why it matters**: `SessionRecord.total_injections` is populated from the in-memory count at SessionClose, while INJECTION_LOG writes are still in-flight. col-011 consumes this field for routing quality scoring. A consistent under-count (if INJECTION_LOG writes fail silently) will silently degrade col-011's routing decisions without any observable error. This is a systemic accuracy risk that spans the P0 → col-011 delivery chain. The synthesizer should flag OQ-01 (specification open question on `total_injections` source of truth) as a required resolution before P0 acceptance.

### R-04 (High) — Abandoned Session Retrospective Contamination

**Why it matters**: The `Abandoned` variant was added (ADR-001) to prevent metric contamination, but the filter must be explicitly applied in `from_structured_events()`. Missing the filter (or filtering on `outcome == "abandoned"` string instead of `status == Abandoned` enum) silently includes cancelled-session injection events in hotspot metrics, producing misleading recommendations. The vision guardian should verify the filter covers both `Abandoned` AND `TimedOut` variants (both represent non-completed work) — the specification currently specifies Abandoned exclusion but TimedOut exclusion requires verification.

---

## Open Questions Surfaced

1. **SR-SEC-02 gap**: `agent_role` and `feature_cycle` sanitization is not specified in SEC-01 (which scopes to `session_id` only). These fields are interpolated into auto-outcome entry content. Recommend the implementer resolve whether to sanitize or truncate these fields.

2. **OQ-01 (total_injections source)**: Specification recommends in-memory count but notes INJECTION_LOG count is the durable record. The accepted discrepancy under write failure should be explicitly documented in the test plan.

3. **OQ-03 (JSONL fallback trigger)**: AC-13 says fall back "when SESSIONS has no records." The specification adds a refinement: fall back only when JSONL also has data. Tester needs a definitive answer from the spec writer on which condition governs.

4. **TimedOut sessions in from_structured_events()**: ADR-001 and specification focus on filtering `Abandoned` sessions. Whether `TimedOut` sessions are also excluded from retrospective metric computation is implied but not explicitly stated in FR-09.2. Recommend explicit clarification.

---

## Reusable Patterns Identified

The fire-and-forget + graceful degradation pattern (used for ONNX embedding in ADR-004 and INJECTION_LOG writes) is a recurring Unimatrix pattern worth recording. Will store as a knowledge pattern entry after session completion.

---

## Self-Check

- [x] Every risk has a Risk ID (R-01 through R-14)
- [x] Every risk has at least one test scenario
- [x] Severity and likelihood assessed for each risk
- [x] Integration Risks section present and non-empty (IR-01, IR-02, IR-03)
- [x] Edge Cases section present and non-empty (EC-01 through EC-06)
- [x] Failure Modes section present (FM-01 through FM-05)
- [x] Security Risks section present (SR-SEC-01 through SR-SEC-04)
- [x] RISK-TEST-STRATEGY.md written to feature root (not in test-plan/)
- [x] All 14 SR-XX scope risks traced in Scope Risk Traceability table
- [x] No placeholder risks — all risks specific to the col-010 architecture
