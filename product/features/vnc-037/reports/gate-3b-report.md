# Gate 3b Report: vnc-037

> Gate: 3b (Code Review)
> Date: 2026-06-16
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | Code follows the locked 3-bucket pseudocode; shared CANON_CTE, ranked select, split count, seam, assembly all match. |
| 2. Architecture compliance | PASS | All 7 ADRs honored, incl. AMENDED ADR-005 3-bucket totals and ADR-007 symmetric canonicalization. |
| 3. Interface implementation | PASS | `EdgeCountSplit`/`EdgeTotals {inbound,outbound,both}`, `GetEdge` 5-field projection, seam `Option<&EdgesView>` — all as specified. |
| 4. Test-case alignment | PASS | Test plans map to implemented tests; risk scenarios (SR-08/09/10/13/14, #744, #3886, #4876) covered. |
| 5. Code quality | PASS | Builds clean; no stubs/TODO; no `.unwrap()`/`.expect()` on production edge path; new files all < 500 lines. |
| 6. Security | PASS | Positional binds throughout; static SQL for CTE/IN-sets; no injection surface; fail-loud on malformed reads. |
| 7. Knowledge stewardship | N/A (validator) | Validator self-block included below; dev-agent stewardship is a 3a/retro concern. |

## Detailed Findings

### Load-bearing item 1 — Symmetric canonicalization (ADR-007, BLOCKER)
**Status**: PASS
**Evidence**: `graph_queries_ranked.rs:78-102` defines ONE shared `CANON_CTE` (`nbr → canon → deduped`). Both surfaces embed it byte-identically: `query_ranked_neighbors` (`:128-138`) applies it before `ORDER BY…LIMIT`, and `count_neighbors_split` (`:212-220`) applies it before the `SUM(CASE…)`. The `CASE WHEN relation_type IN ('Contradicts','CoAccess','Informs') THEN 'both' ELSE leg END` collapses the three symmetric types; the `GROUP BY relation_type, pair_lo, pair_hi, CASE WHEN direction='both' THEN 1 ELSE other_id END` folds the reciprocal pair to one row on both surfaces. Tests `test_query_ranked_symmetric_collapses_to_one_row` and `test_count_symmetric_counted_once` pass independently (display + totals).

### Load-bearing item 2 — Ranking (D-09 / ADR-006)
**Status**: PASS
**Evidence**: Exact clause `ORDER BY (d.source = 'agent') DESC, t.confidence DESC NULLS LAST, target_id ASC LIMIT ?2` (`:134-137`); `?2` bound to `GET_EDGE_DISPLAY_LIMIT` (`:142`), no literal 3. `LEFT JOIN entries t ON t.id = d.other_id` selects `t.confidence` (`:131-133`) — target-entry confidence, never `weight`. The #3886 discriminating test `test_query_ranked_by_target_confidence_proof_outside_cap` proves the high-confidence target (T1) is included despite lowest weight, and the low-confidence target excluded. All 26 ranked tests pass.

### Load-bearing item 3 — 3-bucket totals (AMENDED ADR-005)
**Status**: PASS
**Evidence**: `EdgeCountSplit {inbound, outbound, both, authored}` (`graph_queries_ranked.rs:54-64`); `EdgeTotals {inbound, outbound, both}` (`response/edges.rs:84-93`). `both` = `SUM(direction='both')`, `inbound` = `SUM(direction='inbound')` ONLY (old `IN ('inbound','both')` fold retired). `authored` = `SUM(source='agent')` over `deduped`, digest-only (`#[serde(skip)]` on `EdgesView.authored_total`). The #744 integrity test `test_count_symmetric_increments_both_never_inbound` passes (`↔` ⇒ `both += 1`, `inbound` unchanged). JSON `edge_totals` is a 3-key object (`test_edge_totals_inbound_outbound_both_object`, `test_json_edge_totals_three_keys`). `authored_total` threaded from full set (`get_edges.rs:81`), asserted by `test_authored_total_threaded_from_full_set`.

### Load-bearing item 4 — FR-19 fail-loud (AC-14)
**Status**: PASS
**Evidence**: Handler (`tools.rs:982-997`): `Some(false) ⇒ None` (skips all queries); `None|Some(true) ⇒ build_edges_view(...).await.map_err(...)?` with the SAME `ServerError::Core(CoreError::Store(e))` mapping as the primary `entry_store.get` read. No degrade-with-note, no silent omit. Three RED tests pass: `test_edge_query_failure_fails_loud`, `test_count_query_failure_fails_loud`, `test_title_join_failure_fails_loud`. Zero-edge success is structurally distinct: `test_zero_edges_is_success_distinct_from_failure` returns `Ok` with `{0,0,0}` totals. No `.unwrap()`/`.expect()` on the production edge path (verified by grep — all hits are doc comments or `#[cfg(test)]`).

### Load-bearing item 5 — Serializer seam (ADR-003)
**Status**: PASS
**Evidence**: `entry_to_json` and `format_entry_markdown_section` signatures UNCHANGED (`entries.rs:24` doc + diff confirms only `format_single_entry` gained `edges: Option<&EdgesView>`). `None ⇒ key/section absent` is structural: JSON inserts `edges`/`edge_totals` only inside `if let Some(view) = edges` (`entries.rs:51-58`); markdown `### Related` appended only on `Some` (`:43-49`); summary digest only on `Some` (`:30-42`). All list-view callers (`format_search_results`, `format_lookup_results`, `format_store_success`, `format_correct_success`) call the unchanged helpers and never reach the insertion. Byte-identity tests pass: `test_none_json_byte_identical_to_base_object`, `test_none_edges_key_absent_structural`, `test_some_edges_injected_all_formats`, `test_get_zero_edge_empty_state_all_formats`.

### Load-bearing item 6 — DISCOVERY-LIST guardrail (ADR-002)
**Status**: PASS
**Evidence**: `GetEdge` carries EXACTLY `{edge_type, direction, target_id, target_title, authored}` (`response/edges.rs:33-52`) — no `source_id`, `depth`, `metadata`, raw `source`, `weight`, or `target_confidence`. `test_get_edge_exact_five_fields` asserts `obj.len() == 5`; `test_target_confidence_not_in_get_edge` asserts forbidden fields absent. `target_confidence` consumed by SQL ORDER BY and dropped at projection.

### Compilation, stubs, file size
**Status**: PASS
**Evidence**: `cargo build -p unimatrix-store -p unimatrix-server` finishes with no errors. No `todo!`/`unimplemented!`/`TODO`/`FIXME`/`panic!` in vnc-037 files. New files under the 500-line limit: `graph_queries_ranked.rs` (255), `edges.rs` (322), `edges_render.rs` (346), `get_edges.rs` (158). Pre-existing oversized files (`tools.rs` 12312, `read.rs` 3789, `mod.rs` 1982) are not vnc-037-created — vnc-037 added ~107 lines to `tools.rs`, none in scope for the 500-line gate per spawn instructions.

### Additive RawEdgeRow / neighbors source column (ADR-004)
**Status**: PASS
**Evidence**: `RawEdgeRow` gained `source: String` + `target_confidence: Option<f64>` (additive, ADR-004). Neighbors queries (`graph_queries_neighbors.rs`) now `SELECT … source`; `map_edge_row` reads `source` via `try_get` + `StoreError::Database` mapping (no `.unwrap()`), `target_confidence: None` on the plain path. `context_graph` neighbors path unaffected (full server lib suite green).

### Clippy
**Status**: PASS (WARN noted)
**Evidence**: Clippy warnings in `tools.rs` are all at line numbers (51, 599, 713, 1237, 2011, 2541, … 5018) OUTSIDE the vnc-037-added regions (field 256-268, handler 976-997, call 644, tests 5633+). No new warnings introduced by vnc-037 files. Pre-existing warnings out of scope per spawn instructions.

## Test Execution Summary

| Suite | Result |
|-------|--------|
| `cargo test -p unimatrix-store --lib graph_queries_ranked` | 26 passed, 0 failed |
| `cargo test -p unimatrix-server --lib get_edges` | 13 passed, 0 failed |
| `cargo test -p unimatrix-server --lib response::edges*` | 21 passed, 0 failed |
| seam byte-identity tests (4 named) | 4 passed, 0 failed |
| GetParams `include_edges` contract | 2 passed, 0 failed |
| `cargo test -p unimatrix-server --lib` (full) | 4184 passed, 0 failed, 1 ignored |

Note: per spawn ENV, `cargo test --workspace` at default parallelism OOMs the linker; ran per-package as instructed.

## Rework Required

None.

## Knowledge Stewardship
- Stored: nothing novel to store -- Unimatrix MCP disconnected this gate; all findings are feature-specific gate evidence that belongs in this report, not in knowledge store. No cross-feature recurring failure pattern surfaced (clean PASS).
