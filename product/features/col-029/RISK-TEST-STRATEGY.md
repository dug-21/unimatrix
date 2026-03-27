# Risk-Based Test Strategy: col-029

GH Issue: #413

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `connected_entry_count` double-counts entries that appear as both source and target in different edges (COUNT DISTINCT source_id + COUNT DISTINCT target_id overcounts the overlap) | High | High | Critical |
| R-02 | `cross_category_edge_count` NULL guard omission: a LEFT JOIN row where one endpoint is deprecated produces `src_e.category = NULL`; if the CASE guard is incomplete, NULL != NULL evaluates to NULL (not FALSE) and the edge is silently counted | High | Med | High |
| R-03 | `bootstrap_only=1` NLI-sourced edge leaks into `inferred_edge_count`: an edge written with `source='nli'` and `bootstrap_only=1` (bootstrap seeding of an NLI-detected relation) passes the `source='nli'` filter but violates AC-16 | High | Med | High |
| R-04 | `StatusReport::default()` missing field: the struct has a manual (non-derived) `Default` impl; omitting any of the six new fields causes a compile error that could be missed if the implementer adds struct fields but forgets the `default()` block | Med | Med | Medium |
| R-05 | Division-by-zero in `mean_entry_degree` and `graph_connectivity_rate`: if `active_entry_count = 0`, integer division or unchecked `f64` division by zero produces `NaN` or `inf` rather than `0.0` | High | Low | Medium |
| R-06 | `compute_graph_cohesion_metrics()` called from maintenance tick: a future refactor or copy-paste in `status.rs` places the call inside `load_maintenance_snapshot()`, causing per-tick SQL overhead that NFR-01 explicitly forbids | Med | Low | Medium |
| R-07 | ~~write_pool_server() contention~~ — **eliminated by ADR-003 correction**: `compute_graph_cohesion_metrics()` now uses `read_pool()`, removing all write-pool contention risk | — | — | — |
| R-11 | WAL read staleness: `read_pool()` connections may see a snapshot that lags the most recent GRAPH_EDGES writes by up to one checkpoint interval; cohesion counts in `context_status` may momentarily undercount NLI-inferred edges immediately after a batch run | Low | Med | Low |
| R-08 | Query 2 `connected_raw` sum is not clamped: if UNION sub-query approach is abandoned for Rust HashSet post-processing, a logic error in the set construction could under- or over-count connected entries | Med | Med | Medium |
| R-09 | `EDGE_SOURCE_NLI` constant not re-exported from `lib.rs`: `nli_detection.rs` cannot import it; bare `"nli"` literals persist in both files, defeating SR-01 mitigation | Med | Med | Medium |
| R-10 | Format output conditional guard: Summary line is suppressed when all six metrics are zero (empty store), but an operator on a fresh store sees no graph cohesion line at all — no confirmation whether the feature is present | Low | Med | Low |

---

## Risk-to-Scenario Mapping

### R-01: connected_entry_count double-count via COUNT DISTINCT sum

**Severity**: High
**Likelihood**: High
**Impact**: `graph_connectivity_rate` exceeds 1.0 or returns an incorrect fraction; `isolated_entry_count` goes negative (wraps if u64). Historical evidence: entry #1043 (Subquery Dedup Before JOIN Aggregation to Prevent Count Inflation) and #1044 (Risk-based test strategy caught COUNT DISTINCT bug at implementation time in crt-018) document this exact failure mode in this codebase.

**Test Scenarios**:
1. Create 3 active entries A, B, C. Insert edges A→B and B→C (B appears as both source_id and target_id). Assert `graph_connectivity_rate = 1.0` and `isolated_entry_count = 0`. Any implementation using `COUNT(DISTINCT source_id) + COUNT(DISTINCT target_id)` without dedup will report `connected_entry_count = 4` (A, B, B, C) instead of 3 — rate becomes 4/3 > 1.0.
2. Create 2 active entries A, B. Single edge A→B. Assert `connectivity_rate = 1.0`, `isolated_entry_count = 0`, `mean_entry_degree = 2.0` (2*1 edge / 2 entries).
3. Assert `graph_connectivity_rate` is always in `[0.0, 1.0]` — a value > 1.0 is a definitive signal of double-counting.

**Coverage Requirement**: The `all_connected` and `mixed_connectivity` unit test cases must include at least one entry that appears on both sides of different edges (bidirectional or chain topology). A star topology (one central node with all edges pointing out) does not exercise the overlap.

---

### R-02: cross_category_edge_count NULL-guard failure for deprecated endpoints

**Severity**: High
**Likelihood**: Med
**Impact**: Edges where one endpoint is deprecated/quarantined are silently counted as cross-category edges, inflating `cross_category_edge_count` and misrepresenting PPR-relevant graph diversity.

**Test Scenarios**:
1. Insert edge E1 between active entry (category="decision") and deprecated entry (category="pattern", status=1). Assert `cross_category_edge_count = 0` — the deprecated endpoint should exclude this edge.
2. Insert edge E2 between active entries with different categories. Assert `cross_category_edge_count = 1` — the active cross-category edge is counted.
3. Insert edge E3 between two active entries with the same category. Assert `cross_category_edge_count` does not increase (still 1).

**Coverage Requirement**: The `cross_category_edges` unit test must include a deprecated-endpoint edge case (AC-08). The ADR-004 CASE guard (`ge.id IS NOT NULL AND src_e.category IS NOT NULL AND tgt_e.category IS NOT NULL`) must be verified to exclude deprecated endpoints via the missing JOIN match.

---

### R-03: bootstrap_only=1 NLI edge leaks into inferred_edge_count

**Severity**: High
**Likelihood**: Med
**Impact**: `inferred_edge_count` overstates the number of NLI-inferred edges. An operator monitoring #412 progress sees a non-zero count even before real inference has run, because bootstrap rows with `source='nli'` pass the source filter without the `bootstrap_only=0` guard. AC-16 explicitly addresses this.

**Test Scenarios**:
1. Insert one edge with `source='nli'` and `bootstrap_only=1`. Assert `inferred_edge_count = 0`.
2. Insert one edge with `source='nli'` and `bootstrap_only=0`. Assert `inferred_edge_count = 1`.
3. Insert both together. Assert `inferred_edge_count = 1` (only the non-bootstrap row counts).
4. Verify the same `bootstrap_only=0` guard applies to `supports_edge_count`: a `relation_type='Supports'` edge with `bootstrap_only=1` must not appear in `supports_edge_count`.

**Coverage Requirement**: The `bootstrap_excluded` unit test (AC-13, AC-16) must explicitly insert an NLI-sourced bootstrap edge and assert `inferred_edge_count = 0`.

---

### R-04: StatusReport::default() missing field — compile error or silent zero

**Severity**: Med
**Likelihood**: Med
**Impact**: `StatusReport` uses a hand-written `Default` impl (no derives). Adding a struct field without adding it to `default()` is a compile error. However, if the implementer adds a field with a default value via a fallback (e.g., `Default::default()` on the struct), the omission may be silently zero. Historical evidence: entry #3544 (col-028 cascading struct field addition compile cycles) documents this pattern causing multiple recompile cycles.

**Test Scenarios**:
1. Compile check: `cargo check -p unimatrix-server` must pass after adding all six fields. Confirms no field is omitted from `default()`.
2. Assert `StatusReport::default().graph_connectivity_rate == 0.0`.
3. Assert `StatusReport::default().isolated_entry_count == 0`.
4. Assert `StatusReport::default().mean_entry_degree == 0.0`.
5. (All six fields — one assertion each.)

**Coverage Requirement**: A dedicated unit test (or inline const assertion) in `mcp/response/status.rs` that constructs `StatusReport::default()` and checks all six new field values. AC-12.

---

### R-05: Division-by-zero on empty store

**Severity**: High
**Likelihood**: Low
**Impact**: `mean_entry_degree` and `graph_connectivity_rate` produce `NaN` or `inf` if the Rust division is not guarded. `NaN` serializes to `null` in JSON, which silently breaks the response schema for callers expecting `f64`.

**Test Scenarios**:
1. Open a test store with no entries and no edges. Call `compute_graph_cohesion_metrics()`. Assert `connectivity_rate = 0.0`, `mean_entry_degree = 0.0`, `isolated_entry_count = 0`, all other fields 0.
2. Open a test store with two deprecated entries (status=1) and edges between them. No active entries. Call the function. Assert same as above — zero active entries means denominator is zero, result must be `0.0` not `NaN`.

**Coverage Requirement**: The `all_isolated` unit test must include an explicit assertion on `mean_entry_degree = 0.0`. A separate test for zero-active-entries scenario (all deprecated) is required.

---

### R-06: compute_graph_cohesion_metrics() called from maintenance tick

**Severity**: Med
**Likelihood**: Low
**Impact**: Every 15-minute tick issues two SQL queries against the write pool unnecessarily. Under write load (NLI inference actively running) this adds pool contention to each tick. NFR-01 explicitly forbids this call from the tick path.

**Test Scenarios**:
1. Code review / grep: `grep -r "compute_graph_cohesion_metrics" crates/` must return exactly one call site — in `compute_report()` in `services/status.rs`, not in `load_maintenance_snapshot()` or `maintenance_tick()`. AC-15.
2. Negative assertion: `load_maintenance_snapshot()` call graph must not include any call to `compute_graph_cohesion_metrics`.

**Coverage Requirement**: AC-15 verification is a static check, not a runtime test. The tester must confirm the single call site as part of delivery gate review.

---

### R-07: ~~write_pool_server() acquire timeout under concurrent inference~~ — ELIMINATED

**Eliminated by ADR-003 correction.** `compute_graph_cohesion_metrics()` was changed to use `read_pool()` (not `write_pool_server()`). The write-pool contention scenario described in entries #2058 and #2130 no longer applies to this function.

The non-fatal error-path requirement (warn + zero fields) is retained as implementation guidance but is no longer driven by pool timeout risk — it is driven by general store error resilience (see Failure Modes table).

**Coverage Requirement**: The non-fatal error path in `compute_report()` Phase 5 still requires test coverage — confirm the `Err(e) => tracing::warn!(...)` arm does not abort the report. This is now categorised under general store-error resilience, not pool contention.

---

### R-11: WAL read staleness in cohesion metrics

**Severity**: Low
**Likelihood**: Med
**Impact**: `read_pool()` connections operate on a WAL snapshot. Under active NLI inference, recent edge writes may not have been checkpointed before `context_status` is called. Cohesion counts (`inferred_edge_count`, `connected_entry_count`) may momentarily undercount by the number of edges written since the last checkpoint. The lag is bounded by `wal_autocheckpoint` (default 1000 pages) and is non-corrupting — data is not lost, only briefly unreported. An operator who calls `context_status` mid-inference batch may see a lower `inferred_edge_count` than expected and incorrectly conclude inference is stalled.

**Test Scenarios**:
1. Documentation/comment check: `compute_graph_cohesion_metrics()` must carry a comment noting that read-pool snapshot semantics are intentional and that a bounded lag is acceptable per ADR-003. Absence of this comment risks a future developer "correcting" the pool choice back to `write_pool_server()`.
2. Operator-facing: the `context_status` help text or response notes (if any) should not claim "current" or "real-time" graph counts. Assert that no response field label implies instantaneous consistency.

**Coverage Requirement**: No runtime test is required (the staleness window is not deterministically reproducible in unit tests without WAL manipulation). The risk is mitigated by design (ADR-003 documents the acceptable trade-off). The tester confirms the ADR-003 comment is present at the call site as part of delivery gate review.

---

### R-08: Rust HashSet post-processing logic error for connected_entry_count

**Severity**: Med
**Likelihood**: Med
**Impact**: If the implementer chooses Rust-side HashSet dedup instead of the UNION sub-query approach (ADR-002 permits both), an off-by-one or status-filter omission in the HashSet construction silently reports incorrect connectivity.

**Test Scenarios**:
1. Mixed-connectivity test: 4 active entries, 2 deprecated entries. Edges: A→B (both active), C→deprecated (active→deprecated). Assert `connected_entry_count = 2` (only A and B count; the edge to deprecated does not make C "connected" in the active-only sense).
2. Chain test (R-01 scenario 1 above) also validates this path.

**Coverage Requirement**: If the HashSet approach is used, the test must include an edge where one endpoint is deprecated to confirm the active-only filter applies to the HashSet population, not just the JOIN.

---

### R-09: EDGE_SOURCE_NLI not re-exported from lib.rs

**Severity**: Med
**Likelihood**: Med
**Impact**: `nli_detection.rs` cannot import the constant; bare `"nli"` literals persist. SR-01 mitigation (ADR-001) requires the constant to be re-exported from `unimatrix-store/src/lib.rs` so the server crate can import it. Without the re-export the constant exists but is unused — the coupling risk SR-01 identified is not actually resolved.

**Test Scenarios**:
1. Compile check: `cargo check -p unimatrix-server` must succeed with `use unimatrix_store::EDGE_SOURCE_NLI` at the top of a new test or in `read.rs`. If the re-export is missing this fails at compile time.
2. Grep check: `grep -r "EDGE_SOURCE_NLI" crates/unimatrix-store/src/lib.rs` must return a `pub use` line.

**Coverage Requirement**: The re-export is a compile-time verifiable contract. The tester confirms presence in `lib.rs` as part of AC-01 validation.

---

### R-10: Summary line omitted on empty store, no diagnostic confirmation

**Severity**: Low
**Likelihood**: Med
**Impact**: On a fresh store (zero active entries, zero edges), the Summary format omits the graph cohesion line entirely. An operator cannot tell whether the feature deployed successfully or whether the store is simply empty. Low severity because the Markdown format always shows the sub-section header.

**Test Scenarios**:
1. With an empty store, call `context_status` with Summary format. Confirm the response does not crash and either shows a zero-state cohesion line or omits it consistently with the stated conditional.
2. With a non-empty store (at least one active entry with at least one edge), call `context_status` Summary format. Assert the graph cohesion line is present and contains the numeric values.

**Coverage Requirement**: AC-09 validation — the Summary format test must use a non-empty store to confirm the line appears.

---

## Integration Risks

**Layer boundary: StatusService → Store (compute_graph_cohesion_metrics)**

The call is non-fatal on error (warn + skip). The risk is that the error branch is never exercised in tests and a future regression silently drops all six metrics from the report without any visible failure. The `Err` arm must be covered by at least one test that injects a store-level error and asserts the report is returned with zero cohesion fields.

**Layer boundary: Store SQL → SQLite (read_pool)**

Both cohesion queries use `read_pool()` (ADR-003 corrected). This eliminates write-pool contention with NLI inference. The residual integration risk is WAL snapshot staleness (R-11): the read pool may see an older GRAPH_EDGES snapshot if a checkpoint has not occurred since the last write batch. This is non-corrupting and bounded; see R-11 for mitigation.

**Layer boundary: StatusReport → format_status_report**

The Summary format conditional (`isolated + cross_category + inferred > 0`) means a store with only `supports_edge_count > 0` and all others zero would suppress the Summary line while still having real data. This edge case is unlikely in practice but could mislead operators who rely on Summary exclusively.

**Cross-crate: unimatrix-store ↔ unimatrix-server (EDGE_SOURCE_NLI)**

The constant is defined in `unimatrix-store/src/read.rs` and must be re-exported. If the re-export is missing, `nli_detection.rs` in `unimatrix-server` cannot use it. The bare `"nli"` literals in `nli_detection.rs` remain — creating a divergence risk that ADR-001 was specifically designed to prevent.

---

## Edge Cases

| Edge Case | Risk | Scenario |
|-----------|------|----------|
| Zero active entries (all deprecated/quarantined) | Denominator = 0; division guard required | Store with only deprecated entries and edges between them |
| Entry appears on both sides of different edges (chain A→B→C) | COUNT DISTINCT overlap (R-01) | Three-entry chain with B as both source and target |
| Edge with one active + one deprecated endpoint | cross_category logic (R-02) and connectivity (R-08) | Deprecated endpoint should not contribute to connected count |
| NLI edge with bootstrap_only=1 | R-03 — AC-16 | Should not appear in inferred_edge_count |
| All edges are bootstrap_only=1 | All six metrics should return zero/0.0 | Store with only bootstrap edges, multiple active entries |
| Same-category edges only | cross_category_edge_count = 0 | All edges within category "decision" |
| Single active entry, no edges | connectivity_rate=0.0, isolated=1, mean_degree=0.0 | Trivial case, validates all formulas at boundary |
| mean_entry_degree formula for 1 edge, 2 active entries | (2*1)/2 = 1.0 | Verifies in+out degree formula |

---

## Security Risks

**Untrusted input surface**: `compute_graph_cohesion_metrics()` accepts no external input. All data comes from the `GRAPH_EDGES` and `entries` tables, which are populated by trusted server-side services (NLI inference, co-access, manual store operations). The SQL queries are parameterless aggregates — no user-supplied values are interpolated into query strings.

**Blast radius if compromised**: The function is read-only. It executes `SELECT` aggregates only. No writes, no deletes. A bug in this function cannot corrupt store state.

**Indirect injection surface**: The `source` and `category` columns are read from the database, not compared against user input in this function. There is no SQL injection risk from data values because the queries use aggregate functions over column values, not dynamic predicates constructed from those values.

**Conclusion**: No security risks specific to this feature. The attack surface is internal (trusted data, parameterless read-only SQL).

---

## Failure Modes

| Failure | Expected Behavior | Verification |
|---------|------------------|--------------|
| `compute_graph_cohesion_metrics()` returns `Err` | `tracing::warn!` logged; report returned with all six cohesion fields at `0` / `0.0`; no error propagated to MCP caller | Test with injected store error; assert report is returned |
| WAL snapshot lag (`read_pool()` sees older GRAPH_EDGES snapshot) | Cohesion counts undercount by recent-write delta; non-fatal, self-correcting after next checkpoint; no error returned to caller | Not testable deterministically; mitigated by ADR-003 documentation (R-11) |
| `active_entry_count = 0` (empty store) | `connectivity_rate = 0.0`, `mean_entry_degree = 0.0` via explicit guard; no `NaN`/`inf` | Unit test with empty store |
| `StatusReport` fields missing from `default()` | Compile error — caught at `cargo check` | Compile-time; no runtime failure mode |
| `connected_entry_count > active_entry_count` due to double-count | `graph_connectivity_rate > 1.0`; would surface in the `all_connected` unit test | Unit test R-01 scenarios catch this |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 — `source='nli'` literal coupling with #412 | R-09 | ADR-001 introduces `EDGE_SOURCE_NLI` constant in `unimatrix-store/src/read.rs`, re-exported from `lib.rs`. R-09 tests verify the re-export exists and is usable from the server crate. |
| SR-02 — Expensive cross-join at maintenance-tick frequency | R-06, R-11 | Resolved by per-call design (NFR-01) — cohesion is never run from the tick. Pool contention risk (original R-07) eliminated by ADR-003 correction: `read_pool()` is used, not `write_pool_server()`. R-11 captures the residual WAL staleness trade-off, which ADR-003 explicitly accepts. |
| SR-03 — Caching plumbing back to `compute_report` | — | Accepted/resolved by scope decision: per-call SQL, no caching, no `MaintenanceDataSnapshot` involvement. No architecture-level risk remains. |
| SR-04 — Cross-category double JOIN cartesian product risk | R-02 | ADR-004 uses explicit LEFT JOIN aliases with tight equality predicates and a CASE guard. R-02 tests verify the NULL-guard excludes deprecated endpoints and prevents false positives. |
| SR-05 — Six new fields missing from `StatusReport::default()` | R-04 | R-04 tests and compile-check verify all six fields appear in the `default()` block. |
| SR-06 — New fields not populated via tick path | R-06 | NFR-01 + AC-15: cohesion metrics are not in the tick path. R-06 static check confirms single call site. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 3 scenarios (double-count, bidirectional chain, rate bounds) |
| High | 3 (R-02, R-03, R-05) | 3 + 4 + 2 = 9 scenarios |
| Medium | 4 (R-04, R-06, R-08, R-09) | 5 + 2 + 2 + 2 = 11 scenarios |
| Low | 2 (R-10, R-11) | 2 + 2 = 4 scenarios |
| Eliminated | 1 (R-07) | — |

**Total required test scenarios**: 27

The seven mandatory unit test functions (AC-13) cover R-01, R-02, R-03, R-05 directly. R-04, R-06, R-09 are compile-time or static-check verifications. R-07 is eliminated (write-pool contention removed by ADR-003 correction); its non-fatal error-path requirement is retained under general store-error resilience (see R-07 section). R-08 is covered by the mixed-connectivity test. R-10 requires one non-empty-store Summary format test. R-11 requires an ADR-003 comment check at the call site — no runtime test.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `"lesson-learned failures gate rejection SQL aggregate"` — found #1044 (COUNT DISTINCT bug caught by risk strategy, crt-018) and #1043 (Subquery Dedup Before JOIN Aggregation) directly applicable to R-01.
- Queried: `/uni-knowledge-search` for `"risk pattern SQL JOIN entries status filter"` — found #1588 (Active-only query gotcha), #1043 (dedup pattern).
- Queried: `/uni-knowledge-search` for `"StatusReport struct fields Default impl"` — found #3544 (col-028 cascading struct field compile cycles) elevating R-04 likelihood.
- Queried: `/uni-knowledge-search` for `"write_pool_server connection pool timeout"` — found #2058 (write pool 5s timeout), #2130 (max_connections=1 WAL contention) informing R-07.
- Queried: `/uni-knowledge-search` for `"connected_entry_count COUNT DISTINCT UNION subquery"` — found #1043, #1044 confirming R-01 as Critical.
- Stored: nothing novel to store — R-01 COUNT DISTINCT double-count pattern already captured in #1043/#1044. No new cross-feature pattern visible from col-029 alone.
