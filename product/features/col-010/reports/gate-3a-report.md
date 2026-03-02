# Gate 3a Report: col-010

> Gate: 3a (Component Design Review)
> Date: 2026-03-02
> Result: PASS

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 7 components map correctly to architecture decomposition |
| Specification coverage | PASS | All 24 ACs have test plan coverage; FRs traced to pseudocode |
| Risk coverage | PASS | All 14 risks from Risk Strategy have test scenarios; R-03 discrepancy documented |
| Interface consistency | PASS | Shared types in OVERVIEW.md match per-component usage; one crate boundary clarification in structured-retrospective.md |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**:

- `storage-layer.md` correctly adds `SESSIONS` as table 16 and `INJECTION_LOG` as table 17, matching Architecture §1.1. Uses `TableDefinition<&str, &[u8]>` and `TableDefinition<u64, &[u8]>` as specified.
- `SessionRecord` struct fields match Architecture §1.2 exactly: `session_id`, `feature_cycle`, `agent_role`, `started_at`, `ended_at`, `status`, `compaction_count`, `outcome`, `total_injections`.
- `SessionLifecycleStatus` has all 4 variants (Active, Completed, TimedOut, Abandoned) per ADR-001.
- `InjectionLogRecord` fields match Architecture §1.3: `log_id`, `session_id`, `entry_id`, `confidence`, `timestamp`.
- `GcStats` struct matches Architecture §1 with correct field names.
- Migration v4→v5 pattern correctly mirrors `migrate_v3_to_v4` (open tables + write counter only if absent).
- `uds-listener.md` SessionRegister → `insert_session`, SessionClose → `update_session` + auto-outcome, ContextSearch → `insert_injection_log_batch` all match Architecture §2.
- `session-gc.md` documents all 5 GC phases in one `WriteTransaction` per Architecture §3.1.
- `auto-outcomes.md` adds `"session"` to `VALID_TYPES`, writes via `insert_entry` with `embedding_dim=0` per Architecture §4.
- `structured-retrospective.md` correctly resolves the crate boundary issue (unimatrix-observe cannot import unimatrix-store) by placing `from_observation_stream` in the observe crate and the store-aware orchestration in tools.rs. This is consistent with lib.rs `#![forbid(unsafe_code)]` and the existing ADR-001 in lib.rs.
- `tiered-output.md` adds `evidence_limit: Option<usize>` to wire.rs and applies truncation post-report-build per Architecture §6.2 (maps to the evidence_limit approach per SPECIFICATION.md FR-10).
- `lesson-learned.md` adds `PROVENANCE_BOOST = 0.02` in `confidence.rs`, applies at both `uds_listener.rs` and `tools.rs` per ADR-005.
- ADR-001 through ADR-006 are all represented in pseudocode.

**Note**: ARCHITECTURE.md §6 uses `detail_level`/`HotspotSummary` tiered approach, while SPECIFICATION.md FR-10 and IMPLEMENTATION-BRIEF.md use `evidence_limit`. Pseudocode follows the SPECIFICATION/BRIEF approach (evidence_limit). This is a minor architecture vs. spec divergence — the SPECIFICATION is the authoritative functional requirement. The pseudocode makes the correct decision.

### Check 2: Specification Coverage

**Status**: PASS

**Evidence**:

- FR-01 (Schema v5 migration): `storage-layer.md` §4 covers migration with idempotency guard.
- FR-02 (SessionRecord SESSIONS table): `storage-layer.md` §2 covers all CRUD operations.
- FR-03 (InjectionLogRecord INJECTION_LOG table): `storage-layer.md` §3 with batch-only write API.
- FR-04 (session_id sanitization): `uds-listener.md` §1 with `[a-zA-Z0-9-_]`, max 128 chars.
- FR-05 (SessionRegister hook write): `uds-listener.md` §2 covers fire-and-forget insert.
- FR-06 (SessionClose hook write): `uds-listener.md` §3 with status resolution per outcome.
- FR-07 (ContextSearch batch write): `uds-listener.md` §5 with ADR-003 one-transaction-per-response.
- FR-08 (auto-outcome on non-abandoned close): `auto-outcomes.md` §2 with injection_count > 0 guard.
- FR-09 (GC sweep): `session-gc.md` §2 calls gc_sessions from maintain=true path.
- FR-10 (evidence_limit): `tiered-output.md` §2 applies truncation with default=3, 0=unlimited.
- FR-11 (lesson-learned auto-persist): `lesson-learned.md` §2 with fire-and-forget embed and supersede chain.
- SEC-01 (session_id sanitization): covered in `uds-listener.md` §1.
- SEC-02 (feature_cycle/agent_role sanitization): `uds-listener.md` §3 resolves the SR-SEC-02 gap with `sanitize_metadata_field`.
- SEC-03 (trust_source="system"): documented in `auto-outcomes.md` §2 and `lesson-learned.md` §2.
- OQ-01 (total_injections source): `uds-listener.md` §3 uses in-memory count from session_registry.
- OQ-03 (JSONL fallback trigger): `structured-retrospective.md` §4 checks JSONL directory before fallback.

All 24 acceptance criteria have corresponding test plan scenarios in component test plan files.

### Check 3: Risk Coverage

**Status**: PASS

**Evidence**:

All 14 risks from RISK-TEST-STRATEGY.md are addressed:

| Risk | Test Plan Coverage |
|------|--------------------|
| R-01 (migration idempotency) | `storage-layer.md`: `test_schema_v5_migration_idempotency`, `test_schema_v5_migration_from_v4` |
| R-02 (GC atomicity) | `session-gc.md`: `test_gc_atomicity_no_orphan_injection_records` |
| R-03 (total_injections discrepancy) | `uds-listener.md`: documented as accepted discrepancy with explicit comment test |
| R-04 (Abandoned filter) | `structured-retrospective.md`: `test_structured_path_excludes_abandoned_sessions` |
| R-05 (batch write contention) | `uds-listener.md`: `test_context_search_one_transaction_per_response` |
| R-06 (ONNX failure) | `lesson-learned.md`: `test_lesson_learned_onnx_failure_writes_entry_without_embedding` |
| R-07 (provenance boost two callsites) | `lesson-learned.md`: `test_provenance_boost_applied_in_mcp_context_search` + `test_provenance_boost_applied_in_hook_context_search` |
| R-08 (concurrent supersede race) | `lesson-learned.md`: `test_lesson_learned_second_call_supersedes_first` with documented tolerance |
| R-09 (evidence_limit default truncation) | `tiered-output.md`: R-09 audit protocol + backward compat tests |
| R-10 (P0/P1 delivery split) | `OVERVIEW.md` integration plan; `structured-retrospective.md` path selection tests |
| R-11 (session_id bypass) | `uds-listener.md`: `test_sanitize_session_id_invalid_char`, `test_session_register_invalid_session_id_returns_error` |
| R-12 (auto-outcome validation bypass) | `auto-outcomes.md`: category + tag validation tests |
| R-13 (trust_source missing) | `auto-outcomes.md` + `lesson-learned.md`: explicit trust_source verification |
| R-14 (lesson-learned allowlist) | `lesson-learned.md`: noted; implementation should verify allowlist |
| IR-01, IR-02, IR-03 | Covered in `uds-listener.md` and `session-gc.md` edge cases |

Risk priorities are reflected in test plan emphasis: R-01 (Critical) has 4 explicit test scenarios; R-02 through R-04 (High) have multiple scenarios each.

### Check 4: Interface Consistency

**Status**: PASS

**Evidence**:

- `OVERVIEW.md` defines `SessionRecord`, `SessionLifecycleStatus`, `InjectionLogRecord`, `GcStats` — all used consistently in `storage-layer.md`, `uds-listener.md`, `session-gc.md`.
- `OVERVIEW.md` defines `HotspotNarrative`, `EvidenceCluster`, `Recommendation` — all used in `structured-retrospective.md` and `tiered-output.md`.
- Constants defined in `OVERVIEW.md` (`TIMED_OUT_THRESHOLD_SECS`, `DELETE_THRESHOLD_SECS`, `CLUSTER_WINDOW_SECS`, `PROVENANCE_BOOST`) match per-component usage.
- `insert_injection_log_batch` is the sole INJECTION_LOG write API in both `storage-layer.md` and `uds-listener.md` — no single-record insert invented.
- GC threshold constants are imported from `sessions.rs` in both `session-gc.md` (tools.rs import) and `storage-layer.md` (definition site).
- Data flow is coherent: SessionRegister writes SESSIONS → ContextSearch writes INJECTION_LOG → SessionClose updates SESSIONS + writes ENTRIES → GC reads both → structured-retrospective reads both.

**Minor observation**: `structured-retrospective.md` correctly identifies that `from_structured_events` function signature in ARCHITECTURE.md takes `store: &Store` which violates the crate boundary. The pseudocode resolves this correctly by describing the pattern where `tools.rs` loads data and passes `Vec<ObservationRecord>` to the observe crate. This is a pre-emptive fix for a potential implementation confusion. No rework needed.

---

## Integration Harness Plan Adequacy

The `test-plan/OVERVIEW.md` identifies applicable infra-001 suites:
- `smoke` (mandatory gate) ✓
- `tools` (evidence_limit parameter) ✓
- `lifecycle` (session persistence roundtrip) ✓
- `security` (session_id sanitization) ✓

New integration tests planned for Stage 3c cover AC-15, AC-16 (evidence_limit MCP tests) and session persistence tests. The R-09 audit protocol (blocking gate for P1) is documented in both `tiered-output.md` and `OVERVIEW.md`.

---

## Rework Required

None.

---

## Gate 3a: PASS

All checks passed. Pseudocode and test plans are consistent with architecture, specification, and risk strategy. The component design is ready for Stage 3b implementation.
