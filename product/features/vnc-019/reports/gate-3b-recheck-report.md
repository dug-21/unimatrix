# Gate 3b Recheck Report: vnc-019

> Gate: 3b (Code Review — rework iteration 1)
> Date: 2026-05-20
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| FAIL-1: direction default changed to "both" | PASS | Line 100: `unwrap_or("both")` confirmed |
| FAIL-2: Integration test added (≥5 entries, full call path) | PASS | 3 tests pass in graph_subgraph_integration.rs |
| WARN-1: edge_type error message corrected | PASS | "unrecognized", em-dash, "recognized types:", all 16 variants |
| Compilation | PASS | Clean build, 21 pre-existing warnings only |
| No new regressions | PASS | All 3 integration tests pass |

## Detailed Findings

### FAIL-1: direction default changed from "outgoing" to "both"

**Status**: PASS

`graph_read_subgraph.rs` line 100:
```rust
let direction_str = params.direction.as_deref().unwrap_or("both");
```

FR-05 is now satisfied. The default matches the IMPLEMENTATION-BRIEF BFS Algorithm Contract step 1 and the pseudocode (`unwrap_or("both")`). Integration test `test_subgraph_default_direction_both` exercises this code path — seed B with direction omitted returns incoming neighbors A and D alongside outgoing neighbor C, confirming bidirectional traversal is active by default.

### FAIL-2: Integration test added (FR-23, AC-14)

**Status**: PASS

`crates/unimatrix-server/tests/graph_subgraph_integration.rs` (329 lines) contains 3 tests covering the full MCP call path (`handle_graph → validate_no_unsupported_params → handle_subgraph → SQL reads`):

| Test | Topology | Assertions |
|------|----------|------------|
| `test_subgraph_single_hop_five_entries` | A→B, B→C, A→D; E isolated; seed=[A] max_depth=1 | A,B,D in nodes; C excluded; edge triples A→B and A→D present; B→C absent (dangling-edge filter); depth_reached=1; truncated=false; seed_ids=[A] |
| `test_subgraph_two_hop_linear_chain` | A→B→C→D→E; seed=[A] max_depth=2 | A,B,C in nodes; D,E excluded; depth_reached=2 |
| `test_subgraph_default_direction_both` | B→C, A→B, D→B, E→A; seed=[B] max_depth=1 no direction | B,A,C,D in nodes; E excluded (depth-2 via A) |

All three use `TestHarness` helpers (`insert_graph_edge`, `rebuild_typed_graph`, `call_graph`) which exercise the full store-backed path. All 3 pass:

```
test test_subgraph_single_hop_five_entries ... ok
test test_subgraph_two_hop_linear_chain ... ok
test test_subgraph_default_direction_both ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured
```

FR-23 and AC-14 are satisfied.

### WARN-1: edge_type error message format corrected

**Status**: PASS

`graph_read_subgraph.rs` lines 127–131:
```
"unrecognized edge_type '{}' \u{2014} recognized types: \
 About, Advances, Asserts, Cites, CoAccess, \
 Contradicts, DerivedFrom, Informs, Mentions, \
 Motivates, Prerequisite, Refutes, RelatedTo, \
 Supersedes, Supports, Tests"
```

All four deviations from the IMPLEMENTATION-BRIEF are resolved:
- "unrecognized" (was "unknown") — corrected
- `\u{2014}` em-dash (was `--` double hyphen) — corrected
- "recognized types" (was "valid types") — corrected
- "Supersedes" present (was absent, had 15 types) — added; now 16 types: About, Advances, Asserts, Cites, CoAccess, Contradicts, DerivedFrom, Informs, Mentions, Motivates, Prerequisite, Refutes, RelatedTo, Supersedes, Supports, Tests

### Compilation

**Status**: PASS

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.20s
```

21 warnings — all pre-existing from prior to vnc-019 (confirmed in original gate-3b report). No new warnings introduced by the rework changes.

`cargo audit` is not installed in this environment. This was noted and accepted in the original gate-3b-report (same environment constraint applies).

### No Regressions

**Status**: PASS

The three rework changes (direction default, error message text, new integration test file) are all additive or corrective. No existing tests were modified. The pre-existing 74 graph_read unit tests remain unaffected.

## Knowledge Stewardship

- Stored: nothing novel to store — rework was a targeted fix of three specific items (default value, error string, missing test). The lesson about direction-default deviation was already stored in the original gate-3b session (entry for "direction default 'outgoing' vs 'both' in subgraph mode"). No new patterns emerged from this recheck.
- Queried: context_briefing not called — recheck scope is narrow (verify three previously-failed items only per iteration cap instructions).
