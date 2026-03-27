# Agent Report: 421-agent-2-verify

**Phase:** Test Execution (Bug Fix Verification)
**Bug:** GH #421 — graph inference tick stall due to deterministic source selection
**Fix location:** `crates/unimatrix-server/src/services/nli_detection_tick.rs`

---

## Test Results

### 1. Targeted Unit Tests (nli_detection_tick module)

**Command:** `cargo test -p unimatrix-server --lib nli_detection_tick`

All 22 tests pass. Key fix-verification tests confirmed:

| Test | Result |
|------|--------|
| `test_select_source_candidates_excludes_no_embedding_entries` | PASS |
| `test_select_source_candidates_nondeterministic_rotation` | PASS |
| `test_select_source_candidates_remainder_by_created_at` | PASS |

All pre-existing tests (`cap_enforced`, `cap_larger_than_entries`, `empty_input`, `max_sources_zero`, `isolated_second`, `priority_ordering_combined`, `all_isolated`, `write_inferred_edges_*`, `run_graph_inference_tick_*`, edge cases) continue to pass.

### 2. Full Workspace Test Suite

**Command:** `cargo test --workspace 2>&1 | grep "test result"`

- **Zero failures** across all crates
- Total test count exceeds 3,600 across all test binaries (consistent with prior ~3,400+ baseline)
- No regressions introduced by the fix

### 3. Clippy

**Command:** `cargo clippy --workspace -- -D warnings 2>&1 | head -30`

Multiple `collapsible_if` and other warnings in `unimatrix-engine` and `unimatrix-server`. **None are in `nli_detection_tick.rs`**. Confirmed pre-existing: running clippy against the stash state (before the fix commit) also produced 60 errors in the same files. The fix introduces zero new clippy warnings.

Pre-existing clippy errors confirmed in: `crates/unimatrix-engine/src/auth.rs` and other non-fix files.

### 4. Integration Smoke Tests (Mandatory Gate)

**Command:** `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60`

**Result: 20/20 PASS**

All smoke tests passed in 2m54s.

### 5. Lifecycle Integration Suite

**Command:** `python -m pytest suites/test_lifecycle.py -v --timeout=60`

**Result: 38 passed, 2 xfailed, 1 xpassed**

| Category | Count |
|----------|-------|
| Passed | 38 |
| XFAIL (expected failures) | 2 |
| XPASS | 1 |
| Failed | 0 |

**XPASS investigation:** `test_search_multihop_injects_terminal_active` was marked `@pytest.mark.xfail(reason="Pre-existing: GH#406 — find_terminal_active multi-hop traversal not implemented")`. It now passes unexpectedly. This is entirely unrelated to the GH#421 fix (different code path: `search.rs` topology traversal vs. `nli_detection_tick.rs` candidate selection). The GH#406 XPASS is a pre-existing marker that should be removed as a follow-up — **not blocking this fix**.

### 6. Adaptation Suite (additional coverage)

**Command:** `python -m pytest suites/test_adaptation.py -v --timeout=60`

**Result: 9 passed, 1 xfailed**

The 1 xfail (`test_volume_with_adaptation_active`) is pre-existing with a known reason. All other adaptation tests pass.

---

## Integration Test Totals

| Suite | Tests Run | Passed | XFAIL | XPASS | Failed |
|-------|-----------|--------|-------|-------|--------|
| Smoke | 20 | 20 | 0 | 0 | 0 |
| Lifecycle | 41 | 38 | 2 | 1 | 0 |
| Adaptation | 10 | 9 | 1 | 0 | 0 |
| **Total** | **71** | **67** | **3** | **1** | **0** |

---

## Fix Verification Summary

| Check | Result |
|-------|--------|
| RC-1: shuffled tiers prevent deterministic re-selection | VERIFIED — `test_select_source_candidates_nondeterministic_rotation` |
| RC-2: no-embedding entries excluded from both tiers | VERIFIED — `test_select_source_candidates_excludes_no_embedding_entries` |
| `tier2.sort_by(...)` removed (was self-defeating) | VERIFIED — code reviewed, absent |
| `embedded_ids: &HashSet<u64>` parameter added | VERIFIED — signature confirmed in source |
| Both tiers independently shuffled with `rand::rng()` | VERIFIED — code reviewed |
| No regressions in full workspace suite | VERIFIED — 0 failures |
| Integration smoke gate | PASSED — 20/20 |

---

## Notes

1. **GH#406 XPASS:** `test_search_multihop_injects_terminal_active` unexpectedly passes. Not caused by this fix. Follow-up: remove the `xfail` marker and close GH#406 in a separate commit.
2. **Pre-existing clippy errors:** Multiple errors in `unimatrix-engine` and other crates exist before this fix and are unrelated to it. Scope for a separate cleanup pass.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entry #3668 (lesson-learned: deterministic tick stall bug pattern) and #3655 (pattern: NLI tick two-bound cap). Both confirmed the fix addresses the exact known failure mode documented during crt-029 design.
- Stored: nothing novel — the lesson (#3668) and pattern (#3655) documenting this fix class were already stored during crt-029 design. Confirmed they are accurate and match the implemented fix.
