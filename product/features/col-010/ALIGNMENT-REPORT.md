# Alignment Report: col-010

> Reviewed: 2026-03-02
> Agent: col-010-vision-guardian
> Artifacts reviewed:
>   - product/features/col-010/architecture/ARCHITECTURE.md
>   - product/features/col-010/specification/SPECIFICATION.md
>   - product/features/col-010/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/col-010/SCOPE.md
> Scope risk source: product/features/col-010/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature correctly advances M5 hook-delivery goals and automatic knowledge lifecycle |
| Milestone Fit | PASS | Correctly positioned as col-010 in the col-006→011 delivery chain |
| Scope Gaps | PASS | All 13 SCOPE.md goals are addressed in source documents |
| Scope Additions | WARN | Source docs add an `Abandoned` SessionLifecycleStatus variant and a TimedOut filter in retrospective — small, well-justified additions not in SCOPE.md |
| Architecture Consistency | PASS | Architecture aligns with specification; P0/P1 split is consistent across all three documents |
| Risk Completeness | PASS | All 14 scope risks are traced into architecture decisions and test scenarios |
| Vision Document Discrepancy (SR-10) | VARIANCE | PRODUCT-VISION.md col-010 row references `session_id: Option<String>` on EntryRecord — explicitly a Non-Goal in SCOPE.md |

**Overall: 1 VARIANCE, 1 WARN, 5 PASS**

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | `Abandoned` as distinct `SessionLifecycleStatus` variant | SCOPE.md proposes `Active`, `Completed`, `TimedOut` only; abandoned sessions would be `Completed + outcome="abandoned"`. Architecture ADR-001 adds `Abandoned` as a 4th variant for precise filtering. Well-justified by SR-06 and the retrospective contamination risk. No functional scope expansion — the data captured is unchanged. |
| Addition | `TimedOut` sessions excluded from `from_structured_events()` retrospective metrics | SCOPE.md does not specify that timed-out sessions are excluded; it specifies only that Abandoned sessions are excluded. Architecture and specification (FR-09.2, R-04) extend the filter to include `TimedOut`. Rationale is sound (timed-out sessions are not completed meaningful work), but the exclusion was not in SCOPE.md. |
| Addition | `scan_sessions_by_feature_with_status()` filter variant | SCOPE.md specifies `scan_sessions_by_feature(feature_cycle)` only. Specification FR-02.7 adds an optional status-filter variant. Minor API surface addition, directly required by the `Abandoned`/`TimedOut` filter. |
| Addition | SR-SEC-02 identifies `agent_role` and `feature_cycle` sanitization gap not in SCOPE.md | Risk strategy flags that SEC-01 sanitizes `session_id` but not `agent_role` or `feature_cycle`, which are also interpolated into auto-outcome content. This is a new security concern not identified in SCOPE.md. Risk strategy leaves resolution to the implementer. |
| Simplification | Auto-outcome entry written without embedding (`embedding_dim = 0`) | SCOPE.md Proposed Approach explicitly decides this (component 4 decision). Architecture confirms. Rationale: SessionClose must not block on ONNX. Entries are structured metadata, queryable by tag/category lookup. Acceptable simplification. |
| Simplification | Supersede de-duplication race (SR-09) accepted as tolerated edge case | SCOPE.md notes the race; architecture §7.2 and specification FR-11.6 accept it as a known limitation without a write lock. Correctly scoped given the low likelihood of concurrent retrospective calls for the same cycle. |

---

## Variances Requiring Approval

### VARIANCE-01: PRODUCT-VISION.md col-010 Row References `session_id: Option<String>` on EntryRecord

**What**: The PRODUCT-VISION.md col-010 feature summary states the feature "adds `session_id: Option<String>` field on `EntryRecord`." The SCOPE.md explicitly lists this as a Non-Goal: "would require a full scan-and-rewrite migration (bincode is positional). The benefit... is low priority. Deferred to a future feature." This was flagged as SR-10 in the scope risk assessment.

**Why it matters**: PRODUCT-VISION.md is the authoritative source agents read to understand feature scope and schema evolution. An incorrect entry creates a direct contradiction between the authoritative vision document and the approved feature scope. Any agent or human reading PRODUCT-VISION.md to understand what col-010 delivers will form incorrect expectations about schema v5. This could cause downstream agents (col-011 scoping, future retrospective features) to assume `EntryRecord` has a `session_id` field when it does not.

**Confirmation**: Verified in PRODUCT-VISION.md line 123 (col-010 row): "adds `session_id: Option<String>` field on `EntryRecord`." Verified in SCOPE.md Non-Goals section: "`session_id` field on `EntryRecord` — would require a full scan-and-rewrite migration... Deferred to a future feature." The three source documents are internally consistent — all correctly treat `session_id` on `EntryRecord` as a Non-Goal. The discrepancy is solely in PRODUCT-VISION.md.

**Recommendation**: Update PRODUCT-VISION.md col-010 row to remove the `session_id: Option<String>` field reference. Replace with accurate description: "Adds SESSIONS table (16th) and INJECTION_LOG table (17th). No `session_id` field added to `EntryRecord` — deferred." This is a documentation correction, not an implementation change. No source documents need to change.

---

## Detailed Findings

### Vision Alignment

**PASS** — col-010 is well-aligned with the product vision's core themes.

**Invisible delivery**: The vision states agents "don't even need to ask — Unimatrix delivers knowledge automatically via Claude Code's hook system." col-010 makes session lifecycle persistent, which is a direct enabler of reliable hook-based delivery. The SCOPE.md Problem Statement correctly articulates this: "col-009 closes the confidence feedback loop using in-memory session state — but in-memory is ephemeral."

**Trust + Lifecycle + Integrity**: The auto-generated lesson-learned entries (with hash chains via the supersede path), provenance boost, and `trust_source = "system"` corrections are all consistent with the "auditable knowledge lifecycle" principle. The vision states "hash-chained correction histories with attribution, confidence evolution from real usage signals" — col-010's supersede-based de-duplication and fire-and-forget embedding directly serve this.

**Learning loop**: The vision's Milestone 5 goal is "automatic knowledge delivery via hooks... system observes agent behavior, identifies process hotspots from evidence." The `from_structured_events()` path — reading structured session data instead of JSONL telemetry — directly improves retrospective accuracy and advances this goal.

**Strategic continuity**: The PRODUCT-VISION.md milestone dependency graph shows `col-009 → col-010 → col-011` explicitly. The source documents honor this: all three documents state the hard dependency on col-009 and document col-011 as the downstream consumer. The P0/P1 split correctly identifies that only P0 (schema, UDS writes, GC, auto-outcomes, structured retrospective) is required for col-011; P1 (tiered output, lesson-learned) resolves issue #65 independently.

**Domain-agnostic engine note**: The vision notes "The core engine is domain-agnostic." col-010 introduces no domain-coupled logic — `SessionRecord`, `InjectionLogRecord`, and the retrospective pipeline are all domain-neutral.

### Milestone Fit

**PASS** — col-010 fits correctly within Milestone 5 (Collective Phase).

The PRODUCT-VISION.md milestone dependency graph places col-010 immediately after col-009 (schema v4) and before col-011 (Semantic Agent Routing). The feature is scoped to:
- Schema v5: SESSIONS + INJECTION_LOG (15th, 16th tables per vision — note: vision says 15th/16th while SCOPE.md says 16th/17th due to SIGNAL_QUEUE being table 15 from col-009; the numbering in PRODUCT-VISION.md is consistent)
- UDS listener integration for hook persistence
- col-002 retrospective enhancement via `from_structured_events()`

No M6+ capabilities are being pulled forward. The lesson-learned auto-persistence and provenance boost are within M5 scope — the vision describes "process intelligence" and "automatic knowledge delivery" as M5 goals. The lesson-learned category was planned from the outset (it is in the initial category allowlist per MEMORY.md).

The feature does not attempt to implement cross-session dashboards (M7), multi-project isolation (M8), or thin-shell migration (M6). These are explicitly listed as Non-Goals in SCOPE.md.

### Architecture Review

**PASS** — Architecture is internally consistent, addresses all major scope risks, and follows established patterns.

**Schema migration pattern**: The v4→v5 migration in §1.4 follows the established 3-step process (schema.rs constant bump + `migrate_v4_to_v5()` + `migrate_if_needed()` chain). The idempotency guard (`if counters.get("next_log_id").is_none()`) for the counter write correctly addresses SR-05.

**GC cascade (SR-04)**: Architecture §3.1 explicitly defines a 5-phase GC that collects session_ids to delete, scans INJECTION_LOG for matching records, deletes log entries, then deletes sessions — all in one `WriteTransaction`. This fully resolves the orphan-record gap identified in SR-04.

**Abandoned variant (SR-06)**: ADR-001 adds `Abandoned` as a distinct `SessionLifecycleStatus` variant. The architecture's §5 `from_structured_events()` correctly filters both `Abandoned` and `TimedOut` sessions from metric computation. This is a sound decision with a minor scope addition (see Scope Alignment above).

**Fire-and-forget pattern (SR-07)**: The lesson-learned ONNX embedding uses `tokio::spawn` detached from the response future (§7.1). The `context_retrospective` tool returns before embedding completes. This is consistent with the existing fire-and-forget pattern used throughout the server.

**Provenance boost location (§7.3)**: Correctly placed in `unimatrix-engine/src/confidence.rs` as a named constant (`PROVENANCE_BOOST: f64 = 0.02`), applied at both `uds_listener.rs` and `tools.rs` search re-ranking sites. The stored confidence weight invariant (`W_BASE + ... + W_TRUST = 0.92`) is preserved.

**P0/P1 split**: The architecture explicitly labels each component as P0 (col-011 blocking) or P1 (issue #65, independent). This matches the SCOPE.md component list and the risk strategy's delivery risk (SR-02/R-10).

**SR-13 partial gap**: Architecture §4.2 applies `trust_source = "system"` to auto-outcome entries and §7.1 applies it to lesson-learned entries. However, the architecture's open questions §1 note that "historical entries written by cortical implant hooks without an explicit trust_source (if any exist) may need a one-time migration." This is acknowledged but not resolved. The risk strategy (R-13) marks it Low and covers the forward path only. Acceptable for col-010 scope.

### Specification Review

**PASS** — Specification is complete, covers all 24 acceptance criteria, and addresses all 14 scope risks.

**Coverage**: All 24 ACs from SCOPE.md are mapped in the specification's AC verification map (SPECIFICATION.md §5). Each AC maps to specific FR requirements and a verification method. No AC is left without a covering requirement.

**FR completeness against SCOPE goals**: All 13 SCOPE.md goals map to FR requirements:
- Goals 1–4 (SESSIONS, INJECTION_LOG, migration, UDS writes) → FR-01 through FR-07
- Goals 5 (INJECTION_LOG writes in UDS listener) → FR-07
- Goal 6 (structured retrospective) → FR-09
- Goal 7 (tiered output) → FR-10
- Goal 8 (evidence synthesis) → FR-10.7
- Goal 9 (recommendation templates) → FR-10.6
- Goal 10 (auto-generated session outcomes) → FR-08
- Goal 11 (lesson-learned auto-persistence) → FR-11
- Goal 12 (provenance boost) → FR-12
- Goal 13 (session GC) → FR-04

**Security requirements**: SEC-01 (session_id sanitization), SEC-02 (auto-outcome pre-validation), and SEC-03 (trust_source correctness) are present and correctly reference the scope risk mitigations.

**OQ-01 (`total_injections` source of truth)**: Specification §OQ-01 correctly documents the accepted discrepancy — `total_injections` in SESSIONS uses the in-memory `signal_output.injection_count`, not the INJECTION_LOG database count. The fire-and-forget race is documented as a known limitation. The risk strategy (R-03) requires tests to explicitly verify and document this accepted discrepancy.

**OQ-03 (JSONL fallback condition)**: The specification's recommendation (use JSONL fallback only when JSONL has data too) is stricter than AC-13's wording ("falls back to JSONL path otherwise"). The risk strategy (R-10) flags this for tester clarification. The specification's stricter interpretation is safer and more aligned with the vision of the structured path being authoritative post-deployment.

**SR-10 disposition**: Specification §SR-10 entry correctly states "Out of scope for spec; post-approval: update PRODUCT-VISION.md col-010 row to remove session_id field reference." This correctly defers the documentation fix to post-approval. However, the fix must occur before implementation begins to prevent agent confusion.

**SR-SEC-02 gap (agent_role/feature_cycle sanitization)**: The specification's SEC-01 and SEC-02 sanitize `session_id` and validate category/tags, but do not explicitly sanitize `agent_role` and `feature_cycle` before interpolating them into auto-outcome entry content. The risk strategy (SR-SEC-02) identifies this gap and leaves resolution to the implementer. This is a minor security gap in the specification that should be resolved before implementation.

### Risk Strategy Review

**PASS** — Risk strategy is thorough, with 14 risks, 45+ test scenarios, and full traceability from scope risks through architecture decisions to test coverage.

**Critical risks covered**:
- R-01 (schema migration idempotency): 4 test scenarios covering the check-then-write guard, repeated migration call, schema version gate, and post-migration record integrity.
- R-09 (default `detail_level` behavior change): 5 test scenarios including the mandatory test-audit prerequisite (FR-10.8), backward-compatibility snapshot, byte-size assertion, and invalid `detail_level` handling. The risk strategy correctly marks this as blocking for P1 implementation.

**High risks covered**:
- R-02 (GC cascade atomicity): 4 scenarios with exact `GcStats` count verification.
- R-03 (`total_injections` accuracy): 3 scenarios explicitly testing the accepted discrepancy under write failure.
- R-04 (Abandoned/TimedOut filter): 4 scenarios including the edge case of only abandoned/timed-out sessions (must return empty report).
- R-07 (provenance boost at two callsites): 4 scenarios covering unit test, MCP path, hook path, and constant-reference check.

**Integration risks**: IR-01 (ContextSearch before SessionRegister ordering), IR-02 (GC during active sessions creating TimedOut injection races), and IR-03 (col-011 dependency on fire-and-forget write reliability) are all documented with specific test scenarios.

**Security risks**: SR-SEC-01 through SR-SEC-04 are documented. SR-SEC-02 (agent_role/feature_cycle injection) is correctly flagged as a gap not fully addressed by SEC-01. SR-SEC-03 (adversarial lesson-learned pollution) and SR-SEC-04 (task queue growth) are noted as future hardening concerns with no mitigation in v1 — acceptable given the low severity and the statistical hotspot thresholds providing natural resistance.

**Traceability**: The scope risk traceability table at the end of RISK-TEST-STRATEGY.md maps all 14 scope risks to architecture resolutions. All 14 entries are populated. SR-10 (vision doc discrepancy) and SR-08 (evidence synthesis fragility) are the only risks without a paired architecture-level risk ID — both are correctly treated as documentation/calibration concerns rather than implementation risks.

**Open question impact on risk**: The risk strategy correctly identifies that OQ-01 directly creates R-03 and OQ-03 affects R-10 path selection logic. These open questions should be resolved by the implementer before the relevant test scenarios are written.

---

## Pre-Implementation Actions Required

The following must be completed before col-010 implementation begins, in priority order:

1. **[VARIANCE-01 — REQUIRED]** Update `product/PRODUCT-VISION.md` col-010 row to remove the `session_id: Option<String>` field reference. The correction is to the vision document only; no source documents require changes.

2. **[SR-SEC-02 — RECOMMENDED]** Resolve the `agent_role`/`feature_cycle` sanitization gap before implementation. The specification should add explicit sanitization for these fields at the `SessionRegister` write point, parallel to the `session_id` sanitization in SEC-01. This is a low-severity security hardening item but affects content written to the knowledge base with `trust_source = "system"`.

3. **[SR-01 — GATE]** Confirm col-009 PR is merged and all col-009 acceptance criteria pass before beginning col-010 implementation. The SessionClose handler design depends on `SignalOutput.final_outcome`.
