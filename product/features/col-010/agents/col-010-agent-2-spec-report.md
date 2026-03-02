# Agent Report: col-010-agent-2-spec

Feature: col-010 Session Lifecycle Persistence & Structured Retrospective
Role: Specification Writer
Date: 2026-03-02
Status: Complete

---

## Output

**SPECIFICATION.md**: `/workspaces/unimatrix/product/features/col-010/specification/SPECIFICATION.md`

---

## Summary of Requirements Produced

### Functional Requirements (12 groups, 50+ sub-requirements)

| FR Group | Coverage |
|----------|----------|
| FR-01: Schema v5 Migration | 6 sub-requirements. Idempotent next_log_id counter write (SR-05). Single write transaction. |
| FR-02: SESSIONS Table | 7 sub-requirements. SessionRecord fields, CRUD operations, scan_sessions_by_feature with status filter (SR-06). |
| FR-03: INJECTION_LOG Table | 4 sub-requirements. Batch insert per ContextSearch response (SR-12). Full-scan filter for reads. |
| FR-04: Session GC | 5 sub-requirements. Cascade delete of INJECTION_LOG on session deletion (SR-04). Named constants. |
| FR-05: UDS SessionRegister Write | 3 sub-requirements. session_id sanitization (SR-11). Fire-and-forget spawn_blocking. |
| FR-06: UDS SessionClose Write | 4 sub-requirements. Abandoned status variant (SR-06). Conditional auto-outcome trigger. |
| FR-07: UDS ContextSearch Injection Log | 5 sub-requirements. Batch per response (SR-12). Confidence-at-injection-time captured. |
| FR-08: Auto-Outcome Entries | 6 sub-requirements. Pre-validation (SR-11). embedding_dim=0. trust_source=system (SR-13). |
| FR-09: Structured Retrospective | 6 sub-requirements. Abandoned exclusion (SR-06). ObservationRecord extensions with serde(default). |
| FR-10: Tiered Output + Evidence Synthesis (P1) | 8 sub-requirements. detail_level param. HotspotSummary/HotspotNarrative/Recommendation types. 4 template types. Integration test audit requirement (SR-03). |
| FR-11: Lesson-Learned Auto-Persistence (P1) | 7 sub-requirements. Fire-and-forget embedding (SR-07). Supersede de-dup. Known limitation for SR-09 race. CategoryAllowlist check (SR-14). |
| FR-12: Provenance Boost (P1) | 4 sub-requirements. PROVENANCE_BOOST=0.02 named constant. Query-time only (no stored weight change). |

### Non-Functional Requirements

- NFR-01: Performance — fire-and-forget patterns, SessionClose ≤200ms budget, scan performance constraints
- NFR-02: Backward Compatibility — JSONL path unchanged, full mode preserves prior format, serde compat
- NFR-03: Reliability — restart durability, write failure handling pattern

### Security Requirements

- SEC-01: session_id sanitization (alphanumeric + `-_`, max 128 chars)
- SEC-02: Auto-outcome pre-validation (category allowlist + tag validation)
- SEC-03: trust_source="system" correctness for system-generated entries

### Verification Map

All 24 acceptance criteria from SCOPE.md are mapped to implementing FRs and verification methods in the AC table.

---

## All 14 Scope Risks Addressed

Every risk from SCOPE-RISK-ASSESSMENT.md is addressed in the specification:

- **SR-01** (Critical, col-009 dependency): Not spec-addressable — gate check required. Noted in Risk Addressal Summary.
- **SR-02** (High, bundle risk): Explicit P0/P1 split at component granularity in Component Breakdown section.
- **SR-03** (High, default behavior change): FR-10.8 requires integration test audit before implementing tiered output. `detail_level="full"` preserves backward compat.
- **SR-04** (Medium, INJECTION_LOG orphans): FR-04.1/04.3 requires cascade delete in same write transaction as session deletion.
- **SR-05** (Medium, migration idempotency): FR-01.2 specifies check-then-write for `next_log_id = 0`.
- **SR-06** (Medium, Abandoned status): FR-02.3 adds Abandoned variant. FR-09.2 excludes Abandoned from metric computation. FR-06.2 assigns Abandoned status on SessionClose.
- **SR-07** (Medium, ONNX latency): FR-11.3 makes lesson-learned write fire-and-forget.
- **SR-08** (Medium, heuristic fragility): FR-10.7 uses named CLUSTER_WINDOW_SECS constant; empty results return None gracefully.
- **SR-09** (Medium, supersede race): FR-11.6 documents as known limitation. Race is tolerated.
- **SR-10** (Low, vision doc discrepancy): Out of spec scope; flagged for post-approval update to PRODUCT-VISION.md.
- **SR-11** (Low, MCP validation bypass): SEC-01/SEC-02 apply minimum validation before auto-outcome writes.
- **SR-12** (Low, counter contention): FR-03.3 specifies batch writes per ContextSearch response.
- **SR-13** (Low, trust_source inconsistency): SEC-03 specifies trust_source="system" for all system-generated entries.
- **SR-14** (Low, lesson-learned allowlist): FR-11.7 verifies allowlist before write. OQ-04 flags for implementation verification.

---

## Open Questions

Four open questions were identified during specification:

1. **OQ-01**: `total_injections` source of truth under concurrent fire-and-forget writes (recommendation: use in-memory count at SessionClose)
2. **OQ-02**: `compaction_count` update cadence (recommendation: update at SessionClose only, not on every CompactPayload)
3. **OQ-03**: JSONL fallback trigger condition when SESSIONS returns empty (recommendation: use JSONL only if JSONL data also exists for the feature_cycle)
4. **OQ-04**: Verification that `"lesson-learned"` is in CategoryAllowlist initial set (implementation verification point)

---

## P0/P1 Split

**P0** (Components 1–5) is required before col-011 can proceed. Covers the entire session lifecycle persistence foundation: SESSIONS table, INJECTION_LOG table, UDS writes, GC, auto-outcomes, and structured retrospective.

**P1** (Components 6–7) resolves issue #65 and adds observability quality (tiered output, evidence synthesis, lesson-learned, provenance boost). Independent of col-011. Can ship in a follow-on PR if needed.

This split is explicitly reflected in both the Component Breakdown table and the FR groupings.
