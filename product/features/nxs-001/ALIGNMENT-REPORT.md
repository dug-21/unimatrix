# nxs-001: Embedded Storage Engine — Alignment Report

**Agent**: nxs-001-vision-guardian
**Date**: 2026-02-22
**Documents Reviewed**: SPECIFICATION.md, ARCHITECTURE.md, RISK-TEST-STRATEGY.md, ADR-001 through ADR-005

---

## Summary

| Dimension | Verdict |
|-----------|---------|
| 1. Vision Alignment | **PASS** |
| 2. Scope Compliance | **PASS** |
| 3. Evolution Readiness | **WARN** |
| 4. Integration Points | **WARN** |
| 5. Risk Coverage | **PASS** |
| 6. Acceptance Criteria Coverage | **PASS** |

**Totals**: 4 PASS, 2 WARN, 0 VARIANCE, 0 FAIL

---

## 1. Vision Alignment — PASS

The architecture directly supports the product vision of a "self-learning context engine" serving as the "knowledge backbone for multi-agent development orchestration."

**Evidence of alignment:**

- **8-table layout** matches the roadmap exactly (ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP, COUNTERS).
- **EntryRecord schema** pre-includes fields needed by future milestones: `confidence` (crt-002), `access_count`/`last_accessed_at` (crt-001), `supersedes`/`superseded_by` (vnc-003 correction chains), `embedding_dim` (nxs-003). All use `#[serde(default)]` for zero-migration addition.
- **QueryFilter extensibility** is designed for M4-M7 additions (usage ranking, feature scoping, project scoping) without changing callers.
- **Cargo workspace** anticipates future crates: `unimatrix-core` (nxs-004), `unimatrix-vector` (nxs-002), `unimatrix-embed` (nxs-003), `unimatrix-mcp` (vnc-001).
- **VECTOR_MAP bridge table** is created but not populated — correctly deferring hnsw_rs integration to nxs-002 while establishing the storage seam.
- **Synchronous API** (ADR-004) keeps the storage layer clean while enabling downstream async consumers via the proven `Arc<Database>` + `spawn_blocking` pattern.

The strategic approach from PRODUCT-VISION.md ("Start with Proposal A, evolve toward Proposal C") is faithfully executed: this feature builds pure Knowledge Oracle infrastructure. No Proposal C concerns (usage tracking, outcome analysis, retrospective intelligence) leak into this foundation.

---

## 2. Scope Compliance — PASS

All non-goals from SCOPE.md are respected:

| Non-Goal | Status |
|----------|--------|
| No vector index integration | Respected. VECTOR_MAP table created but no hnsw_rs code. |
| No embedding pipeline | Respected. Stores raw content only. |
| No MCP server or tool interface | Respected. Pure Rust library API. |
| No async API | Respected. ADR-004 enforces this. |
| No confidence computation | Respected. Field exists, defaults to 0.0, computation deferred. |
| No usage tracking tables | Respected. No USAGE_LOG, FEATURE_ENTRIES, or OUTCOME_INDEX. |
| No CLI | Respected. No binary targets. |
| No project isolation | Respected. Single database path, caller-provided. |
| No near-duplicate detection | Respected. Requires vector similarity (nxs-002 + nxs-003). |

**Minor additions beyond SCOPE (justified):**

1. **FR-04.5 Delete Entry** — Not in SCOPE's ACs but explicitly noted as "for completeness and test infrastructure." The Specification correctly documents that status transitions, not deletion, are the standard lifecycle mechanism. Justified for test cleanup and data recovery.
2. **FR-05.9 Entry Existence Check** — Internal utility method. Does not expand scope.
3. **`total_proposed` counter** — SCOPE mentions `total_active` and `total_deprecated`. The Specification adds `total_proposed` for completeness since `Status::Proposed` exists. Logical extension, not scope creep.

All additions are internal utilities that support the core ACs without expanding the feature's external surface.

---

## 3. Evolution Readiness — WARN

**What works well:**

- ADR-002 documents clear rules for schema evolution: fields always appended, never removed or reordered, always have `#[serde(default)]`, types never changed.
- `QueryFilter` derives `Default`, enabling extension with new optional fields that default to `None`.
- Risk Strategy R4 specifically targets schema evolution testing with hardcoded byte fixtures.
- The `#[serde(default)]` pattern is battle-tested across the Rust ecosystem.

**Concern W1: bincode v2 configuration not explicitly resolved.**

ADR-002 states "serde-compatible mode" for bincode v2 configuration, but does not specify the exact configuration constant. Risk Strategy Open Question 1 correctly flags this: "Which bincode v2 `Configuration` should be used? The default (`standard`) vs `legacy` affects how `#[serde(default)]` fields are handled."

bincode v2 has multiple encoding configurations:
- `bincode::config::standard()` — variable-length integer encoding
- `bincode::config::legacy()` — fixed-length integers (v1 compatible)
- Serde compatibility mode (`bincode::serde::encode_to_vec` vs `bincode::encode_to_vec`)

The choice between `Encode`/`Decode` derives (bincode-native) and `Serialize`/`Deserialize` derives (serde-compatible) determines whether `#[serde(default)]` is respected. ADR-002 mentions both derive sets (`Encode`/`Decode` AND `Serialize`/`Deserialize`) on EntryRecord, but the Specification and SCOPE only show `Serialize`/`Deserialize`.

**This must be resolved before implementation.** If the wrong bincode v2 API path is used, `#[serde(default)]` will not work, and the entire zero-migration strategy — foundational to milestones M1 through M9 — breaks.

**Recommendation**: The implementation must use bincode v2's serde compatibility functions (`bincode::serde::encode_to_vec` / `bincode::serde::decode_from_slice`) rather than bincode-native `Encode`/`Decode` derives. The first test written should be R4's schema evolution test, verifying `#[serde(default)]` behavior with the chosen configuration before any data is persisted.

---

## 4. Integration Points — WARN

**What works well:**

- Architecture Section "Integration Points Summary" explicitly maps all downstream features to their integration surfaces.
- VECTOR_MAP provides a clean bridge for nxs-002 with `put_vector_mapping` / `get_vector_mapping` / `delete_vector_mapping`.
- `Arc<Database>` + `spawn_blocking` pattern for vnc-001 is documented with code examples.
- nxs-003 integration is read-only (content + title for embedding) — minimal coupling.
- nxs-004 trait integration is deferred correctly — the trait will be defined in the core crate, implemented by the store crate.
- col-001 integration via "new tables added to the same database file" leverages redb's ability to add tables to existing databases.

**Concern W2: API shape discrepancy between Architecture and Specification.**

The Architecture defines the API as **free functions** operating on `&Database` references:

```rust
// Architecture style
pub fn insert_entry(db: &Database, record: &EntryRecord) -> Result<u64>;
pub fn get_by_id(db: &Database, entry_id: u64) -> Result<Option<EntryRecord>>;
pub fn query(db: &Database, filter: &QueryFilter) -> Result<Vec<EntryRecord>>;
```

The Specification defines the API as **methods on a `Store` wrapper type**:

```rust
// Specification style
impl Store {
    pub fn insert(&self, entry: NewEntry) -> Result<u64>;
    pub fn get(&self, entry_id: u64) -> Result<EntryRecord>;
    pub fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>>;
}
```

Key differences:
1. **`Store` wrapper vs raw `Database`** — Specification encapsulates redb's `Database` type; Architecture exposes it.
2. **`NewEntry` vs `EntryRecord` for inserts** — Specification introduces a separate input type (type-safer); Architecture passes `&EntryRecord` (simpler).
3. **Counter API exposure** — Architecture exposes `next_entry_id(txn: &WriteTransaction)` at the transaction level; Specification hides this behind `Store` methods.
4. **Module organization** — Architecture has 6 source files (`schema`, `database`, `write`, `read`, `counter`, `error`); Specification has 8 (`schema`, `db`, `write`, `read`, `query`, `counter`, `error`, `lib`).

The Specification's `Store` wrapper is arguably the better API (more idiomatic Rust, better encapsulation, `NewEntry` provides type safety for insert vs update), but downstream integration documentation in the Architecture references `Arc<Database>` while it should reference `Arc<Store>`.

**Recommendation**: Align to the Specification's `Store` wrapper API. Update Architecture's downstream integration examples to use `Arc<Store>`. The `Store` type should be `Send + Sync` (stated in Specification Section 6). This is a documentation consistency fix, not a design change.

---

## 5. Risk Coverage — PASS

The Risk Strategy identifies 12 risks, ranked by severity and likelihood, with comprehensive test scenario mappings.

**Strengths:**

- R1 (Index-Entry Desync) and R2 (Update Path Orphaning) are correctly identified as the two highest risks and receive the most testing attention. This matches the architectural reality — manual index maintenance across 5-6 tables is the most error-prone aspect.
- R4 (Schema Evolution) is correctly flagged as critical-impact despite low likelihood. A single test with hardcoded byte fixtures provides high-confidence protection.
- R7 (QueryFilter Intersection) includes property testing recommendation — essential for combinatorial coverage.
- Edge cases (Section 6) are thorough: empty values, boundary values, unicode, status transitions, tag intersection, time ranges, simultaneous field updates, database lifecycle.
- Test infrastructure design (Section 5) aligns with AC-19 and downstream reuse requirements.

**Minor gaps (acceptable):**

1. **No explicit MVCC test** — Concurrent read-while-write is redb's responsibility, but a smoke test would increase confidence. This is a testing nicety, not a gap.
2. **No performance benchmarks** — NFR-01 defines performance targets but the Risk Strategy has no verification plan. Acceptable for M1 where the targets are "design targets, not hard SLA guarantees" (NFR-01). Performance regressions will surface naturally as downstream features add load.
3. **Open Question 5** (forward-compatibility of update path for entries missing new indexes) is a real concern but only materializes in future milestones. Noting it now is appropriate.

The Risk Strategy's prioritization (R2 > R1 > R7 > R4 > R8) is sound and matches the actual risk profile of the architecture.

---

## 6. Acceptance Criteria Coverage — PASS

All 19 ACs from SCOPE.md are covered in the Specification with explicit verification methods:

| AC | Specification Section | Risk Strategy Mapping | Covered |
|----|----------------------|----------------------|---------|
| AC-01 | §4 Workspace and Compilation | — | Yes |
| AC-02 | §4 Schema (round-trip) | R3 scenarios | Yes |
| AC-03 | §4 Table Creation | R10 scenarios | Yes |
| AC-04 | §4 Write Operations (atomic insert) | R1, R6 scenarios | Yes |
| AC-05 | §4 Write Operations (monotonic ID) | R5 scenarios | Yes |
| AC-06 | §4 Read Operations (point lookup) | R1 scenarios | Yes |
| AC-07 | §4 Read Operations (topic query) | R1, R2 scenarios | Yes |
| AC-08 | §4 Read Operations (category query) | R1, R2 scenarios | Yes |
| AC-09 | §4 Read Operations (tag intersection) | R9 scenarios | Yes |
| AC-10 | §4 Read Operations (time range) | R1 scenarios | Yes |
| AC-11 | §4 Read Operations (status query) | R8 scenarios | Yes |
| AC-12 | §4 Write Operations (status update) | R8 scenarios | Yes |
| AC-13 | §4 Write Operations (VECTOR_MAP) | R11 scenarios | Yes |
| AC-14 | §4 Database Lifecycle | R10 scenarios | Yes |
| AC-15 | §4 Error Handling | R12 scenarios | Yes |
| AC-16 | §4 Schema (evolution) | R4 scenarios | Yes |
| AC-17 | §4 Combined Query | R7 scenarios | Yes |
| AC-18 | §4 Write Operations (update + index migration) | R2 scenarios | Yes |
| AC-19 | §4 Test Infrastructure | §5.5 Downstream Reuse Design | Yes |

The Risk Strategy's scenario mapping back to ACs is complete — every AC appears in at least one risk-to-scenario mapping, and every scenario maps to at least one AC. No gaps.

---

## Warnings Requiring Human Attention

### W1: Resolve bincode v2 configuration before implementation

**Dimension**: Evolution Readiness
**Severity**: High — incorrect choice breaks the zero-migration strategy for all milestones
**Action**: Confirm that `bincode::serde::encode_to_vec` / `bincode::serde::decode_from_slice` (serde-compatible path) is used, NOT `bincode::encode_to_vec` with `Encode`/`Decode` derives. Write the R4 schema evolution test first.
**ADR-002 update needed**: Replace "serde-compatible mode" with the exact function names to prevent ambiguity.

### W2: Align API shape between Architecture and Specification

**Dimension**: Integration Points
**Severity**: Low — both designs are functionally correct; this is a consistency issue
**Action**: Adopt the Specification's `Store` wrapper pattern. Update Architecture's downstream integration examples from `Arc<Database>` to `Arc<Store>`. Confirm `Store` is `Send + Sync`.

---

## Verdict

**Proceed to pseudocode/implementation.** Both warnings are resolvable during implementation without design changes. W1 requires confirming the bincode v2 API path early (first test). W2 is a documentation alignment that can be resolved as the Architecture evolves alongside implementation.

No variances. No scope violations. No vision misalignment. The foundation is sound.
