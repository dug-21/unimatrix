# Gate 3b Report: crt-042

> Gate: 3b (Code Review)
> Date: 2026-04-02
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity — graph_expand.rs | PASS | BFS logic, visited-set, depth limit, sorted frontier, early exit all match pseudocode exactly |
| Pseudocode fidelity — Phase 0 in search.rs | PASS | Insertion point, guard structure, fetch loop, quarantine check, debug trace all match pseudocode |
| Pseudocode fidelity — InferenceConfig | PASS | All five coordinated sites implemented; merge_configs literal updated |
| Architecture compliance | PASS | #[path] submodule pattern, re-export from graph.rs, lock order preserved, no lib.rs exposure |
| Interface implementation | PASS | graph_expand signature matches; SearchService three fields wired from InferenceConfig in new() |
| Test case alignment — graph_expand | PASS | All 21 test cases from test plan implemented in graph_expand_tests.rs |
| Test case alignment — InferenceConfig | PASS | All 16 config tests implemented; AC-17 through AC-21 including boundary cases and merge test |
| Test case alignment — Phase 0 search.rs | FAIL | Zero Phase 0 tests implemented: AC-01, AC-02, AC-13/14, AC-15, AC-24, AC-25 all absent |
| Code quality — no stubs/TODOs | PASS | No todo!(), unimplemented!(), TODO, or FIXME in crt-042 files |
| Code quality — no .unwrap() in non-test code | PASS | graph_expand.rs clean; search.rs .unwrap() instances are pre-existing and in test sections |
| Code quality — file size | PASS | graph_expand.rs 178 lines, graph_expand_tests.rs 415 lines; both within 500-line limit |
| Compilation | PASS | cargo build --workspace clean; Finished dev profile with 0 errors |
| Clippy — crt-042 files | PASS | No clippy errors in graph_expand.rs, graph_expand_tests.rs, search.rs (Phase 0 block), config.rs |
| Clippy — workspace | WARN | Pre-existing collapsible_if errors in auth.rs, event_queue.rs, unimatrix-observe; not introduced by crt-042 |
| Test suite | PASS | All tests pass: cargo test --workspace 0 failures |
| AC-16 traversal boundary — grep check | PASS | grep for edges_directed/neighbors_directed in graph_expand.rs returns zero real calls (only doc-comment mentions) |
| R-08 InferenceConfig literal scan | PASS | All InferenceConfig { literals use ..InferenceConfig::default() or ..Default::default() spread; merge_configs literal explicitly updated with three new fields |
| Security — no hardcoded secrets | PASS | No credentials or API keys in crt-042 files |
| Security — input validation | PASS | Config validation unconditional; quarantine check on every expanded entry |
| Knowledge stewardship — implementation agents | PASS | All three rust-dev agent reports (agent-3 graph-expand, agent-4 config, agent-5 phase0) contain Queried + Stored entries |
| Eval profile | PASS | ppr-expander-enabled.toml committed at correct location with correct content |

---

## Detailed Findings

### Pseudocode Fidelity — graph_expand.rs

**Status**: PASS

**Evidence**: Implementation at `/workspaces/unimatrix/crates/unimatrix-engine/src/graph_expand.rs` matches pseudocode exactly:
- Degenerate case guards (empty seeds, empty graph, depth == 0) at lines 75–83
- Visited set seeded with all seed IDs via `seed_ids.iter().copied().collect()` (line 89)
- BFS queue carries `(entry_id, current_hop_depth)` (line 91)
- Seeds whose `entry_id` is NOT in `graph.node_index` are silently skipped (lines 95–99)
- `can_expand_further = current_depth < depth` (line 111)
- Four separate `edges_of_type` calls for CoAccess, Supports, Informs, Prerequisite with Direction::Outgoing (lines 121–134)
- `neighbors.sort_unstable(); neighbors.dedup()` (lines 139–140)
- `if !can_expand_further { continue; }` guard (lines 146–148) — depth-limit nodes are in result but don't enqueue neighbors
- Early exit via `break 'outer` when `result.len() >= max_candidates` (lines 151–154)
- All neighbors added to `visited` and `result` before being enqueued (lines 160–167)

One minor behavioral note: the pseudocode describes "still process the node itself, but don't enqueue its neighbors" for depth-limit nodes. The implementation handles this correctly because depth-limit nodes were already added to `result` by their parent at `current_depth - 1`; the `!can_expand_further` guard then skips processing their neighbors. Tests AC-05 and AC-06 verify this is correct.

### Pseudocode Fidelity — Phase 0 in search.rs

**Status**: PASS

**Evidence**: Implementation at lines 870–960 of search.rs:
- Phase 0 block is the FIRST block inside `if !use_fallback` (line 870), before Phase 1 seed_scores construction (line 969) — correct insertion point
- `if self.ppr_expander_enabled` guard (line 888) — Instant::now() is inside this guard (line 889), zero overhead when false
- Seed collection from `results_with_scores` (line 892)
- `graph_expand` call with correct arguments (lines 896–901)
- `in_pool: HashSet<u64>` dedup guard (line 906)
- Sorted expanded IDs for determinism (lines 910–911)
- Entry fetch with silent skip on error (lines 920–923)
- `SecurityGateway::is_quarantined` check before push (lines 925–928) — correct order: after fetch, before push
- `vector_store.get_embedding` with silent skip on None (lines 935–938)
- `cosine_similarity(&embedding, &emb)` with `embedding` (the pre-normalized query embedding) (line 942) — pseudocode uses `query_embedding` but `embedding` is the correct variable name in scope
- `tracing::debug!` with all six mandatory fields: `seeds`, `expanded_count`, `fetched_count`, `elapsed_ms`, `expansion_depth`, `max_expansion_candidates` (lines 951–959)

### Architecture Compliance

**Status**: PASS

**Evidence**:
- `graph.rs` lines 34–36: `#[path = "graph_expand.rs"] mod graph_expand; pub use graph_expand::graph_expand;` — matches ADR-001 submodule pattern
- `lib.rs` not modified — matches ADR-001 (no exposure in lib.rs)
- Phase 0 uses pre-cloned `typed_graph` (lock already released before Step 6d) — lock order preserved (C-04, NFR-06)
- No `spawn_blocking` — pure CPU BFS is synchronous as specified (C-05)

### Interface Implementation

**Status**: PASS

**Evidence**:
- `graph_expand` signature matches specification exactly: `pub fn graph_expand(graph: &TypedRelationGraph, seed_ids: &[u64], depth: usize, max_candidates: usize) -> HashSet<u64>`
- `SearchService` struct: three new fields at lines 382–388 matching spec types and docs
- `SearchService::new()` parameters: three new args at lines 514–516 after `ppr_max_expand`
- Constructor body: three fields assigned at lines 541–543
- `services/mod.rs` call site: three new arguments at lines 432–434 from `inference_config`

### Test Case Alignment — graph_expand

**Status**: PASS

**Evidence**: `graph_expand_tests.rs` (415 lines) implements all 21 test functions from the test plan:
- AC-03: `test_graph_expand_coaccess_surfaces_neighbor`, `test_graph_expand_supports_surfaces_neighbor`, `test_graph_expand_informs_surfaces_neighbor`, `test_graph_expand_prerequisite_surfaces_neighbor`
- AC-04: `test_graph_expand_backward_edge_does_not_surface`
- AC-05: `test_graph_expand_two_hop_depth2_surfaces_both`
- AC-06: `test_graph_expand_two_hop_depth1_surfaces_only_first`
- AC-07: `test_graph_expand_supersedes_not_traversed`, `test_graph_expand_contradicts_not_traversed`
- AC-08: `test_graph_expand_seeds_excluded_from_result`, `test_graph_expand_self_loop_seed_not_returned`
- AC-09: `test_graph_expand_max_candidates_cap` (with sorted-frontier verification)
- AC-10: `test_graph_expand_empty_seeds_returns_empty`
- AC-11: `test_graph_expand_empty_graph_returns_empty`
- AC-12: `test_graph_expand_depth_zero_returns_empty`
- R-11: `test_graph_expand_bidirectional_terminates`, `test_graph_expand_triangular_cycle_terminates`
- R-13/NFR-04: `test_graph_expand_deterministic_across_calls` (with budget-boundary verification asserting {2,3,4} are returned)
- R-02 (S1/S2 docs): `test_graph_expand_unidirectional_informs_from_higher_id_seed_misses`, `test_graph_expand_bidirectional_informs_after_backfill`
- R-17 (S8 docs): `test_graph_expand_s8_coaccess_unidirectional_from_higher_id_misses`

All 22 tests pass (22 in `running 22 tests` result).

### Test Case Alignment — InferenceConfig

**Status**: PASS

**Evidence**: config.rs contains all 16+ tests from the test plan (lines 7649–7888):
- AC-17: `test_inference_config_expander_fields_defaults`, `test_inference_config_expander_fields_serde_defaults`, `test_unimatrix_config_expander_toml_omitted_produces_defaults`, `test_inference_config_expander_serde_fn_matches_default`
- AC-18: `test_validate_expansion_depth_zero_fails`, `test_validate_expansion_depth_one_passes`
- AC-19: `test_validate_expansion_depth_eleven_fails`, `test_validate_expansion_depth_ten_passes`
- AC-20: `test_validate_max_expansion_candidates_zero_fails`, `test_validate_max_expansion_candidates_one_passes`
- AC-21: `test_validate_max_expansion_candidates_1001_fails`, `test_validate_max_expansion_candidates_1000_passes`
- Error message: `test_validate_expansion_depth_error_names_field`, `test_validate_max_expansion_candidates_error_names_field`
- Merge: `test_inference_config_merged_propagates_expander_fields`, `test_inference_config_merged_expander_enabled_project_wins`
- TOML round-trip: `test_inference_config_expander_toml_explicit_override`

All pass in `running 2673 tests test result: ok. 2673 passed`.

### Test Case Alignment — Phase 0 search.rs

**Status**: FAIL

**Evidence**: Zero Phase 0 tests were implemented in `search.rs` or any other unimatrix-server test file. The test plan at `product/features/crt-042/test-plan/phase0_search.md` specifies 10 unit/integration tests. Grep confirms no test functions exist for:

- `test_search_flag_off_pool_size_unchanged` (AC-01) — flag-off regression
- `test_search_phase0_expands_before_phase1` (AC-02) — Phase 0 runs before Phase 1
- `test_search_phase0_excludes_quarantined_direct` (AC-13) — quarantine safety
- `test_search_phase0_excludes_quarantined_transitive` (AC-14) — transitive quarantine
- `test_search_phase0_skips_entry_with_no_embedding` (AC-15) — embedding skip
- `test_search_phase0_emits_debug_trace_when_enabled` (AC-24) — tracing instrumentation
- `test_search_phase0_does_not_emit_trace_when_disabled` (R-10) — no overhead when disabled
- `test_search_phase0_cross_category_entry_visible_with_flag_on` (AC-25) — behavioral proof

AC-25 is classified as MANDATORY in the test plan: "This test is MANDATORY regardless of eval gate outcome." AC-24 was called out in the test plan as mandatory, citing entry #3935: "This test is mandatory. Entry #3935 documents a gate failure where tracing tests were deferred. Do not defer."

The absence of these tests means:
- The core behavioral guarantee (AC-25) is unproven in test
- Quarantine safety for expanded entries (AC-13/14) has no test coverage in the Phase 0 path
- The tracing instrumentation (AC-24) that is mandatory before the flag can default to true is unverified

### Code Quality

**Status**: PASS

**Evidence**:
- No `todo!()`, `unimplemented!()` in crt-042 files
- No `.unwrap()` in graph_expand.rs; `.unwrap()` in search.rs is pre-existing and in test sections only
- graph_expand.rs: 178 lines; graph_expand_tests.rs: 415 lines — both within 500-line limit
- `cargo build --workspace` exits clean: `Finished dev profile [unoptimized + debuginfo] target(s) in 0.20s` with 0 errors

### Clippy

**Status**: WARN (pre-existing, not introduced by crt-042)

**Evidence**: `cargo clippy --workspace -- -D warnings` reports errors in:
- `crates/unimatrix-engine/src/auth.rs:113` — collapsible_if
- `crates/unimatrix-engine/src/event_queue.rs:164` — collapsible_if
- `crates/unimatrix-observe/src/...` — multiple collapsible_if

Zero clippy errors in any crt-042 file (`graph_expand.rs`, `graph_expand_tests.rs`, `search.rs` Phase 0 block, `config.rs` new sections). All errors are pre-existing.

### Security

**Status**: PASS

**Evidence**:
- No hardcoded secrets in any crt-042 file
- Input validation on `expansion_depth` and `max_expansion_candidates` is unconditional at server start
- `SecurityGateway::is_quarantined` called on every expanded entry before push to results_with_scores (line 927 of search.rs) — correct order: after fetch, before push

### Knowledge Stewardship

**Status**: PASS

**Evidence**:
- `crt-042-agent-3-graph-expand-report.md`: Queried entries #3740, #3650, #3950; Stored entry reported
- `crt-042-agent-4-config-report.md`: Queried entries #3817, #4044, #2730; Stored entry #4070 "InferenceConfig extension: five sites..."
- `crt-042-agent-5-phase0-report.md`: Queried entries #3753, #3746, #3730, #4050, #4052, #4054; Stored pattern attempted (MCP error documented; O(1) infeasibility finding documented in report)

All three implementation agent reports contain `Queried:` and `Stored:` (or documented reason) entries.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Phase 0 tests missing: AC-01 (flag-off regression), AC-02 (Phase 0 before Phase 1), AC-13/14 (quarantine safety), AC-15 (embedding skip), AC-24 (tracing instrumentation — mandatory per entry #3935), AC-25 (cross-category behavioral proof — mandatory per test plan) | rust-dev (Phase 0 / search.rs) | Implement all 8 missing test functions in `crates/unimatrix-server/src/services/search.rs` (or a companion `search_phase0_tests.rs` split file). Tests must use the existing Step 6d test infrastructure (see `test_step_6d_*` helpers in search.rs ~lines 4310+). AC-24 requires `tracing-test` subscriber. AC-25 requires mock or real vector store with a dissimilar embedding. |

---

## Knowledge Stewardship

- Stored: nothing novel to store — the Phase 0 test absence is a feature-specific gap, not a recurring pattern novel enough to store separately. The existing lesson for deferred tracing tests (entry #3935) already covers this class of issue.
