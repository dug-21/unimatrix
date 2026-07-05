# Gate 3b Report: vnc-043

> Gate: 3b (Code Review)
> Date: 2026-07-05 (rework iteration 1 re-validated)
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | Dispatch + shared sort helper implemented exactly as `subgraph-depth1-dispatch.md`; doc edits match `doc-surfaces.md` |
| 2. Architecture compliance | PASS | ADR-001/002/003 honored; exact `==1` before lock, no depth-1 lock, depth>1 set-unchanged, no GraphParams change |
| 3. Interface implementation | PASS | `sort_subgraph_output` shared free fn on both paths; signatures unchanged; no new helper invented |
| 4. Test case alignment | PASS | Every risk-driven scenario in the test plan has a corresponding test; R-06 depth>1 sweep + R-10 wire = 3c |
| 5. Code quality | PASS* | Compiles clean; no stubs/todo/unsafe; no `.unwrap()` in non-test. *Pre-existing >500-line files (see WARN) |
| 6. Security | PASS | Doc-only + read-path; no secrets, no new input surface, no path/command injection, no deser change |
| 7. Knowledge stewardship | PASS | Rework f0983673: wave-1 report now present with `## Knowledge Stewardship` (Queried context_briefing/context_get → ADR-001/003, lessons #4562/#4479; Stored #5456) |

**Rework iteration 1 (commit f0983673) — re-validated 2026-07-05:** all three narrowed items confirmed. (1) `agents/vnc-043-agent-3-subgraph-depth1-dispatch-report.md` now exists with a proper stewardship block (Queried evidence + Stored #5456). (2) crt-057 `build_report` reflow reverted — tools.rs feature diff vs merge-base is exactly 3 intended hunks (description const @91, impl attr @3959, test assertions @6248); no `build_report` in the diff. (3) No code drift — `cargo test -p unimatrix-server --lib` = 4373 passed / 0 failed / 1 ignored (incl. #869 byte-equality guard). Gate result flips REWORKABLE FAIL → PASS.

Build: `cargo build -p unimatrix-server` clean. Tests: `cargo test -p unimatrix-server --lib` = 4373 passed, 0 failed, 1 ignored. Clippy: `-p unimatrix-server --all-targets` no warnings. #869 byte-equality guard GREEN.

## Detailed Findings

### 1. Pseudocode fidelity — PASS
`graph_read_subgraph.rs` implements both edits from the pseudocode verbatim:
- Depth-1 dispatch: `if max_depth == 1 { return subgraph_via_db(store, &seed_ids, max_depth, max_nodes, &petgraph_dirs, &edge_types, resolve_supersessions).await; }` inserted after `resolve_supersessions` resolution, before "Step 2: Acquire graph snapshot" lock block — the load-bearing insertion point (SR-07).
- Uniform ordering: `sort_subgraph_output(&mut nodes, &mut edges)` added as final assembly step in BOTH `handle_subgraph` warm-BFS and `subgraph_via_db`, after the R-05 dangling `retain` filter, before `SubgraphResponse`. `depth_reached` computed before the sort and left in place. Stable `sort_by` used for edges as specified.

### 2. Architecture compliance — PASS
- Exact `== 1` match (never `<=1`/range) — depth>1 falls through unchanged (AC-02). ✓
- Depth-1 path takes ZERO `TypedGraphState` lock — early return precedes `typed_graph_state.read()` (A3/AC-10). ✓
- depth>1 + `use_fallback` cold-start branch byte-unchanged at SET level; only added effect is the presentation-only sort. ✓
- Shared `sort_subgraph_output` on both paths after the dangling filter (SR-03 — one ordering contract, structurally enforced). ✓
- No `GraphParams` wire/struct change — only schemars doc-comment text edited. ✓

### 3. Interface implementation — PASS
`sort_subgraph_output(nodes: &mut [EntryRecord], edges: &mut [EdgeRecord])` private free fn; slice params (sort works on slices, cleaner than `&mut Vec`). No `subgraph_sql` helper invented. `subgraph_via_db` signature reused unchanged.

### 4. Test case alignment — PASS
Risk-to-test mapping verified against `test-plan/subgraph-depth1-dispatch.md` and `doc-surfaces.md`:
- R-01: `test_subgraph_depth1_routes_live`, `test_subgraph_depth2_served_from_cache` ✓
- R-03: `test_subgraph_depth1_set_parity_vs_warm_cache` ✓
- R-04: `test_subgraph_depth1_dangling_edge_filtered_under_cap` ✓
- R-05: `test_subgraph_depth1_entryrecord_field_and_tag_parity` ✓
- R-06: `test_subgraph_depth1_node_and_edge_ordering`, `test_subgraph_depth_gt1_same_ordering_keys`, `test_subgraph_dod_oneshot_deterministic` ✓
- R-09: `test_subgraph_depth1_truncated_false_realistic_fanin` ✓
- R-11: `test_subgraph_depth1_direction_label_invariant` ✓
- R-07 (doc): `test_graph_tool_attr_description_matches_const` (#869, twin byte-equality) + extended substring asserts + `test_graphparams_schemars_docs_state_subgraph_applies` ✓
- R-08 (no-lock): structural — verified by source review (early return precedes lock). ✓
- R-06 depth>1 fixed-order sweep and R-10 write-then-read wire test are 3c tester scope (per spawn) — correctly deferred.

### 5. Code quality — PASS (with pre-existing WARN)
- Compiles clean, clippy `--all-targets` no warnings.
- No `todo!()`/`unimplemented!()`/`FIXME`/`TODO`/`unsafe` in added lines. No `.unwrap()` in non-test source.
- **WARN (pre-existing, not this feature's rework):** three changed source files exceed the 500-line rule:
  `graph_read_subgraph.rs` (742, pre-existing per spawn note — module split is a planned follow-up),
  `graph_read_subgraph_bfs_tests.rs` (1271; was ~754, grew +517 this feature — test file, cumulative-extend pattern),
  `tools.rs` (13222, long-standing crate hub). All pre-existing; recommend a test-module split for bfs_tests in the same follow-up issue as the subgraph.rs split.

### 6. Security — PASS
Change is doc text + a read-path early-return + a pure sort. No secrets, no new external input surface (validation runs before dispatch, unchanged), no file/path/command handling, no serialization change. `subgraph_via_db` was already the cold-start path — promoting it to the default read path introduces no new sink.

### 7. Knowledge stewardship — PASS (was REWORKABLE FAIL; fixed in f0983673)
**Rework resolved.** `agents/vnc-043-agent-3-subgraph-depth1-dispatch-report.md` now exists with a `## Knowledge Stewardship` block: `Queried:` `context_briefing` + `context_get` surfacing ADR-001 vnc-043 (#5448), ADR-003 vnc-043 (#5450), lesson #4562 (`use_fallback` lock guard), ADR-005 vnc-018 (#4479); `Stored:` entry #5456 (pattern — cached-read→live-SQL dispatch breaks in-memory graph fixtures). Deviations: none declared. Original finding below retained for provenance.

**Original finding (superseded):**
`product/features/vnc-043/agents/` contains only two reports:
- `vnc-043-agent-1-architect-report.md` (design phase) — has a stewardship-equivalent decisions/edges block.
- `vnc-043-agent-4-doc-surfaces-report.md` (wave-2 rust-dev) — has a proper `## Knowledge Stewardship` block (Queried #5449/#4479; Stored #5457).

**Missing:** the wave-1 implementer that produced commit `dbee4c7e` (subgraph depth-1 dispatch + uniform ordering + 517 lines of BFS tests) left **no agent report at all**, hence no `## Knowledge Stewardship` block. Per the Gate 3b check-7 rule, a missing stewardship block is a REWORKABLE FAIL. The code itself is production-ready; this is a glass-box/process gap, not a code defect.

## Rework Required — RESOLVED

| Issue | Which Agent | What to Fix | Status |
|-------|-------------|-------------|--------|
| Wave-1 dispatch implementer left no agent report → no stewardship block | wave-1 uni-rust-dev | Write agent report with `## Knowledge Stewardship` block (Queried + Stored/nothing-novel). | DONE (f0983673) — report present, Queried evidence + Stored #5456 |

## Notes (non-blocking)
- Out-of-scope fmt churn (crt-057 `build_report(...)` reflow in `tools.rs`) — REVERTED in f0983673. Confirmed: tools.rs feature diff vs merge-base is now exactly 3 intended hunks (description const, impl attr, test assertions); no `build_report` in the diff.
- Pre-existing 500-line violations (subgraph.rs 742, bfs_tests.rs 1271, tools.rs 13222) — track a module/test-module split in the planned follow-up GH issue.
