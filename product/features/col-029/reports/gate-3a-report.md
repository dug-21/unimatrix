# Gate 3a Report: col-029

> Gate: 3a (Component Design Review)
> Date: 2026-03-26
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All four components match Layer 1–4 decomposition exactly; data flow, file assignments, and Phase 5 insertion point all match |
| Specification coverage (all 17 AC) | WARN | FR-12 summary condition differs between spec text ("when total_active > 0") and architecture/pseudocode (conditional on three fields > 0); architecture is authoritative and consistent |
| Risk coverage | PASS | All critical and high risks (R-01, R-02, R-03, R-05) addressed in both pseudocode and test plans; medium/low risks appropriately handled |
| Interface consistency | PASS | GraphCohesionMetrics and StatusReport fields consistent across all four pseudocode files; no contradictions |
| ADR compliance | PASS | ADR-001 (EDGE_SOURCE_NLI constant), ADR-002 (two queries), ADR-003 (read_pool), ADR-004 (cross-category LEFT JOIN guards) — all confirmed present |
| read_pool() usage | PASS | Both fetch_one calls use self.read_pool(); write_pool_server() absent from function |
| UNION sub-query for connected_entry_count | PASS | Scalar sub-query using UNION eliminates double-count (R-01 critical); not COUNT DISTINCT sum |
| cross_category CASE guard IS NOT NULL | PASS | All three guards present: ge.id IS NOT NULL, src_e.category IS NOT NULL, tgt_e.category IS NOT NULL (R-02) |
| Knowledge stewardship | PASS | All four component pseudocode files contain a Knowledge Stewardship section with Queried entries |

---

## Detailed Findings

### Architecture Alignment

**Status**: PASS

**Evidence**:

OVERVIEW.md maps four components to exact files:
- `store-cohesion-query` → `crates/unimatrix-store/src/read.rs` + `lib.rs` (matches ARCHITECTURE.md Layer 1)
- `status-report-fields` → `crates/unimatrix-server/src/mcp/response/status.rs` (matches Layer 2)
- `service-call-site` → `crates/unimatrix-server/src/services/status.rs` (matches Layer 3)
- `format-output` → `crates/unimatrix-server/src/mcp/response/status.rs` (matches Layer 4)

Data flow in OVERVIEW.md matches ARCHITECTURE.md exactly: Query 1 (pure graph_edges) → Query 2 (entries LEFT JOIN graph_edges) → Rust derivation → six field assignments → format output.

Phase 5 insertion point in service-call-site.md: "after report.graph_stale_ratio is assigned (~line 669), before embed_dim computation" — consistent with ARCHITECTURE.md Layer 3 specification.

ADR-003 comment present in store-cohesion-query.md function docstring: "Uses read_pool() — consistent with compute_status_aggregates() (ADR-003 col-029). WAL snapshot semantics are intentional: bounded staleness is acceptable for this diagnostic aggregate."

Component interactions match ARCHITECTURE.md integration surface table — all six StatusReport fields, GraphCohesionMetrics struct, and lib.rs re-export are accounted for.

---

### Specification Coverage (All 17 AC)

**Status**: WARN

**Evidence for each FR**:

- **FR-01** (`pub async fn` returning `Result<GraphCohesionMetrics>`, using `read_pool()`, two SQL queries): store-cohesion-query.md line 71. PASS.
- **FR-02** (`bootstrap_only = 0` filter on all metrics): Query 1 WHERE clause and Query 2 LEFT JOIN condition both include `bootstrap_only = 0`. PASS.
- **FR-03** (Active-only entry join `status = 0`): `WHERE e.status = 0` on outer FROM in Query 2; `src_e.status = 0` and `tgt_e.status = 0` in LEFT JOIN ON clauses; `ce.status = 0` in UNION sub-query. PASS.
- **FR-04** (connectivity_rate = connected/active, 0.0 if active=0): `connectivity_rate = if active > 0 { connected as f64 / active as f64 } else { 0.0 }`. PASS.
- **FR-05** (isolated = total_active - connected_active, saturating_sub): `let isolated = (active as u64).saturating_sub(connected as u64)`. PASS.
- **FR-06** (cross_category with IS NOT NULL on both aliases): CASE guard `ge.id IS NOT NULL AND src_e.category IS NOT NULL AND tgt_e.category IS NOT NULL AND src_e.category != tgt_e.category`. PASS.
- **FR-07** (`supports_edge_count` from `relation_type = 'Supports'`): Query 1 CASE expression. PASS.
- **FR-08** (`mean_entry_degree = (2 * total_edges) / active`, 0.0 guard): `if active > 0 { (2.0 * total_edges as f64) / active as f64 } else { 0.0 }`. PASS.
- **FR-09** (`inferred_edge_count` from `source = 'nli'`): Query 1 CASE expression using the literal `'nli'`. PASS.
- **FR-10** (six StatusReport fields with correct types): status-report-fields.md specifies all six with correct types. PASS.
- **FR-11** (Phase 5 call, non-fatal Err handling, six field assignments): service-call-site.md shows all six assignments in Ok arm and `tracing::warn!` in Err arm. PASS.
- **FR-12** (Summary format line): **WARN** — spec FR-12 says "append a graph cohesion line whenever total_active > 0" but the pseudocode condition is `isolated_entry_count > 0 || cross_category_edge_count > 0 || inferred_edge_count > 0`. ARCHITECTURE.md also uses this condition ("present only when isolated_entry_count + cross_category_edge_count + inferred_edge_count > 0"). The architecture document supersedes the spec text; the pseudocode is consistent with architecture. The format-output pseudocode documents this as an intentional trade-off ("avoids noise on empty stores"). The test plan notes the edge case where only `supports_edge_count > 0` suppresses the Summary line. No fix required; documented as accepted trade-off.
- **FR-13** (Markdown `#### Graph Cohesion` sub-section): format-output.md uses `####` not `###`. This is explicitly documented in the pseudocode: "The SPECIFICATION FR-13 uses `### Graph Cohesion`. The IMPLEMENTATION BRIEF and ARCHITECTURE both use `#### Graph Cohesion`." Architecture is authoritative. PASS (the discrepancy is documented and resolved).
- **FR-14** (seven unit test scenarios): All seven named test functions present in store-cohesion-query.md. Plus one additional `test_graph_cohesion_empty_store` for R-05 zero-denominator coverage. PASS.
- **NFR-01** (per-call, not tick): service-call-site.md constraint block states "The block is placed inside compute_report(), not inside load_maintenance_snapshot() or maintenance_tick()." PASS.
- **NFR-02** (two-query maximum): Exactly two `sqlx::query` + `fetch_one` calls. PASS.
- **NFR-03** (no schema migration): No DDL in any pseudocode. PASS.
- **NFR-04** (no new crate dependency): Only `sqlx` and scalar arithmetic. PASS.
- **NFR-05** (≤50 lines for struct+function): store-cohesion-query.md shows the function is ~40 lines; struct is ~14 lines; constant is 2 lines. Estimated ~56 lines total. Slightly above budget. **WARN** — actual line count will depend on Rust formatting; the agent should monitor this during implementation. The OVERVIEW.md notes "50 additional lines" and documents the existing 1570-line concern.
- **NFR-06** (no lambda change): No lambda-related code in any pseudocode. PASS.
- **NFR-07** (read_pool usage): Confirmed above. PASS.

**Issue (WARN only)**: NFR-05 estimates ~56 lines for the new code vs. the 50-line budget. The spec also says "if read.rs reaches 500 lines, split to read_graph.rs" but it is already at 1570 lines — the ARCHITECTURE clarifies the correct threshold. This is a pre-existing concern documented in the architecture and pseudocode; it does not block implementation.

---

### Risk Coverage

**Status**: PASS

**Evidence**:

**R-01 (Critical — double-count)**:
- Pseudocode uses UNION scalar sub-query for `connected_entry_count`: `SELECT COUNT(*) FROM (SELECT source_id AS id ... UNION SELECT target_id AS id ...) JOIN entries ce ON ce.id = connected_ids.id AND ce.status = 0`. This eliminates overlap between entries appearing on both sides of different edges.
- `test_graph_cohesion_all_connected`: chain A→B→C where B is both source and target. Asserts `connectivity_rate == 1.0` (not 4/3). The `connectivity_rate <= 1.0` explicit bounds check also present.
- `test_graph_cohesion_mixed_connectivity`: partial connectivity scenario also catches double-count.
- Coverage requirement from risk strategy is met: chain topology is used (not just star topology).

**R-02 (High — NULL guard on deprecated endpoints)**:
- CASE guard: `WHEN ge.id IS NOT NULL AND src_e.category IS NOT NULL AND tgt_e.category IS NOT NULL AND src_e.category != tgt_e.category`. All three IS NOT NULL guards present.
- `test_graph_cohesion_cross_category`: inserts A→D where D is deprecated (status=1). Asserts `cross_category_edge_count == 1` (not 2, proving the NULL guard works).
- `test_graph_cohesion_mixed_connectivity`: C→E edge (active→deprecated) asserts `cross_category_edge_count == 0`.

**R-03 (High — bootstrap NLI edge leak)**:
- Query 1 WHERE clause `WHERE bootstrap_only = 0` applies to ALL three aggregates including `inferred_edge_count`.
- `test_graph_cohesion_bootstrap_excluded`: all edges have `bootstrap_only=1`, including one with `source='nli'`. Asserts `inferred_edge_count == 0`.
- `test_graph_cohesion_nli_source`: inserts both `bootstrap_only=0` and `bootstrap_only=1` NLI edges. Asserts `inferred_edge_count == 1`.

**R-05 (High/Medium — division by zero)**:
- Division guards: `if active > 0 { ... } else { 0.0 }` for both `connectivity_rate` and `mean_entry_degree`.
- `test_graph_cohesion_all_isolated`: 3 active entries, 0 edges. Asserts `mean_entry_degree == 0.0` and explicitly asserts `!is_nan()` and `!is_infinite()`.
- `test_graph_cohesion_empty_store` (additional): zero active entries, zero edges — the `active=0` branch.
- Both branches of the guard are covered.

**R-04 (Medium — StatusReport::default() missing field)**:
- status-report-fields.md shows all six fields in both struct body and default() block.
- test_status_report_default_cohesion_fields: constructs `StatusReport::default()` and asserts all six values.
- Compile check is primary enforcement.

**R-06 (Medium — call from maintenance tick)**:
- service-call-site.md documents: "The block is placed inside compute_report(), not inside load_maintenance_snapshot() or maintenance_tick()."
- service-call-site test plan: single call site grep check documented.

**R-08 (Medium — HashSet logic if not using UNION)**:
- Pseudocode uses UNION sub-query approach, not HashSet, eliminating R-08 entirely. The sub-query inherently handles active-only filter via the JOIN to entries.
- `test_graph_cohesion_mixed_connectivity` still exercises the deprecated-endpoint case as secondary coverage.

**R-09 (Medium — EDGE_SOURCE_NLI re-export missing)**:
- OVERVIEW.md shows explicit re-export: `pub use read::{..., GraphCohesionMetrics, EDGE_SOURCE_NLI, ...}`.
- store-cohesion-query.md lib.rs section shows the same.
- test plan includes grep check for `pub use` in lib.rs.

**R-10 (Low — Summary line omitted on empty store)**:
- format-output test plan has explicit sub-test for `StatusReport::default()` asserting the line is suppressed.

**R-11 (Low — WAL staleness)**:
- ADR-003 comment present in function docstring; accepted trade-off documented. No runtime test required per risk strategy.

---

### Interface Consistency

**Status**: PASS

**Evidence**:

`GraphCohesionMetrics` struct fields are consistent across:
- ARCHITECTURE.md Integration Surface table (six fields with types)
- OVERVIEW.md Shared Types section
- store-cohesion-query.md struct definition
- format-output.md data flow section

`StatusReport` six new fields are consistent across:
- ARCHITECTURE.md Layer 2 specification
- SPECIFICATION.md FR-10 domain model
- OVERVIEW.md Modified section
- status-report-fields.md struct modification
- service-call-site.md Ok arm assignments
- format-output.md data flow

Function signature `pub async fn compute_graph_cohesion_metrics(&self) -> Result<GraphCohesionMetrics>` is consistent between ARCHITECTURE.md Integration Surface and store-cohesion-query.md.

The format-output pseudocode uses `report.total_active` to compute the connected-count display (`total_active.saturating_sub(isolated_entry_count)`). This is a pre-existing StatusReport field. The computation is correctly documented as derived arithmetic, not a new field.

No contradictions between any component pseudocode files.

---

### ADR Compliance

**Status**: PASS

**Evidence**:

- **ADR-001 (EDGE_SOURCE_NLI constant)**: Constant defined as `pub const EDGE_SOURCE_NLI: &str = "nli"` in store-cohesion-query.md; re-exported from lib.rs in OVERVIEW.md. Location is read.rs near GraphEdgeRow definitions, per ADR-001.
- **ADR-002 (two SQL queries)**: store-cohesion-query.md has exactly two `sqlx::query(...).fetch_one(self.read_pool()).await` calls — `sql_q1` and `sql_q2`. No third query. Mean degree and connectivity rate are Rust-computed from the two query results.
- **ADR-003 (read_pool for both queries)**: Both `fetch_one(self.read_pool())` calls confirmed. `write_pool_server()` not present. ADR-003 comment in function docstring explicitly references the decision. Test plan service-call-site.md includes pool usage grep check as AC-17 verification.
- **ADR-004 (explicit LEFT JOIN, no cartesian product)**: Query 2 uses `LEFT JOIN entries src_e ON src_e.id = ge.source_id AND src_e.status = 0` and `LEFT JOIN entries tgt_e ON tgt_e.id = ge.target_id AND tgt_e.status = 0`. The CASE guard includes all four conditions per ADR-004.

---

### Spawn-Prompt Specific Checks

**Status**: PASS

All three specific checks from the spawn prompt validated:

1. **read_pool() not write_pool_server()**: store-cohesion-query.md shows `fetch_one(self.read_pool())` for both queries. The function docstring states this explicitly.

2. **UNION sub-query approach for connected_entry_count**: The pseudocode uses a scalar sub-query:
   ```
   ( SELECT COUNT(*) FROM (
       SELECT source_id AS id FROM graph_edges WHERE bootstrap_only = 0
       UNION
       SELECT target_id AS id FROM graph_edges WHERE bootstrap_only = 0
   ) AS connected_ids
   JOIN entries ce ON ce.id = connected_ids.id AND ce.status = 0
   ) AS connected_entry_count
   ```
   This is NOT `COUNT(DISTINCT source_id) + COUNT(DISTINCT target_id)`. R-01 is resolved at the SQL level.

3. **cross_category CASE guard IS NOT NULL on both src_e.category and tgt_e.category**: The CASE expression is `WHEN ge.id IS NOT NULL AND src_e.category IS NOT NULL AND tgt_e.category IS NOT NULL AND src_e.category != tgt_e.category`. All three guards confirmed.

---

### Knowledge Stewardship

**Status**: PASS

All four component pseudocode files contain a `## Knowledge Stewardship` section:

- **store-cohesion-query.md**: Queried for graph cohesion SQL patterns; found #726, #1588. Notes deviation from established patterns: none.
- **status-report-fields.md**: Queried and found #3544 (col-028 cascading struct compile cycles), #704. Elevated R-04 risk rating.
- **service-call-site.md**: Queried and found #726, #1588. Confirmed established Phase 4 co-access precedent.
- **format-output.md**: Queried and found #298, #307. Confirmed push_str + format! pattern.

All four agents used `/uni-query-patterns` before designing and documented what they found. The stewardship entries have specific entry numbers with rationale. No "nothing novel to store" without reason.

---

## Warnings (non-blocking)

| Warning | Source | Disposition |
|---------|--------|-------------|
| FR-12 summary condition: spec says "whenever total_active > 0" but pseudocode/architecture use `isolated + cross_category + inferred > 0` | Spec/Architecture inconsistency | Architecture is authoritative; pseudocode correct; documented in format-output.md |
| FR-13 heading level: spec uses `### Graph Cohesion`, architecture uses `#### Graph Cohesion` | Spec/Architecture inconsistency | Architecture is authoritative; documented in format-output.md and test plan |
| NFR-05 line budget: estimated ~56 lines vs. 50-line budget | Minor over-estimate | Rust formatting may bring actual within budget; split to read_graph.rs is out of scope per architecture; monitor during implementation |
| R-05 zero-active-entries branch not covered by mandatory 7 tests | test_graph_cohesion_empty_store is an 8th additional test | Covered by additional test; R-05 requirement from risk strategy is satisfied |

---

## Rework Required

None. Gate 3a PASS.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns prior to gate review for `graph cohesion SQL double-count validation patterns` and `StatusReport struct field alignment gate review` — confirmed that UNION sub-query dedup pattern (#1043, #1044) is the correct R-01 mitigation and that this pseudocode applies it correctly.
- Stored: nothing novel to store — the validation patterns for COUNT DISTINCT double-count and NULL guard completeness are already in the knowledge base (#1043, #1044). This gate confirms correct application; no new cross-feature pattern.
