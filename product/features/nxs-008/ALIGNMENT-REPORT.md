# nxs-008: Vision Alignment Report

**Feature**: nxs-008 — Schema Normalization
**Date**: 2026-03-05
**Guardian**: uni-vision-guardian
**Artifacts Reviewed**: PRODUCT-VISION.md, SCOPE.md, SCOPE-RISK-ASSESSMENT.md, ARCHITECTURE.md, SPECIFICATION.md, RISK-TEST-STRATEGY.md
**ADRs Referenced**: #352, #354, #355-#362

---

## Summary

| Dimension | Verdict | Issues |
|-----------|---------|--------|
| 1. Feature Goals vs Vision Principles | **PASS** | 0 |
| 2. Architecture vs Existing Patterns/ADRs | **WARN** | 1 |
| 3. Scope Creep | **WARN** | 2 |
| 4. Risk Strategy Coverage | **PASS** | 0 |
| 5. Non-Goals Respected | **PASS** | 0 |

**Totals**: 3 PASS, 2 WARN, 0 VARIANCE, 0 FAIL

---

## 1. Feature Goals vs Vision Principles — PASS

nxs-008 is a Nexus-phase foundation feature that restructures the storage layer from bincode blobs to SQL columns. The vision states Unimatrix must be "trustworthy, retrievable, and ever-improving." This feature directly serves all three:

| Vision Principle | nxs-008 Contribution |
|-----------------|---------------------|
| **Trustworthy** | Behavioral parity (AC-17) — all 12 MCP tools produce identical results. No data loss via automated backup + round-trip migration tests. |
| **Retrievable** | SQL WHERE clauses replace HashSet intersection. N+1 fetch eliminated. Indexed columns enable efficient queries. |
| **Ever-improving** | Normalized schema enables ASS-016 multi-table JOINs for entry effectiveness analytics — the learning feedback loop the vision describes. Future field additions via `ALTER TABLE ADD COLUMN` instead of scan-and-rewrite. |

The vision's three-leg model (Files / Unimatrix / Hooks) is unaffected — nxs-008 is internal to Unimatrix's storage engine. No MCP tool interface changes, no hook changes, no file structure changes.

**Verdict**: Feature goals are well-aligned with vision principles. No issues.

---

## 2. Architecture vs Existing Patterns/ADRs — WARN

### Consistent

| Pattern | Alignment |
|---------|-----------|
| ADR #352 (server decoupling rejected) | Respected — server direct table access accepted, updates are mechanical per wave. |
| ADR #354 (OBSERVATION_METRICS excluded) | Respected — stays as bincode blob, explicitly out of scope. |
| ADR #59 (bincode v2 with serde default) | Migration leverages serde(default) for historical schema compatibility. |
| ADR #75 (scan-and-rewrite migration) | Superseded by design — nxs-008 makes future migrations use ALTER TABLE ADD COLUMN instead. |
| Wave-based delivery | Consistent with established nxs-005/nxs-006/nxs-007 delivery pattern. |
| Compilation gates per wave | Consistent with `cargo build --workspace && cargo test --workspace` pattern. |
| 8 new ADRs (#355-#362) stored in Unimatrix | Follows convention — ADRs in Unimatrix, not files. |

### WARN-01: Counter Helpers — Inconsistency Between Architecture and Specification

**Architecture** ADR-002 (#356): "Counter Helpers Move to counters.rs Module." Wave 0 explicitly creates `counters.rs`.

**Specification** AD-03: "Counter Helpers — Inline into Callers. Remove `tables::next_entry_id`... The equivalent functions already exist in write.rs (or a small counter.rs module)."

The architecture commits to a new `counters.rs` module. The specification hedges between inlining into `write.rs` callers and creating `counters.rs`. These must agree before implementation begins.

**Recommended resolution**: Update Specification AD-03 to match Architecture ADR-002 — create `counters.rs`. The architecture is the authoritative source for structural decisions.

**Severity**: Low. Implementation will follow the architecture regardless, but the spec should not contradict it.

---

## 3. Scope Creep — WARN

### WARN-02: Additional SQL Indexes Beyond SCOPE.md

SCOPE.md Goal #3 specifies 5 indexes replacing the 5 manual index tables:
- `idx_entries_topic`, `idx_entries_category`, `idx_entries_status`, `idx_entries_created_at`, `idx_entry_tags_tag`

The Architecture and Specification add 6 additional indexes not listed in SCOPE.md:

| Extra Index | Table | Justification in Artifacts |
|-------------|-------|---------------------------|
| `idx_entry_tags_entry_id` | entry_tags | CASCADE performance |
| `idx_co_access_b` | co_access | Reverse lookup for partner queries |
| `idx_sessions_started_at` | sessions | GC age queries |
| `idx_sessions_feature_cycle` | sessions | ASS-016 JOIN readiness |
| `idx_injection_log_entry` | injection_log | ASS-016 entry effectiveness |
| `idx_audit_log_agent_id` | audit_log | write_count_since performance |
| `idx_audit_log_timestamp` | audit_log | Time-range audit queries |

**Assessment**: These are reasonable performance optimizations that do not change the feature boundary, introduce new tables, or alter the public API. They are consistent with the SCOPE's stated goal of replacing client-side filtering with SQL WHERE clauses. However, the SCOPE should be updated to reflect the full index set so the acceptance criteria are complete.

**Recommended resolution**: Amend SCOPE.md to list all indexes, or add a note that "additional indexes for operational query performance may be added during architecture." No human approval required — this is additive infrastructure within the feature's boundary.

### WARN-03: Type Movement from Server to Store Crate

The Architecture recommends moving `AgentRecord`, `AuditEvent`, `TrustLevel`, `Capability`, and `Outcome` type definitions from the server crate to the store crate. This is not mentioned in SCOPE.md.

**Assessment**: This is a necessary consequence of the migration architecture (store-crate migration code must deserialize server-crate types). It is consistent with ADR #352 (server decoupling rejected — coupling is accepted). The types are data structures, not server logic. The alternative (serde_json::Value intermediate) is explicitly rejected in the architecture as fragile.

**Recommended resolution**: No human approval required. Document in SCOPE.md under "Server code updates are mechanical" that type definitions move to the store crate as part of migration infrastructure.

---

## 4. Risk Strategy Coverage — PASS

### Vision-Critical Areas Verified

| Vision-Critical Area | Risk Coverage | Tests |
|---------------------|--------------|-------|
| **Data integrity** (trustworthy) | RISK-01 (CRITICAL), RISK-02 (CRITICAL) | RT-01–RT-17: migration round-trips, bind parameter accuracy |
| **Query correctness** (retrievable) | RISK-03 (CRITICAL), RISK-04 (CRITICAL) | RT-18–RT-34: semantic equivalence, tag consistency |
| **Behavioral parity** (no regression) | RISK-21 (LOW) | RT-76–RT-85: all 12 MCP tools verified |
| **Future analytics** (ever-improving) | RISK-08 (HIGH), SR-06 | RT-46–RT-50: JSON column handling; C-18: ASS-016 JOIN indexes |
| **Migration safety** (one-way door) | RISK-01 (CRITICAL), RISK-16 (MEDIUM) | RT-07: backup verification; RT-08: atomic transaction |

### Risk Severity Distribution

| Severity | Count | Coverage |
|----------|-------|----------|
| Critical | 4 | 34 tests (RT-01–RT-34) |
| High | 6 | 28 tests (RT-35–RT-52, RT-67–RT-68) |
| Medium | 7 | 16 tests (RT-53–RT-66, RT-69–RT-70) |
| Low | 4 | 7 tests (RT-71–RT-75, RT-27, RT-72) |
| **Total** | **21** | **85 tests** |

The risk strategy is thorough. All 8 scope risks (SR-01 through SR-08) trace to specific RISK entries, ADRs, and test cases. The scope risk assessment's top-3 risks (SR-01, SR-02, SR-04) are all addressed with concrete mitigations.

**Verdict**: Risk strategy covers all vision-critical areas. No gaps identified.

---

## 5. Non-Goals Respected — PASS

| Non-Goal (from SCOPE.md) | Architecture | Specification | Risk Strategy |
|--------------------------|-------------|---------------|---------------|
| No server decoupling (ADR #352) | Respected — server updates mechanical, same-wave delivery | Respected — C-11 requires both crates per wave | Respected — RISK-06 validates cross-crate sync |
| No OBSERVATION_METRICS normalization (ADR #354) | Respected — listed in "Tables Unchanged" | Respected — Section 4 "Tables Unchanged" | Not mentioned (correct — no risk for unchanged tables) |
| No HNSW/vector changes | Respected — VECTOR_MAP in "Tables Unchanged" | Respected — Section 4 | Not mentioned (correct) |
| No Store public API changes | Respected — EntryRecord, Store methods unchanged | Respected — C-02 | Respected — RT-12 verifies Store API round-trip |
| No new MCP tools or behavioral changes | Respected — AC-17 behavioral parity | Respected — C-03 | Respected — RT-76–RT-85 verify all 12 tools |
| No serialization format change for OBSERVATION_METRICS | Respected — stays bincode | Respected — AC-15 excludes it | Not mentioned (correct) |
| No new tables beyond entry_tags | Respected — temporary migration tables (entries_v6) are transient | Respected | Not mentioned (correct — transient tables are not permanent) |

**Verdict**: All 7 non-goals are consistently respected across all artifacts. No violations.

---

## Variance Summary

**No variances requiring human approval.** All issues are WARN-level and can be resolved by the architect before implementation:

1. **WARN-01**: Spec AD-03 contradicts Architecture ADR-002 on counter helpers location. Update spec to match architecture.
2. **WARN-02**: 6 additional SQL indexes beyond SCOPE.md. Amend SCOPE to list full index set.
3. **WARN-03**: Type movement (server → store) not mentioned in SCOPE. Document under existing "mechanical updates" clause.

---

## Conclusion

nxs-008 design artifacts are well-aligned with the product vision. The feature is correctly scoped as an internal restructuring that preserves behavioral parity while enabling the vision's "ever-improving" principle through SQL-native analytics. Risk coverage is comprehensive with 85 tests across 21 identified risks. Three minor inconsistencies between artifacts should be resolved before implementation begins, but none require human design decisions — they are documentation updates to bring the SCOPE and SPECIFICATION into alignment with the ARCHITECTURE.

**Recommendation**: Resolve WARN-01 through WARN-03, then proceed to implementation.
