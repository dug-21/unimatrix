# crt-014 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | petgraph with `stable_graph` feature added to `unimatrix-engine/Cargo.toml`; workspace builds without warnings | shell | `cargo build --workspace 2>&1 \| grep "^error" \| wc -l` == 0 | PENDING |
| AC-02 | `pub mod graph` exported from `unimatrix-engine` | shell | `cargo doc --package unimatrix-engine 2>&1 \| grep -q "graph"` | PENDING |
| AC-03 | `build_supersession_graph` returns `Err(CycleDetected)` for cyclic supersession entries (A→B→A) | test | `#[test] fn cycle_two_node_detected()` in `graph.rs` | PENDING |
| AC-04 | `build_supersession_graph` returns `Ok` for valid DAGs: depth 1, 2, 3+ | test | `#[test] fn valid_dag_depth_{1,2,3}()` in `graph.rs` | PENDING |
| AC-05 | `graph_penalty` returns value in `(0.0, 1.0)` for all penalized inputs | test | `#[test] fn penalty_range_all_scenarios()` in `graph.rs` — sample each topology type | PENDING |
| AC-06 | Orphan deprecated entry receives softer penalty than superseded entry with active terminal | test | `#[test] fn orphan_softer_than_clean_replacement()`: assert `ORPHAN_PENALTY > CLEAN_REPLACEMENT_PENALTY` | PENDING |
| AC-07 | 2-hop outdated entry receives harsher penalty than 1-hop outdated entry | test | `#[test] fn two_hop_harsher_than_one_hop()`: assert `graph_penalty(A_2hop) < graph_penalty(A_1hop)` | PENDING |
| AC-08 | Partially-superseded entry (>1 successor) receives softer penalty than single-successor entry | test | `#[test] fn partial_supersession_softer_than_clean()`: assert `PARTIAL_SUPERSESSION_PENALTY > CLEAN_REPLACEMENT_PENALTY` | PENDING |
| AC-09 | `find_terminal_active` returns `Some(C)` for A→B→C where B is superseded and C is Active | test | `#[test] fn terminal_active_three_hop_chain()` in `graph.rs`: chain A→B→C, assert result == C.id | PENDING |
| AC-10 | `find_terminal_active` returns `None` when no active terminal reachable | test | `#[test] fn terminal_active_no_reachable()` in `graph.rs`: chain terminates at Deprecated entry, assert `None` | PENDING |
| AC-11 | `find_terminal_active` returns `None` when chain depth exceeds `MAX_TRAVERSAL_DEPTH` | test | `#[test] fn terminal_active_depth_cap()` in `graph.rs`: chain of 11 entries, assert `None` | PENDING |
| AC-12 | In `search.rs` Flexible mode, `penalty_map` populated via `graph_penalty`, not removed constants | test + grep | Integration test: deprecate B, supersede A→B, search returns topology-derived penalty; `grep -r DEPRECATED_PENALTY crates/unimatrix-server` == 0 | PENDING |
| AC-13 | Multi-hop injection: search for superseded A (chain A→B→C, C active) injects C, not B | test | Integration test in `search.rs` or engine integration tests: assert injected successor ID == C.id | PENDING |
| AC-14 | `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` absent from production code | grep | `grep -r "DEPRECATED_PENALTY\|SUPERSEDED_PENALTY" crates/ --include="*.rs"` returns zero non-test hits | PENDING |
| AC-15 | Behavioral ordering tests replace removed constant-value tests in `confidence.rs` | test + grep | No `assert_eq!(DEPRECATED_PENALTY, 0.7)` style assertions; ordering tests present in `graph.rs` test module | PENDING |
| AC-16 | Cycle fallback: `CycleDetected` causes search to log error and use `FALLBACK_PENALTY` | test | Integration test with injected cycle data: assert search succeeds + penalty == `FALLBACK_PENALTY` + log contains cycle message | PENDING |
| AC-17 | Dangling `supersedes` reference skipped with `tracing::warn!` (no panic, no error) | test | `#[test] fn dangling_supersedes_ref_is_skipped()` in `graph.rs`: entry references non-existent pred_id, assert `Ok(graph)` | PENDING |
| AC-18 | Workspace builds clean with no new warnings after all changes | shell | `cargo build --workspace 2>&1 \| grep "^error" \| wc -l` == 0; `cargo build --workspace 2>&1 \| grep "^warning" \| wc -l` same as or fewer than baseline | PENDING |
