# Gate 3a Report: crt-047

> Gate: 3a (Component Design Review)
> Date: 2026-04-06
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All component boundaries, interfaces, ADRs, and pool discipline match ARCHITECTURE.md exactly |
| Specification coverage | PASS | All FRs, NFRs, and ACs mapped in pseudocode; superseded SPEC sections correctly overridden by IMPLEMENTATION-BRIEF |
| Risk coverage | PASS | All 14 risks (2 Critical, 5 High, 5 Medium, 2 Low) covered by named tests in test plans |
| Interface consistency | PASS | Shared types in OVERVIEW.md match per-component usage; no contradictions across files |
| Knowledge stewardship compliance | PASS | Both agent reports contain stewardship sections with Queried and Stored entries |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**: All five pseudocode components match the architecture decomposition exactly.

1. **`cycle_review_index.md`** — Two-step upsert for `store_cycle_review()` explicitly coded (Step 1: SELECT `first_computed_at`, Step 2a: INSERT on missing, Step 2b: UPDATE with `first_computed_at` excluded from SET clause). This matches ARCHITECTURE.md `store_cycle_review() INSERT OR REPLACE with snapshot columns` note and ADR-001's requirement.

2. **`migration.md`** — Both `migration.rs` and `db.rs` paths are covered. Seven pre-checks run before any ALTER TABLE (all-pre-checks-before-any-alter pattern from ADR-004). The pseudocode explicitly names the DDL for the fresh-schema path and notes byte-consistency requirement.

3. **`curation_health.md`** — New file `services/curation_health.rs` containing all curation types and pure functions (ADR-005 extraction). `compute_curation_snapshot()` uses `read_pool()`. No write operations. Consistent with the architecture interaction diagram.

4. **`context_cycle_review.md`** — Step 8a-pre calls `compute_curation_snapshot()` before `store_cycle_review()` (I-01 read-before-write requirement from architecture). Pool discipline is correct: read via `read_pool()`, write via `write_pool_server()`.

5. **`context_status_phase7c.md`** — Phase 7c is ~15-20 lines delegating to `curation_health.rs`. Reads only from `cycle_review_index` via `read_pool()` (NFR-04, FR-17). No retrospective pipeline invocation.

ADR compliance verified:
- ADR-001 (baseline ordering key `first_computed_at DESC`, excluded when = 0): enforced in `get_curation_baseline_window` SQL and in `store_cycle_review` two-step upsert.
- ADR-002 (`corrections_total = agent + human`, system excluded): explicit in `CurationSnapshot.corrections_total` field comment and in `compute_curation_snapshot` algorithm.
- ADR-003 (ENTRIES-only orphan attribution, no AUDIT_LOG join): all three SQL queries use ENTRIES-only (`superseded_by IS NULL`, `updated_at` window).
- ADR-004 (migration atomicity, `pragma_table_info` per column): all seven pre-checks present, all ALTERs after pre-checks, outer transaction boundary maintained.
- ADR-005 (curation_health module extraction pre-planned): confirmed as new file with all curation logic.

---

### Check 2: Specification Coverage

**Status**: PASS

**Evidence**: All functional and non-functional requirements from the SPECIFICATION are covered. The IMPLEMENTATION-BRIEF's contradiction resolutions are correctly applied.

**Key items per spawn prompt verified:**

1. **Two-step upsert for `store_cycle_review()` to preserve `first_computed_at`** (spawn check 1): PASS. `cycle_review_index.md` shows explicit two-step pattern: SELECT existing `first_computed_at`, then INSERT (new row) or UPDATE without `first_computed_at` in SET clause. Anti-fix comment present: "DO NOT 'fix' this by using record.first_computed_at when preserved is 0."

2. **Orphan attribution uses ENTRIES-only (updated_at window), no AUDIT_LOG join** (spawn check 2): PASS. `curation_health.md` shows three SQL queries using ENTRIES exclusively. `superseded_by IS NULL` filter on orphan query matches FR-05 authoritative SQL (as overridden by ADR-003 / IMPLEMENTATION-BRIEF FAIL-01 resolution). No AUDIT_LOG reference anywhere in pseudocode.

3. **`corrections_total = corrections_agent + corrections_human` (corrections_system excluded)** (spawn check 3): PASS. Explicit in OVERVIEW.md shared type definition: `corrections_total: u32 — = corrections_agent + corrections_human (computed sum, NOT count(*))`. Also explicit in `CurationSnapshot` struct comment in `curation_health.md`. The SQL bucketing in `compute_curation_snapshot` computes agent and human counts separately, then sums them for `corrections_total`.

4. **Baseline window uses `WHERE first_computed_at > 0 ORDER BY first_computed_at DESC`** (spawn check 4): PASS. `cycle_review_index.md` `get_curation_baseline_window` SQL is exactly:
   ```sql
   WHERE first_computed_at > 0
   ORDER BY first_computed_at DESC
   LIMIT ?1
   ```
   Matches IMPLEMENTATION-BRIEF Resolution 2 authoritative SQL verbatim.

5. **TrendDirection variants Increasing/Decreasing/Stable (NOT Improving/Worsening)** (spawn check 5): PASS. Both OVERVIEW.md and `curation_health.md` define: `pub enum TrendDirection { Increasing, Decreasing, Stable }`. No Improving/Worsening variants present anywhere.

6. **Migration pseudocode updates BOTH `db.rs` and `migration.rs`** (spawn check 7): PASS. `migration.md` explicitly covers both files: `migration.rs` v23→v24 block with `CURRENT_SCHEMA_VERSION = 24`, and `db.rs` CREATE TABLE DDL update with all seven columns.

**FR coverage spot checks:**
- FR-01 (`CurationSnapshot` struct with six fields): PASS — struct defined in `curation_health.md` with all six fields (corrections_total, corrections_agent, corrections_human, corrections_system, deprecations_total, orphan_deprecations).
- FR-07 (snapshot written atomically with review): PASS — `context_cycle_review.md` passes snapshot into `build_cycle_review_record` which produces a single `CycleReviewRecord`, stored in one `store_cycle_review()` call.
- FR-08 (seven new columns): PASS — all seven enumerated in `migration.md` DDL and `cycle_review_index.md` `CycleReviewRecord` extension.
- FR-09 (`compute_curation_baseline` pure function): PASS — defined in `curation_health.md`, no I/O.
- FR-10 (baseline window `first_computed_at DESC`, excluding = 0): PASS — confirmed above.
- FR-11 (cold-start behavior, raw counts only when < MIN_HISTORY): PASS — `CurationHealthBlock.baseline = None` when `compute_curation_baseline` returns None.
- FR-14 (trend direction at ≥ 6 cycles): PASS — `compute_trend` returns None when qualifying rows < `CURATION_MIN_TREND_HISTORY = 6`.
- FR-15 (`SUMMARY_SCHEMA_VERSION` bumped to 2): PASS — constant defined in `cycle_review_index.md` as `pub const SUMMARY_SCHEMA_VERSION: u32 = 2`.
- NFR-01 (legacy DEFAULT-0 rows excluded via `schema_version < 2` check): PASS — `is_qualifying_row` helper in `curation_health.md` implements exact exclusion logic.
- NFR-02 (no NaN): PASS — zero-denominator guards in `compute_curation_baseline` (orphan ratio) and `compare_to_baseline` (zero stddev), with `population_stddev` helper documented as NaN-free.

**Scope compliance**: No pseudocode adds functionality not in the specification. The feature is purely additive on the read path (new columns, new query functions, new output fields).

---

### Check 3: Risk Coverage

**Status**: PASS

**Evidence**: All 14 risks from RISK-TEST-STRATEGY.md map to at least one named test scenario in the test plans.

| Risk ID | Priority | Test Plan Coverage |
|---------|----------|--------------------|
| R-01 (Critical) | ENTRIES-only vs AUDIT_LOG | CH-U-01 (feature_cycle join), CH-U-03 (superseded_by IS NULL), CH-U-04 (chain exclusion), CH-U-05 (out-of-window) |
| R-02 (Critical) | Ordering key mismatch | CRS-V24-U-06 (DESC order), CRS-V24-U-07 (cap at n), CRS-V24-U-08 (force=true stability), CS7C-U-06 (window cap) |
| R-03 (High) | Schema cascade — 3 paths | MIG-V24-U-01 through U-05; cascade touchpoints for migration_v22_to_v23.rs, sqlite_parity.rs, server.rs |
| R-04 (High) | `corrections_total` accounting | CH-U-02 (all 6 trust_source values; asserts corrections_total = agent+human, NOT 7) |
| R-05 (High) | Legacy DEFAULT-0 rows bias | CH-U-14 (excluded from MIN_HISTORY), CH-U-15 (genuine zero IS included) |
| R-06 (High) | NaN from zero denominator | CH-U-12 (zero deprecations → 0.0), CH-U-13 (mixed window finite), CH-U-11 (zero stddev) |
| R-07 (High) | Upsert clobbering `first_computed_at` | CRS-V24-U-03 (overwrite preserves first write), CRS-V24-U-04 (first write sets value) |
| R-08 (Medium) | AUDIT_LOG outcome filter | AC-13 grep check (no AUDIT_LOG query issued — vacuous per ADR-003) |
| R-09 (Medium) | `corrections_system` inconsistency | CRS-V24-U-09 (round-trip through store); AC-03 coverage |
| R-10 (Medium) | Schema cascade test failures | migration.md cascade touchpoints; pre-delivery grep check documented |
| R-11 (Medium) | Cold-start boundary conditions | CH-U boundary suite at 2, 3, 5, 6, 7, 10 rows; CH-U-18 to CH-U-22 trend boundaries |
| R-12 (Medium) | SUMMARY_SCHEMA_VERSION blast radius | CCR-U-04 (advisory present), CCR-U-05 (no silent recompute negative assertion) |
| R-13 (Low) | `updated_at` future mutation | CH-U-06 (window boundary); documented as known limitation |
| R-14 (Low) | Unattributed out-of-cycle orphans | CH-U-05 (orphan outside window excluded), AC-18 coverage |

**Risk priorities reflected in test emphasis**: The two Critical risks (R-01, R-02) each have 4+ test scenarios. High risks (R-03 through R-07) each have 2+ scenarios including AC acceptance criteria. Both Critical risks are also flagged in IMPLEMENTATION-BRIEF as requiring resolution before pseudocode, and that resolution is documented (FAIL-01 → ADR-003, FAIL-02 → ADR-001).

**Integration and edge case coverage**:
- I-01 (read before write): CCR-U-07 (structural grep check)
- I-02 (pool discipline): CCR-U-08 (grep check)
- I-03 (cycle_start_ts derivation): CCR-U-09 (no panic fallback)
- I-04 (single `store_cycle_review` call site): CCR-U-10 (grep check)
- EC-01 through EC-06 edge cases all mapped

**Integration suite plan**: OVERVIEW.md defines a concrete integration harness plan with 5 new integration tests across `test_lifecycle.py` and `test_tools.py`, using `server` and `shared_server` fixtures appropriately.

---

### Check 4: Interface Consistency

**Status**: PASS

**Evidence**: Shared types in OVERVIEW.md are used consistently across all per-component pseudocode files, with no contradictions.

**Type shape verification**:

`CurationSnapshot` fields: OVERVIEW.md defines 6 fields (corrections_total, corrections_agent, corrections_human, corrections_system, deprecations_total, orphan_deprecations) — all `u32`. `curation_health.md` defines identical struct. `context_cycle_review.md` passes snapshot into `build_cycle_review_record` mapping all 6 fields to `i64` for `CycleReviewRecord`. Consistent.

`CurationBaselineRow` fields: OVERVIEW.md defines 6 fields (5 metric fields + `schema_version: i64`). `cycle_review_index.md` defines identical struct for `get_curation_baseline_window()` output. `curation_health.md` uses `CurationBaselineRow` slices as input to all pure functions. Consistent.

`TrendDirection` variants: OVERVIEW.md `Increasing | Decreasing | Stable`. `curation_health.md` `pub enum TrendDirection { Increasing, Decreasing, Stable }`. `compute_trend` match arms use all three variants. Consistent.

`CurationHealthSummary.trend`: typed as `Option<TrendDirection>`. Used in `context_status_phase7c.md` output. Consistent.

**Data flow across component boundaries**:
- `get_curation_baseline_window()` produces `Vec<CurationBaselineRow>` (store layer) consumed by `compute_curation_baseline()` and `compute_curation_summary()` (services layer). Both consumer functions accept `&[CurationBaselineRow]`. Consistent.
- `CycleReviewRecord` with 7 new `i64` fields flows from `build_cycle_review_record()` into `store_cycle_review()`. Both sides of the boundary use `i64`. Consistent with ARCHITECTURE.md Integration Surface table.

**Constants consistency**:
- `CURATION_SIGMA_THRESHOLD = 1.5`: defined in `curation_health.md` constants block. OVERVIEW.md lists same value. IMPLEMENTATION-BRIEF lists same.
- `CURATION_MIN_HISTORY = 3`: consistent across OVERVIEW.md and `curation_health.md`.
- `CURATION_MIN_TREND_HISTORY = 6`: consistent.
- `CURATION_BASELINE_WINDOW = 10`: defined in `status.rs` per `context_status_phase7c.md`; `context_cycle_review.md` uses a separate `CURATION_BASELINE_WINDOW_FOR_REVIEW = 10` local constant at the call site (same value, minor naming split). This is architecturally acceptable — both resolve to 10.

**Pool discipline**: OVERVIEW.md pool table and per-component pseudocode are consistent:
- `compute_curation_snapshot`: `read_pool()` everywhere
- `store_cycle_review`: `write_pool_server()` everywhere
- `get_curation_baseline_window`: `read_pool()` everywhere
- Phase 7c: `read_pool()` via `get_curation_baseline_window`

No contradictions found between component files.

---

### Check 5: Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**: Both agent reports include `## Knowledge Stewardship` sections with evidence of Unimatrix queries.

**Pseudocode agent (`crt-047-agent-1-pseudocode-report.md`)**:
- Queried: two `context_search` calls (schema migration patterns; crt-047 architectural decisions)
- Read: all five ADR files, ARCHITECTURE.md, SPECIFICATION.md, RISK-TEST-STRATEGY.md, IMPLEMENTATION-BRIEF.md, actual source files
- Stored: "Deviations from established patterns: none" with explicit reason
- Status: Compliant (Queried entries present; "nothing novel" has reason)

**Test plan agent (`crt-047-agent-2-testplan-report.md`)**:
- Queried: three `context_search`/`context_briefing` calls (gate-3b test omission lesson, schema version cascade pattern, cycle_review_index migration test patterns)
- Stored: entry #4185 "Two-step upsert preservation test" pattern via `context_store`
- Status: Compliant (Queried entries present; novel pattern stored)

---

## Spawn Prompt Key Items — Verdict

| Item | Check | Status |
|------|-------|--------|
| 1. Two-step upsert (NOT INSERT OR REPLACE) for `store_cycle_review()` | `cycle_review_index.md` Step 2a/2b with explicit anti-fix comment | PASS |
| 2. Orphan attribution ENTRIES-only (updated_at window), no AUDIT_LOG | `curation_health.md` three SQL queries — ENTRIES only, no AUDIT_LOG reference | PASS |
| 3. `corrections_total = corrections_agent + corrections_human` (system excluded) | OVERVIEW.md, `curation_health.md`, `compute_curation_snapshot` algorithm | PASS |
| 4. Baseline window `WHERE first_computed_at > 0 ORDER BY first_computed_at DESC` | `cycle_review_index.md` SQL matches IMPLEMENTATION-BRIEF verbatim | PASS |
| 5. TrendDirection variants Increasing/Decreasing/Stable (NOT Improving/Worsening) | OVERVIEW.md and `curation_health.md` enum | PASS |
| 6. Test plans cover AC-R01 (`first_computed_at` preserved on force=true) | CRS-V24-U-03, CCR-U-05 | PASS |
| 7. Migration pseudocode updates BOTH `db.rs` and `migration.rs` | `migration.md` files table and DDL blocks for both files | PASS |

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for "gate 3a validation patterns" before writing this report — no directly applicable cross-feature patterns found; the seven-item spawn checklist is feature-specific.
- Stored: nothing novel to store — this gate validation found no recurring cross-feature failure pattern. All checks passed; if patterns emerge post-delivery they can be stored then.
