# Agent Report: 881-agent-1-fix (uni-rust-dev)

Fix committed: `ee75d043` on `bugfix/881-graph-resolve-supersessions-default`.
Fix-summary posted: https://github.com/dug-21/unimatrix/issues/881#issuecomment-4870444570

## Outcome
All five approved steps implemented, plus one required extension.

### Files modified
- `crates/unimatrix-server/src/mcp/graph_read_neighbors.rs` — default flip + G1 (depth-1 `neighbors_sql` resolve) + `resolve_memoized` per-request memo (N1).
- `crates/unimatrix-server/src/mcp/graph_read_subgraph.rs` — default flip + record RESOLVED edge endpoints (in-memory + DB-fallback) so resolved edges survive the dangling filter.
- `crates/unimatrix-server/src/mcp/graph_read_path.rs` — default flip.
- `crates/unimatrix-server/src/mcp/get_edges.rs` — NG-1 edge-target resolution threaded on `resolve_targets`, byte-identity-preserving.
- `crates/unimatrix-server/src/mcp/graph_read.rs` — `GraphParams.resolve_supersessions` field doc.
- `crates/unimatrix-server/src/mcp/tools.rs` — both `CONTEXT_GRAPH_DESCRIPTION` copies + `build_edges_view` call site.
- `crates/unimatrix-server/src/mcp/get_edges_tests.rs` — call-site arg + 2 NG-1 tests.
- `crates/unimatrix-server/src/mcp/graph_read_tests.rs` — register cross-surface child module.
- `crates/unimatrix-server/src/mcp/graph_read_cross_surface_tests.rs` (new) — 2 cross-surface tests.

### Extension beyond the 5 steps (subgraph edge endpoints)
The subgraph handler resolved the neighbor NODE but recorded the edge with the RAW target; the post-BFS dangling-edge filter then dropped that edge (raw target absent from the resolved node set), leaving X' as a disconnected node. Latent even under explicit `true`; subgraph resolve had zero test coverage, so it was invisible. Fixed both edge-recording sites to substitute the resolved endpoint. Required for the cross-surface consistency deliverable. No existing subgraph test regressed (all fixtures use active targets => resolved == raw).

### New tests
- `test_cross_surface_default_resolves_to_terminal` — supersede X->X' once; assert context_get + neighbors@**default depth 1** + subgraph + path all surface X' by default.
- `test_neighbors_explicit_false_returns_raw` — `resolve_supersessions:false` returns raw X.
- `test_ng1_deprecated_target_resolves_to_terminal` / `test_ng1_escape_hatch_keeps_raw_target`.

### Tests: pass/fail
Full-workspace `cargo test` green (0 failures). Targeted `mcp::` lib: 961 passed. `graph_subgraph_integration`: 3 passed. clippy clean, fmt applied (out-of-scope churn reverted). All source files <=500 lines.

### ADR correction (step 5, atomic)
`context_correct` #5388 -> #5409: retracted only the graph flip-direction prohibition; preserved the per-intent principle; kept the reverse prohibition (never flip context_get to as-stored); recorded N2 (silent vs loud resolution, incl. uni-capability gating risk) and N3 (subgraph node-collapse deliberate). 2 incoming edges redirected.

### Issues / blockers
None. Follow-up flagged (not in this PR): vision session must correct SL7 (#5390) non-goal post-merge.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #5388, #5387, #4507, #4494, #4468; applied directly (ADR override, NG-1 semantics, resolution mechanics, 50-hop CTE cap).
- Stored: corrected #5388 -> #5409 via `context_correct` (the load-bearing stewardship action). Nothing else novel — remaining findings are issue-specific code defects (bugs are GH issues, not lessons); resolution mechanics already exist as patterns #4494/#4468.
